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
use tauri::{Menu, MenuItem, PhysicalSize, Submenu, Window, WindowEvent};

pub trait OverlayView: HasRawWindowHandle {
    fn set_frame(&mut self, x: f64, y: f64, size: PhysicalSize<u32>);
}

unsafe fn add_overlay<W: HasRawWindowHandle>(window: &W) -> impl OverlayView {
    // Create an NSView with an NSRect
    // Add the view as a subview of contentView
    // Draw on the subview
    match window.raw_window_handle() {
        #[cfg(target_os = "macos")]
        raw_window_handle::RawWindowHandle::AppKit(handle) => macos::add_overlay(handle),
        #[cfg(target_os = "windows")]
        raw_window_handle::RawWindowHandle::Win32(handle) => windows::add_overlay(handle),
        _ => unimplemented!("this only works on macos or windows"),
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

    pub fn add_overlay(handle: AppKitHandle) -> OverlayView {
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
}

#[cfg(windows)]
pub mod windows {

    use crate::OverlayView;
    use raw_window_handle::{HasRawWindowHandle, Win32Handle};
    use tauri::PhysicalSize;

    pub struct WindowsOverlayView {
    }
    impl OverlayView for WindowsOverlayView {
        fn set_frame(&mut self, x: f64, y: f64, size: PhysicalSize<u32>) {
                unimplemented!("implement set_frame for windows");
        }
    }
    
    unsafe impl HasRawWindowHandle for WindowsOverlayView {
        fn raw_window_handle(&self) -> raw_window_handle::RawWindowHandle {
            use raw_window_handle::Win32Handle;

            unimplemented!("raw_window_handle for windows");
            // let handle = Win32Handle::empty();
            // raw_window_handle::RawWindowHandle::Win32(handle)
        }
    }

    pub fn add_overlay(handle: Win32Handle) -> impl OverlayView {
        todo!("fixme");

        WindowsOverlayView{}
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
    tauri::Builder::default()
        .menu(build_menu())
        .on_page_load(|window: Window, _| {
            add_gpu_overlay_window(&window);
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

fn add_gpu_overlay_window(window: &Window) {
    let video_view = unsafe { add_overlay(window) };
    let wgpu_state = match tokio::runtime::Runtime::new() {
        Ok(runtime) => runtime.block_on(async {
            // load data in separate async thread
            // workaround for https://github.com/tauri-apps/tauri/issues/2838
            return State::new(
                &video_view,
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
    let video_view = Arc::new(Mutex::new(video_view));
    let state1 = wgpu_state.clone();
    window.on_window_event(move |event| match event {
        WindowEvent::Resized(size) => {
            // let size = size.to_logical(2.0);
            let size = PhysicalSize {
                width: size.width,
                height: size.height
            };
            let overlay_width = size.width as f64 * 0.3;
            let overlay_height = size.height as f64 * 0.1;
            let overlay_x = 100;
            let overlay_y = 100;
            let x = (size.width as f64 - overlay_width) / 2.0;
            let y = overlay_y as f64;
            let overlay_size = PhysicalSize {
                width: overlay_width as u32,
                height: overlay_height as u32,
            };
            video_view.lock().unwrap().set_frame(x, y, overlay_size);
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
