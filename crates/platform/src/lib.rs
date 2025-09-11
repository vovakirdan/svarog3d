//! Platform layer: windowing & event loop.
//! Step A1: create a window and process basic events.
//!
//! Design goals:
//! - No busy loop: don't request redraws every tick yet.
//! - Proper handling of resize/scale/close.
//! - Clear log messages to help future debugging.

use anyhow::Result;
use winit::{
    dpi::PhysicalSize,
    event::{Event, WindowEvent},
    event_loop::{ControlFlow, EventLoop},
    window::WindowBuilder,
};

/// Run a basic window with minimal event handling.
/// Returns when the window is closed.
pub fn run_basic_window() -> Result<()> {
    // Create event loop (new API with Result return).
    let event_loop: EventLoop<()> = EventLoop::new().expect("Failed to create event loop");

    let window = WindowBuilder::new()
        .with_title("Svarog3D")
        .with_inner_size(PhysicalSize::new(1280_u32, 720_u32))
        .build(&event_loop)
        .expect("Failed to create window");

    log::info!(
        "Window created: {}x{}",
        window.inner_size().width,
        window.inner_size().height
    );

    // No continuous redraws for A1; keep CPU low at idle.
    event_loop
        .run(move |event, window_target| {
            match event {
                Event::WindowEvent { event, .. } => {
                    match event {
                        WindowEvent::CloseRequested => {
                            log::info!("Close requested. Exiting event loop.");
                            window_target.exit();
                        }
                        WindowEvent::Resized(new_size) => {
                            log::info!("Resized: {}x{}", new_size.width, new_size.height);
                            // We'll pass this to the renderer in next steps.
                        }
                        WindowEvent::ScaleFactorChanged { scale_factor, new_inner_size } => {
                            log::info!(
                                "Scale factor changed: {:.3}, new_inner_size={}x{}",
                                scale_factor,
                                new_inner_size.width,
                                new_inner_size.height
                            );
                            // In future we will reconfigure the surface here.
                        }
                        _ => {}
                    }
                }
                Event::AboutToWait => {
                    // Place to request redraw when we have a renderer:
                    // window.request_redraw();
                }
                _ => {}
            }
        })
        .map_err(|e| anyhow::anyhow!("Event loop error: {e:?}"))?;

    Ok(())
}
