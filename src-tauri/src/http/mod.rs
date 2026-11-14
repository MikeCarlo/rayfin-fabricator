//! The local server: the built page, every command, and the event socket.
//!
//! It listens on 127.0.0.1 only. Each launch picks a token; the browser tab we
//! open carries it, and every `/api` and WebSocket call has to present it.
//! Another page on this computer, or a site you have open, cannot drive the
//! commands without that token, and a request whose Origin is some other site
//! is refused even with it.

use std::collections::HashMap;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::time::Duration;

use axum::body::Body;
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::{DefaultBodyLimit, Multipart, Path as UrlPath, Query, State};
use axum::http::{header, HeaderMap, Method, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::Deserialize;
use serde_json::{json, Value};
use tokio::net::TcpListener;
use tokio::sync::broadcast;

use crate::services::paths;
use crate::services::preview;
use tauri::{commands, AppHandle, InvokeCtx};

const DEFAULT_PORT: u16 = 5173;
const DEV_PORTS: &[u16] = &[1420];

#[derive(rust_embed::Embed)]
#[folder = "../dist"]
struct Assets;

#[derive(Clone)]
struct Gate {
  port: u16,
  instance: String,
}

pub async fn serve(app: AppHandle) -> std::io::Result<()> {
  let instance = uuid::Uuid::new_v4().simple().to_string();
  let preferred = std::env::var("FABRICATOR_PORT").ok().and_then(|text| text.parse().ok()).unwrap_or(DEFAULT_PORT);
  if std::env::var_os("FABRICATOR_REPLACE").is_none() {
    if let Some(url) = existing_session(&instance).await {
      println!("Fabricator is already running at {url}");
      let _ = open_browser(&url);
      return Ok(());
    }
  }

  let listener = bind_loopback(preferred)?;
  let port = listener.local_addr()?.port();
  let origin = format!("http://127.0.0.1:{port}");
  let token = uuid::Uuid::new_v4().simple().to_string() + &uuid::Uuid::new_v4().simple().to_string();
  app.set_origin(origin.clone());
  app.set_token(token.clone());
  let url = format!("{origin}/?token={token}");
  write_session(&instance, port, &token, &url);
  write_dev_file(port, &token, &url);

  let router = router(app.clone(), port, instance);
  println!("Fabricator is running at {url}");
  println!("Close this window to quit.");
  let _ = open_browser(&url);

  axum::serve(listener, router).with_graceful_shutdown(shutdown(app)).await
}

fn router(app: AppHandle, port: u16, instance: String) -> Router {
  Router::new()
    .route("/api/health", get(health))
    .route("/api/cmd/{name}", post(command))
    .route("/api/pick/{id}", post(resolve_pick))
    .route("/api/eval/{id}", post(resolve_eval))
    .route("/api/fs/list", get(list_dir))
    .route("/api/fs/stage", post(stage_files))
    .route("/api/fs/reveal", post(reveal_path))
    .route("/ws", get(events))
    .fallback(static_file)
    .layer(DefaultBodyLimit::max(64 * 1024 * 1024))
    .with_state((app, Gate { port, instance }))
}

/// The token is stored on the handle, which the tests set themselves. The gate
/// in router state is filled in `serve`; handlers read the handle.
type AppState = (AppHandle, Gate);

fn token_of(headers: &HeaderMap, query: Option<&str>) -> Option<String> {
  if let Some(query) = query.filter(|text| !text.is_empty()) {
    return Some(query.to_string());
  }
  let header = headers.get(header::AUTHORIZATION)?.to_str().ok()?;
  let rest = header.strip_prefix("Bearer ").or_else(|| header.strip_prefix("bearer "))?;
  Some(rest.trim().to_string())
}

fn origin_ok(headers: &HeaderMap, port: u16) -> bool {
  let Some(origin) = headers.get(header::ORIGIN).and_then(|value| value.to_str().ok()) else {
    return true;
  };
  if origin == format!("http://127.0.0.1:{port}") || origin == format!("http://localhost:{port}") {
    return true;
  }
  DEV_PORTS.iter().any(|dev| origin == format!("http://127.0.0.1:{dev}") || origin == format!("http://localhost:{dev}"))
}

fn authorized(app: &AppHandle, gate: &Gate, headers: &HeaderMap, query_token: Option<&str>) -> Result<(), StatusCode> {
  if !origin_ok(headers, gate.port) {
    return Err(StatusCode::FORBIDDEN);
  }
  match token_of(headers, query_token) {
    Some(token) if token == app.token() && !app.token().is_empty() => Ok(()),
    _ => Err(StatusCode::UNAUTHORIZED),
  }
}

async fn health(State((_, gate)): State<AppState>) -> Json<Value> {
  Json(json!({ "ok": true, "instance": gate.instance }))
}

async fn command(
  State((app, gate)): State<AppState>,
  UrlPath(name): UrlPath<String>,
  headers: HeaderMap,
  body: String,
) -> Response {
  if let Err(status) = authorized(&app, &gate, &headers, None) {
    return status.into_response();
  }
  let args: Value = if body.trim().is_empty() { json!({}) } else { serde_json::from_str(&body).unwrap_or(Value::Null) };
  if !args.is_object() && !args.is_null() {
    return (StatusCode::BAD_REQUEST, Json(json!({ "ok": false, "error": "Command arguments must be a JSON object." }))).into_response();
  }
  let Some(entry) = command_table().get(name.as_str()).copied() else {
    return (StatusCode::NOT_FOUND, Json(json!({ "ok": false, "error": format!("Unknown command {name}.") }))).into_response();
  };
  let label = headers
    .get("x-fabricator-window")
    .and_then(|value| value.to_str().ok())
    .filter(|label| !label.is_empty())
    .unwrap_or("main")
    .to_string();
  let ctx = InvokeCtx { app: app.clone(), window_label: label };
  match (entry)(ctx, if args.is_null() { json!({}) } else { args }).await {
    Ok(data) => Json(json!({ "ok": true, "data": data })).into_response(),
    Err(error) => (StatusCode::UNPROCESSABLE_ENTITY, Json(json!({ "ok": false, "error": error }))).into_response(),
  }
}

fn command_table() -> &'static HashMap<&'static str, tauri::CommandEntryFn> {
  static TABLE: OnceLock<HashMap<&'static str, tauri::CommandEntryFn>> = OnceLock::new();
  TABLE.get_or_init(|| commands().into_iter().map(|entry| (entry.name, entry.call)).collect())
}

#[derive(Deserialize)]
struct PickBody {
  paths: Option<Vec<String>>,
  cancel: Option<bool>,
}

async fn resolve_pick(
  State((app, gate)): State<AppState>,
  UrlPath(id): UrlPath<String>,
  headers: HeaderMap,
  Json(body): Json<PickBody>,
) -> Response {
  if let Err(status) = authorized(&app, &gate, &headers, None) {
    return status.into_response();
  }
  let paths = if body.cancel.unwrap_or(false) {
    None
  } else {
    Some(body.paths.unwrap_or_default().into_iter().map(PathBuf::from).collect())
  };
  let found = app.resolve_pick(&id, paths);
  Json(json!({ "ok": found })).into_response()
}

#[derive(Deserialize)]
struct EvalBody {
  result: Option<String>,
}

async fn resolve_eval(
  State((app, gate)): State<AppState>,
  UrlPath(id): UrlPath<String>,
  headers: HeaderMap,
  Json(body): Json<EvalBody>,
) -> Response {
  if let Err(status) = authorized(&app, &gate, &headers, None) {
    return status.into_response();
  }
  let _ = app;
  Json(json!({ "ok": preview::resolve_eval(&id, body.result) })).into_response()
}

#[derive(Deserialize)]
struct ListQuery {
  path: Option<String>,
}

async fn list_dir(State((app, gate)): State<AppState>, headers: HeaderMap, Query(query): Query<ListQuery>) -> Response {
  if let Err(status) = authorized(&app, &gate, &headers, None) {
    return status.into_response();
  }
  match list_folder(query.path.as_deref()) {
    Ok(value) => Json(value).into_response(),
    Err(error) => (StatusCode::BAD_REQUEST, Json(json!({ "ok": false, "error": error }))).into_response(),
  }
}

fn list_folder(path: Option<&str>) -> Result<Value, String> {
  let path = path.unwrap_or("").trim();
  if path.is_empty() {
    return Ok(json!({
      "path": "",
      "parent": Value::Null,
      "entries": roots(),
    }));
  }
  let dir = PathBuf::from(path);
  if !dir.is_dir() {
    return Err("That folder isn't there.".into());
  }
  let mut entries = Vec::new();
  let read = std::fs::read_dir(&dir).map_err(|error| error.to_string())?;
  for entry in read.flatten() {
    let file_type = entry.file_type().ok();
    let dir_entry = file_type.as_ref().is_some_and(|kind| kind.is_dir());
    entries.push(json!({
      "name": entry.file_name().to_string_lossy(),
      "path": entry.path().display().to_string(),
      "dir": dir_entry,
    }));
  }
  entries.sort_by(|a, b| {
    let ad = a["dir"].as_bool().unwrap_or(false);
    let bd = b["dir"].as_bool().unwrap_or(false);
    bd.cmp(&ad).then_with(|| a["name"].as_str().unwrap_or("").to_lowercase().cmp(&b["name"].as_str().unwrap_or("").to_lowercase()))
  });
  let parent = dir.parent().filter(|parent| *parent != dir).map(|parent| parent.display().to_string());
  Ok(json!({ "path": dir.display().to_string(), "parent": parent, "entries": entries }))
}

fn roots() -> Vec<Value> {
  let mut roots = Vec::new();
  if let Some(home) = dirs::home_dir() {
    roots.push(json!({ "name": "Home", "path": home.display().to_string(), "dir": true }));
  }
  if cfg!(windows) {
    for letter in 'A'..='Z' {
      let path = format!("{letter}:\\");
      if Path::new(&path).is_dir() {
        roots.push(json!({ "name": format!("{letter}:"), "path": path, "dir": true }));
      }
    }
  } else if Path::new("/").is_dir() {
    roots.push(json!({ "name": "/", "path": "/", "dir": true }));
  }
  roots
}

async fn stage_files(State((app, gate)): State<AppState>, headers: HeaderMap, mut multipart: Multipart) -> Response {
  if let Err(status) = authorized(&app, &gate, &headers, None) {
    return status.into_response();
  }
  let dir = paths::cache_dir().join("drops").join(uuid::Uuid::new_v4().simple().to_string());
  if let Err(error) = std::fs::create_dir_all(&dir) {
    return (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({ "ok": false, "error": error.to_string() }))).into_response();
  }
  let mut paths = Vec::new();
  while let Ok(Some(field)) = multipart.next_field().await {
    let name = field.file_name().unwrap_or("file").to_string();
    let name = Path::new(&name).file_name().map(|name| name.to_string_lossy().to_string()).unwrap_or_else(|| "file".into());
    let dest = dir.join(&name);
    match field.bytes().await {
      Ok(bytes) => {
        if std::fs::write(&dest, bytes).is_ok() {
          paths.push(dest.display().to_string());
        }
      }
      Err(error) => {
        return (StatusCode::BAD_REQUEST, Json(json!({ "ok": false, "error": error.to_string() }))).into_response();
      }
    }
  }
  Json(json!({ "ok": true, "paths": paths })).into_response()
}

