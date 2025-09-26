//! Renderer: wgpu init + depth + cube.
//! D1: camera/transform from `core` with setters.
//! G2: Mini-FrameGraph system for explicit render passes.

pub mod framegraph;

use std::num::NonZeroU64;
use std::sync::Arc;
use std::time::Instant;

use crate::framegraph::{FrameGraph, ResourceDesc};

use asset::{
    mesh::{MeshData, MeshVertex},
    texture::TextureData,
};
use bytemuck::{Pod, Zeroable};
use corelib::{
    Mat4, Vec3,
    camera::Camera,
    ecs::{MaterialId, MeshId, TextureId},
    transform::Transform,
};
use wgpu::{
    BindGroup, BindGroupLayoutDescriptor, BindGroupLayoutEntry, BindingType,
    BlendState, Buffer, BufferBindingType, BufferUsages, ColorTargetState, ColorWrites,
    CommandEncoderDescriptor, DepthBiasState, DepthStencilState, Device, Extent3d, FragmentState,
    Instance, InstanceDescriptor, LoadOp, Operations, PipelineLayoutDescriptor, PowerPreference,
    PresentMode, Queue, RenderPassColorAttachment, RenderPassDescriptor, RenderPipeline,
    RenderPipelineDescriptor, Sampler, SamplerDescriptor, ShaderModuleDescriptor, ShaderSource,
    ShaderStages, StoreOp, Surface, SurfaceConfiguration, SurfaceError, TextureDescriptor,
    TextureDimension, TextureFormat, TextureSampleType, TextureUsages, TextureView, TextureViewDescriptor,
    TextureViewDimension, VertexBufferLayout, VertexState, VertexStepMode, util::DeviceExt,
};
use winit::{dpi::PhysicalSize, window::Window};

/// Vertex: position + normal + uv.
#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct Vertex {
    pub position: [f32; 3],
    pub normal: [f32; 3],
    pub uv: [f32; 2],
}

impl Vertex {
    pub const LAYOUT: VertexBufferLayout<'static> = VertexBufferLayout {
        array_stride: std::mem::size_of::<Vertex>() as u64,
        step_mode: VertexStepMode::Vertex,
        attributes: &wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x3, 2 => Float32x2],
    };
}

impl From<MeshVertex> for Vertex {
    fn from(v: MeshVertex) -> Self {
        Self {
            position: v.position,
            normal: v.normal,
            uv: v.uv,
        }
    }
}

/// CPU draw command passed from ECS/scene to renderer.
#[derive(Clone, Copy, Debug)]
pub struct DrawInstance {
    pub transform: Transform,
    pub mesh: MeshId,
    pub material: MaterialId,
    pub texture: TextureId,
}

impl DrawInstance {
    pub fn new(transform: Transform, mesh: MeshId, material: MaterialId, texture: TextureId) -> Self {
        Self {
            transform,
            mesh,
            material,
            texture,
        }
    }

    pub fn new_with_default_texture(transform: Transform, mesh: MeshId, material: MaterialId) -> Self {
        Self {
            transform,
            mesh,
            material,
            texture: TextureId::INVALID, // Will be replaced with default texture
        }
    }
}

/// Per-instance model matrix as 4 **columns** (WGSL mat4x4 expects columns).
#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct InstanceRaw {
    pub col0: [f32; 4],
    pub col1: [f32; 4],
    pub col2: [f32; 4],
    pub col3: [f32; 4],
}

impl InstanceRaw {
    pub const LAYOUT: VertexBufferLayout<'static> = VertexBufferLayout {
        array_stride: std::mem::size_of::<InstanceRaw>() as u64,
        step_mode: VertexStepMode::Instance,
        attributes: &wgpu::vertex_attr_array![
            3 => Float32x4, // col0
            4 => Float32x4, // col1
            5 => Float32x4, // col2
            6 => Float32x4, // col3
        ],
    };

    pub fn from_model(m: Mat4) -> Self {
        // glam::Mat4 хранится в column-major; берём колонки напрямую.
        let c = m.to_cols_array();
        Self {
            col0: [c[0], c[1], c[2], c[3]],
            col1: [c[4], c[5], c[6], c[7]],
            col2: [c[8], c[9], c[10], c[11]],
            col3: [c[12], c[13], c[14], c[15]],
        }
    }
}

