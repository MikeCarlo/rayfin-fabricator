//! An installed app's window. The desktop build opened a native window on
//! Fabricator's page, then navigated it to the app. The browser build asks the
//! main page to open a tab, and that tab keeps Fabricator's page so it can show
//! the app in an iframe (and come back here when the app stops).

use std::path::PathBuf;
use std::time::Duration;

use tauri::{AppHandle, Emitter, Manager, Url, WindowEvent};

use super::Apps;

/// The labels of app windows: `app-<key>`, or `app-error-<id>` for a file that
/// can't be opened.
const PREFIX: &str = "app-";
/// Tauri event asking an app window's page to look again at what it's for.
pub const APPS_WINDOW: &str = "apps:window";

/// What an app window is for, and what it may show.
pub(super) struct WindowState {
  key: String,
  file: Option<PathBuf>,
  error: Option<String>,
  page_allowed: bool,
  app_url: Option<Url>,
}

pub fn is_app_window(label: &str) -> bool {
  label.starts_with(PREFIX)
}

fn label_of(key: &str) -> String {
  format!("{PREFIX}{key}")
}

#[derive(Debug, PartialEq, Eq)]
enum Nav {
  Allow,
  Deny,
  Browser,
}

fn same_app(app: &Url, url: &Url) -> bool {
  let loopback = |u: &Url| matches!(u.host_str(), Some("localhost" | "127.0.0.1" | "[::1]"));
  app.scheme() == url.scheme() && app.port_or_known_default() == url.port_or_known_default() && loopback(app) && loopback(url)
}

fn decide(url: &Url, own: Option<&Url>, page_allowed: bool, app: Option<&Url>, whole_page: bool) -> Nav {
  if own.is_some_and(|own| own.origin() == url.origin()) {
    return if page_allowed { Nav::Allow } else { Nav::Deny };
  }
  match url.scheme() {
    "http" | "https" if app.is_some_and(|app| same_app(app, url)) => Nav::Allow,
    "http" | "https" if whole_page => Nav::Browser,
    "http" | "https" | "about" | "blob" | "data" => Nav::Allow,
    _ => Nav::Deny,
  }
}

fn bring_forward(app: &AppHandle, label: &str) {
  let _ = app.emit("apps:focus", label.to_string());
}

/// Open the app's window, or bring it forward.
pub(super) fn open(
  app: &AppHandle,
  key: &str,
  title: &str,
  icon: Option<Vec<u8>>,
  file: Option<PathBuf>,
  error: Option<String>,
) {
  let _ = icon;
  let label = if key.is_empty() { format!("{PREFIX}error-{}", super::short_id()) } else { label_of(key) };
  let apps = app.state::<Apps>();
  if app.known_window(&label) {
    if file.is_some() || error.is_some() {
      if let Some(state) = apps.windows.lock().unwrap().get_mut(&label) {
        state.file = file;
        state.error = error;
      }
      show_page(app, &label);
    }
    bring_forward(app, &label);
    return;
  }
  let state = WindowState { key: key.to_string(), file, error, page_allowed: true, app_url: None };
  apps.windows.lock().unwrap().insert(label.clone(), state);
  let window = app.open_window(&label);
  let hook_app = app.clone();
  let hook_label = label.clone();
  window.on_window_event(move |event| {
    if let WindowEvent::Destroyed = event {
      closed(&hook_app, &hook_label);
    }
  });
  let _ = window.set_title(title);
  #[cfg(windows)]
  if !key.is_empty() {
    super::shell::identify(&window, key, title);
  }
  let _ = app.emit(
    "apps:open",
    serde_json::json!({ "label": label, "title": title, "url": app.page_url(&label) }),
  );
  let shown = window.clone();
  let shown_label = label.clone();
  std::thread::spawn(move || {
    std::thread::sleep(Duration::from_secs(4));
    if shown.is_visible().is_ok_and(|visible| !visible) {
      bring_forward(&shown.window().app(), &shown_label);
    }
  });
  let _ = app.emit(super::APPS_CHANGED, ());
}

fn closed(app: &AppHandle, label: &str) {
  let state = app.state::<Apps>().windows.lock().unwrap().remove(label);
  if let Some(key) = state.map(|s| s.key).filter(|key| !key.is_empty()) {
    super::stop(app, &key);
  }
  let _ = app.emit(super::APPS_CHANGED, ());
}

