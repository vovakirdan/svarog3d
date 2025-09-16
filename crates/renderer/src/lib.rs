//! Renderer: wgpu init + depth + cube.
//! D1: camera/transform from `core` with setters.

use std::num::NonZeroU64;
use std::sync::Arc;
use std::time::Instant;

use bytemuck::{Pod, Zeroable};
use corelib::{Mat4, Vec3, camera::Camera, transform::Transform};
use wgpu::{
    BindGroup, BindGroupLayout, BindGroupLayoutDescriptor, BindGroupLayoutEntry, BindingType,
    BlendState, Buffer, BufferBindingType, BufferUsages, ColorTargetState, ColorWrites,
    CommandEncoderDescriptor, DepthBiasState, DepthStencilState, Device, Extent3d, FragmentState,
    Instance, InstanceDescriptor, LoadOp, Operations, PipelineLayoutDescriptor, PowerPreference,
    PresentMode, Queue, RenderPassColorAttachment, RenderPassDescriptor, RenderPipeline,
    RenderPipelineDescriptor, ShaderModuleDescriptor, ShaderSource, ShaderStages, StoreOp, Surface,
    SurfaceConfiguration, SurfaceError, TextureDescriptor, TextureDimension, TextureFormat,
    TextureUsages, TextureView, TextureViewDescriptor, VertexBufferLayout, VertexState,
    VertexStepMode, util::DeviceExt,
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
            2 => Float32x4, // col0
            3 => Float32x4, // col1
            4 => Float32x4, // col2
            5 => Float32x4, // col3
        ],
    };

    pub fn from_model(m: Mat4) -> Self {
        // glam::Mat4 хранится в column-major; берём колонки напрямую.
        let c = m.to_cols_array();
        Self {
            col0: [c[0],  c[1],  c[2],  c[3]],
            col1: [c[4],  c[5],  c[6],  c[7]],
            col2: [c[8],  c[9],  c[10], c[11]],
            col3: [c[12], c[13], c[14], c[15]],
        }
    }
}