#[derive(Deserialize)]
struct RevealBody {
  path: String,
}

async fn reveal_path(State((app, gate)): State<AppState>, headers: HeaderMap, Json(body): Json<RevealBody>) -> Response {
  if let Err(status) = authorized(&app, &gate, &headers, None) {
    return status.into_response();
  }
  use tauri_plugin_opener::OpenerExt;
  match app.opener().reveal_item_in_dir(Path::new(&body.path)) {
    Ok(()) => Json(json!({ "ok": true })).into_response(),
    Err(error) => (StatusCode::BAD_REQUEST, Json(json!({ "ok": false, "error": error.to_string() }))).into_response(),
  }
}

#[derive(Deserialize)]
struct WsQuery {
  token: Option<String>,
}

async fn events(
  State((app, gate)): State<AppState>,
  headers: HeaderMap,
  Query(query): Query<WsQuery>,
  upgrade: WebSocketUpgrade,
) -> Response {
  if let Err(status) = authorized(&app, &gate, &headers, query.token.as_deref()) {
    return status.into_response();
  }
  upgrade.on_upgrade(move |socket| socket_loop(app, socket))
}

async fn socket_loop(app: AppHandle, mut socket: WebSocket) {
  let mut incoming: broadcast::Receiver<String> = app.subscribe();
  let pending = app.take_pending();
  for message in pending {
    if socket.send(Message::Text(message.into())).await.is_err() {
      app.subscriber_left();
      return;
    }
  }
  loop {
    tokio::select! {
      message = incoming.recv() => {
        match message {
          Ok(text) => {
            if socket.send(Message::Text(text.into())).await.is_err() {
              break;
            }
          }
          Err(broadcast::error::RecvError::Lagged(_)) => continue,
          Err(broadcast::error::RecvError::Closed) => break,
        }
      }
      incoming_message = socket.recv() => {
        match incoming_message {
          Some(Ok(Message::Close(_)) | Err(_)) | None => break,
          _ => {}
        }
      }
    }
  }
  app.subscriber_left();
  app.cancel_picks();
}

