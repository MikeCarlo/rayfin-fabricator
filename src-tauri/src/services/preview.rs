//! The project preview.
//!
//! The desktop app painted this in a native child webview positioned over the
//! page. The browser app paints it in an iframe inside that same host: these
//! commands remember the URL and the back/forward stack, and tell the page what
//! to show. Script injection (Touch Up, clearing site data, screenshots) only
//! reaches a preview served from this computer, and only when the page reports
//! the result of `preview:eval`.

use std::collections::{HashMap, VecDeque};
use std::sync::Mutex;
use std::time::Duration;

use serde::Serialize;
use tauri::{AppHandle, Emitter, LogicalPosition, LogicalSize, Manager, PhysicalSize, State, Url};
use tokio::sync::oneshot;

use crate::error::{AppError, AppResult};

/// Renderer event carrying preview navigation state (matches `IpcChannels.previewNav`).
const PREVIEW_NAV: &str = "preview:nav";
/// Ask the page to run JavaScript in the preview iframe and post the result.
const PREVIEW_EVAL: &str = "preview:eval";
/// Ask the page to reload the preview iframe.
pub const PREVIEW_RELOAD: &str = "preview:reload";

/// Kept so older notes can still name the still-image scheme. Stills are now a
/// data URL; nothing is served from this name.
pub const STILL_SCHEME: &str = "preview-still";

/// Renderer visual-viewport CSS bounds plus its physical-pixel ratio.
#[derive(Debug, Clone, Copy, serde::Deserialize)]
pub struct PreviewBounds {
  pub x: f64,
  pub y: f64,
  pub width: f64,
  pub height: f64,
  #[serde(default, rename = "pixelRatio")]
  pub pixel_ratio: Option<f64>,
}

impl PreviewBounds {
  pub(crate) fn to_rect(self) -> AppResult<tauri::Rect> {
    let ratio = self.pixel_ratio.unwrap_or(1.0);
    if !ratio.is_finite() || ratio <= 0.0
      || !self.width.is_finite() || !self.height.is_finite()
      || self.width < 0.0 || self.height < 0.0
      || [self.x, self.y, self.width.max(1.0), self.height.max(1.0)].iter().any(|n| {
        !n.is_finite() || !(i32::MIN as f64..=i32::MAX as f64).contains(&(n * ratio).round())
      })
    {
      return Err(AppError::Msg("Invalid native preview bounds or pixel ratio.".into()));
    }
    let position = LogicalPosition::new(self.x, self.y);
    let size = LogicalSize::new(self.width.max(1.0), self.height.max(1.0));
    Ok(match self.pixel_ratio {
      Some(ratio) => {
        let physical_size = size.to_physical::<u32>(ratio);
        tauri::Rect {
          position: position.to_physical::<i32>(ratio).into(),
          size: PhysicalSize::new(physical_size.width.max(1), physical_size.height.max(1)).into(),
        }
      }
      None => tauri::Rect { position: position.into(), size: size.into() },
    })
  }

  #[cfg(windows)]
  fn parked(self) -> Self {
    Self { x: 20_000.0, y: 20_000.0, ..self }
  }
}

/// Navigation state pushed to the renderer toolbar (matches `PreviewNavState`).
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct PreviewNavState {
  url: String,
  loading: bool,
  can_go_back: bool,
  can_go_forward: bool,
}

/// Managed state for the preview's back/forward history.
#[derive(Default)]
pub struct PreviewState {
  inner: Mutex<Inner>,
  stills: Mutex<VecDeque<Still>>,
}

const STILLS_KEPT: usize = 3;

struct Still {
  key: String,
  bytes: Vec<u8>,
}

impl PreviewState {
  fn keep_still(&self, bytes: Vec<u8>) -> String {
    let key = next_still_key();
    let mut stills = self.stills.lock().unwrap();
    if stills.len() == STILLS_KEPT {
      stills.pop_front();
    }
    stills.push_back(Still { key: key.clone(), bytes });
    key
  }

  fn still_bytes(&self, key: &str) -> Option<Vec<u8>> {
    self.stills.lock().unwrap().iter().find(|s| s.key == key).map(|s| s.bytes.clone())
  }
}

