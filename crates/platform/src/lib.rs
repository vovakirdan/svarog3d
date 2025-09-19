//! Platform layer: window & event loop (winit 0.30.12).
//! Step B1 integration: create WGPU surface and clear screen.

use anyhow::Result;
use std::{env, path::PathBuf, sync::Arc, time::Instant};
use wgpu;
use winit::{
    application::ApplicationHandler,
    dpi::{LogicalSize, PhysicalSize},
    event::WindowEvent,
    event_loop::{ActiveEventLoop, ControlFlow, EventLoop},
    window::{Window, WindowId},
};

use asset::obj;
use corelib::{
    camera::Camera,
    ecs::{MaterialId, MeshId, Renderable, World},
    transform::Transform,
    vec3,
};
use renderer::DrawInstance;

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

    // Config
    backends: wgpu::Backends,
    show_fps: bool,
    width: u32,
    height: u32,

    // FPS counters
    frames: u32,
    last_fps_instant: Option<Instant>,

    // Animation
    last_time: Option<Instant>,

    // Scene via ECS
    world: World,
    camera: Option<Camera>,

    // Reusable per-frame draw list to avoid allocs
    draw_list: Vec<DrawInstance>,

    // Window state
    is_minimized: bool,

    // Mesh handles
    cube_mesh: MeshId,
    suzanne_mesh: MeshId,
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
            vec3(0.0, 3.0, 8.0),
            vec3(0.0, 0.0, 0.0),
            corelib::Vec3::Y,
            60f32.to_radians(),
            0.1,
            200.0,
            aspect,
        );
        self.camera = Some(camera);
        gpu.set_camera(&camera);

        // Mesh handles
        let cube_mesh = gpu.cube_mesh_id();
        let suzanne_mesh = {
            let asset_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("..")
                .join("..")
                .join("assets")
                .join("models")
                .join("suzanne.obj");
            match obj::load_obj_from_path(&asset_path) {
                Ok(mesh) => {
                    log::info!(
                        "Loaded Suzanne OBJ ({} vertices, {} indices)",
                        mesh.vertices.len(),
                        mesh.indices.len()
                    );
                    gpu.upload_mesh("Suzanne", &mesh)
                }
                Err(err) => {
                    log::error!(
                        "Failed to load Suzanne OBJ from {}: {err:?}",
                        asset_path.display()
                    );
                    cube_mesh
                }
            }
        };
        self.cube_mesh = cube_mesh;
        self.suzanne_mesh = suzanne_mesh;

        // Build scene: grid of cubes + Suzanne centerpiece
        self.world = World::new();
        let grid_x = 10u32;
        let grid_y = 10u32;
        let spacing = 2.5_f32;
        let origin_offset_x = (grid_x as f32 - 1.0) * spacing * 0.5;
        let origin_offset_y = (grid_y as f32 - 1.0) * spacing * 0.5;
        let default_material = MaterialId::new(0);

        for gy in 0..grid_y {
            for gx in 0..grid_x {
                let x = gx as f32 * spacing - origin_offset_x;
                let z = gy as f32 * spacing - origin_offset_y;
                let t =
                    Transform::from_trs(vec3(x, 0.0, z), vec3(0.0, 0.0, 0.0), vec3(0.9, 0.9, 0.9));
                let r = Renderable::new(cube_mesh, default_material);
                let _ = self.world.spawn(t, Some(r));
            }
        }

        let suzanne_transform = Transform::from_trs(
            vec3(0.0, 1.5, 0.0),
            vec3(0.0, 0.0, 0.0),
            vec3(1.6, 1.6, 1.6),
        );
        let suzanne_renderable = Renderable::new(suzanne_mesh, default_material);
        let _ = self
            .world
            .spawn(suzanne_transform, Some(suzanne_renderable));

        // Control flow + first frame
        event_loop.set_control_flow(ControlFlow::Wait);
        window.request_redraw();

        self.gpu = Some(gpu);
        self.window = Some(window);
        self.frames = 0;
        self.last_fps_instant = Some(Instant::now());
        self.last_time = Some(Instant::now());
        self.draw_list = Vec::with_capacity((grid_x * grid_y + 1) as usize);
        self.is_minimized = false;
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
                let w = new_size.width.max(0);
                let h = new_size.height.max(0);
                log::info!("Resized: {}x{}", w, h);

                self.is_minimized = w == 0 || h == 0;

                if let Some(gpu) = self.gpu.as_mut() {
                    gpu.resize(w, h);
                }

                if !self.is_minimized {
                    // обновляем aspect только если не ноль
                    if let Some(cam) = self.camera {
                        let aspect = (w.max(1) as f32) / (h.max(1) as f32);
                        let cam2 = Camera { aspect, ..cam };
                        self.camera = Some(cam2);
                        if let Some(gpu) = self.gpu.as_mut() {
                            gpu.set_camera(&cam2);
                        }
                    }
                    if let Some(wnd) = &self.window {
                        wnd.request_redraw();
                    }
                }
            }
            WindowEvent::ScaleFactorChanged {
                scale_factor,
                inner_size_writer: _,
            } => {
                log::info!("Scale factor changed: {:.3}", scale_factor);
            }
            WindowEvent::RedrawRequested => {
                if self.is_minimized {
                    // не анимируем, не рендерим
                    return;
                }
                // dt
                let dt = if let Some(t0) = self.last_time {
                    let d = t0.elapsed().as_secs_f32().max(1e-6);
                    self.last_time = Some(Instant::now());
                    d
                } else {
                    self.last_time = Some(Instant::now());
                    0.0
                };

                // Animate: rotate all transforms a bit
                self.world.system_rotate_all(dt, [0.3, 0.6, 0.0]);

                // Build draw list WITHOUT allocation (reuse vector)
                self.draw_list.clear();
                for (t, r) in self.world.iter_renderables() {
                    self.draw_list
                        .push(DrawInstance::new(*t, r.mesh, r.material));
                }

                // Render
                if let Some(gpu) = self.gpu.as_mut() {
                    match gpu.render_models(&self.draw_list) {
                        Ok(()) => {}
                        Err(e) => {
                            log::warn!("Render error: {e:?}");
                            if renderer::GpuState::is_surface_lost(&e) {
                                log::warn!("Surface lost/outdated. Recreating…");
                                gpu.recreate_surface();
                            } else if matches!(e, wgpu::SurfaceError::OutOfMemory) {
                                log::error!("Out of GPU memory — exiting.");
                                let _ = self.gpu.take();
                                let _ = self.window.take();
                                event_loop.exit();
                                return;
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