async fn static_file(method: Method, uri: axum::http::Uri) -> Response {
  if method != Method::GET && method != Method::HEAD {
    return StatusCode::METHOD_NOT_ALLOWED.into_response();
  }
  let path = uri.path().trim_start_matches('/');
  let path = if path.is_empty() { "index.html" } else { path };
  if let Some(file) = Assets::get(path) {
    return asset_response(path, file.data.as_ref());
  }
  if Path::new(path).extension().is_none() {
    if let Some(file) = Assets::get("index.html") {
      return asset_response("index.html", file.data.as_ref());
    }
  }
  StatusCode::NOT_FOUND.into_response()
}

fn asset_response(path: &str, bytes: &[u8]) -> Response {
  let mime = mime_guess::from_path(path).first_or_octet_stream();
  Response::builder()
    .header(header::CONTENT_TYPE, mime.as_ref())
    .header(header::CACHE_CONTROL, if path == "index.html" { "no-store" } else { "public, max-age=3600" })
    .body(Body::from(bytes.to_vec()))
    .unwrap_or_else(|_| StatusCode::INTERNAL_SERVER_ERROR.into_response())
}

fn bind_loopback(preferred: u16) -> std::io::Result<TcpListener> {
  let mut last = None;
  for offset in 0..50 {
    let port = preferred.saturating_add(offset);
    match std::net::TcpListener::bind(SocketAddr::from(([127, 0, 0, 1], port))) {
      Ok(listener) => {
        listener.set_nonblocking(true)?;
        return TcpListener::from_std(listener);
      }
      Err(error) => last = Some(error),
    }
  }
  Err(last.unwrap_or_else(|| std::io::Error::other("no free port")))
}

