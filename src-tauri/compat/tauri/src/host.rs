//! Process-wide state, events, and the stand-ins for Tauri's window handles.

use std::any::{Any, TypeId};
use std::collections::HashMap;
use std::ops::Deref;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

use serde::Serialize;
use serde_json::Value;
use tokio::runtime::Handle;
use tokio::sync::{broadcast, oneshot};

use crate::ipc::{Channel, InvokeError, InvokeResponseBody};

static RUNTIME: OnceLock<Handle> = OnceLock::new();

pub fn install_runtime(handle: Handle) {
  let _ = RUNTIME.set(handle);
}

pub fn installed_runtime() -> Option<Handle> {
  RUNTIME.get().cloned()
}

/// A runtime for work started off the server thread, including tests.
pub fn runtime_handle() -> Handle {
  if let Some(handle) = installed_runtime() {
    return handle;
  }
  static FALLBACK: OnceLock<Handle> = OnceLock::new();
  FALLBACK
    .get_or_init(|| {
      let runtime = tokio::runtime::Runtime::new().expect("fallback runtime");
      let handle = runtime.handle().clone();
      std::mem::forget(runtime);
      handle
    })
    .clone()
}

#[derive(Debug)]
pub struct Error(pub String);

impl std::fmt::Display for Error {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    f.write_str(&self.0)
  }
}

impl std::error::Error for Error {}

#[derive(Clone, Debug)]
pub struct Event {
  payload: String,
}

impl Event {
  pub fn payload(&self) -> &str {
    &self.payload
  }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EventId(u64);

#[derive(Clone, Debug)]
pub enum WindowEvent {
  Destroyed,
}

type ListenerFn = Arc<dyn Fn(Event) + Send + Sync>;

struct PickWait {
  tx: oneshot::Sender<Option<Vec<PathBuf>>>,
}

/// What the page should ask the user to choose.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PickRequest {
  pub id: String,
  pub mode: String,
  pub title: String,
  pub filters: Vec<PickFilter>,
  pub file_name: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
pub struct PickFilter {
  pub name: String,
  pub extensions: Vec<String>,
}

struct Bus {
  events: broadcast::Sender<String>,
  listeners: Mutex<HashMap<String, Vec<ListenerFn>>>,
  /// Events emitted before any browser socket is connected, then handed to the first one.
  pending: Mutex<Vec<String>>,
  subscribers: AtomicUsize,
  picks: Mutex<HashMap<String, PickWait>>,
  pick_seq: AtomicU64,
}

impl Bus {
  fn new() -> Self {
    let (events, _) = broadcast::channel(512);
    Self {
      events,
      listeners: Mutex::new(HashMap::new()),
      pending: Mutex::new(Vec::new()),
      subscribers: AtomicUsize::new(0),
      picks: Mutex::new(HashMap::new()),
      pick_seq: AtomicU64::new(1),
    }
  }

  fn publish(&self, event: &str, payload: &Value) {
    let body = serde_json::json!({ "event": event, "payload": payload }).to_string();
    if let Some(listeners) = self.listeners.lock().unwrap().get(event) {
      for listener in listeners {
        listener(Event { payload: payload.to_string() });
      }
    }
    if self.subscribers.load(Ordering::SeqCst) == 0 {
      let mut pending = self.pending.lock().unwrap();
      if pending.len() >= 200 {
        pending.remove(0);
      }
      pending.push(body);
    } else {
      let _ = self.events.send(body);
    }
  }
}

struct Inner {
  states: Mutex<HashMap<TypeId, Arc<dyn Any + Send + Sync>>>,
  bus: Bus,
  version: String,
  /// `http://127.0.0.1:<port>` once the socket is bound. Commands that open a
  /// browser tab read it.
  origin: Mutex<String>,
  token: Mutex<String>,
  windows: Mutex<HashMap<String, ()>>,
  window_hooks: Mutex<HashMap<String, Vec<Arc<dyn Fn(WindowEvent) + Send + Sync>>>>,
}

/// The running server. Cheap to clone; every command shares one.
#[derive(Clone)]
pub struct AppHandle {
  inner: Arc<Inner>,
}

/// Built once at startup and turned into an [`AppHandle`].
pub struct Host {
  inner: Arc<Inner>,
}

impl Host {
  pub fn new(version: impl Into<String>) -> Self {
    Self {
      inner: Arc::new(Inner {
        states: Mutex::new(HashMap::new()),
        bus: Bus::new(),
        version: version.into(),
        origin: Mutex::new(String::new()),
        token: Mutex::new(String::new()),
        windows: Mutex::new(HashMap::new()),
        window_hooks: Mutex::new(HashMap::new()),
      }),
    }
  }

