//! Platform layer: window & event loop (winit 0.30.12).
//! Step B1 integration: create WGPU surface and clear screen.

use anyhow::Result;
use std::{env, sync::Arc};
use winit::{
    application::ApplicationHandler,
    dpi::{LogicalSize, PhysicalSize},
    event::WindowEvent,
    event_loop::{ActiveEventLoop, ControlFlow, EventLoop},
    window::{Window, WindowId},
};

/// Public entry: runs a window + renderer. Returns on close.
pub fn run_with_renderer() -> Result<()> {
    log::info!(
        "Env: DISPLAY={:?}, WAYLAND_DISPLAY={:?}",
        env::var("DISPLAY").ok(),
        env::var("WAYLAND_DISPLAY").ok()
    );

    let event_loop: EventLoop<()> = EventLoop::new().expect("Failed to create event loop");
    let mut app = App::default();
    event_loop
        .run_app(&mut app)
        .map_err(|e| anyhow::anyhow!(format!("{e:?}")))?;
    Ok(())
}

#[derive(Default)]
struct App {
    gpu: Option<renderer::GpuState>,
    window: Option<Arc<Window>>,
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        // Create window
        let attrs = Window::default_attributes()
            .with_title("Svarog3D")
            .with_inner_size(LogicalSize::new(1280.0_f64, 720.0_f64));
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
        let gpu = pollster::block_on(renderer::GpuState::new(window.clone()));

        // Low CPU at idle + первый кадр
        event_loop.set_control_flow(ControlFlow::Wait);
        window.request_redraw();

        self.gpu = Some(gpu);
        self.window = Some(window);
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
