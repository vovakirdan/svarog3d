//! Renderer: wgpu init + depth + rotating cube (B3).
//! wgpu = 0.26.x, winit = 0.30.x

use std::num::NonZeroU64;
use std::sync::Arc;
use std::time::Instant;

use bytemuck::{Pod, Zeroable};
use glam::{Mat4, Vec3};
use wgpu::{
    util::DeviceExt,
    BindGroup, BindGroupLayout, BindGroupLayoutDescriptor, BindGroupLayoutEntry, BindingType,
    BlendState, Buffer, BufferBindingType, BufferUsages, ColorTargetState, ColorWrites,
    CommandEncoderDescriptor, DepthBiasState, DepthStencilState, Device, DeviceDescriptor,
    Extent3d, Features, FragmentState,
    Instance, InstanceDescriptor, Limits, LoadOp, Operations, PipelineLayoutDescriptor,
    PowerPreference, PresentMode, Queue, RenderPassColorAttachment, RenderPassDescriptor,
    RenderPipeline, RenderPipelineDescriptor, ShaderModuleDescriptor, ShaderSource, ShaderStages,
    StoreOp, Surface, SurfaceConfiguration, SurfaceError, TextureDescriptor, TextureDimension,
    TextureFormat, TextureUsages, TextureView, TextureViewDescriptor, VertexBufferLayout,
    VertexState, VertexStepMode,
};

use winit::{dpi::PhysicalSize, window::Window};

/// Vertex: position + color.
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
        attributes: &wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x3],
    };
}

/// Camera UBO (16-byte aligned).
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct CameraUniform {
    mvp: [[f32; 4]; 4],
}

const DEPTH_FORMAT: TextureFormat = TextureFormat::Depth32Float;

pub struct GpuState {
    // Surface
    surface: Surface<'static>,
    #[allow(dead_code)]
    surface_format: TextureFormat,
    surface_config: SurfaceConfiguration,

    // Device/queue
    device: Device,
    queue: Queue,

    // Pipeline & geometry
    pipeline: RenderPipeline,
    vertex_buf: Buffer,
    index_buf: Buffer,
    index_count: u32,

    // Camera
    #[allow(dead_code)]
    camera_bgl: BindGroupLayout,
    camera_bg: BindGroup,
    camera_buf: Buffer,
    start: Instant,

    // Depth
    depth_view: TextureView,

    // Size cache
    width: u32,
    height: u32,
}

impl GpuState {
    /// Create GPU state bound to an Arc<Window>.
    pub async fn new(window: Arc<Window>) -> Self {
        let PhysicalSize { width, height } = window.inner_size();
        let width = width.max(1);
        let height = height.max(1);

        // Instance & surface
        let instance = Instance::new(&InstanceDescriptor::default());
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

        // Depth texture
        let depth_view = create_depth_view(&device, &surface_config);

        // ==== Shaders ====
        let shader_src: &str = include_str!("shaders/triangle.wgsl"); // переиспользуем файл, контент обновлён (см. ниже)
        let shader = device.create_shader_module(ShaderModuleDescriptor {
            label: Some("Basic WGSL"),
            source: ShaderSource::Wgsl(shader_src.into()),
        });

        // ==== Camera BGL/BG ====
        let camera_bgl = device.create_bind_group_layout(&BindGroupLayoutDescriptor {
            label: Some("Camera BGL"),
            entries: &[BindGroupLayoutEntry {
                binding: 0,
                visibility: ShaderStages::VERTEX,
                ty: BindingType::Buffer {
                    ty: BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: Some(
                        NonZeroU64::new(std::mem::size_of::<CameraUniform>() as u64).unwrap(),
                    ),
                },
                count: None,
            }],
        });

        // Initial camera (identity, заменим на реальное MVP в render()).
        let camera_init = CameraUniform {
            mvp: Mat4::IDENTITY.to_cols_array_2d(),
        };
        let camera_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Camera UBO"),
            contents: bytemuck::bytes_of(&camera_init),
            usage: BufferUsages::UNIFORM | BufferUsages::COPY_DST,
        });
        let camera_bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Camera BG"),
            layout: &camera_bgl,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: camera_buf.as_entire_binding(),
            }],
        });

        // ==== Pipeline ====
        let pipeline_layout = device.create_pipeline_layout(&PipelineLayoutDescriptor {
            label: Some("Basic PipelineLayout"),
            bind_group_layouts: &[&camera_bgl],
            push_constant_ranges: &[],
        });
        let pipeline = device.create_render_pipeline(&RenderPipelineDescriptor {
            label: Some("Cube Pipeline"),
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
            primitive: wgpu::PrimitiveState {
                cull_mode: Some(wgpu::Face::Back),
                ..Default::default()
            },
            depth_stencil: Some(DepthStencilState {
                format: DEPTH_FORMAT,
                depth_write_enabled: true,
                depth_compare: wgpu::CompareFunction::LessEqual,
                stencil: wgpu::StencilState::default(),
                bias: DepthBiasState::default(),
            }),
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
            cache: None,
        });

        // ==== Geometry: indexed cube ====
        let (vertices, indices) = cube_vertices();
        let vertex_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Cube VB"),
            contents: bytemuck::cast_slice(&vertices),
            usage: BufferUsages::VERTEX,
        });
        let index_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Cube IB"),
            contents: bytemuck::cast_slice(&indices),
            usage: BufferUsages::INDEX,
        });

        Self {
            surface,
            surface_format,
            surface_config,
            device,
            queue,
            pipeline,
            vertex_buf,
            index_buf,
            index_count: indices.len() as u32,
            camera_bgl,
            camera_bg,
            camera_buf,
            start: Instant::now(),
            depth_view,
            width,
            height,
        }
    }

    /// Resize: reconfigure surface & recreate depth view.
    pub fn resize(&mut self, width: u32, height: u32) {
        self.width = width.max(1);
        self.height = height.max(1);
        self.surface_config.width = self.width;
        self.surface_config.height = self.height;
        self.surface.configure(&self.device, &self.surface_config);
        self.depth_view = create_depth_view(&self.device, &self.surface_config);
    }

    /// Render one frame: update MVP + clear + draw cube.
    pub fn render(&mut self) -> Result<(), SurfaceError> {
        // --- update MVP
        let t = self.start.elapsed().as_secs_f32();
        let aspect = self.width as f32 / self.height as f32;
        let proj = Mat4::perspective_rh(60f32.to_radians(), aspect, 0.1, 100.0);
        let view = Mat4::look_at_rh(
            Vec3::new(0.0, 0.0, 4.0),
            Vec3::ZERO,
            Vec3::Y,
        );
        let model = Mat4::from_rotation_y(t) * Mat4::from_rotation_x(0.5 * t);
        let mvp = proj * view * model;
        let cam = CameraUniform {
            mvp: mvp.to_cols_array_2d(),
        };
        self.queue
            .write_buffer(&self.camera_buf, 0, bytemuck::bytes_of(&cam));

        // --- frame & pass
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
                    depth_slice: None, // required in 0.26
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
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &self.depth_view,
                    depth_ops: Some(Operations {
                        load: LoadOp::Clear(1.0),
                        store: StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                occlusion_query_set: None,
                timestamp_writes: None,
            });

            rpass.set_pipeline(&self.pipeline);
            rpass.set_bind_group(0, &self.camera_bg, &[]);
            rpass.set_vertex_buffer(0, self.vertex_buf.slice(..));
            rpass.set_index_buffer(self.index_buf.slice(..), wgpu::IndexFormat::Uint16);
            rpass.draw_indexed(0..self.index_count, 0, 0..1);
        }

        self.queue.submit(Some(encoder.finish()));
        frame.present();
        Ok(())
    }

    pub fn is_surface_lost(err: &SurfaceError) -> bool {
        matches!(err, SurfaceError::Lost | SurfaceError::Outdated)
    }

    pub fn recreate_surface(&mut self) {
        self.resize(self.width, self.height);
    }
}