  pub fn handle(&self) -> AppHandle {
    AppHandle { inner: self.inner.clone() }
  }
}

impl AppHandle {
  pub fn manage<T: Send + Sync + 'static>(&self, value: T) {
    self.inner.states.lock().unwrap().insert(TypeId::of::<T>(), Arc::new(value));
  }

  pub fn set_origin(&self, origin: impl Into<String>) {
    *self.inner.origin.lock().unwrap() = origin.into();
  }

  pub fn origin(&self) -> String {
    self.inner.origin.lock().unwrap().clone()
  }

  pub fn set_token(&self, token: impl Into<String>) {
    *self.inner.token.lock().unwrap() = token.into();
  }

  pub fn token(&self) -> String {
    self.inner.token.lock().unwrap().clone()
  }

  /// Fabricator's page for an app window, including the launch token.
  pub fn page_url(&self, label: &str) -> String {
    format!("{}?window={}&token={}", self.origin(), label, self.token())
  }

  pub fn package_info(&self) -> PackageInfo {
    PackageInfo { version: Version { raw: self.inner.version.clone() } }
  }

  pub fn path(&self) -> PathResolver {
    let exe_dir = std::env::current_exe().ok().and_then(|path| path.parent().map(Path::to_path_buf)).unwrap_or_else(|| PathBuf::from("."));
    PathResolver { exe_dir }
  }

  pub fn emit_json(&self, event: &str, payload: &Value) {
    self.inner.bus.publish(event, payload);
  }

  pub fn subscribe(&self) -> broadcast::Receiver<String> {
    self.inner.bus.events.subscribe()
  }

  /// Call after subscribing so earlier events are not stuck in the backlog forever.
  pub fn take_pending(&self) -> Vec<String> {
    let mut pending = self.inner.bus.pending.lock().unwrap();
    self.inner.bus.subscribers.fetch_add(1, Ordering::SeqCst);
    std::mem::take(&mut *pending)
  }

  pub fn subscriber_left(&self) {
    self.inner.bus.subscribers.fetch_sub(1, Ordering::SeqCst);
  }

  pub fn channel<T: Serialize>(&self, id: String) -> Channel<T> {
    let app = self.clone();
    Channel::new(move |body| {
      let InvokeResponseBody::Json(json) = body else {
        return Err(InvokeError("terminal events are JSON".into()));
      };
      let payload: Value = serde_json::from_str(&json).unwrap_or(Value::Null);
      app.emit_json("channel", &serde_json::json!({ "id": id, "payload": payload }));
      Ok(())
    })
  }

  pub fn request_pick(&self, mut request: PickRequest) -> oneshot::Receiver<Option<Vec<PathBuf>>> {
    let id = self.inner.bus.pick_seq.fetch_add(1, Ordering::Relaxed).to_string();
    request.id = id.clone();
    let (tx, rx) = oneshot::channel();
    self.inner.bus.picks.lock().unwrap().insert(id, PickWait { tx });
    self.emit_json("host:pick", &serde_json::to_value(&request).unwrap_or(Value::Null));
    rx
  }

  pub fn resolve_pick(&self, id: &str, paths: Option<Vec<PathBuf>>) -> bool {
    let Some(wait) = self.inner.bus.picks.lock().unwrap().remove(id) else {
      return false;
    };
    wait.tx.send(paths).is_ok()
  }

  pub fn cancel_picks(&self) {
    let waits: Vec<_> = std::mem::take(&mut *self.inner.bus.picks.lock().unwrap()).into_values().collect();
    for wait in waits {
      let _ = wait.tx.send(None);
    }
  }

  pub fn window(&self, label: &str) -> Window {
    self.inner.windows.lock().unwrap().insert(label.to_string(), ());
    Window { app: self.clone(), label: label.to_string() }
  }

  pub fn open_window(&self, label: &str) -> WebviewWindow {
    self.window(label);
    WebviewWindow { window: self.window(label) }
  }

  pub fn known_window(&self, label: &str) -> bool {
    self.inner.windows.lock().unwrap().contains_key(label)
  }