struct MeshGpu {
    vertex_buf: Buffer,
    index_buf: Buffer,
    index_count: u32,
    index_format: wgpu::IndexFormat,
}

struct MeshStore {
    meshes: Vec<MeshGpu>,
}

impl MeshStore {
    fn new() -> Self {
        Self { meshes: Vec::new() }
    }

    fn add_mesh(&mut self, device: &Device, label: &str, mesh: &MeshData) -> MeshId {
        assert!(mesh.is_valid(), "Mesh must contain vertices and indices");

        let vertices: Vec<Vertex> = mesh.vertices.iter().copied().map(Vertex::from).collect();
        let indices: &[u32] = &mesh.indices;

        let vertex_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some(&format!("{label} VB")),
            contents: bytemuck::cast_slice(&vertices),
            usage: BufferUsages::VERTEX,
        });

        let index_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some(&format!("{label} IB")),
            contents: bytemuck::cast_slice(indices),
            usage: BufferUsages::INDEX,
        });

        let index_count = u32::try_from(indices.len()).expect("index count exceeds u32");

        let id_raw = u32::try_from(self.meshes.len()).expect("Too many meshes");
        let id = MeshId::new(id_raw);
        self.meshes.push(MeshGpu {
            vertex_buf,
            index_buf,
            index_count,
            index_format: wgpu::IndexFormat::Uint32,
        });

        id
    }

    fn get(&self, id: MeshId) -> Option<&MeshGpu> {
        self.meshes.get(id.0 as usize)
    }
}

struct TextureGpu {
    view: TextureView,
    sampler: Sampler,
}

struct TextureStore {
    textures: Vec<TextureGpu>,
}

impl TextureStore {
    fn new() -> Self {
        Self { textures: Vec::new() }
    }

    fn add_texture(&mut self, device: &Device, queue: &Queue, label: &str, data: &TextureData) -> TextureId {
        assert!(data.is_valid(), "Texture data must be valid");

        let texture = device.create_texture_with_data(
            queue,
            &wgpu::TextureDescriptor {
                label: Some(&format!("{label} Texture")),
                size: Extent3d {
                    width: data.width,
                    height: data.height,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: TextureDimension::D2,
                format: TextureFormat::Rgba8UnormSrgb,
                usage: TextureUsages::TEXTURE_BINDING | TextureUsages::COPY_DST,
                view_formats: &[],
            },
            wgpu::util::TextureDataOrder::LayerMajor,
            &data.data,
        );

        let view = texture.create_view(&TextureViewDescriptor {
            label: Some(&format!("{label} TextureView")),
            format: None,
            dimension: Some(TextureViewDimension::D2),
            aspect: wgpu::TextureAspect::All,
            base_mip_level: 0,
            mip_level_count: None,
            base_array_layer: 0,
            array_layer_count: None,
            usage: None,
        });

        let sampler = device.create_sampler(&SamplerDescriptor {
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::FilterMode::Nearest,
            ..Default::default()
        });

        let id_raw = u32::try_from(self.textures.len()).expect("Too many textures");
        let id = TextureId::new(id_raw);
        self.textures.push(TextureGpu {
            view,
            sampler,
        });

        id
    }

    fn get(&self, id: TextureId) -> Option<&TextureGpu> {
        self.textures.get(id.0 as usize)
    }
}

/// Sorting key for draw commands to minimize state changes.
/// Sort order: PSO (Pipeline) -> Material -> Texture -> Mesh -> Instance
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct DrawKey {
    pso_id: u32,       // Pipeline state object (currently always 0)
    material: MaterialId,
    texture: TextureId,
    mesh: MeshId,
}

#[derive(Clone, Copy)]
struct InstanceEntry {
    key: DrawKey,
    instance: InstanceRaw,
}

struct DrawBatch {
    key: DrawKey,
    start: usize,
    count: usize,
}

/// Camera UBO (16-byte aligned).
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct CameraUniform {
    mvp: [[f32; 4]; 4],
}

/// Material properties (16-byte aligned).
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub struct MaterialUniform {
    pub base_color: [f32; 4],    // RGBA albedo
    pub metallic_roughness: [f32; 2], // metallic, roughness
    pub _padding: [f32; 2],      // Pad to 16 bytes
}

impl Default for MaterialUniform {
    fn default() -> Self {
        Self {
            base_color: [0.8, 0.8, 0.9, 1.0],
            metallic_roughness: [0.0, 0.5],
            _padding: [0.0, 0.0],
        }
    }
}

/// Lighting parameters (16-byte aligned).
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub struct LightingUniform {
    pub light_direction: [f32; 3], // Directional light dir
    pub light_intensity: f32,      // Light intensity
    pub light_color: [f32; 3],     // Light color (RGB)
    pub ambient_intensity: f32,    // Ambient light intensity
}

impl Default for LightingUniform {
    fn default() -> Self {
        Self {
            light_direction: [-0.5, 1.0, -0.3],
            light_intensity: 1.0,
            light_color: [1.0, 1.0, 1.0],
            ambient_intensity: 0.3,
        }
    }
}

const DEPTH_FORMAT: TextureFormat = TextureFormat::Depth24Plus;

/// Converts OpenGL clip space (z in [-1,1]) to WGPU/D3D clip (z in [0,1]).
const OPENGL_TO_WGPU: Mat4 = Mat4::from_cols_array(&[
    1.0, 0.0, 0.0, 0.0, //
    0.0, 1.0, 0.0, 0.0, //
    0.0, 0.0, 0.5, 0.0, //
    0.0, 0.0, 0.5, 1.0,
]);

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
    mesh_store: MeshStore,
    cube_mesh_id: MeshId,
    texture_store: TextureStore,
    default_texture_id: TextureId,
    instance_buf: Buffer,
    instance_capacity: u32,
    instance_count: u32,
    instance_entries: Vec<InstanceEntry>,
    draw_batches: Vec<DrawBatch>,
    instance_data: Vec<InstanceRaw>,

