use std::ffi::c_void;

use crate::OverlayView;
use cocoa::{
    appkit::NSView,
    base::nil,
    foundation::{NSPoint, NSRect, NSSize},
};

use objc::{msg_send, runtime::Object, sel, sel_impl};
use raw_window_handle::{AppKitHandle, HasRawWindowHandle, RawWindowHandle};
use tauri::{AppHandle, Manager};

pub struct MacosOverlayView {
    ns_window: *mut Object,
    ns_view: *mut Object,
}

unsafe impl Send for MacosOverlayView {}
impl MacosOverlayView {
    fn new(ns_window: *mut Object, ns_view: *mut Object) -> Self {
        MacosOverlayView { ns_window, ns_view }
    }
}
impl OverlayView for MacosOverlayView {
    fn set_parent_position(&mut self, _: tauri::Position) {
        // not needed on macOS
    }

    fn set_origin(&mut self, pos: tauri::Position) {
        let (x, y) = match pos {
            tauri::Position::Physical(pos) => (pos.x as f64, pos.y as f64),
            tauri::Position::Logical(pos) => (pos.x, pos.y),
        };
        unsafe {
            let _: () = msg_send![self.ns_view, setFrameOrigin: NSPoint::new(x, y)];
        }
    }

    fn set_size(&mut self, size: tauri::Size) {
        let (width, height) = match size {
            tauri::Size::Physical(size) => (size.width as f64, size.width as f64),
            tauri::Size::Logical(size) => (size.width, size.width),
        };

        unsafe {
            let _: () = msg_send![self.ns_view, setFrameSize: NSSize {
                width,
                height,
            }];
        }
    }
}

unsafe impl HasRawWindowHandle for MacosOverlayView {
    fn raw_window_handle(&self) -> raw_window_handle::RawWindowHandle {
        let mut handle = AppKitHandle::empty();
        handle.ns_window = self.ns_window as *mut c_void;
        handle.ns_view = self.ns_view as *mut c_void;
        raw_window_handle::RawWindowHandle::AppKit(handle)
    }
}

pub fn add_overlay(handle: &AppHandle) -> impl OverlayView {
    let window = handle
        .get_window("main")
        .expect("failed to get main window");
    if let RawWindowHandle::AppKit(handle) = window.raw_window_handle() {
        unsafe {
            let ns_window = handle.ns_window as *mut Object;
            let content_view: *mut Object = msg_send![ns_window, contentView];

            // Make a new view
            let new_view = NSView::alloc(nil).initWithFrame_(NSRect::new(
                NSPoint::new(100.0, 0.0),
                NSSize::new(200.0, 200.0),
            ));
            new_view.setWantsLayer(true);

            // Add it to the contentView, as a sibling of webview, so that it appears on top
            let _: c_void = msg_send![content_view, addSubview: new_view];

            // Quick check: How many views?
            let subviews: *mut Object = msg_send![content_view, subviews];
            let count: usize = msg_send![subviews, count];
            println!("contentView now has {} views", count);
            MacosOverlayView::new(ns_window, new_view)
        }
    } else {
        unreachable!("only runs on windows")
    }
}
