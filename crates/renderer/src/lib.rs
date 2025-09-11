//! Renderer facade: minimal WGPU init + clear screen (B1).
//! Target: stable "clear color", correct resize & surface loss recovery.

use std::sync::Arc;
use wgpu::{
    CommandEncoderDescriptor, Device, DeviceDescriptor, Features, Instance, InstanceDescriptor,
    Limits, LoadOp, Operations, PowerPreference, PresentMode, Queue, RenderPassColorAttachment,
    RenderPassDescriptor, StoreOp, Surface, SurfaceConfiguration, SurfaceError, TextureFormat,
    TextureUsages,
};
use winit::{dpi::PhysicalSize, window::Window};

pub struct GpuState {
    // Surface & its configuration
    surface: Surface<'static>,
    surface_format: TextureFormat,
    surface_config: SurfaceConfiguration,

    // Core device & queue
    device: Device,
    queue: Queue,

    // Cached size
    width: u32,
    height: u32,

    // Keep window alive
    _window: Arc<Window>,
}

unsafe impl Send for GpuState {}
unsafe impl Sync for GpuState {}

impl GpuState {
    /// Create GPU state bound to a window surface.
    pub async fn new(window: Window) -> Self {
        let window = Arc::new(window);
        let PhysicalSize { width, height } = window.inner_size();
        let width = width.max(1);
        let height = height.max(1);

        // Instance & surface
        let instance = Instance::new(&InstanceDescriptor::default());
        let surface = instance.create_surface(&window).expect("create_surface failed");
        // SAFETY: We extend the lifetime to 'static because we store Arc<Window>
        // This is safe because the Arc<Window> is stored in the struct
        let surface = unsafe { std::mem::transmute::<Surface<'_>, Surface<'static>>(surface) };

        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: PowerPreference::HighPerformance,
                compatible_surface: Some(&surface),
                force_fallback_adapter: false,
            })
            .await
            .expect("No suitable GPU adapter");

        // Device & queue
        let (device, queue) = adapter
            .request_device(
                &DeviceDescriptor {
                    label: Some("Svarog3D Device"),
                    required_features: Features::empty(),
                    required_limits: Limits::downlevel_webgl2_defaults().using_resolution(adapter.limits()),
                    memory_hints: Default::default(),
                    trace: Default::default(),
                },
            )
            .await
            .expect("request_device failed");

        // Surface format (prefer sRGB)
        let caps = surface.get_capabilities(&adapter);
        let surface_format = caps
            .formats
            .iter()
            .copied()
            .find(|f| f.is_srgb())
            .unwrap_or(caps.formats[0]);

        // Configure surface
        let surface_config = SurfaceConfiguration {
            usage: TextureUsages::RENDER_ATTACHMENT,
            format: surface_format,
            width,
            height,
            present_mode: PresentMode::AutoVsync,
            alpha_mode: caps.alpha_modes[0],
            view_formats: vec![],
            desired_maximum_frame_latency: 2,
        };
        surface.configure(&device, &surface_config);

        Self {
            surface,
            surface_format,
            surface_config,
            device,
            queue,
            width,
            height,
            _window: window,
        }
    }

    /// Resize surface on window events.
    pub fn resize(&mut self, width: u32, height: u32) {
        self.width = width.max(1);
        self.height = height.max(1);
        self.surface_config.width = self.width;
        self.surface_config.height = self.height;
        self.surface.configure(&self.device, &self.surface_config);
    }

    /// Render one frame with clear color.
    pub fn render(&mut self) -> Result<(), SurfaceError> {
        let frame = self.surface.get_current_texture()?;
        let view = frame.texture.create_view(&Default::default());

        let mut encoder = self
            .device
            .create_command_encoder(&CommandEncoderDescriptor {
                label: Some("MainEncoder"),
            });

        {
            let _rpass = encoder.begin_render_pass(&RenderPassDescriptor {
                label: Some("ClearPass"),
                color_attachments: &[Some(RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: Operations {
                        load: LoadOp::Clear(wgpu::Color {
                            r: 0.05,
                            g: 0.05,
                            b: 0.08,
                            a: 1.0,
                        }),
                        store: StoreOp::Store,
                    },
                    depth_slice: None,
                })],
                depth_stencil_attachment: None,
                occlusion_query_set: None,
                timestamp_writes: None,
            });
            // nothing to draw for B1
            // rpass will be dropped automatically
        }

        self.queue.submit(Some(encoder.finish()));
        frame.present();
        Ok(())
    }

    /// Whether the error requires surface re-create/reconfigure.
    pub fn is_surface_lost(err: &SurfaceError) -> bool {
        matches!(err, SurfaceError::Lost | SurfaceError::Outdated)
    }

    /// Reconfigure the surface with the current size.
    pub fn recreate_surface(&mut self) {
        self.resize(self.width, self.height);
    }
}
