//! Platform layer: windowing & event loop.
//! Step A1: create a window and process basic events.
//!
//! Design goals:
//! - No busy loop: don't request redraws every tick yet.
//! - Proper handling of resize/scale/close.
//! - Clear log messages to help future debugging.

use anyhow::Result;
use std::env;
use winit::{
    application::ApplicationHandler,
    dpi::{LogicalSize, PhysicalSize},
    event::WindowEvent,
    event_loop::{ActiveEventLoop, ControlFlow, EventLoop},
    window::{Window, WindowId},
};

/// Public entry: runs a basic window loop (returns on close).
pub fn run_basic_window() -> Result<()> {
    log::info!(
        "Env: DISPLAY={:?}, WAYLAND_DISPLAY={:?}",
        env::var("DISPLAY").ok(),
        env::var("WAYLAND_DISPLAY").ok()
    );
    // New API: EventLoop::new() -> Result<...>
    let event_loop: EventLoop<()> = EventLoop::new().expect("Failed to create event loop");

    // Our app state implementing ApplicationHandler
    let mut app = App::default();

    // Run until exit()
    event_loop.run_app(&mut app).map_err(|e| anyhow::anyhow!(format!("{e:?}")))?;
    Ok(())
}

/// Simple app state for step A1.
#[derive(Default)]
struct App {
    window: Option<Window>,
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        // Create a window with attributes (replacement for WindowBuilder).
        let attrs = Window::default_attributes()
            .with_title("Svarog3D")
            .with_inner_size(LogicalSize::new(1280.0_f64, 720.0_f64));

        let window = event_loop
            .create_window(attrs)
            .expect("Failed to create window");

        log::info!(
            "Window created: {}x{} (physical pixels)",
            window.inner_size().width,
            window.inner_size().height
        );

        // Keep CPU low at idle: don't request redraws yet.
        event_loop.set_control_flow(ControlFlow::Wait);

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
                event_loop.exit();
            }
            WindowEvent::Resized(new_size) => {
                log::info!("Resized: {}x{}", new_size.width, new_size.height);
                // In next steps we'll reconfigure the surface here.
            }
            WindowEvent::ScaleFactorChanged {
                scale_factor,
                inner_size_writer: _,
            } => {
                log::info!(
                    "Scale factor changed: {:.3}",
                    scale_factor
                );
                // Future: surface reconfig. For now we only log.
            }
            _ => {}
        }
    }

    fn about_to_wait(&mut self, _event_loop: &ActiveEventLoop) {
        // No continuous redraws in A1. When renderer appears,
        // we'll call `window.request_redraw()` here as needed.
        if let Some(w) = &self.window {
            // placeholder: nothing; keep CPU low
            let _size: PhysicalSize<u32> = w.inner_size();
            let _ = _size; // silence unused for now
        }
    }
}
