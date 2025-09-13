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

/// Public entry: runs a window + renderer. Returns on close.
pub fn run_with_renderer(backends: wgpu::Backends, show_fps: bool, width: u32, height: u32) -> Result<()> {
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
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        // Create window
        let attrs = Window::default_attributes()
            .with_title(format!("Svarog3D (running with wgpu backend {:?})", self.backends))
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
        let gpu = pollster::block_on(renderer::GpuState::new(window.clone(), self.backends));

        // Low CPU at idle + первый кадр
        event_loop.set_control_flow(ControlFlow::Wait);
        window.request_redraw();

        self.gpu = Some(gpu);
        self.window = Some(window);
        
        self.frames = 0;
        self.last_fps_instant = Some(Instant::now());
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
                if let Some(gpu) = self.gpu.as_mut() {
                    match gpu.render() {
                        Ok(()) => {}
                        Err(e) => {
                            log::warn!("Render error: {e:?}");
                            if renderer::GpuState::is_surface_lost(&e) {
                                log::warn!("Surface lost/outdated. Recreating…");
                                gpu.recreate_surface();
                            }
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
                                w.set_title(&format!("Svarog3D (running with wgpu backend {:?}) {:.1} FPS", self.backends, fps));
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
