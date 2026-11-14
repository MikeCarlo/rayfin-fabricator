//! Folder and file prompts. The command waits; the page shows the picker and
//! posts the choice back, so the command signatures stay the same.

use std::path::{Path, PathBuf};

use tauri::{AppHandle, PickFilter, PickRequest};

pub struct FilePath(PathBuf);

impl FilePath {
  pub fn into_path(self) -> Result<PathBuf, &'static str> {
    Ok(self.0)
  }
}

#[derive(Clone)]
pub struct FileDialogBuilder {
  app: AppHandle,
  title: String,
  file_name: Option<String>,
  filters: Vec<PickFilter>,
}

impl FileDialogBuilder {
  pub fn set_title(mut self, title: impl AsRef<str>) -> Self {
    self.title = title.as_ref().to_string();
    self
  }

  pub fn set_file_name(mut self, name: impl AsRef<str>) -> Self {
    self.file_name = Some(name.as_ref().to_string());
    self
  }

  pub fn add_filter(mut self, name: impl AsRef<str>, extensions: &[&str]) -> Self {
    self.filters.push(PickFilter {
      name: name.as_ref().to_string(),
      extensions: extensions.iter().map(|ext| (*ext).to_string()).collect(),
    });
    self
  }

  pub fn pick_file<F>(self, callback: F)
  where
    F: FnOnce(Option<FilePath>) + Send + 'static,
  {
    self.ask("file", move |paths| callback(paths.and_then(first)));
  }

  pub fn pick_files<F>(self, callback: F)
  where
    F: FnOnce(Option<Vec<FilePath>>) + Send + 'static,
  {
    self.ask("files", move |paths| callback(paths.map(many)));
  }

  pub fn pick_folder<F>(self, callback: F)
  where
    F: FnOnce(Option<FilePath>) + Send + 'static,
  {
    self.ask("folder", move |paths| callback(paths.and_then(first)));
  }

  pub fn pick_folders<F>(self, callback: F)
  where
    F: FnOnce(Option<Vec<FilePath>>) + Send + 'static,
  {
    self.ask("folders", move |paths| callback(paths.map(many)));
  }

  /// Files and folders together, for adding them to a chat message.
  pub fn pick_mixed<F>(self, callback: F)
  where
    F: FnOnce(Option<Vec<FilePath>>) + Send + 'static,
  {
    self.ask("paths", move |paths| callback(paths.map(many)));
  }

  pub fn save_file<F>(self, callback: F)
  where
    F: FnOnce(Option<FilePath>) + Send + 'static,
  {
    self.ask("save", move |paths| callback(paths.and_then(first)));
  }

  fn ask<F>(self, mode: &str, callback: F)
  where
    F: FnOnce(Option<Vec<PathBuf>>) + Send + 'static,
  {
    let request = PickRequest {
      id: String::new(),
      mode: mode.to_string(),
      title: self.title,
      filters: self.filters,
      file_name: self.file_name,
    };
    let rx = self.app.request_pick(request);
    tokio::spawn(async move {
      let paths = rx.await.unwrap_or(None);
      callback(paths);
    });
  }
}

fn first(mut paths: Vec<PathBuf>) -> Option<FilePath> {
  paths.pop().map(FilePath)
}

fn many(paths: Vec<PathBuf>) -> Vec<FilePath> {
  paths.into_iter().map(FilePath).collect()
}

pub struct Dialog {
  app: AppHandle,
}

impl Dialog {
  pub fn file(self) -> FileDialogBuilder {
    FileDialogBuilder { app: self.app, title: String::new(), file_name: None, filters: Vec::new() }
  }
}

pub trait DialogExt {
  fn dialog(&self) -> Dialog;
}

impl DialogExt for AppHandle {
  fn dialog(&self) -> Dialog {
    Dialog { app: self.clone() }
  }
}

/// Kept so callers can name a path the picker returned.
pub fn is_dir(path: &Path) -> bool {
  path.is_dir()
}
