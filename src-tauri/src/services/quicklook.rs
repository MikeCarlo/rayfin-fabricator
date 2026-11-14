//! A published app's page, shown in an iframe over the gallery.
//!
//! The desktop build used a second native webview. Here the command only checks
//! that the address is https; the page puts an iframe over the panel.

use std::sync::Mutex;

use once_cell::sync::Lazy;
use tauri::{AppHandle, Emitter, Url};

use crate::error::{AppError, AppResult};
use crate::services::preview::PreviewBounds;

const QUICKLOOK: &str = "quicklook:show";

static SHOWN: Lazy<Mutex<Option<String>>> = Lazy::new(Default::default);

fn https(url: &str) -> AppResult<Url> {
  let parsed: Url = url.parse().map_err(|_| AppError::Msg("That isn't an address.".into()))?;
  if parsed.scheme() != "https" || parsed.host_str().is_none_or(str::is_empty) {
    return Err(AppError::Msg("Quick look opens https addresses only.".into()));
  }
  Ok(parsed)
}

#[tauri::command]
pub async fn quicklook_show(app: AppHandle, url: String, bounds: PreviewBounds) -> AppResult<()> {
  let parsed = https(&url)?;
  bounds.to_rect()?;
  *SHOWN.lock().unwrap() = Some(parsed.to_string());
  let _ = app.emit(QUICKLOOK, parsed.to_string());
  Ok(())
}

#[tauri::command]
pub fn quicklook_set_bounds(_app: AppHandle, bounds: PreviewBounds) -> AppResult<()> {
  bounds.to_rect()?;
  Ok(())
}

#[tauri::command]
pub fn quicklook_close(_app: AppHandle) -> AppResult<()> {
  *SHOWN.lock().unwrap() = None;
  Ok(())
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn only_https_pages_get_a_quick_look() {
    assert!(https("https://flat-oak-1a2b-westus.webapp.fabricapps.net").is_ok());
    assert!(https("https://app.fabric.microsoft.com/groups/x/appbackends/y").is_ok());
    for url in ["http://localhost:5173", "javascript:alert(1)", "file:///C:/x.html", "not a url", "https://"] {
      assert!(https(url).is_err(), "{url}");
    }
  }
}
