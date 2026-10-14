//! Type-check the real Windows adapter and tray on macOS without GPUI's SDK build scripts.
//! This is not a build or runtime test of the Windows application.
#![cfg(target_os = "windows")]
extern crate self as gpui_kit;

pub type Pixels = f32;
#[derive(Default)]
pub struct Point<T> {
    pub x: T,
    pub y: T,
}
#[derive(Default)]
pub struct Size<T> {
    pub width: T,
    pub height: T,
}
#[derive(Default)]
pub struct Bounds<T> {
    pub origin: Point<T>,
    pub size: Size<T>,
}
pub trait PlatformDisplay {
    fn id(&self) -> u64;
    fn visible_bounds(&self) -> Bounds<Pixels>;
}
pub struct App;
pub struct Window;
impl Window {
    pub fn bounds(&self) -> Bounds<Pixels> {
        Bounds::default()
    }
    pub fn scale_factor(&self) -> f32 {
        1.
    }
    // Signature-only stubs; this crate does not execute GPUI's event loop.
    pub fn defer(&self, _: &mut App, _: impl FnOnce(&mut Window, &mut App) + 'static) {}
    pub fn bounds_changed(&mut self, _: &mut App) {}
}
impl raw_window_handle::HasWindowHandle for Window {
    fn window_handle(
        &self,
    ) -> Result<raw_window_handle::WindowHandle<'_>, raw_window_handle::HandleError> {
        Err(raw_window_handle::HandleError::Unavailable)
    }
}
pub enum Event {
    Tick,
    Displays,
    Error(String),
    Command(Command),
}
pub enum Command {
    ToggleFloating,
    Open,
    OpenSettings,
    PauseMinutes(i64),
    NextDay,
    Pause,
    Resume,
    Quit,
}
#[path = "../../../crates/gulu-app/src/platform/windows.rs"]
pub mod platform;
#[path = "../../../crates/gulu-app/src/resident.rs"]
pub mod resident;
#[path = "../../../crates/gulu-app/src/tray.rs"]
pub mod tray;