pub(super) fn purpose(app: &AppHandle, label: &str) -> Option<(String, Option<PathBuf>, Option<String>)> {
  let apps = app.state::<Apps>();
  let windows = apps.windows.lock().unwrap();
  let state = windows.get(label)?;
  Some((state.key.clone(), state.file.clone(), state.error.clone()))
}

pub(super) fn is_open(app: &AppHandle, key: &str) -> bool {
  app.known_window(&label_of(key))
}

pub(super) fn forget_file(app: &AppHandle, label: &str) {
  if let Some(state) = app.state::<Apps>().windows.lock().unwrap().get_mut(label) {
    state.file = None;
  }
}

pub(super) fn installed(app: &AppHandle, label: &str, name: &str, icon: Option<Vec<u8>>) {
  let key = {
    let apps = app.state::<Apps>();
    let mut windows = apps.windows.lock().unwrap();
    let Some(state) = windows.get_mut(label) else { return };
    state.file = None;
    state.key.clone()
  };
  let Some(window) = app.get_webview_window(label) else { return };
  let _ = window.set_title(name);
  let _ = icon;
  #[cfg(windows)]
  super::shell::identify(&window, &key, name);
  #[cfg(not(windows))]
  let _ = key;
}

pub(super) fn show_app(app: &AppHandle, key: &str, url: Url) {
  let label = label_of(key);
  let Some(window) = app.get_webview_window(&label) else { return };
  {
    let apps = app.state::<Apps>();
    let mut windows = apps.windows.lock().unwrap();
    let Some(state) = windows.get_mut(&label) else { return };
    if state.file.is_some() || state.error.is_some() || (!state.page_allowed && state.app_url.as_ref() == Some(&url)) {
      return;
    }
    state.app_url = Some(url.clone());
    state.page_allowed = false;
  }
  if let Err(error) = window.navigate(url) {
    log::warn!("[apps] couldn't show {key}: {error}");
  }
}

pub(super) fn show_launcher(app: &AppHandle, key: &str) {
  show_page(app, &label_of(key));
}

fn show_page(app: &AppHandle, label: &str) {
  let Some(window) = app.get_webview_window(label) else { return };
  let already = {
    let apps = app.state::<Apps>();
    let mut windows = apps.windows.lock().unwrap();
    let Some(state) = windows.get_mut(label) else { return };
    let already = state.page_allowed;
    state.page_allowed = true;
    state.app_url = None;
    already
  };
  if already {
    let _ = window.emit(APPS_WINDOW, label.to_string());
  } else if let Err(error) = window.navigate(app.page_url(label)) {
    log::warn!("[apps] couldn't show Fabricator's page in {label}: {error}");
  }
}

pub(super) fn close(app: &AppHandle, key: &str) {
  let label = label_of(key);
  if app.known_window(&label) {
    // Destroyed runs `closed`, which stops the app.
    app.close_window(&label);
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  fn url(text: &str) -> Url {
    Url::parse(text).unwrap()
  }

  #[test]
  fn an_app_window_stays_on_its_app() {
    let own = url("http://tauri.localhost/");
    let app = url("http://localhost:47100/");
    let decide = |to: &str, page_allowed: bool, whole_page: bool| decide(&url(to), Some(&own), page_allowed, Some(&app), whole_page);

    assert_eq!(decide("http://tauri.localhost/index.html", true, true), Nav::Allow);
    assert_eq!(decide("http://tauri.localhost/index.html", false, true), Nav::Deny);
    assert_eq!(decide("http://tauri.localhost/index.html", false, false), Nav::Deny, "nor in a frame");
    assert_eq!(decide("http://localhost:47100/notes?id=1", false, true), Nav::Allow);
    assert_eq!(decide("http://127.0.0.1:47100/.rayfin-local/portal", false, true), Nav::Allow);
    assert_eq!(decide("http://localhost:47101/", false, true), Nav::Browser);
    assert_eq!(decide("https://contoso.com/help", false, true), Nav::Browser);
    assert_eq!(decide("https://www.youtube.com/embed/x", false, false), Nav::Allow);
    assert_eq!(decide("file:///C:/Windows/win.ini", false, true), Nav::Deny);
    assert_eq!(decide("about:blank", false, true), Nav::Allow);
  }

  #[test]
  fn in_development_fabricators_page_is_its_dev_server() {
    let own = url("http://localhost:1420/");
    let app = url("http://localhost:47100/");
    assert_eq!(decide(&url("http://localhost:1420/index.html"), Some(&own), false, Some(&app), true), Nav::Deny);
    assert_eq!(decide(&url("http://localhost:47100/"), Some(&own), false, Some(&app), true), Nav::Allow);
  }
}
