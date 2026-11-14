//! Update check against the GitHub release that ships `fabricator` for this
//! computer. Download keeps the new file beside this one; install starts it
//! after this process exits and replaces the executable.

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use serde::Deserialize;
use tauri::{AppHandle, Emitter};

use crate::error::{AppError, AppResult};
use crate::services::emit::UPDATE_PROGRESS;
use crate::types::{UpdateInfo, UpdateProgress};

/// A release newer than this build, and the asset that replaces the executable.
struct Pending {
  version: String,
  current_version: String,
  notes: Option<String>,
  date: Option<String>,
  asset_url: String,
  asset_name: String,
}

pub struct UpdaterState {
  pending: Mutex<Option<Pending>>,
  file: Mutex<Option<PathBuf>>,
}

impl Default for UpdaterState {
  fn default() -> Self {
    Self { pending: Mutex::new(None), file: Mutex::new(None) }
  }
}

#[derive(Deserialize)]
struct Release {
  tag_name: String,
  body: Option<String>,
  published_at: Option<String>,
  assets: Vec<Asset>,
}

#[derive(Deserialize)]
struct Asset {
  name: String,
  browser_download_url: String,
}

const REPO: &str = "spatney/rayfin-fabricator";

fn asset_name() -> &'static str {
  if cfg!(windows) {
    "fabricator.exe"
  } else if cfg!(target_os = "macos") {
    "fabricator-macos"
  } else {
    "fabricator-linux"
  }
}

fn info_from(update: &Pending) -> UpdateInfo {
  UpdateInfo {
    version: update.version.clone(),
    current_version: update.current_version.clone(),
    notes: update.notes.clone(),
    date: update.date.clone(),
  }
}

fn newer(latest: &str, current: &str) -> bool {
  let parse = |text: &str| semver::Version::parse(text.trim().trim_start_matches('v')).ok();
  match (parse(latest), parse(current)) {
    (Some(latest), Some(current)) => latest > current,
    _ => false,
  }
}

async fn fetch_release() -> AppResult<Release> {
  let client = reqwest::Client::builder().timeout(std::time::Duration::from_secs(30)).build().map_err(|e| AppError::Msg(e.to_string()))?;
  let release = client
    .get(format!("https://api.github.com/repos/{REPO}/releases/latest"))
    .header("User-Agent", "rayfin-fabricator")
    .header("Accept", "application/vnd.github+json")
    .send()
    .await
    .map_err(|e| AppError::Msg(format!("Couldn't check for updates: {e}")))?;
  if !release.status().is_success() {
    return Err(AppError::Msg(format!("Couldn't check for updates ({}).", release.status())));
  }
  release.json().await.map_err(|e| AppError::Msg(format!("Couldn't read the update: {e}")))
}

async fn run_check(current: &str) -> AppResult<Option<Pending>> {
  let release = fetch_release().await?;
  let version = release.tag_name.trim().trim_start_matches('v').to_string();
  if !newer(&version, current) {
    return Ok(None);
  }
  let name = asset_name();
  let Some(asset) = release.assets.into_iter().find(|asset| asset.name == name) else {
    return Err(AppError::Msg(format!("The latest release has no {name}.")));
  };
  Ok(Some(Pending {
    version,
    current_version: current.to_string(),
    notes: release.body,
    date: release.published_at,
    asset_url: asset.browser_download_url,
    asset_name: asset.name,
  }))
}

pub async fn check(app: &AppHandle, state: &UpdaterState) -> AppResult<Option<UpdateInfo>> {
  let found = run_check(&app.package_info().version.to_string()).await?;
  let info = found.as_ref().map(info_from);
  *state.file.lock().unwrap() = None;
  *state.pending.lock().unwrap() = found;
  Ok(info)
}

