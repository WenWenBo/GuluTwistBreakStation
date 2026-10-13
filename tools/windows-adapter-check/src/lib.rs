//! Type-check the real Windows adapter on macOS without GPUI's Windows SDK build scripts.
//! This is not a build or runtime test of the Windows application.
#![cfg(target_os="windows")]
extern crate self as gpui_kit;
pub struct Window;
impl raw_window_handle::HasWindowHandle for Window {
    fn window_handle(&self)->Result<raw_window_handle::WindowHandle<'_>,raw_window_handle::HandleError>{Err(raw_window_handle::HandleError::Unavailable)}
}
pub enum Event{Tick,Error(String)}
#[path="../../../crates/gulu-app/src/platform/windows.rs"]
pub mod platform;
