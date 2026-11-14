//! Channels the terminal (and anything else that streamed over a Tauri channel)
//! use to push one JSON value at a time.

use serde::Serialize;
use serde_json::Value;

use crate::host::{AppHandle, Webview, Window};

#[derive(Clone)]
pub struct InvokeCtx {
  pub app: AppHandle,
  pub window_label: String,
}

impl InvokeCtx {
  pub fn window(&self) -> Window {
    self.app.window(&self.window_label)
  }

  pub fn webview(&self) -> Webview {
    Webview::new(self.window_label.clone())
  }
}

#[derive(Debug)]
pub enum InvokeResponseBody {
  Json(String),
  Raw(Vec<u8>),
}

#[derive(Debug)]
pub struct InvokeError(pub String);

impl std::fmt::Display for InvokeError {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    f.write_str(&self.0)
  }
}

type Callback = std::sync::Arc<dyn Fn(InvokeResponseBody) -> Result<(), InvokeError> + Send + Sync>;

/// A stream of JSON values back to one browser call (a terminal, for example).
pub struct Channel<T> {
  callback: Callback,
  _value: std::marker::PhantomData<fn() -> T>,
}

impl<T> Clone for Channel<T> {
  fn clone(&self) -> Self {
    Self { callback: self.callback.clone(), _value: std::marker::PhantomData }
  }
}

impl<T: Serialize> Channel<T> {
  pub fn new<F>(callback: F) -> Self
  where
    F: Fn(InvokeResponseBody) -> Result<(), InvokeError> + Send + Sync + 'static,
  {
    Self { callback: std::sync::Arc::new(callback), _value: std::marker::PhantomData }
  }

  pub fn from_arg(ctx: &InvokeCtx, value: Option<&Value>) -> Result<Self, String> {
    let id = value
      .and_then(|value| value.get("__channel"))
      .and_then(Value::as_str)
      .unwrap_or("")
      .to_string();
    if id.is_empty() {
      return Err("missing channel".into());
    }
    Ok(ctx.app.channel(id))
  }

  pub fn send(&self, value: T) -> Result<(), InvokeError> {
    let json = serde_json::to_string(&value).map_err(|error| InvokeError(error.to_string()))?;
    (self.callback)(InvokeResponseBody::Json(json))
  }
}

/// Raw bytes returned to the page (a migration PDF). Serialized as base64 so it
/// fits the JSON command response; the page turns `__bytes` back into an ArrayBuffer.
#[derive(Clone)]
pub struct Response {
  body: Vec<u8>,
}

impl Response {
  pub fn new(body: impl Into<Vec<u8>>) -> Self {
    Self { body: body.into() }
  }
}

impl Serialize for Response {
  fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
    use base64::Engine;
    use serde::ser::SerializeMap;
    let mut map = serializer.serialize_map(Some(1))?;
    map.serialize_entry("__bytes", &base64::engine::general_purpose::STANDARD.encode(&self.body))?;
    map.end()
  }
}
