//! Stand-in for the Tauri types Fabricator's commands use.
//!
//! Commands stay ordinary Rust functions. The local web server injects [`AppHandle`],
//! [`State`], [`Window`], and [`Webview`], and delivers [`Emitter`] events to the
//! browser over a WebSocket. There is no webview and no installer.

mod geom;
mod host;
#[path = "ipc.rs"]
mod ipc_impl;

pub use geom::{LogicalPosition, LogicalSize, PhysicalPosition, PhysicalSize, Position, Rect, Size};
pub use host::{
  install_runtime, AppHandle, Emitter, Error, Event, EventId, Host, Listener, Manager, PickFilter, PickRequest, State,
  Webview, WebviewWindow, Window, WindowEvent,
};
pub use ipc_impl::InvokeCtx;

pub mod ipc {
  pub use crate::ipc_impl::{Channel, InvokeError, InvokeResponseBody, Response};
}

pub use tauri_macros::command;
pub use inventory;
pub use url::Url;

use std::future::Future;
use std::pin::Pin;

use serde::de::DeserializeOwned;
use serde_json::Value;

pub type CommandEntryFn =
  fn(InvokeCtx, Value) -> Pin<Box<dyn Future<Output = std::result::Result<Value, String>> + Send>>;

/// One `#[tauri::command]`, collected when the binary starts.
pub struct CommandEntry {
  pub name: &'static str,
  pub call: CommandEntryFn,
}

inventory::collect!(CommandEntry);

pub fn commands() -> Vec<&'static CommandEntry> {
  inventory::iter::<CommandEntry>().collect()
}

/// JSON argument `name` (already camelCase), or `null` when the client left it out.
pub fn arg<T: DeserializeOwned>(name: &str, value: Option<&Value>) -> std::result::Result<T, String> {
  let value = value.unwrap_or(&Value::Null);
  serde_json::from_value(value.clone()).map_err(|error| format!("{name}: {error}"))
}

pub type Result<T> = std::result::Result<T, Error>;

/// Shown in About where the desktop build reported the Tauri version.
pub const VERSION: &str = "web";

pub fn webview_version() -> Result<String> {
  Ok("browser".into())
}

/// Debug builds are the ones compiled without `--release`.
pub fn is_dev() -> bool {
  cfg!(debug_assertions)
}

pub mod async_runtime {
  use std::future::Future;

  use tokio::runtime::Handle;
  use tokio::task::JoinHandle;

  /// Spawn onto the running server runtime, including from a worker thread
  /// that has no runtime of its own (telemetry, for example).
  pub fn spawn<F>(future: F) -> JoinHandle<F::Output>
  where
    F: Future + Send + 'static,
    F::Output: Send + 'static,
  {
    if let Ok(handle) = Handle::try_current() {
      return handle.spawn(future);
    }
    crate::host::runtime_handle().spawn(future)
  }
}

pub mod window {
  #[derive(Clone, Copy, Debug, PartialEq, Eq)]
  pub struct Color(pub u8, pub u8, pub u8, pub u8);
}

pub mod image {
  use std::marker::PhantomData;

  /// RGBA icon decoded from a PNG. The lifetime matches Tauri's owned image.
  pub struct Image<'a> {
    rgba: Vec<u8>,
    width: u32,
    height: u32,
    _lifetime: PhantomData<&'a ()>,
  }

  impl Image<'static> {
    pub fn new_owned(rgba: Vec<u8>, width: u32, height: u32) -> Self {
      Self { rgba, width, height, _lifetime: PhantomData }
    }
  }

  impl Image<'_> {
    pub fn width(&self) -> u32 {
      self.width
    }

    pub fn height(&self) -> u32 {
      self.height
    }

    pub fn rgba(&self) -> &[u8] {
      &self.rgba
    }
  }
}