    // Camera / model state (from core)
    camera: Camera,
    model: Transform,

    // Bindings
    camera_bg: BindGroup,
    camera_buf: Buffer,
    material_bg: BindGroup,
    material_buf: Buffer,
    lighting_buf: Buffer,
    texture_bg: BindGroup,

    // Depth
    depth_view: TextureView,

    // G2: FrameGraph system
    framegraph: FrameGraph,

    // Time (only for FPS in platform; left here in case we need timers)
    #[allow(dead_code)]
    start: Instant,

    // Size cache
    width: u32,
    height: u32,
}

impl GpuState {
    /// Create GPU state bound to an Arc<Window>. Backends are selectable (A2).
    pub async fn new(window: Arc<Window>, backends: wgpu::Backends) -> Self {
        let PhysicalSize { width, height } = window.inner_size();
        let width = width.max(1);
        let height = height.max(1);

        // Instance & surface with requested backends
        let mut instance = Instance::new(&InstanceDescriptor {
            backends,
            ..Default::default()
        });
        let mut surface_opt: Option<Surface<'static>> =
            instance.create_surface(window.clone()).ok();

        let adapter = match surface_opt.as_ref() {
            Some(surf_ref) => {
                match instance
                    .request_adapter(&wgpu::RequestAdapterOptions {
                        power_preference: PowerPreference::HighPerformance,
                        compatible_surface: Some(surf_ref),
                        force_fallback_adapter: false,
                    })
                    .await
                {
                    Ok(a) => a,
                    Err(_) => {
                        log::warn!(
                            "No adapter for requested backends {:?}. Falling back to Backends::all()",
                            backends
                        );
                        instance = Instance::new(&InstanceDescriptor {
                            backends: wgpu::Backends::all(),
                            ..Default::default()
                        });
                        surface_opt = instance.create_surface(window.clone()).ok();
                        let surf_ref = surface_opt
                            .as_ref()
                            .expect("create_surface failed (fallback)");
                        instance
                            .request_adapter(&wgpu::RequestAdapterOptions {
                                power_preference: PowerPreference::HighPerformance,
                                compatible_surface: Some(surf_ref),
                                force_fallback_adapter: false,
                            })
                            .await
                            .expect("No suitable GPU adapter even on fallback")
                    }
                }
            }
            None => {
                log::warn!(
                    "Surface creation failed on requested backends {:?}. Falling back to Backends::all()",
                    backends
                );
                instance = Instance::new(&InstanceDescriptor {
                    backends: wgpu::Backends::all(),
                    ..Default::default()
                });
                surface_opt = instance.create_surface(window.clone()).ok();
                let surf_ref = surface_opt
                    .as_ref()
                    .expect("create_surface failed (fallback)");
                instance
                    .request_adapter(&wgpu::RequestAdapterOptions {
                        power_preference: PowerPreference::HighPerformance,
                        compatible_surface: Some(surf_ref),
                        force_fallback_adapter: false,
                    })
                    .await
                    .expect("No suitable GPU adapter even on fallback")
            }
        };

        let surface = surface_opt.expect("surface is None");

        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor {
                label: Some("Svarog3D Device"),
                required_features: wgpu::Features::empty(),
                required_limits: wgpu::Limits::downlevel_webgl2_defaults()
                    .using_resolution(adapter.limits()),
                memory_hints: Default::default(),
                trace: Default::default(),
            })
            .await
            .expect("request_device failed");