fn session_path() -> PathBuf {
  paths::data_dir().join("web-session.json")
}

fn write_session(instance: &str, port: u16, token: &str, url: &str) {
  let _ = std::fs::create_dir_all(paths::data_dir());
  let body = json!({
    "instance": instance,
    "port": port,
    "token": token,
    "url": url,
    "pid": std::process::id(),
  });
  let _ = std::fs::write(session_path(), body.to_string());
}

fn write_dev_file(port: u16, token: &str, url: &str) {
  let path = Path::new(env!("CARGO_MANIFEST_DIR")).join(".fabricator-dev.json");
  let body = json!({ "port": port, "token": token, "url": url });
  let _ = std::fs::write(path, body.to_string());
}

async fn existing_session(current: &str) -> Option<String> {
  let raw = std::fs::read_to_string(session_path()).ok()?;
  let value: Value = serde_json::from_str(&raw).ok()?;
  let port = value.get("port")?.as_u64()? as u16;
  let instance = value.get("instance")?.as_str()?;
  if instance == current {
    return None;
  }
  let url = value.get("url")?.as_str()?.to_string();
  let client = reqwest::Client::builder().timeout(Duration::from_millis(400)).build().ok()?;
  let health = client.get(format!("http://127.0.0.1:{port}/api/health")).send().await.ok()?;
  let body: Value = health.json().await.ok()?;
  (body.get("instance").and_then(Value::as_str) == Some(instance)).then_some(url)
}