pub async fn download(app: &AppHandle, state: &UpdaterState) -> AppResult<Option<UpdateInfo>> {
  if state.pending.lock().unwrap().is_none() {
    *state.pending.lock().unwrap() = run_check(&app.package_info().version.to_string()).await?;
  }
  let Some(update) = state.pending.lock().unwrap().as_ref().map(|update| Pending {
    version: update.version.clone(),
    current_version: update.current_version.clone(),
    notes: update.notes.clone(),
    date: update.date.clone(),
    asset_url: update.asset_url.clone(),
    asset_name: update.asset_name.clone(),
  }) else {
    return Ok(None);
  };
  let info = info_from(&update);
  let client = reqwest::Client::builder().timeout(std::time::Duration::from_secs(600)).build().map_err(|e| AppError::Msg(e.to_string()))?;
  let response = client
    .get(&update.asset_url)
    .header("User-Agent", "rayfin-fabricator")
    .send()
    .await
    .map_err(|e| AppError::Msg(format!("Couldn't download the update: {e}")))?;
  if !response.status().is_success() {
    return Err(AppError::Msg(format!("Couldn't download the update ({}).", response.status())));
  }
  let total = response.content_length();
  let mut downloaded = 0u64;
  let bytes = {
    use futures::StreamExt;
    let mut stream = response.bytes_stream();
    let mut body = Vec::new();
    while let Some(chunk) = stream.next().await {
      let chunk = chunk.map_err(|e| AppError::Msg(format!("Couldn't download the update: {e}")))?;
      downloaded += chunk.len() as u64;
      let _ = app.emit(UPDATE_PROGRESS, UpdateProgress { downloaded, total });
      body.extend_from_slice(&chunk);
    }
    body
  };
  let dir = std::env::temp_dir().join("rayfin-fabricator-update");
  std::fs::create_dir_all(&dir)?;
  let path = dir.join(&update.asset_name);
  std::fs::write(&path, bytes)?;
  *state.file.lock().unwrap() = Some(path);
  Ok(Some(info))
}

pub async fn install(app: &AppHandle, state: &UpdaterState) -> AppResult<()> {
  let Some(file) = state.file.lock().unwrap().clone() else {
    return Err(AppError::Msg("No downloaded update to install. Check for updates first.".into()));
  };
  let exe = std::env::current_exe().map_err(|e| AppError::Msg(e.to_string()))?;
  spawn_replacer(&exe, &file)?;
  app.restart();
}

/// After this process exits, replace `exe` with `file` and start it.
fn spawn_replacer(exe: &Path, file: &Path) -> AppResult<()> {
  let script = if cfg!(windows) {
    let path = std::env::temp_dir().join("rayfin-fabricator-update.cmd");
    let body = format!(
      "@echo off\r\ntimeout /t 2 /nobreak >nul\r\ncopy /Y \"{}\" \"{}\" >nul\r\nstart \"\" \"{}\"\r\n",
      file.display(),
      exe.display(),
      exe.display()
    );
    std::fs::write(&path, body)?;
    path
  } else {
    let path = std::env::temp_dir().join("rayfin-fabricator-update.sh");
    let body = format!(
      "#!/bin/sh\nsleep 2\nmv -f \"{}\" \"{}\"\nchmod +x \"{}\"\n\"{}\" &\n",
      file.display(),
      exe.display(),
      exe.display(),
      exe.display()
    );
    std::fs::write(&path, body)?;
    #[cfg(unix)]
    {
      use std::os::unix::fs::PermissionsExt;
      let _ = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755));
    }
    path
  };
  let mut command = if cfg!(windows) {
    let mut command = std::process::Command::new("cmd");
    command.args(["/C", &script.display().to_string()]);
    command
  } else {
    std::process::Command::new(script)
  };
  command
    .stdin(std::process::Stdio::null())
    .stdout(std::process::Stdio::null())
    .stderr(std::process::Stdio::null())
    .spawn()
    .map_err(|e| AppError::Msg(format!("Couldn't start the update: {e}")))?;
  Ok(())
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn a_newer_release_is_the_only_update() {
    assert!(newer("1.55.0", "1.54.0"));
    assert!(newer("v1.55.0", "1.54.0"));
    assert!(!newer("1.54.0", "1.54.0"));
    assert!(!newer("1.53.0", "1.54.0"));
    assert!(!newer("not-a-version", "1.54.0"));
  }

  #[test]
  fn each_platform_looks_for_its_own_file() {
    let name = asset_name();
    assert!(name.starts_with("fabricator"));
    if cfg!(windows) {
      assert_eq!(name, "fabricator.exe");
    }
  }
}
