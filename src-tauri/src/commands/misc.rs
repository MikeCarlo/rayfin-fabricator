//! Misc app commands: ping, version info, open-external, open-logs, window reveal,
//! relaunch.

use tauri::AppHandle;
use tauri_plugin_opener::OpenerExt;

use crate::error::{AppError, AppResult};
use crate::services::{local_backend, paths, store};
use crate::types::{AppVersions, OpenInEditorResult};

#[tauri::command]
pub fn ping() -> &'static str {
  "pong"
}

/// Version info for the About/Settings panel: the app, the local server, the
/// browser, the bundled Copilot CLI and rayfin-local.
#[tauri::command]
pub async fn get_versions(app: AppHandle) -> AppVersions {
  let app_version = app.package_info().version.to_string();
  let (copilot, rayfin_local) =
    tokio::join!(crate::services::copilot::bundled_cli_version(), rayfin_local_version(&app));
  let copilot_bundled = crate::services::copilot::bundled_cli_pinned_version();
  AppVersions {
    app: app_version,
    tauri: tauri::VERSION.to_string(),
    webview2: webview_version(),
    copilot,
    copilot_bundled,
    rayfin_local,
  }
}

fn webview_version() -> String {
  tauri::webview_version().unwrap_or_default()
}

/// The rayfin-local that runs apps on this computer: the version a personal
/// project's backend reported this session, else what the one Fabricator runs
/// says it is.
async fn rayfin_local_version(app: &AppHandle) -> Option<String> {
  let reported = store::get_state()
    .projects
    .into_iter()
    .filter(|p| p.team.is_none())
    .find_map(|p| local_backend::snapshot(app, &p.id).version);
  match reported {
    Some(version) => Some(version),
    None => local_backend::doctor_status().await.version,
  }
}

/// Open a URL in the user's default browser (http/https only).
#[tauri::command]
pub async fn open_external(app: AppHandle, url: String) -> AppResult<()> {
  if url.starts_with("http://") || url.starts_with("https://") {
    app
      .opener()
      .open_url(url, None::<&str>)
      .map_err(|e| AppError::Msg(e.to_string()))?;
  }
  Ok(())
}

/// Open the logs folder in the OS file manager; returns its path.
#[tauri::command]
pub async fn open_logs(app: AppHandle) -> AppResult<String> {
  let dir = paths::logs_dir();
  let path = dir.to_string_lossy().to_string();
  app
    .opener()
    .open_path(path.clone(), None::<&str>)
    .map_err(|e| AppError::Msg(e.to_string()))?;
  Ok(path)
}

/// Open the project folder in VS Code (`code <dir>`). Falls back to revealing the
/// folder in the OS file manager when VS Code's CLI isn't installed.
#[tauri::command]
pub async fn open_in_editor(app: AppHandle, id: String) -> AppResult<OpenInEditorResult> {
  let project = crate::services::store::find_project(&id)
    .ok_or_else(|| AppError::Msg("Project not found.".into()))?;
  let path = project.path.clone();

  if launch_vscode(&path) {
    return Ok(OpenInEditorResult { opened: true, revealed_folder: None });
  }
  app
    .opener()
    .open_path(path.clone(), None::<&str>)
    .map_err(|e| AppError::Msg(e.to_string()))?;
  Ok(OpenInEditorResult { opened: false, revealed_folder: Some(true) })
}

/// Try to launch VS Code on `dir`, detached. Returns false when `code` isn't on PATH
/// or the spawn failed. On Windows `code` is a `.cmd` shim that must run via cmd.exe.
fn launch_vscode(dir: &str) -> bool {
  use std::process::{Command, Stdio};
  if which::which("code").is_err() {
    return false;
  }
  #[cfg(windows)]
  let mut cmd = {
    let mut c = Command::new("cmd");
    c.args(["/C", "code"]).arg(dir);
    c
  };
  #[cfg(not(windows))]
  let mut cmd = {
    let code = which::which("code").expect("checked above");
    let mut c = Command::new(code);
    c.arg(dir);
    c
  };
  // The Rayfin commands run in VS Code aren't Fabricator's.
  crate::services::rayfin_telemetry::for_user_program(&mut cmd);
  cmd
    .stdin(Stdio::null())
    .stdout(Stdio::null())
    .stderr(Stdio::null())
    .spawn()
    .is_ok()
}

/// The browser is already showing the page. Remember the window so later
/// commands can name it, and keep the background colour check for the renderer.
#[tauri::command]
pub fn window_reveal(window: tauri::Window, background: Option<String>) {
  let _ = background.as_deref().and_then(parse_hex_color);
  let _ = window.set_focus();
}

/// The page is a browser tab: ask it to come forward. There is no native window
/// to create.
pub fn bring_forward(app: &AppHandle) {
  let _ = app.window("main").set_focus();
}

/// The server has already opened the browser. Nothing else has to be created.
pub fn open_main_window(app: &AppHandle) -> tauri::Result<()> {
  let _ = app.window("main");
  Ok(())
}

fn parse_hex_color(value: &str) -> Option<tauri::window::Color> {
  let hex = value.strip_prefix('#')?;
  if hex.len() != 6 || !hex.is_ascii() {
    return None;
  }
  let channel = |at: usize| u8::from_str_radix(&hex[at..at + 2], 16).ok();
  Some(tauri::window::Color(channel(0)?, channel(2)?, channel(4)?, 255))
}

/// Restart the app (used to pick up newly installed Node/Git on PATH).
#[tauri::command]
pub fn relaunch(app: AppHandle) {
  app.restart();
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn reads_the_renderers_background_colour() {
    let color = parse_hex_color("#0b0e14").expect("a #rrggbb colour");
    assert_eq!((color.0, color.1, color.2, color.3), (0x0b, 0x0e, 0x14, 255));
    assert!(parse_hex_color("#ECEFF4").is_some());
    for bad in ["0b0e14", "#0b0e1", "#0b0e14ff", "#zzzzzz", "", "#é0e14"] {
      assert!(parse_hex_color(bad).is_none(), "{bad:?} is not a colour");
    }
  }
}