/// Camera UBO (16-byte aligned).
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct CameraUniform {
    mvp: [[f32; 4]; 4],
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
    vertex_buf: Buffer,
    index_buf: Buffer,
    index_count: u32,
    instance_buf: Buffer,
    instance_capacity: u32,
    instance_count: u32,

    // Camera / model state (from core)
    camera: Camera,
    model: Transform,

    // Bindings
    #[allow(dead_code)]
    camera_bgl: BindGroupLayout,
    camera_bg: BindGroup,
    camera_buf: Buffer,

    // Depth
    depth_view: TextureView,

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
            bind_group_layouts: &[&camera_bgl],
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

        // Geometry: indexed cube
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
            vertex_buf,
            index_buf,
            index_count: indices.len() as u32,
            camera_bgl,
            camera_bg,
            camera_buf,
            depth_view,
            start: Instant::now(),
            camera,
            model,
            width,
            height,
            instance_buf,
            instance_capacity,
            instance_count: 0,
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
        // Compute MVP (GL-style PV → convert to WGPU NDC)
        let pv = self.camera.proj_view();
        let m = self.model.matrix();
        let mvp = OPENGL_TO_WGPU * pv * m;
        self.queue.write_buffer(
            &self.camera_buf,
            0,
            bytemuck::bytes_of(&CameraUniform {
                mvp: mvp.to_cols_array_2d(),
            }),
        );

        // Acquire frame & encode pass
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

    /// Render a list of models (one draw call).
    pub fn render_models(
        &mut self,
        models: &[corelib::transform::Transform],
    ) -> Result<(), SurfaceError> {
        if self.width == 0 || self.height == 0 {
            // ничего не рисуем, окно свернуто
            return Ok(());
        }
        // 1) Подготовим массив инстансов (модельные матрицы)
        //    ОДНА аллокация на кадр у вызывающей стороны уже есть (draw_list),
        //    здесь — превращаем в InstanceRaw (можно использовать стаck-alloc via smallvec, но пока простое Vec).
        let mut instances: Vec<InstanceRaw> = Vec::with_capacity(models.len());
        let pv = self.camera.proj_view();
        for m in models {
            let model = m.matrix();
            // Вершинный шейдер умножает: MVP * model * pos; здесь подаём только model как инстанс-атрибут.
            instances.push(InstanceRaw::from_model(model));
        }

        // 2) Обновляем/расширяем буфер, если нужно (grow only, без лишних recreate)
        let needed = (instances.len().max(1) as u64) * std::mem::size_of::<InstanceRaw>() as u64;
        if needed > self.instance_buf.size() {
            // Реаллоцируем с запасом (next power of two)
            let new_cap = needed.next_power_of_two();
            self.instance_buf = self.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("Instance Buffer (grown)"),
                size: new_cap,
                usage: BufferUsages::VERTEX | BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            self.instance_capacity = (new_cap / std::mem::size_of::<InstanceRaw>() as u64) as u32;
        }

        // 3) Один upload на кадр
        if !instances.is_empty() {
            self.queue
                .write_buffer(&self.instance_buf, 0, bytemuck::cast_slice(&instances));
        }
        self.instance_count = instances.len() as u32;

        // 4) Рендер одним проходом и ОДНИМ draw_indexed с instance_count
        let frame = match self.surface.get_current_texture() {
            Ok(f) => f,
            Err(e @ SurfaceError::Lost | e @ SurfaceError::Outdated) => {
                // реконфигурируем и пропускаем кадр
                self.recreate_surface();
                return Err(e);
            }
            Err(SurfaceError::Timeout) => {
                // вежливо пропускаем кадр без паники
                log::warn!("Surface timeout — skipping this frame");
                return Ok(());
            }
            Err(e @ SurfaceError::OutOfMemory) => {
                // нехватка памяти — bubbling up (platform решит завершиться)
                return Err(e);
            }
            Err(e @ SurfaceError::Other) => {
                // другие ошибки — пропускаем кадр
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

        rpass.set_pipeline(&self.pipeline);
        rpass.set_bind_group(0, &self.camera_bg, &[]);
        rpass.set_vertex_buffer(0, self.vertex_buf.slice(..));
        rpass.set_vertex_buffer(1, self.instance_buf.slice(..)); // <--- инстансы
        rpass.set_index_buffer(self.index_buf.slice(..), wgpu::IndexFormat::Uint16);

        // ВНИМАНИЕ: теперь шейдер умножает model из инстанса, поэтому здесь UBO `mvp` — это только PV!
        let mvp = OPENGL_TO_WGPU * pv; // без model
        self.queue.write_buffer(
            &self.camera_buf,
            0,
            bytemuck::bytes_of(&CameraUniform {
                mvp: mvp.to_cols_array_2d(),
            }),
        );

        rpass.draw_indexed(0..self.index_count, 0, 0..self.instance_count);
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

fn cube_vertices() -> (Vec<Vertex>, Vec<u16>) {
    let v = [
        // back z=-1
        Vertex {
            pos: [-1.0, -1.0, -1.0],
            color: [1.0, 0.0, 0.0],
        }, // 0
        Vertex {
            pos: [1.0, -1.0, -1.0],
            color: [0.0, 1.0, 0.0],
        }, // 1
        Vertex {
            pos: [1.0, 1.0, -1.0],
            color: [0.0, 0.0, 1.0],
        }, // 2
        Vertex {
            pos: [-1.0, 1.0, -1.0],
            color: [1.0, 1.0, 0.0],
        }, // 3
        // front z=+1
        Vertex {
            pos: [-1.0, -1.0, 1.0],
            color: [1.0, 0.0, 1.0],
        }, // 4
        Vertex {
            pos: [1.0, -1.0, 1.0],
            color: [0.0, 1.0, 1.0],
        }, // 5
        Vertex {
            pos: [1.0, 1.0, 1.0],
            color: [1.0, 1.0, 1.0],
        }, // 6
        Vertex {
            pos: [-1.0, 1.0, 1.0],
            color: [1.0, 0.5, 0.0],
        }, // 7
    ];
    let idx: [u16; 36] = [
        4, 5, 6, 4, 6, 7, // +Z
        0, 2, 1, 0, 3, 2, // -Z
        3, 6, 2, 3, 7, 6, // +Y
        0, 1, 5, 0, 5, 4, // -Y
        0, 3, 7, 0, 7, 4, // -X
        1, 2, 6, 1, 6, 5, // +X
    ];
    (v.to_vec(), idx.to_vec())
}