/// Create a depth texture view matching the surface config.
fn create_depth_view(device: &Device, sc: &SurfaceConfiguration) -> TextureView {
    let tex = device.create_texture(&TextureDescriptor {
        label: Some("DepthTex"),
        size: Extent3d {
            width: sc.width.max(1),
            height: sc.height.max(1),
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: TextureDimension::D2,
        format: DEPTH_FORMAT,
        usage: TextureUsages::RENDER_ATTACHMENT,
        view_formats: &[],
    });
    tex.create_view(&TextureViewDescriptor::default())
}

/// Unit cube (positions + colors) and indices (CCW).
fn cube_vertices() -> (Vec<Vertex>, Vec<u16>) {
    let v = [
        // back z=-1
        Vertex { pos: [-1.0, -1.0, -1.0], color: [1.0, 0.0, 0.0] }, // 0
        Vertex { pos: [ 1.0, -1.0, -1.0], color: [0.0, 1.0, 0.0] }, // 1
        Vertex { pos: [ 1.0,  1.0, -1.0], color: [0.0, 0.0, 1.0] }, // 2
        Vertex { pos: [-1.0,  1.0, -1.0], color: [1.0, 1.0, 0.0] }, // 3
        // front z=+1
        Vertex { pos: [-1.0, -1.0,  1.0], color: [1.0, 0.0, 1.0] }, // 4
        Vertex { pos: [ 1.0, -1.0,  1.0], color: [0.0, 1.0, 1.0] }, // 5
        Vertex { pos: [ 1.0,  1.0,  1.0], color: [1.0, 1.0, 1.0] }, // 6
        Vertex { pos: [-1.0,  1.0,  1.0], color: [1.0, 0.5, 0.0] }, // 7
    ];
    let idx: [u16; 36] = [
        // front (+Z): 4,5,6, 4,6,7
        4, 5, 6, 4, 6, 7,
        // back (-Z): 0,2,1, 0,3,2
        0, 2, 1, 0, 3, 2,
        // top (+Y): 3,2,6, 3,6,7
        3, 2, 6, 3, 6, 7,
        // bottom (-Y): 0,5,1, 0,4,5
        0, 5, 1, 0, 4, 5,
        // left (-X): 0,3,7, 0,7,4
        0, 3, 7, 0, 7, 4,
        // right (+X): 1,2,6, 1,6,5
        1, 2, 6, 1, 6, 5,
    ];
    (v.to_vec(), idx.to_vec())
}