fn next_still_key() -> String {
  use std::sync::atomic::{AtomicU64, Ordering};
  static COUNT: AtomicU64 = AtomicU64::new(0);
  let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or_default();
  format!("{now:x}-{}", COUNT.fetch_add(1, Ordering::Relaxed))
}

#[derive(Default)]
struct Inner {
  commanded: Option<String>,
  nav_state: Option<PreviewNavState>,
  stack: Vec<String>,
  cursor: usize,
  visible: bool,
  pick: Option<PickSession>,
}

#[derive(Clone, Debug, PartialEq)]
struct PickSession {
  on: bool,
  relay: Option<String>,
}

impl Inner {
  fn can_back(&self) -> bool {
    self.cursor > 0
  }

  fn can_forward(&self) -> bool {
    self.cursor + 1 < self.stack.len()
  }

  fn reset_to(&mut self, url: &str) {
    self.commanded = Some(url.to_string());
    self.stack = vec![url.to_string()];
    self.cursor = 0;
    self.pick = None;
  }
}

fn emit_nav(app: &AppHandle, url: &str, loading: bool, can_back: bool, can_fwd: bool) {
  let nav = PreviewNavState {
    url: url.to_string(),
    loading,
    can_go_back: can_back,
    can_go_forward: can_fwd,
  };
  app.state::<PreviewState>().inner.lock().unwrap().nav_state = Some(nav.clone());
  let _ = app.emit(PREVIEW_NAV, nav);
}

fn show_url(app: &AppHandle, state: &PreviewState, url: &str, visible: bool) -> AppResult<()> {
  let _: Url = url.parse().map_err(|error| AppError::Msg(format!("invalid preview url {url:?}: {error}")))?;
  let changed = {
    let mut inner = state.inner.lock().unwrap();
    let changed = inner.commanded.as_deref() != Some(url);
    if changed {
      inner.reset_to(url);
    }
    inner.visible = visible;
    (changed, inner.can_back(), inner.can_forward())
  };
  let (changed, back, forward) = changed;
  if changed {
    emit_nav(app, url, true, back, forward);
    emit_nav(app, url, false, back, forward);
  } else if let Some(nav) = state.inner.lock().unwrap().nav_state.clone() {
    let _ = app.emit(PREVIEW_NAV, nav);
  }
  Ok(())
}

#[tauri::command]
pub async fn preview_show_url(app: AppHandle, state: State<'_, PreviewState>, url: String, bounds: PreviewBounds) -> AppResult<()> {
  bounds.to_rect()?;
  show_url(&app, &state, &url, true)
}

#[tauri::command]
pub async fn preview_preload(app: AppHandle, state: State<'_, PreviewState>, url: String, bounds: PreviewBounds) -> AppResult<()> {
  bounds.to_rect()?;
  show_url(&app, &state, &url, false)
}

#[tauri::command]
pub fn preview_navigate(app: AppHandle, state: State<'_, PreviewState>, url: String, bounds: PreviewBounds) -> AppResult<()> {
  bounds.to_rect()?;
  show_url(&app, &state, &url, state.inner.lock().unwrap().visible)
}

#[tauri::command]
pub fn preview_set_bounds(_app: AppHandle, bounds: PreviewBounds) -> AppResult<()> {
  bounds.to_rect()?;
  Ok(())
}

#[tauri::command]
pub fn preview_hide(app: AppHandle) -> AppResult<()> {
  app.state::<PreviewState>().inner.lock().unwrap().visible = false;
  Ok(())
}

#[tauri::command]
pub fn preview_suppress(app: AppHandle, bounds: PreviewBounds) -> AppResult<()> {
  bounds.to_rect()?;
  app.state::<PreviewState>().inner.lock().unwrap().visible = false;
  Ok(())
}

#[tauri::command]
pub fn preview_reload(app: AppHandle) -> AppResult<()> {
  let _ = app.emit(PREVIEW_RELOAD, ());
  Ok(())
}

/// Reload the preview when it shows a page from `origin`.
pub fn reload_if_showing(app: &AppHandle, origin: &str) {
  let prefix = format!("{}/", origin.trim_end_matches('/'));
  let showing = app.state::<PreviewState>().inner.lock().unwrap().commanded.clone();
  if showing.is_some_and(|url| url.starts_with(&prefix) || url.trim_end_matches('/') == origin.trim_end_matches('/')) {
    let _ = app.emit(PREVIEW_RELOAD, ());
  }
}

