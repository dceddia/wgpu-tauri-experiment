use raw_window_handle::HasRawWindowHandle;
use tauri::{AppHandle, Position, Size};

#[cfg(target_os = "macos")]
pub mod macos;

#[cfg(target_os = "windows")]
pub mod windows;

pub trait OverlayView: HasRawWindowHandle {
    fn set_parent_position(&mut self, pos: Position);
    fn set_origin(&mut self, pos: Position);
    fn set_size(&mut self, size: Size);
}

pub unsafe fn add_overlay(handle: &AppHandle) -> impl OverlayView {
    cfg_if::cfg_if! {
        if #[cfg(target_os = "macos")] {
            macos::add_overlay(handle)
        } else if #[cfg(target_os = "windows")] {
            windows::add_overlay(handle)
        }
    }
}
