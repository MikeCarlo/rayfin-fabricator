//! The preview-bounds math the desktop build used to place a native webview.
//! The browser preview is an iframe, but the same checks still reject a bad rect.

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LogicalPosition<T> {
  pub x: T,
  pub y: T,
}

impl<T> LogicalPosition<T> {
  pub fn new(x: T, y: T) -> Self {
    Self { x, y }
  }
}

impl LogicalPosition<f64> {
  pub fn to_physical<T: Px>(&self, scale: f64) -> PhysicalPosition<T> {
    PhysicalPosition { x: T::from_f64(self.x * scale), y: T::from_f64(self.y * scale) }
  }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LogicalSize<T> {
  pub width: T,
  pub height: T,
}

impl<T> LogicalSize<T> {
  pub fn new(width: T, height: T) -> Self {
    Self { width, height }
  }
}

impl LogicalSize<f64> {
  pub fn to_physical<T: Px>(&self, scale: f64) -> PhysicalSize<T> {
    PhysicalSize { width: T::from_f64(self.width * scale), height: T::from_f64(self.height * scale) }
  }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PhysicalPosition<T> {
  pub x: T,
  pub y: T,
}

impl<T> PhysicalPosition<T> {
  pub fn new(x: T, y: T) -> Self {
    Self { x, y }
  }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PhysicalSize<T> {
  pub width: T,
  pub height: T,
}

impl<T> PhysicalSize<T> {
  pub fn new(width: T, height: T) -> Self {
    Self { width, height }
  }
}

pub trait Px: Copy {
  fn from_f64(value: f64) -> Self;
}

impl Px for i32 {
  fn from_f64(value: f64) -> Self {
    value.round() as i32
  }
}

impl Px for u32 {
  fn from_f64(value: f64) -> Self {
    value.round().max(0.0) as u32
  }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Position {
  Physical(PhysicalPosition<i32>),
  Logical(LogicalPosition<f64>),
}

impl Position {
  pub fn to_physical<T: Px>(&self, scale: f64) -> PhysicalPosition<T> {
    match self {
      Position::Physical(position) => PhysicalPosition {
        x: T::from_f64(position.x as f64),
        y: T::from_f64(position.y as f64),
      },
      Position::Logical(position) => position.to_physical(scale),
    }
  }
}

impl From<LogicalPosition<f64>> for Position {
  fn from(position: LogicalPosition<f64>) -> Self {
    Position::Logical(position)
  }
}

impl From<PhysicalPosition<i32>> for Position {
  fn from(position: PhysicalPosition<i32>) -> Self {
    Position::Physical(position)
  }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Size {
  Physical(PhysicalSize<u32>),
  Logical(LogicalSize<f64>),
}

impl Size {
  pub fn to_physical<T: Px>(&self, scale: f64) -> PhysicalSize<T> {
    match self {
      Size::Physical(size) => PhysicalSize {
        width: T::from_f64(size.width as f64),
        height: T::from_f64(size.height as f64),
      },
      Size::Logical(size) => size.to_physical(scale),
    }
  }
}

impl From<LogicalSize<f64>> for Size {
  fn from(size: LogicalSize<f64>) -> Self {
    Size::Logical(size)
  }
}

impl From<PhysicalSize<u32>> for Size {
  fn from(size: PhysicalSize<u32>) -> Self {
    Size::Physical(size)
  }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rect {
  pub position: Position,
  pub size: Size,
}
