#![cfg_attr(
    all(not(debug_assertions), target_os = "windows"),
    windows_subsystem = "windows"
)]

mod overlay;

use std::{
    sync::{Arc, Mutex},
    time::Duration,
};

use overlay::OverlayView;
use raw_window_handle::HasRawWindowHandle;
use serde::Deserialize;
use tauri::{
    AppHandle, Manager, Menu, MenuItem, PhysicalPosition, PhysicalSize, Position, Size, State,
    Submenu, WindowEvent,
};

struct WgpuState {
    surface: wgpu::Surface,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    size: tauri::PhysicalSize<u32>,
}

impl WgpuState {
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

#[tauri::command]
fn set_overlay_position(x: f64, y: f64, overlay: State<Overlay>) {
    println!("mouse moved to {}, {}", x, y);
    let overlay = overlay.0.lock().unwrap();
    overlay.as_ref().map(|overlay| {
        overlay
            .lock()
            .unwrap()
            .set_origin(Position::Physical(PhysicalPosition {
                x: x as i32,
                y: y as i32,
            }));
    });
}

struct Overlay(Mutex<Option<Arc<Mutex<dyn OverlayView + Send>>>>);

fn main() {
    let app = tauri::Builder::default()
        .menu(build_menu())
        .manage(Overlay(Mutex::new(None)))
        .invoke_handler(tauri::generate_handler![set_overlay_position])
        .build(tauri::generate_context!())
        .expect("failed to build app");

    app.run(|handle, event| match event {
        tauri::RunEvent::Ready => {
            let overlay = add_wgpu_overlay(handle);
            let state: tauri::State<Overlay> = handle.state();
            let mut state = state.0.lock().unwrap();
            *state = Some(overlay);
        }
        _ => {}
    });
}

fn add_wgpu_overlay(handle: &AppHandle) -> Arc<Mutex<dyn OverlayView + Send>> {
    let overlay_view = unsafe { overlay::add_overlay(handle) };
    let wgpu_state = match tokio::runtime::Runtime::new() {
        Ok(runtime) => runtime.block_on(async {
            // load data in separate async thread
            // workaround for https://github.com/tauri-apps/tauri/issues/2838
            return WgpuState::new(
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
    let overlay_view = Arc::new(Mutex::new(overlay_view));
    let state1 = wgpu_state.clone();
    let window = handle.get_window("main").unwrap();

    let local_overlay = overlay_view.clone();
    window.on_window_event(move |event| match event {
        WindowEvent::Moved(pos) => {
            let mut overlay = local_overlay.lock().unwrap();
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
            let mut overlay = local_overlay.lock().unwrap();
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

    overlay_view
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
