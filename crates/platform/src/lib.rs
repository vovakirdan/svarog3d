//! Platform layer: window & event loop (winit 0.30.12).
//! Step B1 integration: create WGPU surface and clear screen.

use anyhow::Result;
use std::{env, sync::Arc, time::Instant};
use wgpu;
use winit::{
    application::ApplicationHandler,
    dpi::{LogicalSize, PhysicalSize},
    event::WindowEvent,
    event_loop::{ActiveEventLoop, ControlFlow, EventLoop},
    window::{Window, WindowId},
};

use corelib::{camera::Camera, transform::Transform, vec3};

/// Public entry: runs a window + renderer. Returns on close.
pub fn run_with_renderer(
    backends: wgpu::Backends,
    show_fps: bool,
    width: u32,
    height: u32,
) -> Result<()> {
    log::info!(
        "Env: DISPLAY={:?}, WAYLAND_DISPLAY={:?}",
        env::var("DISPLAY").ok(),
        env::var("WAYLAND_DISPLAY").ok()
    );

    let event_loop: EventLoop<()> = EventLoop::new().expect("Failed to create event loop");
    let mut app = App {
        backends,
        show_fps,
        width,
        height,
        ..Default::default()
    };
    event_loop
        .run_app(&mut app)
        .map_err(|e| anyhow::anyhow!(format!("{e:?}")))?;
    Ok(())
}

#[derive(Default)]
struct App {
    gpu: Option<renderer::GpuState>,
    window: Option<Arc<Window>>,
    // Конфиг
    backends: wgpu::Backends,
    show_fps: bool,
    width: u32,
    height: u32,

    frames: u32,
    last_fps_instant: Option<Instant>,

    // animation state
    last_time: Option<Instant>,
    angle: f32,

    // scene
    camera: Option<Camera>,
    model: Transform,
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        // Create window
        let attrs = Window::default_attributes()
            .with_title(format!(
                "Svarog3D (running with wgpu backend {:?})",
                self.backends
            ))
            .with_inner_size(LogicalSize::new(self.width as f64, self.height as f64));
        let w = event_loop
            .create_window(attrs)
            .expect("Failed to create window");
        let window = Arc::new(w);

        log::info!(
            "Window created: {}x{} (physical pixels)",
            window.inner_size().width,
            window.inner_size().height
        );

        // Init GPU (pass Arc<Window>)
        let mut gpu = pollster::block_on(renderer::GpuState::new(window.clone(), self.backends));

        // Setup camera & model
        let size = window.inner_size();
        let aspect = size.width.max(1) as f32 / size.height.max(1) as f32;
        let camera = Camera::new_perspective(
            vec3(0.0, 0.0, 4.0),
            vec3(0.0, 0.0, 0.0),
            corelib::Vec3::Y,
            60f32.to_radians(),
            0.1,
            100.0,
            aspect,
        );
        self.camera = Some(camera);
        self.model = Transform::from_trs(
            vec3(0.0, 0.0, 0.0),
            vec3(0.0, 0.0, 0.0),
            vec3(1.0, 1.0, 1.0),
        );

        gpu.set_camera(&camera);
        gpu.set_model(&self.model);

        // Control flow + first frame
        event_loop.set_control_flow(ControlFlow::Wait);
        window.request_redraw();

        self.gpu = Some(gpu);
        self.window = Some(window);
        self.frames = 0;
        self.last_fps_instant = Some(Instant::now());
        self.last_time = Some(Instant::now());
        self.angle = 0.0;
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        _window_id: WindowId,
        event: WindowEvent,
    ) {
        match event {
            WindowEvent::CloseRequested => {
                log::info!("Close requested. Exiting.");
                // Сначала корректно освобождаем все GPU-ресурсы (Surface/Device/Queue).
                let _ = self.gpu.take(); // drop happens here
                // Затем окно (опционально, но порядок уже безопасный).
                let _ = self.window.take();
                event_loop.exit();
            }
            WindowEvent::Resized(new_size) => {
                log::info!("Resized: {}x{}", new_size.width, new_size.height);
                if let Some(gpu) = self.gpu.as_mut() {
                    gpu.resize(new_size.width, new_size.height);
                }

                // update camera aspect
                if let Some(cam) = self.camera {
                    let aspect = (new_size.width.max(1) as f32) / (new_size.height.max(1) as f32);
                    let cam2 = Camera { aspect, ..cam };
                    self.camera = Some(cam2);
                    if let Some(gpu) = self.gpu.as_mut() {
                        gpu.set_camera(&cam2);
                    }
                }
                if let Some(w) = &self.window {
                    w.request_redraw();
                }
            }
            WindowEvent::ScaleFactorChanged {
                scale_factor,
                inner_size_writer: _,
            } => {
                log::info!("Scale factor changed: {:.3}", scale_factor);
            }
            WindowEvent::RedrawRequested => {
                // animate model (rotate)
                if let Some(t0) = self.last_time {
                    let dt = t0.elapsed().as_secs_f32().max(1e-6);
                    self.last_time = Some(Instant::now());
                    self.angle += dt;
                    self.model = Transform::from_trs(
                        vec3(0.0, 0.0, 0.0),
                        vec3(0.5 * self.angle, self.angle, 0.0),
                        vec3(1.0, 1.0, 1.0),
                    );
                    if let Some(gpu) = self.gpu.as_mut() {
                        gpu.set_model(&self.model);
                    }
                }

                if let Some(gpu) = self.gpu.as_mut() {
                    if let Err(e) = gpu.render() {
                        log::warn!("Render error: {e:?}");
                        if renderer::GpuState::is_surface_lost(&e) {
                            log::warn!("Surface lost/outdated. Recreating…");
                            gpu.recreate_surface();
                        }
                    }
                }
                // FPS: счёт и обновление заголовка раз в ~1 сек
                if self.show_fps {
                    self.frames += 1;
                    if let Some(t0) = self.last_fps_instant {
                        let dt = t0.elapsed();
                        if dt.as_secs_f32() >= 1.0 {
                            let fps = self.frames as f32 / dt.as_secs_f32();
                            if let Some(w) = &self.window {
                                w.set_title(&format!(
                                    "Svarog3D (running with wgpu backend {:?}) {:.1} FPS",
                                    self.backends, fps
                                ));
                            }
                            // log::info!("FPS: {:.1}", fps);
                            self.frames = 0;
                            self.last_fps_instant = Some(Instant::now());
                        }
                    }
                }
                // Для анимации можно зациклить:
                if let Some(w) = &self.window {
                    w.request_redraw();
                }
            }
            _ => {}
        }
    }

    fn about_to_wait(&mut self, _event_loop: &ActiveEventLoop) {
        if let Some(w) = &self.window {
            let _size: PhysicalSize<u32> = w.inner_size();
            let _ = _size;
        }
    }
}
