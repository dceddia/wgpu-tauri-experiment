use std::sync::Weak;

use crate::overlay::OverlayView;
use raw_window_handle::{HasRawWindowHandle, Win32Handle};
use tao::platform::windows::{WindowBuilderExtWindows, WindowExtWindows};
use tauri::{AppHandle, Manager, PhysicalPosition, Position, Size};
use windows::Win32::{
    Foundation::HWND,
    UI::WindowsAndMessaging::{
        GetWindowLongW, SetWindowLongW, GWL_EXSTYLE, WS_EX_LAYERED, WS_EX_NOACTIVATE,
        WS_EX_TRANSPARENT,
    },
};

pub struct WindowsOverlayView {
    overlay: Weak<tao::window::Window>,
    parent_pos: Position,
    last_origin: Position,
}

impl WindowsOverlayView {
    pub fn new(overlay: Weak<tao::window::Window>) -> Self {
        WindowsOverlayView {
            overlay,
            parent_pos: Position::Physical(PhysicalPosition { x: 0, y: 0 }),
            last_origin: Position::Physical(PhysicalPosition { x: 0, y: 0 }),
        }
    }
}

impl OverlayView for WindowsOverlayView {
    fn set_parent_position(&mut self, pos: Position) {
        self.parent_pos = pos;
        self.set_origin(self.last_origin.clone());
    }

    fn set_origin(&mut self, pos: Position) {
        if let Some(overlay) = self.overlay.upgrade() {
            self.last_origin = pos;

            // Translate the origin by the parent window position
            let translated = match (&self.last_origin, &self.parent_pos) {
                (Position::Physical(origin), Position::Physical(parent)) => {
                    tao::dpi::PhysicalPosition {
                        x: origin.x + parent.x,
                        y: origin.y + parent.y,
                    }
                }
                _ => unimplemented!("set_origin does not support Logical positions yet"),
            };
            overlay.set_outer_position(translated);
        }
    }

    fn set_size(&mut self, size: Size) {
        if let Some(overlay) = self.overlay.upgrade() {
            match size {
                Size::Physical(size) => overlay.set_inner_size(tao::dpi::PhysicalSize {
                    width: size.width,
                    height: size.height,
                }),
                Size::Logical(size) => overlay.set_inner_size(tao::dpi::LogicalSize {
                    width: size.width,
                    height: size.height,
                }),
            }
        }
    }
}

unsafe impl HasRawWindowHandle for WindowsOverlayView {
    fn raw_window_handle(&self) -> raw_window_handle::RawWindowHandle {
        let window = self.overlay.upgrade().expect("window was deallocated?");
        let mut handle = Win32Handle::empty();
        handle.hwnd = window.hwnd();
        handle.hinstance = window.hinstance();

        raw_window_handle::RawWindowHandle::Win32(handle)
    }
}

pub fn add_overlay(app_handle: &AppHandle) -> impl OverlayView {
    let window = app_handle
        .get_window("main")
        .expect("failed to get main window");

    let hwnd = HWND(window.hwnd().expect("failed to get HWND") as _);
    let overlay = app_handle
        .create_tao_window(move || {
            let window_builder = tao::window::WindowBuilder::new()
                .with_always_on_top(false)
                .with_decorations(false)
                .with_resizable(false)
                .with_visible(true)
                .with_position(tao::dpi::LogicalPosition::<u32>::new(30, 30))
                .with_owner_window(hwnd)
                .with_inner_size(tao::dpi::LogicalSize::<u32>::new(200, 200));

            ("WGPU Target".to_string(), window_builder)
        })
        .expect("failed to create overlay window");
    make_window_passthrough_events(
        overlay
            .upgrade()
            .expect("failed to get Arc<Window>")
            .as_ref(),
    );

    WindowsOverlayView::new(overlay)
}

/// Make it so that mouse events pass through the window and it's excluded from tab order
fn make_window_passthrough_events(window: &tao::window::Window) {
    let hwnd = HWND(window.hwnd() as _);
    unsafe {
        // Based on https://stackoverflow.com/a/50245502
        let cur_style = GetWindowLongW(hwnd, GWL_EXSTYLE) as u32;
        SetWindowLongW(
            hwnd,
            GWL_EXSTYLE,
            (cur_style | WS_EX_TRANSPARENT | WS_EX_LAYERED | WS_EX_NOACTIVATE) as i32,
        );
    }
}