#[tauri::command]
pub fn preview_back(app: AppHandle, state: State<'_, PreviewState>) -> AppResult<()> {
  let target = {
    let mut inner = state.inner.lock().unwrap();
    if !inner.can_back() {
      return Ok(());
    }
    inner.cursor -= 1;
    inner.stack.get(inner.cursor).cloned()
  };
  if let Some(url) = target {
    let (back, forward) = {
      let inner = state.inner.lock().unwrap();
      (inner.can_back(), inner.can_forward())
    };
    emit_nav(&app, &url, false, back, forward);
  }
  Ok(())
}

#[tauri::command]
pub fn preview_forward(app: AppHandle, state: State<'_, PreviewState>) -> AppResult<()> {
  let target = {
    let mut inner = state.inner.lock().unwrap();
    if !inner.can_forward() {
      return Ok(());
    }
    inner.cursor += 1;
    inner.stack.get(inner.cursor).cloned()
  };
  if let Some(url) = target {
    let (back, forward) = {
      let inner = state.inner.lock().unwrap();
      (inner.can_back(), inner.can_forward())
    };
    emit_nav(&app, &url, false, back, forward);
  }
  Ok(())
}

fn pick_set_js(armed: bool, on: bool, relay_origin: Option<&str>, options: Option<&serde_json::Value>) -> String {
  let mode = if relay_origin.is_some() { "relay" } else { "direct" };
  let origin = serde_json::to_string(&relay_origin).unwrap_or_else(|_| "null".into());
  let mut opts = options.cloned().filter(serde_json::Value::is_object).unwrap_or_else(|| serde_json::json!({}));
  opts["armed"] = serde_json::Value::Bool(armed);
  opts["on"] = serde_json::Value::Bool(armed && on);
  let opts = serde_json::to_string(&opts).unwrap_or_else(|_| "{}".into());
  format!("try{{window.__fabricatorPick&&window.__fabricatorPick.set('{mode}',{origin},{opts})}}catch(e){{}}")
}

fn pick_poll_js(page: Option<&str>, since: Option<u64>) -> String {
  let opts = serde_json::json!({ "page": page, "since": since.unwrap_or(0) });
  format!("(function(){{try{{return window.__fabricatorPick?window.__fabricatorPick.peek({opts}):null}}catch(e){{return null}}}})()")
}

fn pick_command_js(command: &serde_json::Value) -> String {
  let json = serde_json::to_string(command).unwrap_or_else(|_| "null".into());
  format!("try{{window.__fabricatorPick&&window.__fabricatorPick.command({json})}}catch(e){{}}")
}

fn url_origin(url: &str) -> Option<String> {
  match Url::parse(url).map(|u| u.origin()) {
    Ok(origin) if origin.is_tuple() => Some(origin.ascii_serialization()),
    _ => None,
  }
}

fn loopback_origin(url: &Url) -> Option<String> {
  if !matches!(url.scheme(), "http" | "https") {
    return None;
  }
  let host = url.host_str()?;
  let loopback = host.eq_ignore_ascii_case("localhost")
    || host
      .trim_start_matches('[')
      .trim_end_matches(']')
      .parse::<std::net::IpAddr>()
      .is_ok_and(|ip| ip.is_loopback());
  loopback.then(|| url.origin().ascii_serialization())
}

fn reset_site_data_js(origin: &str) -> String {
  let origin = serde_json::to_string(origin).unwrap_or_else(|_| "null".into());
  format!(
    "(function(){{try{{if(location.origin!=={origin})return false;\
     try{{localStorage.clear()}}catch(e){{}}\
     try{{sessionStorage.clear()}}catch(e){{}}\
     return true}}catch(e){{return false}}}})()"
  )
}

static EVALS: Mutex<Option<HashMap<String, oneshot::Sender<Option<String>>>>> = Mutex::new(None);

fn evals() -> std::sync::MutexGuard<'static, Option<HashMap<String, oneshot::Sender<Option<String>>>>> {
  let mut guard = EVALS.lock().unwrap();
  if guard.is_none() {
    *guard = Some(HashMap::new());
  }
  guard
}

/// The page posts the iframe's answer here. `None` when it could not run.
pub fn resolve_eval(id: &str, result: Option<String>) -> bool {
  let Some(tx) = evals().as_mut().and_then(|map| map.remove(id)) else {
    return false;
  };
  tx.send(result).is_ok()
}