        // Surface format
        let caps = surface.get_capabilities(&adapter);
        let surface_format = caps
            .formats
            .iter()
            .copied()
            .find(|f| f.is_srgb())
            .unwrap_or(caps.formats[0]);

        // Configure surface
        let surface_config = wgpu::SurfaceConfiguration {
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

        // Shaders
        let shader_src: &str = include_str!("shaders/triangle.wgsl");
        let shader = device.create_shader_module(ShaderModuleDescriptor {
            label: Some("Basic WGSL"),
            source: ShaderSource::Wgsl(shader_src.into()),
        });

        // Camera BGL/BG
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

        let camera_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Camera UBO"),
            contents: bytemuck::bytes_of(&CameraUniform {
                mvp: Mat4::IDENTITY.to_cols_array_2d(),
            }),
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

        // Material/Lighting BGL/BG
        let material_bgl = device.create_bind_group_layout(&BindGroupLayoutDescriptor {
            label: Some("Material BGL"),
            entries: &[
                BindGroupLayoutEntry {
                    binding: 0,
                    visibility: ShaderStages::FRAGMENT,
                    ty: BindingType::Buffer {
                        ty: BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: Some(
                            NonZeroU64::new(std::mem::size_of::<MaterialUniform>() as u64).unwrap(),
                        ),
                    },
                    count: None,
                },
                BindGroupLayoutEntry {
                    binding: 1,
                    visibility: ShaderStages::FRAGMENT,
                    ty: BindingType::Buffer {
                        ty: BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: Some(
                            NonZeroU64::new(std::mem::size_of::<LightingUniform>() as u64).unwrap(),
                        ),
                    },
                    count: None,
                },
            ],
        });

        let material_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Material UBO"),
            contents: bytemuck::bytes_of(&MaterialUniform::default()),
            usage: BufferUsages::UNIFORM | BufferUsages::COPY_DST,
        });

        let lighting_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Lighting UBO"),
            contents: bytemuck::bytes_of(&LightingUniform::default()),
            usage: BufferUsages::UNIFORM | BufferUsages::COPY_DST,
        });

        let material_bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Material BG"),
            layout: &material_bgl,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: material_buf.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: lighting_buf.as_entire_binding(),
                },
            ],
        });

        // Texture store with default texture
        let mut texture_store = TextureStore::new();
        let default_texture_data = TextureData::create_test_texture(64);
        let default_texture_id = texture_store.add_texture(&device, &queue, "Default", &default_texture_data);

        // Texture BGL/BG
        let texture_bgl = device.create_bind_group_layout(&BindGroupLayoutDescriptor {
            label: Some("Texture BGL"),
            entries: &[
                BindGroupLayoutEntry {
                    binding: 0,
                    visibility: ShaderStages::FRAGMENT,
                    ty: BindingType::Texture {
                        multisampled: false,
                        view_dimension: TextureViewDimension::D2,
                        sample_type: TextureSampleType::Float { filterable: true },
                    },
                    count: None,
                },
                BindGroupLayoutEntry {
                    binding: 1,
                    visibility: ShaderStages::FRAGMENT,
                    ty: BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });

        let default_texture_gpu = texture_store.get(default_texture_id).expect("Default texture should exist");
        let texture_bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Texture BG"),
            layout: &texture_bgl,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&default_texture_gpu.view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&default_texture_gpu.sampler),
                },
            ],
        });

        let instance_capacity = 0;
        let instance_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Instance Buffer"),
            size: 64, // минимальный заглушечный размер (64 байта), всё равно перезальём позже
            usage: BufferUsages::VERTEX | BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        // Pipeline
        let pipeline_layout = device.create_pipeline_layout(&PipelineLayoutDescriptor {
            label: Some("Basic PipelineLayout"),
            bind_group_layouts: &[&camera_bgl, &material_bgl, &texture_bgl],
            push_constant_ranges: &[],
        });
        let pipeline = device.create_render_pipeline(&RenderPipelineDescriptor {
            label: Some("Cube Pipeline"),
            layout: Some(&pipeline_layout),
            vertex: VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                buffers: &[Vertex::LAYOUT, InstanceRaw::LAYOUT],
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
            // На WSL/GLES — без culling для стабильности
            primitive: wgpu::PrimitiveState {
                cull_mode: None,
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

        // Geometry: store meshes (start with built-in cube)
        let mut mesh_store = MeshStore::new();
        let cube_mesh = cube_mesh_data();
        let cube_mesh_id = mesh_store.add_mesh(&device, "Cube", &cube_mesh);

        // Default camera/model (будут заданы снаружи)
        let camera = Camera::new_perspective(
            Vec3::new(0.0, 0.0, 4.0),
            Vec3::new(0.0, 0.0, 0.0),
            Vec3::Y,
            60f32.to_radians(),
            0.1,
            100.0,
            width as f32 / height as f32,
        );
        let model = Transform::default();

        Self {
            surface,
            surface_format,
            surface_config,
            device,
            queue,
            pipeline,
            mesh_store,
            cube_mesh_id,
            texture_store,
            default_texture_id,
            camera_bg,
            camera_buf,
            material_bg,
            material_buf,
            lighting_buf,
            texture_bg,
            depth_view,
            framegraph: FrameGraph::new(),
            start: Instant::now(),
            camera,
            model,
            width,
            height,
            instance_buf,
            instance_capacity,
            instance_count: 0,
            instance_entries: Vec::new(),
            draw_batches: Vec::new(),
            instance_data: Vec::new(),
        }
    }

    /// External API: set camera from core (copied into internal state).
    pub fn set_camera(&mut self, camera: &Camera) {
        self.camera = *camera;
    }

    /// External API: set current model transform.
    pub fn set_model(&mut self, model: &Transform) {
        self.model = *model;
    }

    /// Default cube mesh identifier.
    pub fn cube_mesh_id(&self) -> MeshId {
        self.cube_mesh_id
    }

    /// Upload mesh data to the GPU mesh store and receive a [`MeshId`].
    pub fn upload_mesh(&mut self, label: &str, mesh: &MeshData) -> MeshId {
        self.mesh_store.add_mesh(&self.device, label, mesh)
    }

    /// Upload texture data to the GPU texture store and receive a [`TextureId`].
    pub fn upload_texture(&mut self, label: &str, texture: &TextureData) -> TextureId {
        self.texture_store.add_texture(&self.device, &self.queue, label, texture)
    }

    /// Get the default texture ID.
    pub fn default_texture_id(&self) -> TextureId {
        self.default_texture_id
    }

    /// Update material properties.
    pub fn update_material(&self, material: &MaterialUniform) {
        self.queue.write_buffer(
            &self.material_buf,
            0,
            bytemuck::bytes_of(material),
        );
    }

    /// Update lighting properties.
    pub fn update_lighting(&self, lighting: &LightingUniform) {
        self.queue.write_buffer(
            &self.lighting_buf,
            0,
            bytemuck::bytes_of(lighting),
        );
    }

    /// G2: Setup a simple framegraph example with post-processing.
    /// This demonstrates how easy it is to add post-effects without touching existing code.
    pub fn setup_framegraph_example(&mut self) {
        use crate::framegraph::{PassDesc, ResourceUsage};

        // Clear existing framegraph
        self.framegraph = FrameGraph::new();

        // G2: Create intermediate render target for main scene
        let scene_target = self.framegraph.add_resource(ResourceDesc {
            label: "SceneTarget".to_string(),
            width: self.width,
            height: self.height,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
        });

        // G2: Create main scene render pass
        let _main_pass = self.framegraph.add_pass(
            PassDesc {
                label: "MainScenePass".to_string(),
                inputs: vec![], // No inputs for the main pass
                outputs: vec![(scene_target, ResourceUsage::Write)],
            },
            Box::new(|_render_pass, _resources| {
                // In a real implementation, this would render the main scene
                // For now, just a placeholder to show the concept
                log::info!("G2: Executing main scene pass");
            }),
        );

        // G2: Create post-processing pass (gamma correction example)
        let _post_pass = self.framegraph.add_pass(
            PassDesc {
                label: "PostProcessPass".to_string(),
                inputs: vec![(scene_target, ResourceUsage::Read)],
                outputs: vec![], // Output to swapchain
            },
            Box::new(|_render_pass, resources| {
                // In a real implementation, this would apply post-processing
                log::info!("G2: Executing post-processing pass with {} resources", resources.len());
            }),
        );

        // G2: Compile the framegraph
        self.framegraph.compile(&self.device);

        log::info!("G2: FrameGraph setup complete - main pass -> post pass");
    }

    /// Resize: reconfigure surface & recreate depth view.
    pub fn resize(&mut self, width: u32, height: u32) {
        self.width = width.max(0);
        self.height = height.max(0);

        if self.width == 0 || self.height == 0 {
            // окно свернуто/минимизировано — не трогаем surface
            return;
        }

        self.surface_config.width = self.width;
        self.surface_config.height = self.height;
        self.surface.configure(&self.device, &self.surface_config);
        self.depth_view = create_depth_view(&self.device, &self.surface_config);
    }

    /// Render one frame: compute MVP from core::Camera/Transform, write UBO, draw cube.
    pub fn render(&mut self) -> Result<(), SurfaceError> {
        let draw = [DrawInstance::new(
            self.model,
            self.cube_mesh_id,
            MaterialId::INVALID,
            self.default_texture_id,
        )];
        self.render_models(&draw)
    }

    pub fn is_surface_lost(err: &SurfaceError) -> bool {
        matches!(err, SurfaceError::Lost | SurfaceError::Outdated)
    }

    pub fn recreate_surface(&mut self) {
        self.resize(self.width, self.height);
    }

    /// Render a list of draw instances with optimized batching (G1).
    /// Sort order: PSO -> Material -> Texture -> Mesh to minimize state changes.
    pub fn render_models(&mut self, draw_list: &[DrawInstance]) -> Result<(), SurfaceError> {
        if self.width == 0 || self.height == 0 {
            return Ok(());
        }

        // G1: Prepare and sort draw commands for optimal batching
        self.instance_entries.clear();
        self.instance_entries.reserve(draw_list.len());

        for item in draw_list {
            // Replace INVALID texture with default texture
            let texture = if item.texture == TextureId::INVALID {
                self.default_texture_id
            } else {
                item.texture
            };

            let key = DrawKey {
                pso_id: 0, // Currently only one PSO
                material: item.material,
                texture,
                mesh: item.mesh,
            };

            self.instance_entries.push(InstanceEntry {
                key,
                instance: InstanceRaw::from_model(item.transform.matrix()),
            });
        }

        // G1: Sort by DrawKey (PSO -> Material -> Texture -> Mesh)
        self.instance_entries.sort_by_key(|entry| entry.key);

        // G1: Create batches with same render state
        self.draw_batches.clear();
        self.draw_batches.reserve(self.instance_entries.len());
        self.instance_data.clear();
        self.instance_data.reserve(self.instance_entries.len());

        if !self.instance_entries.is_empty() {
            let mut batch_start = 0;
            let mut current_key = self.instance_entries[0].key;

            for (idx, entry) in self.instance_entries.iter().enumerate() {
                if entry.key != current_key {
                    // End current batch
                    let batch_count = idx - batch_start;
                    self.draw_batches.push(DrawBatch {
                        key: current_key,
                        start: batch_start,
                        count: batch_count,
                    });

                    // Start new batch
                    batch_start = idx;
                    current_key = entry.key;
                }
                self.instance_data.push(entry.instance);
            }

            // Add final batch
            let batch_count = self.instance_entries.len() - batch_start;
            self.draw_batches.push(DrawBatch {
                key: current_key,
                start: batch_start,
                count: batch_count,
            });
        }

        let needed =
            (self.instance_data.len().max(1) as u64) * std::mem::size_of::<InstanceRaw>() as u64;
        if needed > self.instance_buf.size() {
            let new_cap = needed.next_power_of_two();
            self.instance_buf = self.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("Instance Buffer (grown)"),
                size: new_cap,
                usage: BufferUsages::VERTEX | BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            self.instance_capacity = (new_cap / std::mem::size_of::<InstanceRaw>() as u64) as u32;
        }

        if !self.instance_data.is_empty() {
            self.queue.write_buffer(
                &self.instance_buf,
                0,
                bytemuck::cast_slice(&self.instance_data),
            );
        }
        self.instance_count = self.instance_data.len() as u32;

        let frame = match self.surface.get_current_texture() {
            Ok(f) => f,
            Err(e @ SurfaceError::Lost | e @ SurfaceError::Outdated) => {
                self.recreate_surface();
                return Err(e);
            }
            Err(SurfaceError::Timeout) => {
                log::warn!("Surface timeout — skipping this frame");
                return Ok(());
            }
            Err(e @ SurfaceError::OutOfMemory) => return Err(e),
            Err(e @ SurfaceError::Other) => {
                log::warn!("Surface error: {:?} — skipping this frame", e);
                return Ok(());
            }
        };
        let view = frame.texture.create_view(&Default::default());
        let mut encoder = self
            .device
            .create_command_encoder(&CommandEncoderDescriptor {
                label: Some("MainEncoder"),
            });

        let mut rpass = encoder.begin_render_pass(&RenderPassDescriptor {
            label: Some("MainPass"),
            color_attachments: &[Some(RenderPassColorAttachment {
                view: &view,
                depth_slice: None,
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

        // Set initial pipeline state
        rpass.set_pipeline(&self.pipeline);
        rpass.set_bind_group(0, &self.camera_bg, &[]);

        // Update camera uniforms once per frame
        let pv = self.camera.proj_view();
        let mvp = OPENGL_TO_WGPU * pv;
        self.queue.write_buffer(
            &self.camera_buf,
            0,
            bytemuck::bytes_of(&CameraUniform {
                mvp: mvp.to_cols_array_2d(),
            }),
        );

        // G1: Render batches with minimal state changes
        let stride = std::mem::size_of::<InstanceRaw>() as u64;
        let mut current_material = MaterialId::INVALID;
        let mut current_texture = TextureId::INVALID;
        let mut state_changes = 0u32;

        for batch in &self.draw_batches {
            if batch.count == 0 {
                continue;
            }

            let key = batch.key;

            // G1: Only change material bind group when material changes
            if key.material != current_material {
                rpass.set_bind_group(1, &self.material_bg, &[]);
                current_material = key.material;
                state_changes += 1;
            }

            // G1: Only change texture bind group when texture changes
            if key.texture != current_texture {
                // For now, we use the same texture bind group for all textures
                // In a full implementation, we'd have different bind groups per texture
                rpass.set_bind_group(2, &self.texture_bg, &[]);
                current_texture = key.texture;
                state_changes += 1;
            }

            // Get mesh data
            let Some(mesh) = self.mesh_store.get(key.mesh) else {
                log::warn!("Missing mesh id {:?}", key.mesh);
                continue;
            };

            // Set vertex/index buffers and draw
            let instance_start = batch.start as u64 * stride;
            let instance_end = instance_start + batch.count as u64 * stride;

            rpass.set_vertex_buffer(0, mesh.vertex_buf.slice(..));
            rpass.set_vertex_buffer(1, self.instance_buf.slice(instance_start..instance_end));
            rpass.set_index_buffer(mesh.index_buf.slice(..), mesh.index_format);
            rpass.draw_indexed(0..mesh.index_count, 0, 0..batch.count as u32);
        }

        // G1: Log state changes for performance monitoring
        if !self.draw_batches.is_empty() {
            log::debug!(
                "Rendered {} batches with {} state changes (ratio: {:.2})",
                self.draw_batches.len(),
                state_changes,
                state_changes as f32 / self.draw_batches.len() as f32
            );
        }
        drop(rpass);

        self.queue.submit(Some(encoder.finish()));
        frame.present();
        Ok(())
    }
}

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

fn cube_mesh_data() -> MeshData {
    use asset::mesh::MeshVertex;

    let vertices = vec![
        // +Z
        MeshVertex::new([-1.0, -1.0, 1.0], [0.0, 0.0, 1.0], [0.0, 0.0]),
        MeshVertex::new([1.0, -1.0, 1.0], [0.0, 0.0, 1.0], [1.0, 0.0]),
        MeshVertex::new([1.0, 1.0, 1.0], [0.0, 0.0, 1.0], [1.0, 1.0]),
        MeshVertex::new([-1.0, 1.0, 1.0], [0.0, 0.0, 1.0], [0.0, 1.0]),
        // -Z
        MeshVertex::new([1.0, -1.0, -1.0], [0.0, 0.0, -1.0], [0.0, 0.0]),
        MeshVertex::new([-1.0, -1.0, -1.0], [0.0, 0.0, -1.0], [1.0, 0.0]),
        MeshVertex::new([-1.0, 1.0, -1.0], [0.0, 0.0, -1.0], [1.0, 1.0]),
        MeshVertex::new([1.0, 1.0, -1.0], [0.0, 0.0, -1.0], [0.0, 1.0]),
        // +X
        MeshVertex::new([1.0, -1.0, 1.0], [1.0, 0.0, 0.0], [0.0, 0.0]),
        MeshVertex::new([1.0, -1.0, -1.0], [1.0, 0.0, 0.0], [1.0, 0.0]),
        MeshVertex::new([1.0, 1.0, -1.0], [1.0, 0.0, 0.0], [1.0, 1.0]),
        MeshVertex::new([1.0, 1.0, 1.0], [1.0, 0.0, 0.0], [0.0, 1.0]),
        // -X
        MeshVertex::new([-1.0, -1.0, -1.0], [-1.0, 0.0, 0.0], [0.0, 0.0]),
        MeshVertex::new([-1.0, -1.0, 1.0], [-1.0, 0.0, 0.0], [1.0, 0.0]),
        MeshVertex::new([-1.0, 1.0, 1.0], [-1.0, 0.0, 0.0], [1.0, 1.0]),
        MeshVertex::new([-1.0, 1.0, -1.0], [-1.0, 0.0, 0.0], [0.0, 1.0]),
        // +Y
        MeshVertex::new([-1.0, 1.0, 1.0], [0.0, 1.0, 0.0], [0.0, 0.0]),
        MeshVertex::new([1.0, 1.0, 1.0], [0.0, 1.0, 0.0], [1.0, 0.0]),
        MeshVertex::new([1.0, 1.0, -1.0], [0.0, 1.0, 0.0], [1.0, 1.0]),
        MeshVertex::new([-1.0, 1.0, -1.0], [0.0, 1.0, 0.0], [0.0, 1.0]),
        // -Y
        MeshVertex::new([-1.0, -1.0, -1.0], [0.0, -1.0, 0.0], [0.0, 0.0]),
        MeshVertex::new([1.0, -1.0, -1.0], [0.0, -1.0, 0.0], [1.0, 0.0]),
        MeshVertex::new([1.0, -1.0, 1.0], [0.0, -1.0, 0.0], [1.0, 1.0]),
        MeshVertex::new([-1.0, -1.0, 1.0], [0.0, -1.0, 0.0], [0.0, 1.0]),
    ];

    let indices: Vec<u32> = vec![
        0, 1, 2, 0, 2, 3, // +Z
        4, 5, 6, 4, 6, 7, // -Z
        8, 9, 10, 8, 10, 11, // +X
        12, 13, 14, 12, 14, 15, // -X
        16, 17, 18, 16, 18, 19, // +Y
        20, 21, 22, 20, 22, 23, // -Y
    ];

    MeshData::new(vertices, indices)
}
