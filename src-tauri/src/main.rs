#![cfg_attr(
    all(not(debug_assertions), target_os = "windows"),
    windows_subsystem = "windows"
)]

use std::{
    ffi::c_void,
    sync::{Arc, Mutex},
    time::Duration,
};

use raw_window_handle::HasRawWindowHandle;
use tauri::{
    AppHandle, Manager, Menu, MenuItem, PhysicalPosition, PhysicalSize, Position, Size, Submenu,
    Window, WindowEvent,
};

pub trait OverlayView: HasRawWindowHandle {
    fn set_parent_position<P: Into<Position>>(&mut self, pos: P);
    fn set_origin<P: Into<Position>>(&mut self, pos: P);
    fn set_size<S: Into<Size>>(&mut self, size: S);
}

unsafe fn add_overlay(handle: &AppHandle) -> impl OverlayView {
    cfg_if::cfg_if! {
        if #[cfg(target_os = "macos")] {
            macos::add_overlay(handle)
        } else if #[cfg(target_os = "windows")] {
            windows::add_overlay(handle)
        }
    }
}

#[cfg(macos)]
pub mod macos {
    use crate::OverlayView;
    use cocoa::{
        appkit::NSView,
        base::nil,
        foundation::{NSPoint, NSRect, NSSize},
    };

    use objc::{msg_send, runtime::Object, sel, sel_impl};
    use raw_window_handle::{AppKitHandle, HasRawWindowHandle};

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
        fn set_frame(&mut self, x: f64, y: f64, size: PhysicalSize<u32>) {
            unsafe {
                let _: () = msg_send![self.ns_view, setFrameOrigin: NSPoint::new(x, y)];
                let _: () = msg_send![self.ns_view, setFrameSize: NSSize {
                    width: size.width as f64,
                    height: size.height as f64,
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

    pub fn add_overlay(handle: &AppHandle) -> OverlayView {
        let window = handle
            .get_window("main")
            .expect("failed to get main window");
        if let RawWindowHandle::AppKitHandle(handle) = window.raw_window_handle() {
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
        } else {
            unreachable!("only runs on windows")
        }
    }
}

#[cfg(windows)]
pub mod windows {

    use std::{ffi::c_void, sync::Weak};

    use crate::OverlayView;
    use raw_window_handle::{HasRawWindowHandle, RawWindowHandle, Win32Handle};
    use tao::platform::windows::{WindowBuilderExtWindows, WindowExtWindows};
    use tauri::{AppHandle, Manager, PhysicalPosition, PhysicalSize, Position, Size};
    use windows::Win32::{
        Foundation::{BOOL, HWND, LPARAM},
        UI::WindowsAndMessaging::{
            GetWindowLongW, SetWindowLongW, GWL_EXSTYLE, WS_EX_LAYERED, WS_EX_NOACTIVATE,
            WS_EX_TRANSPARENT,
        },
    };

    pub struct WindowsOverlayView {
        overlay: Weak<tao::window::Window>,
        parent_pos: Position,
        last_origin: Position,
        last_size: Size,
    }

    impl WindowsOverlayView {
        pub fn new(overlay: Weak<tao::window::Window>) -> Self {
            WindowsOverlayView {
                overlay,
                parent_pos: Position::Physical(PhysicalPosition { x: 0, y: 0 }),
                last_origin: Position::Physical(PhysicalPosition { x: 0, y: 0 }),
                last_size: Size::Physical(PhysicalSize {
                    width: 0,
                    height: 0,
                }),
            }
        }
    }

    impl OverlayView for WindowsOverlayView {
        fn set_parent_position<P: Into<Position>>(&mut self, pos: P) {
            self.parent_pos = pos.into();
            self.set_origin(self.last_origin.clone());
        }

        fn set_origin<P: Into<Position>>(&mut self, pos: P) {
            if let Some(overlay) = self.overlay.upgrade() {
                self.last_origin = pos.into();

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

        fn set_size<S: Into<Size>>(&mut self, size: S) {
            if let Some(overlay) = self.overlay.upgrade() {
                match size.into() {
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

        if let RawWindowHandle::Win32(handle) = window.raw_window_handle() {
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
        } else {
            unreachable!("only runs on windows")
        }
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
}

struct State {
    surface: wgpu::Surface,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    size: tauri::PhysicalSize<u32>,
}

impl State {
    async fn new<W: HasRawWindowHandle>(drawable: &W, size: tauri::PhysicalSize<u32>) -> Self {
        // The instance is a handle to our GPU
        // BackendBit::PRIMARY => Vulkan + Metal + DX12 + Browser WebGPU
        let instance = wgpu::Instance::new(wgpu::Backends::all());
        let surface = unsafe { instance.create_surface(drawable) };
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::default(),
                compatible_surface: Some(&surface),
                force_fallback_adapter: false,
            })
            .await
            .unwrap();

        let (device, queue) = adapter
            .request_device(
                &wgpu::DeviceDescriptor {
                    label: None,
                    features: wgpu::Features::empty(),
                    limits: wgpu::Limits::default(),
                },
                // Some(&std::path::Path::new("trace")), // Trace path
                None,
            )
            .await
            .unwrap();

        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format: surface.get_preferred_format(&adapter).unwrap(),
            width: size.width,
            height: size.height,
            present_mode: wgpu::PresentMode::Fifo,
        };
        surface.configure(&device, &config);

        println!("Created State w/ size {:?}", size);

        Self {
            surface,
            device,
            queue,
            config,
            size,
        }
    }

    pub fn resize(&mut self, new_size: tauri::PhysicalSize<u32>) {
        if new_size.width > 0 && new_size.height > 0 {
            self.size = new_size;
            self.config.width = new_size.width;
            self.config.height = new_size.height;
            self.surface.configure(&self.device, &self.config);
        }
    }

    #[allow(unused_variables)]
    fn input(&mut self, event: &WindowEvent) -> bool {
        false
    }

    fn update(&mut self) {}

    fn render(&mut self) -> Result<(), wgpu::SurfaceError> {
        let output = self.surface.get_current_texture()?;
        let view = output
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Render Encoder"),
            });

        {
            let _render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Render Pass"),
                color_attachments: &[wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: 0.1,
                            g: 0.2,
                            b: 0.3,
                            a: 1.0,
                        }),
                        store: true,
                    },
                }],
                depth_stencil_attachment: None,
            });
        }