fn emit_eval(app: &AppHandle, js: &str) -> String {
  use std::sync::atomic::{AtomicU64, Ordering};
  static NEXT: AtomicU64 = AtomicU64::new(1);
  let id = NEXT.fetch_add(1, Ordering::Relaxed).to_string();
  let _ = app.emit(PREVIEW_EVAL, serde_json::json!({ "id": id, "js": js }));
  id
}

async fn page_eval<T: serde::de::DeserializeOwned>(app: &AppHandle, js: &str) -> AppResult<Option<T>> {
  use std::sync::atomic::{AtomicU64, Ordering};
  static NEXT: AtomicU64 = AtomicU64::new(1);
  let (tx, rx) = oneshot::channel();
  let id = NEXT.fetch_add(1, Ordering::Relaxed).to_string();
  evals().as_mut().unwrap().insert(id.clone(), tx);
  let _ = app.emit(PREVIEW_EVAL, serde_json::json!({ "id": id, "js": js }));
  let res = match tokio::time::timeout(Duration::from_secs(4), rx).await {
    Ok(Ok(Some(value))) => value,
    _ => return Ok(None),
  };
  if res.len() > 1024 * 1024 {
    return Ok(None);
  }
  let trimmed = res.trim();
  if trimmed.is_empty() || trimmed == "null" {
    return Ok(None);
  }
  Ok(serde_json::from_str(trimmed).ok())
}

#[tauri::command]
pub fn preview_pick_set(
  app: AppHandle,
  state: State<'_, PreviewState>,
  armed: bool,
  on: bool,
  embedded: Option<bool>,
  app_url: Option<String>,
  options: Option<serde_json::Value>,
) -> AppResult<()> {
  let relay = if armed && embedded.unwrap_or(false) { app_url.as_deref().and_then(url_origin) } else { None };
  state.inner.lock().unwrap().pick = armed.then(|| PickSession { on, relay: relay.clone() });
  let _ = emit_eval(&app, &pick_set_js(armed, on, relay.as_deref(), options.as_ref()));
  Ok(())
}

#[tauri::command]
pub async fn preview_pick_poll(app: AppHandle, page: Option<String>, since: Option<u64>) -> AppResult<Option<serde_json::Value>> {
  page_eval(&app, &pick_poll_js(page.as_deref(), since)).await
}

#[tauri::command]
pub fn preview_pick_command(app: AppHandle, command: serde_json::Value) -> AppResult<()> {
  if !command.get("op").is_some_and(|op| op.is_string()) {
    return Err(AppError::Msg("A Touch Up command needs an `op`.".into()));
  }
  let _ = emit_eval(&app, &pick_command_js(&command));
  Ok(())
}

#[tauri::command]
pub fn preview_release_focus(app: AppHandle) -> AppResult<()> {
  let _ = app.emit("window:focus", "main");
  Ok(())
}

#[tauri::command]
pub async fn preview_capture(_app: AppHandle) -> AppResult<String> {
  Err(AppError::Msg(
    "The browser preview can't photograph a page from another address. Open it and take a screenshot, or use a preview served from this computer once the page allows it.".into(),
  ))
}

#[tauri::command]
pub async fn preview_still(_app: AppHandle, _state: State<'_, PreviewState>) -> AppResult<String> {
  // A transparent pixel. The iframe stays visible under HTML overlays, so the
  // still only has to let the overlay continue.
  Ok("data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8z8BQDwAEhQGAhKmMIQAAAABJRU5ErkJggg==".into())
}

#[tauri::command]
pub async fn preview_reset_site_data(app: AppHandle) -> AppResult<bool> {
  let url = app.state::<PreviewState>().inner.lock().unwrap().commanded.clone();
  let Some(url) = url.as_deref().and_then(|url| Url::parse(url).ok()) else {
    return Ok(false);
  };
  let Some(origin) = loopback_origin(&url) else {
    return Ok(false);
  };
  match page_eval::<bool>(&app, &reset_site_data_js(&origin)).await? {
    Some(true) => {
      let _ = app.emit(PREVIEW_RELOAD, ());
      Ok(true)
    }
    _ => Ok(false),
  }
}