  pub fn close_window(&self, label: &str) {
    self.inner.windows.lock().unwrap().remove(label);
    let hooks = self.inner.window_hooks.lock().unwrap().remove(label).unwrap_or_default();
    for hook in hooks {
      hook(WindowEvent::Destroyed);
    }
    self.emit_json("apps:close", &serde_json::json!({ "label": label }));
  }

  pub fn add_window_hook(&self, label: &str, hook: Arc<dyn Fn(WindowEvent) + Send + Sync>) {
    self.inner.window_hooks.lock().unwrap().entry(label.to_string()).or_default().push(hook);
  }

  /// Replace this process with a fresh one. Skips the "already running" check.
  pub fn restart(&self) -> ! {
    let Ok(exe) = std::env::current_exe() else {
      std::process::exit(1);
    };
    let _ = std::process::Command::new(exe).args(std::env::args().skip(1)).env("FABRICATOR_REPLACE", "1").spawn();
    std::process::exit(0);
  }

  pub fn run_on_main_thread<F>(&self, task: F) -> Result<(), Error>
  where
    F: FnOnce() + Send + 'static,
  {
    task();
    Ok(())
  }
}

#[derive(Clone)]
pub struct Version {
  raw: String,
}

impl std::fmt::Display for Version {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    f.write_str(&self.raw)
  }
}

pub struct PackageInfo {
  pub version: Version,
}

pub struct PathResolver {
  exe_dir: PathBuf,
}

impl PathResolver {
  pub fn resource_dir(&self) -> Result<PathBuf, Error> {
    Ok(self.exe_dir.clone())
  }
}

/// A reference to process-wide state. `'static` because the state lives in an `Arc`.
pub struct State<'a, T: ?Sized> {
  inner: Arc<T>,
  _lifetime: std::marker::PhantomData<&'a ()>,
}

impl<T> Clone for State<'_, T> {
  fn clone(&self) -> Self {
    Self { inner: self.inner.clone(), _lifetime: std::marker::PhantomData }
  }
}

impl<T> Deref for State<'_, T> {
  type Target = T;
  fn deref(&self) -> &T {
    &self.inner
  }
}

impl<T> State<'_, T> {
  pub fn inner(&self) -> &T {
    &self.inner
  }
}

impl AppHandle {
  pub fn state<T: Send + Sync + 'static>(&self) -> State<'static, T> {
    let arc = self
      .inner
      .states
      .lock()
      .unwrap()
      .get(&TypeId::of::<T>())
      .cloned()
      .unwrap_or_else(|| panic!("state {} is not managed", std::any::type_name::<T>()));
    let inner = arc.downcast::<T>().expect("managed state type");
    State { inner, _lifetime: std::marker::PhantomData }
  }

  pub fn try_state<T: Send + Sync + 'static>(&self) -> Option<State<'static, T>> {
    let arc = self.inner.states.lock().unwrap().get(&TypeId::of::<T>()).cloned()?;
    let inner = arc.downcast::<T>().ok()?;
    Some(State { inner, _lifetime: std::marker::PhantomData })
  }
}

pub trait Manager {
  fn state<T: Send + Sync + 'static>(&self) -> State<'static, T>;
  fn try_state<T: Send + Sync + 'static>(&self) -> Option<State<'static, T>>;
  fn get_webview_window(&self, label: &str) -> Option<WebviewWindow>;
  fn get_window(&self, label: &str) -> Option<Window>;
  fn get_webview(&self, label: &str) -> Option<Webview>;
}

impl Manager for AppHandle {
  fn state<T: Send + Sync + 'static>(&self) -> State<'static, T> {
    AppHandle::state(self)
  }

  fn try_state<T: Send + Sync + 'static>(&self) -> Option<State<'static, T>> {
    AppHandle::try_state(self)
  }

  fn get_webview_window(&self, label: &str) -> Option<WebviewWindow> {
    self.known_window(label).then(|| WebviewWindow { window: self.window(label) })
  }

  fn get_window(&self, label: &str) -> Option<Window> {
    self.known_window(label).then(|| self.window(label))
  }

  fn get_webview(&self, _label: &str) -> Option<Webview> {
    None
  }
}

pub trait Emitter {
  fn emit<S: Serialize + Clone>(&self, event: &str, payload: S) -> Result<(), Error>;
}

impl Emitter for AppHandle {
  fn emit<S: Serialize + Clone>(&self, event: &str, payload: S) -> Result<(), Error> {
    let payload = serde_json::to_value(&payload).map_err(|error| Error(error.to_string()))?;
    self.emit_json(event, &payload);
    Ok(())
  }
}