        self.queue.submit(std::iter::once(encoder.finish()));
        output.present();

        Ok(())
    }
}

fn main() {
    let app = tauri::Builder::default()
        .menu(build_menu())
        .build(tauri::generate_context!())
        .expect("failed to build app");

    app.run(|handle, event| match event {
        tauri::RunEvent::Ready => {
            add_wgpu_overlay(handle);
        }
        _ => {}
    });
}

fn add_wgpu_overlay(handle: &AppHandle) {
    let overlay_view = unsafe { add_overlay(handle) };
    let wgpu_state = match tokio::runtime::Runtime::new() {
        Ok(runtime) => runtime.block_on(async {
            // load data in separate async thread
            // workaround for https://github.com/tauri-apps/tauri/issues/2838
            return State::new(
                &overlay_view,
                PhysicalSize {
                    width: 200,
                    height: 200,
                },
            )
            .await;
        }),
        Err(_) => panic!("error creating runtime"),
    };

    let wgpu_state = Arc::new(Mutex::new(wgpu_state));
    let video_view = Arc::new(Mutex::new(overlay_view));
    let state1 = wgpu_state.clone();
    let window = handle.get_window("main").unwrap();
    window.on_window_event(move |event| match event {
        WindowEvent::Moved(pos) => {
            let mut overlay = video_view.lock().unwrap();
            let pos = Position::Physical(pos.clone());
            overlay.set_parent_position(pos);
        }
        WindowEvent::Resized(size) => {
            // let size = size.to_logical(2.0);
            let size = PhysicalSize {
                width: size.width,
                height: size.height,
            };
            let overlay_width = size.width as f64 * 0.3;
            let overlay_height = size.height as f64 * 0.1;
            let overlay_y = 100;
            let x = (size.width as f64 - overlay_width) / 2.0;
            let y = overlay_y as f64;
            let overlay_size = PhysicalSize {
                width: overlay_width as u32,
                height: overlay_height as u32,
            };
            let mut overlay = video_view.lock().unwrap();
            overlay.set_origin(Position::Physical(PhysicalPosition {
                x: x as i32,
                y: y as i32,
            }));
            overlay.set_size(Size::Physical(overlay_size));
            state1.lock().unwrap().resize(overlay_size);
        }
        _ => {}
    });

    let state2 = wgpu_state.clone();
    std::thread::spawn(move || loop {
        // wgpu_state.resize(PhysicalSize {
        //     width: 200,
        //     height: 200,
        // });
        state2.lock().unwrap().render().expect("render failed");
        std::thread::sleep(Duration::from_millis(15));
    });
}

fn build_menu() -> Menu {
    Menu::new()
        .add_submenu(Submenu::new(
            "app",
            Menu::new()
                .add_native_item(MenuItem::Hide)
                .add_native_item(MenuItem::Quit),
        ))
        .add_submenu(Submenu::new(
            "Edit",
            Menu::new()
                .add_native_item(MenuItem::Copy)
                .add_native_item(MenuItem::Cut)
                .add_native_item(MenuItem::Paste)
                .add_native_item(MenuItem::Separator)
                .add_native_item(MenuItem::Undo)
                .add_native_item(MenuItem::Redo)
                .add_native_item(MenuItem::Separator)
                .add_native_item(MenuItem::SelectAll),
        ))
}
