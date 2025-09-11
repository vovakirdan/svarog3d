//! Renderer facade: minimal WGPU init + clear screen (B1).
//! Target: stable "clear color", correct resize & surface loss recovery.

use std::sync::Arc;
use bytemuck::{Pod, Zeroable};
use wgpu::{
    util::DeviceExt,
    BlendState, ColorTargetState, ColorWrites, CommandEncoderDescriptor, Device, DeviceDescriptor,
    Features, FragmentState, Instance, InstanceDescriptor, Limits, LoadOp, Operations,
    PipelineLayoutDescriptor, PowerPreference, PresentMode, Queue, RenderPassColorAttachment,
    RenderPassDescriptor, RenderPipeline, RenderPipelineDescriptor, ShaderModuleDescriptor,
    ShaderSource, StoreOp, Surface, SurfaceConfiguration, SurfaceError, TextureFormat,
    TextureUsages, VertexBufferLayout, VertexState, VertexStepMode,
};
use winit::{dpi::PhysicalSize, window::Window};

/// Interleaved vertex with position + color.
/// Type is POD to upload to GPU safely.
#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct Vertex {
    pub pos: [f32; 3],
    pub color: [f32; 3],
}

impl Vertex {
    pub const LAYOUT: VertexBufferLayout<'static> = VertexBufferLayout {
        array_stride: std::mem::size_of::<Vertex>() as u64,
        step_mode: VertexStepMode::Vertex,
        attributes: &wgpu::vertex_attr_array![
            0 => Float32x3, // @location(0) position
            1 => Float32x3, // @location(1) color
        ],
    };
}

pub struct GpuState {
    surface: Surface<'static>,
    #[allow(dead_code)]
    surface_format: TextureFormat,
    surface_config: SurfaceConfiguration,

    device: Device,
    queue: Queue,

    // Pipeline + geometry (triangle)
    pipeline: RenderPipeline,
    vertex_buf: wgpu::Buffer,
    vertex_count: u32,

    // Cached size
    width: u32,
    height: u32,
}

impl GpuState {
    /// Create GPU state bound to a window surface (via Arc<Window> to satisfy lifetime).
    pub async fn new(window: Arc<Window>) -> Self {
        let PhysicalSize { width, height } = window.inner_size();
        let width = width.max(1);
        let height = height.max(1);

        // Instance & surface
        let instance = Instance::new(&InstanceDescriptor::default());
        // Передаём КЛОН Arc<Window> — паттерн для корректного лайфтайма Surface<'window>.
        let surface: Surface<'static> = instance
            .create_surface(window.clone())
            .expect("create_surface failed");

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
                    required_limits: Limits::downlevel_webgl2_defaults()
                        .using_resolution(adapter.limits()),
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

        // ======== B2: pipeline + vertex buffer ========

        // Shader (WGSL embedded from file within the crate)
        let shader_src: &str = include_str!("shaders/triangle.wgsl");
        let shader = device.create_shader_module(ShaderModuleDescriptor {
            label: Some("Triangle WGSL"),
            source: ShaderSource::Wgsl(shader_src.into()),
        });

        // Pipeline layout: no bind groups yet
        let pipeline_layout = device.create_pipeline_layout(&PipelineLayoutDescriptor {
            label: Some("Triangle PipelineLayout"),
            bind_group_layouts: &[],
            push_constant_ranges: &[],
        });

        let pipeline = device.create_render_pipeline(&RenderPipelineDescriptor {
            label: Some("Triangle Pipeline"),
            layout: Some(&pipeline_layout),
            vertex: VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                buffers: &[Vertex::LAYOUT],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            },
            fragment: Some(FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                targets: &[Some(ColorTargetState {
                    format: surface_format,
                    blend: Some(BlendState::REPLACE),
                    write_mask: ColorWrites::ALL,
                })],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
            cache: None,
        });

        // Simple triangle
        let vertices: [Vertex; 3] = [
            Vertex { pos: [-0.8, -0.6, 0.0], color: [1.0, 0.0, 0.0] },
            Vertex { pos: [ 0.8, -0.6, 0.0], color: [0.0, 1.0, 0.0] },
            Vertex { pos: [ 0.0,  0.6, 0.0], color: [0.0, 0.0, 1.0] },
        ];
        let vertex_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Triangle VertexBuffer"),
            contents: bytemuck::cast_slice(&vertices),
            usage: wgpu::BufferUsages::VERTEX,
        });

        Self {
            surface,
            surface_format,
            surface_config,
            device,
            queue,
            pipeline,
            vertex_buf,
            vertex_count: vertices.len() as u32,
            width,
            height,
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

    /// Render one frame: clear + draw triangle.
    pub fn render(&mut self) -> Result<(), SurfaceError> {
        let frame = self.surface.get_current_texture()?;
        let view = frame.texture.create_view(&Default::default());

        let mut encoder = self
            .device
            .create_command_encoder(&CommandEncoderDescriptor {
                label: Some("MainEncoder"),
            });

        {
            let mut rpass = encoder.begin_render_pass(&RenderPassDescriptor {
                label: Some("MainPass"),
                color_attachments: &[Some(RenderPassColorAttachment {
                    view: &view,
                    depth_slice: None, // required in wgpu 0.26
                    resolve_target: None,
                    ops: Operations {
                        load: LoadOp::Clear(wgpu::Color { r: 0.05, g: 0.05, b: 0.08, a: 1.0 }),
                        store: StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                occlusion_query_set: None,
                timestamp_writes: None,
            });

            rpass.set_pipeline(&self.pipeline);
            rpass.set_vertex_buffer(0, self.vertex_buf.slice(..));
            rpass.draw(0..self.vertex_count, 0..1);
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