#[cfg(test)]
mod tests {
  use super::{loopback_origin, pick_command_js, pick_poll_js, pick_set_js, reset_site_data_js, PreviewBounds, PreviewState};
  use tauri::Url;

  #[test]
  fn stills_are_served_by_key_until_newer_ones_push_them_out() {
    let state = PreviewState::default();
    let keys: Vec<String> = (0..4u8).map(|n| state.keep_still(vec![n])).collect();
    assert_eq!(keys.iter().collect::<std::collections::HashSet<_>>().len(), keys.len());
    assert_eq!(state.still_bytes(&keys[0]), None, "the oldest still was let go");
    for (n, key) in keys.iter().enumerate().skip(1) {
      assert_eq!(state.still_bytes(key), Some(vec![n as u8]), "{key}");
    }
    assert_eq!(state.still_bytes(""), None);
  }

  fn bounds(pixel_ratio: Option<f64>) -> PreviewBounds {
    PreviewBounds { x: 100.0, y: 80.0, width: 900.0, height: 600.0, pixel_ratio }
  }

  #[test]
  fn renderer_pixels_are_not_rescaled_by_native_window_dpi() {
    for (ratio, native_dpi, x, y, width, height) in [
      (1.0, 1.0, 100, 80, 900, 600),
      (1.25, 1.0, 125, 100, 1125, 750),
      (1.5, 1.0, 150, 120, 1350, 900),
      (2.0, 1.0, 200, 160, 1800, 1200),
      (1.5625, 1.25, 156, 125, 1406, 938),
      (2.5, 2.0, 250, 200, 2250, 1500),
    ] {
      let rect = bounds(Some(ratio)).to_rect().unwrap();
      assert!(matches!(rect.position, tauri::Position::Physical(_)));
      assert_eq!(rect.position.to_physical::<i32>(native_dpi), tauri::PhysicalPosition::new(x, y));
      assert_eq!(rect.size.to_physical::<u32>(native_dpi), tauri::PhysicalSize::new(width, height));
    }
  }