pub trait Listener {
  fn listen_any<F>(&self, event: impl Into<String>, handler: F) -> EventId
  where
    F: Fn(Event) + Send + Sync + 'static;
}

impl Listener for AppHandle {
  fn listen_any<F>(&self, event: impl Into<String>, handler: F) -> EventId
  where
    F: Fn(Event) + Send + Sync + 'static,
  {
    static NEXT: AtomicU64 = AtomicU64::new(1);
    let id = EventId(NEXT.fetch_add(1, Ordering::Relaxed));
    self.inner.bus.listeners.lock().unwrap().entry(event.into()).or_default().push(Arc::new(handler));
    id
  }
}

#[derive(Clone)]
pub struct Window {
  app: AppHandle,
  label: String,
}

impl Window {
  pub fn label(&self) -> &str {
    &self.label
  }

  pub fn app(&self) -> AppHandle {
    self.app.clone()
  }

  pub fn close(&self) -> Result<(), Error> {
    self.app.close_window(&self.label);
    Ok(())
  }

  pub fn on_window_event<F>(&self, handler: F)
  where
    F: Fn(WindowEvent) + Send + Sync + 'static,
  {
    self.app.add_window_hook(&self.label, Arc::new(handler));
  }

  pub fn set_background_color(&self, _color: Option<crate::window::Color>) -> Result<(), Error> {
    Ok(())
  }

  pub fn is_visible(&self) -> Result<bool, Error> {
    Ok(true)
  }

  pub fn show(&self) -> Result<(), Error> {
    Ok(())
  }

  pub fn set_focus(&self) -> Result<(), Error> {
    let _ = self.app.emit("window:focus", self.label.clone());
    Ok(())
  }

  pub fn unminimize(&self) -> Result<(), Error> {
    Ok(())
  }
}

#[derive(Clone)]
pub struct Webview {
  label: String,
}

impl Webview {
  pub(crate) fn new(label: impl Into<String>) -> Self {
    Self { label: label.into() }
  }
}

impl Webview {
  pub fn label(&self) -> &str {
    &self.label
  }
}

/// An app tab the server asked the browser to open. It has no native hwnd.
#[derive(Clone)]
pub struct WebviewWindow {
  window: Window,
}

impl WebviewWindow {
  pub fn label(&self) -> &str {
    self.window.label()
  }

  pub fn as_ref(&self) -> &Self {
    self
  }

  pub fn window(&self) -> &Window {
    &self.window
  }

  pub fn hwnd(&self) -> Result<Hwnd, Error> {
    Err(Error("the browser has no window handle".into()))
  }

  pub fn run_on_main_thread<F>(&self, task: F) -> Result<(), Error>
  where
    F: FnOnce() + Send + 'static,
  {
    task();
    Ok(())
  }

  pub fn navigate(&self, url: impl ToString) -> Result<(), Error> {
    self.window.app.emit_json(
      "apps:navigate",
      &serde_json::json!({ "label": self.window.label, "url": url.to_string() }),
    );
    Ok(())
  }

  pub fn set_title(&self, title: &str) -> Result<(), Error> {
    self.window.app.emit_json(
      "apps:title",
      &serde_json::json!({ "label": self.window.label, "title": title }),
    );
    Ok(())
  }

  pub fn set_icon(&self, _icon: crate::image::Image<'_>) -> Result<(), Error> {
    Ok(())
  }

  pub fn emit<S: Serialize + Clone>(&self, event: &str, payload: S) -> Result<(), Error> {
    self.window.app.emit(event, payload)
  }

  pub fn close(&self) -> Result<(), Error> {
    self.window.close()
  }

  pub fn destroy(&self) -> Result<(), Error> {
    self.window.close()
  }

  pub fn show(&self) -> Result<(), Error> {
    Ok(())
  }

  pub fn unminimize(&self) -> Result<(), Error> {
    Ok(())
  }

  pub fn set_focus(&self) -> Result<(), Error> {
    self.window.set_focus()
  }

  pub fn is_visible(&self) -> Result<bool, Error> {
    Ok(self.window.app.known_window(&self.window.label))
  }

  pub fn on_window_event<F>(&self, handler: F)
  where
    F: Fn(WindowEvent) + Send + Sync + 'static,
  {
    self.window.on_window_event(handler);
  }
}

pub struct Hwnd(pub isize);
