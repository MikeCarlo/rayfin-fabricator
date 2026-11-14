//! One prompt that picks files and folders together, for adding them to a chat
//! message. The page shows the folder browser; this waits for the choice.

use std::path::PathBuf;

use tauri::AppHandle;
use tauri_plugin_dialog::DialogExt;

/// What the user picked, or nothing when they closed the dialog.
pub async fn pick_files_and_folders(app: &AppHandle, title: &str) -> Vec<PathBuf> {
  let (tx, rx) = tokio::sync::oneshot::channel();
  app.dialog().file().set_title(title).pick_mixed(move |picked| {
    let _ = tx.send(picked);
  });
  rx.await.ok().flatten().unwrap_or_default().into_iter().filter_map(|path| path.into_path().ok()).collect()
}