fn open_browser(url: &str) -> std::io::Result<()> {
  let mut command = if cfg!(windows) {
    let mut command = std::process::Command::new("cmd");
    command.args(["/C", "start", "", url]);
    command
  } else if cfg!(target_os = "macos") {
    let mut command = std::process::Command::new("open");
    command.arg(url);
    command
  } else {
    let mut command = std::process::Command::new("xdg-open");
    command.arg(url);
    command
  };
  command.stdin(std::process::Stdio::null()).stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null()).spawn().map(|_| ())
}

async fn shutdown(app: AppHandle) {
  let _ = tokio::signal::ctrl_c().await;
  crate::services::dev_server::kill_all(&app);
  if let Some(terminals) = app.try_state::<crate::services::terminal::Terminals>() {
    terminals.kill_all(None);
  }
  crate::services::local_backend::kill_all(&app);
  crate::services::apps::kill_all(&app);
  let _ = std::fs::remove_file(session_path());
}

#[cfg(test)]
mod tests {
  use super::*;
  use axum::body::to_bytes;
  use tower::ServiceExt;

  fn test_app() -> (AppHandle, Router) {
    let host = tauri::Host::new("1.54.0");
    let app = host.handle();
    app.set_token("test-token");
    app.set_origin("http://127.0.0.1:5173");
    let router = router(app.clone(), 5173, "instance".into());
    // The handlers compare against the handle's token. The gate's port is what
    // Origin is checked against; copy the token onto the handle only.
    (app, router)
  }

  async fn body_json(response: Response) -> Value {
    let bytes = to_bytes(response.into_body(), 1024 * 1024).await.unwrap();
    serde_json::from_slice(&bytes).unwrap()
  }

  #[tokio::test]
  async fn missing_token_is_rejected() {
    let (_app, router) = test_app();
    let response = router
      .oneshot(axum::http::Request::post("/api/cmd/ping").body(Body::from("{}")).unwrap())
      .await
      .unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
  }

  #[tokio::test]
  async fn a_foreign_origin_is_rejected_even_with_the_token() {
    let (_app, router) = test_app();
    let response = router
      .oneshot(
        axum::http::Request::post("/api/cmd/ping")
          .header(header::AUTHORIZATION, "Bearer test-token")
          .header(header::ORIGIN, "https://evil.example")
          .body(Body::from("{}"))
          .unwrap(),
      )
      .await
      .unwrap();
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
  }

  #[tokio::test]
  async fn ping_returns_pong() {
    let (_app, router) = test_app();
    let response = router
      .oneshot(
        axum::http::Request::post("/api/cmd/ping")
          .header(header::AUTHORIZATION, "Bearer test-token")
          .header(header::ORIGIN, "http://127.0.0.1:5173")
          .body(Body::from("{}"))
          .unwrap(),
      )
      .await
      .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let json = body_json(response).await;
    assert_eq!(json["data"], "pong");
  }

  #[tokio::test]
  async fn a_bad_project_name_is_refused() {
    let (_app, router) = test_app();
    let response = router
      .oneshot(
        axum::http::Request::post("/api/cmd/projects_check_name")
          .header(header::AUTHORIZATION, "Bearer test-token")
          .body(Body::from(r#"{"name":"","teamWorkspaceId":null,"location":null}"#))
          .unwrap(),
      )
      .await
      .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let json = body_json(response).await;
    assert_eq!(json["ok"], true);
    assert_eq!(json["data"]["ok"], false);
  }

  #[test]
  fn folder_listing_rejects_a_missing_path_and_names_roots() {
    let error = list_folder(Some("/no/such/fabricator-folder")).unwrap_err();
    assert!(error.contains("isn't there"), "{error}");
    let roots = list_folder(None).unwrap();
    assert!(roots["entries"].as_array().is_some_and(|entries| !entries.is_empty()));
  }
}