  #[test]
  fn legacy_bounds_without_pixel_ratio_still_use_native_logical_units() {
    let bounds: PreviewBounds = serde_json::from_str(r#"{"x":100,"y":80,"width":900,"height":600}"#).unwrap();
    let rect = bounds.to_rect().unwrap();
    assert!(matches!(rect.position, tauri::Position::Logical(_)));
    assert_eq!(rect.position.to_physical::<i32>(1.5), tauri::PhysicalPosition::new(150, 120));
    assert_eq!(rect.size.to_physical::<u32>(1.5), tauri::PhysicalSize::new(1350, 900));
  }

  #[test]
  fn renderer_pixel_ratio_round_trips_from_ipc() {
    let bounds: PreviewBounds = serde_json::from_str(r#"{"x":100,"y":80,"width":900,"height":600,"pixelRatio":1.25}"#).unwrap();
    assert_eq!(bounds.pixel_ratio, Some(1.25));
    assert_eq!(bounds.to_rect().unwrap().position.to_physical::<i32>(2.0), tauri::PhysicalPosition::new(125, 100));
  }

  #[test]
  fn invalid_bounds_are_rejected_and_tiny_sizes_remain_nonzero() {
    for invalid in [
      PreviewBounds { x: f64::NAN, ..bounds(Some(1.0)) },
      PreviewBounds { width: f64::NAN, ..bounds(Some(1.0)) },
      PreviewBounds { height: f64::INFINITY, ..bounds(Some(1.0)) },
      PreviewBounds { width: -1.0, ..bounds(Some(1.0)) },
      PreviewBounds { x: f64::MAX, ..bounds(Some(1.0)) },
      bounds(Some(0.0)),
      bounds(Some(-1.0)),
      bounds(Some(f64::INFINITY)),
    ] {
      assert!(invalid.to_rect().is_err(), "{invalid:?}");
    }
    let tiny = PreviewBounds { width: 1.0, height: 1.0, ..bounds(Some(0.25)) };
    assert_eq!(tiny.to_rect().unwrap().size.to_physical::<u32>(2.0), tauri::PhysicalSize::new(1, 1));
  }

  #[cfg(windows)]
  #[test]
  fn parking_preserves_the_actual_physical_viewport_size() {
    let current = bounds(Some(1.875));
    let parked = current.parked();
    assert_eq!(parked.pixel_ratio, current.pixel_ratio);
    assert_eq!(
      parked.to_rect().unwrap().size.to_physical::<u32>(1.25),
      current.to_rect().unwrap().size.to_physical::<u32>(1.25),
    );
    assert!(parked.to_rect().unwrap().position.to_physical::<i32>(1.25).x > 20_000);
  }

  #[test]
  fn pick_set_js_selects_direct_or_relay_and_never_switches_on_unarmed() {
    assert_eq!(
      pick_set_js(true, false, None, None),
      "try{window.__fabricatorPick&&window.__fabricatorPick.set('direct',null,{\"armed\":true,\"on\":false})}catch(e){}"
    );
    let opts = serde_json::json!({ "picks": [{ "id": "k1", "n": 1 }] });
    let relay = pick_set_js(true, true, Some("https://p1.example.app"), Some(&opts));
    assert!(relay.contains("set('relay',\"https://p1.example.app\",{"));
    assert!(relay.contains("\"picks\":[{\"id\":\"k1\",\"n\":1}]"));
    assert!(relay.contains("\"on\":true"));
    assert!(pick_set_js(false, true, None, None).contains("{\"armed\":false,\"on\":false}"));
  }

  #[test]
  fn pick_poll_js_asks_only_for_new_events_on_the_same_page() {
    let js = pick_poll_js(Some("p1"), Some(7));
    assert!(js.contains("peek({\"page\":\"p1\",\"since\":7})"), "{js}");
    assert!(pick_poll_js(None, None).contains("peek({\"page\":null,\"since\":0})"));
  }

  #[test]
  fn pick_command_js_embeds_the_command_as_a_json_literal() {
    let cmd = serde_json::json!({ "op": "focus", "id": "a \"quoted\" \\ </script>" });
    let js = pick_command_js(&cmd);
    assert!(js.starts_with("try{window.__fabricatorPick&&window.__fabricatorPick.command({"));
    let json = js
      .trim_start_matches("try{window.__fabricatorPick&&window.__fabricatorPick.command(")
      .trim_end_matches(")}catch(e){}");
    assert_eq!(serde_json::from_str::<serde_json::Value>(json).unwrap(), cmd);
  }

  #[test]
  fn site_data_is_reset_only_for_pages_served_from_this_computer() {
    for (url, origin) in [
      ("http://localhost:5173/", "http://localhost:5173"),
      ("http://localhost:5173/notes?id=1#top", "http://localhost:5173"),
      ("http://LOCALHOST:5173/", "http://localhost:5173"),
      ("https://localhost:7171/.rayfin-local/portal/embed", "https://localhost:7171"),
      ("http://127.0.0.1:5173/", "http://127.0.0.1:5173"),
      ("http://127.4.5.6/", "http://127.4.5.6"),
      ("http://[::1]:5173/", "http://[::1]:5173"),
    ] {
      assert_eq!(loopback_origin(&Url::parse(url).unwrap()).as_deref(), Some(origin), "{url}");
    }
    for url in [
      "https://app.fabric.microsoft.com/groups/1/appbackends/2",
      "https://notes.example.app/",
      "http://localhost.example.com:5173/",
      "http://192.168.1.4:5173/",
      "http://0.0.0.0:5173/",
      "ws://localhost:5173/",
      "tauri://localhost/",
      "file:///C:/apps/notes/index.html",
      "about:blank",
    ] {
      assert_eq!(loopback_origin(&Url::parse(url).unwrap()), None, "{url}");
    }
  }

  #[test]
  fn the_site_data_reset_runs_only_on_the_origin_it_was_asked_for() {
    let js = reset_site_data_js("http://localhost:5173");
    assert!(js.starts_with("(function(){"), "{js}");
    assert!(js.contains(r#"if(location.origin!=="http://localhost:5173")return false;"#), "{js}");
    assert!(js.contains("try{localStorage.clear()}catch(e){}"), "{js}");
    assert!(js.contains("try{sessionStorage.clear()}catch(e){}"), "{js}");
    assert!(js.ends_with("return true}catch(e){return false}})()"), "{js}");
    let odd = reset_site_data_js(r#"http://localhost:1")||alert(1)//"#);
    assert!(odd.contains(r#"location.origin!=="http://localhost:1\")||alert(1)//")return false;"#), "{odd}");
  }
}
