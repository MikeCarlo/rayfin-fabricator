//! Opens an http(s) URL in the default browser, or a folder in the file manager.

use std::path::Path;
use std::process::Command;

use tauri::AppHandle;

#[derive(Debug)]
pub struct Error(pub String);

impl std::fmt::Display for Error {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    f.write_str(&self.0)
  }
}

pub struct Opener;

impl Opener {
  pub fn open_url(&self, url: impl AsRef<str>, _with: Option<&str>) -> Result<(), Error> {
    let url = url.as_ref();
    if !(url.starts_with("http://") || url.starts_with("https://")) {
      return Err(Error("only http(s) links are opened".into()));
    }
    open_target(url)
  }

  pub fn open_path(&self, path: impl AsRef<str>, _with: Option<&str>) -> Result<(), Error> {
    reveal(Path::new(path.as_ref()))
  }

  pub fn reveal_item_in_dir(&self, path: &Path) -> Result<(), Error> {
    reveal(path)
  }
}

fn reveal(path: &Path) -> Result<(), Error> {
  let target = if path.is_file() { path.parent().unwrap_or(path) } else { path };
  open_target(&target.display().to_string())
}

fn open_target(target: &str) -> Result<(), Error> {
  let mut command = if cfg!(windows) {
    let mut command = Command::new("cmd");
    command.args(["/C", "start", "", target]);
    command
  } else if cfg!(target_os = "macos") {
    let mut command = Command::new("open");
    command.arg(target);
    command
  } else {
    let mut command = Command::new("xdg-open");
    command.arg(target);
    command
  };
  command
    .stdin(std::process::Stdio::null())
    .stdout(std::process::Stdio::null())
    .stderr(std::process::Stdio::null())
    .spawn()
    .map(|_| ())
    .map_err(|error| Error(error.to_string()))
}

pub trait OpenerExt {
  fn opener(&self) -> Opener;
}

impl OpenerExt for AppHandle {
  fn opener(&self) -> Opener {
    Opener
  }
}
