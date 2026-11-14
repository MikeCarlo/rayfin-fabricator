//! Fabricator — local web server.
//!
//! Declares the Rust module tree and starts the server that serves the page and
//! the commands behind `window.api` (see `src/shared/ipc.ts`).

mod commands;
mod error;
mod http;
mod services;
mod state;
mod types;

use state::AppState;

use services::apps::Opened;
use services::preview::PreviewState;
use services::updater::UpdaterState;
use services::dev_server::DevServers;

/// Open what Fabricator was asked to open: projects in its own window, which
/// imports them, and apps in theirs. With nothing to open, a later launch
/// (`asked`) brings Fabricator's own window forward.
fn open_requested(app: &tauri::AppHandle, opened: Vec<Opened>, _asked: bool) {
  let mut projects = Vec::new();
  let mut apps = false;
  for item in opened {
    match item {
      Opened::Project(path) => projects.push(path),
      other => {
        apps = true;
        services::apps::open(app, other);
      }
    }
  }
  commands::export::queue_opened(app, projects);
  let _ = apps;
}

/// Read the bundled telemetry connection string (App Insights) if present.
/// Mirrors the Electron build, which injects `resources/telemetry.json` at
/// package time. Absent in dev → telemetry is a no-op.
fn telemetry_connection_string() -> Option<String> {
  let mut candidates = vec![std::path::PathBuf::from("resources").join("telemetry.json")];
  if let Some(dir) = std::env::current_exe().ok().and_then(|path| path.parent().map(std::path::Path::to_path_buf)) {
    candidates.insert(0, dir.join("resources").join("telemetry.json"));
    candidates.insert(0, dir.join("telemetry.json"));
  }
  for path in candidates {
    if let Ok(raw) = std::fs::read_to_string(&path) {
      if let Ok(json) = serde_json::from_str::<serde_json::Value>(&raw) {
        if let Some(cs) = json.get("connectionString").and_then(|v| v.as_str()) {
          if !cs.trim().is_empty() {
            return Some(cs.to_string());
          }
        }
      }
    }
  }
  None
}

/// Env var the Rayfin CLI reads to permit a plaintext token cache when the OS
/// credential store is unavailable.
const RAYFIN_ENCRYPTION_FALLBACK_ENV: &str = "RAYFIN_ENCRYPTION_FALLBACK_ENABLED";

/// Decide what to set [`RAYFIN_ENCRYPTION_FALLBACK_ENV`] to, given whatever the
/// process already inherited: default it to `"true"` when unset, but never
/// clobber a value the user set deliberately (either direction). Kept pure so
/// the policy is unit-tested without mutating global process state.
fn rayfin_encryption_fallback_value(existing: Option<&str>) -> Option<&'static str> {
  match existing {
    Some(_) => None,
    None => Some("true"),
  }
}

/// Let the Rayfin CLI fall back to a plaintext token cache when the OS keychain
/// (DPAPI on Windows, Keychain on macOS, libsecret on Linux) is unavailable.
///
/// By default the CLI *aborts* when it can't reach the credential store —
/// `createCachePlugin` throws "OS keychain is not available and plaintext token
/// storage is not enabled". On locked-down machines where the native binding
/// can't load, that made `rayfin login` — and every silent token read used to
/// list Fabric workspaces or deploy — fail, so the Sign-in button looked like it
/// did nothing (issue #17). The CLI still prefers encrypted storage whenever the
/// keychain works (it only reaches the plaintext branch after the native path
/// throws), so opting in here is a no-op in the common case and only degrades to
/// plaintext when there is genuinely no credential store to use. Setting it in
/// this process propagates to every spawned CLI/auth-helper child.
fn enable_rayfin_encryption_fallback() {
  let existing = std::env::var(RAYFIN_ENCRYPTION_FALLBACK_ENV).ok();
  if let Some(value) = rayfin_encryption_fallback_value(existing.as_deref()) {
    std::env::set_var(RAYFIN_ENCRYPTION_FALLBACK_ENV, value);
  }
}

pub fn run() {
  services::crashlog::install_panic_hook();

  // GUI launchers can inherit a stale Windows PATH or a minimal macOS PATH
  // that omits installed CLIs. Repair it before spawning any child processes.
  services::env_path::repair();

  // Allow the Rayfin CLI to use a plaintext token cache when the OS keychain is
  // unavailable, so Fabric sign-in works on machines without a usable credential
  // store (issue #17). Must run before anything spawns the CLI or its helpers.
  enable_rayfin_encryption_fallback();

  // Label the Rayfin CLI's telemetry of every command Fabricator runs as
  // Fabricator's (`RAYFIN_TELEMETRY_ENV`). Also before anything spawns the CLI.
  services::rayfin_telemetry::init();

  // Point the Rayfin CLI at the Fabric account chosen in Accounts (each extra
  // account has its own config folder). Also before anything spawns the CLI.
  services::fabric_accounts::init();

  // Quiet npm installs (no audit or funding passes) with normal registry
  // metadata freshness checks; scaffolding additionally revalidates metadata for
  // the latest CLI. Set before any child spawns so deploy-time installs and agent
  // installs inherit it.
  services::npm_env::configure_env();

  let runtime = tokio::runtime::Builder::new_multi_thread().enable_all().build().expect("tokio runtime");
  let _guard = runtime.enter();
  tauri::install_runtime(runtime.handle().clone());

  let host = tauri::Host::new(env!("CARGO_PKG_VERSION"));
  let app = host.handle();
  app.manage(AppState::default());
  app.manage(PreviewState::default());
  app.manage(UpdaterState::default());
  app.manage(DevServers::default());
  app.manage(services::local_backend::LocalBackends::default());
  app.manage(services::apps::Apps::default());
  app.manage(services::terminal::Terminals::default());
  app.window("main");

  services::telemetry::init(telemetry_connection_string(), env!("CARGO_PKG_VERSION").to_string());

  let cwd = std::env::current_dir().ok();
  let opened = services::apps::opened_in(std::env::args_os().skip(1), cwd.as_deref());
  open_requested(&app, opened, false);

  services::watchdog::start(app.clone());
  commands::help::watch_local_backends(&app);

  std::thread::spawn(|| {
    services::agent_skills::ensure_materialized();
    services::diagnostics::prune();
    for (dir, what) in [
      (services::paths::retired_design_studio_dir(), "Design Studio data"),
      (services::paths::retired_npm_cache_dir(), "npm cache"),
    ] {
      if dir.is_dir() {
        if let Err(e) = std::fs::remove_dir_all(&dir) {
          log::warn!("failed to remove retired {what} at {}: {e}", dir.display());
        }
      }
    }
    services::new_project_cache::clean_up();
    services::apps::clean_up();
    commands::security::clean_up();
  });

  if let Err(error) = runtime.block_on(http::serve(app)) {
    eprintln!("Fabricator stopped: {error}");
    std::process::exit(1);
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn defaults_rayfin_encryption_fallback_on_when_unset() {
    // Regression for #17: with no inherited value the CLI would abort on
    // keychain-less machines, so we must default the fallback on.
    assert_eq!(rayfin_encryption_fallback_value(None), Some("true"));
  }

  #[test]
  fn respects_an_explicit_rayfin_encryption_fallback_override() {
    // A value the user set deliberately (either direction) is left untouched.
    assert_eq!(rayfin_encryption_fallback_value(Some("true")), None);
    assert_eq!(rayfin_encryption_fallback_value(Some("false")), None);
    assert_eq!(rayfin_encryption_fallback_value(Some("")), None);
  }
}
