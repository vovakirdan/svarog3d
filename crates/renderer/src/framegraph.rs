//! Mini-FrameGraph system for G2.
//! Explicit render passes with resource dependencies.

use std::collections::HashMap;
use wgpu::{CommandEncoder, Device, RenderPass, TextureView};

/// Handle for a framegraph resource (texture, buffer, etc).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ResourceId(pub u32);

/// Handle for a framegraph render pass.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct PassId(pub u32);

/// Resource usage in a pass.
#[derive(Clone, Copy, Debug)]
pub enum ResourceUsage {
    Read,
    Write,
    ReadWrite,
}

/// Resource description for creation.
#[derive(Clone, Debug)]
pub struct ResourceDesc {
    pub label: String,
    pub width: u32,
    pub height: u32,
    pub format: wgpu::TextureFormat,
    pub usage: wgpu::TextureUsages,
}

/// Render pass description.
pub struct PassDesc {
    pub label: String,
    pub inputs: Vec<(ResourceId, ResourceUsage)>,
    pub outputs: Vec<(ResourceId, ResourceUsage)>,
}

/// A framegraph resource (texture for now).
pub struct Resource {
    pub desc: ResourceDesc,
    pub texture: Option<wgpu::Texture>,
    pub view: Option<TextureView>,
}

/// Render pass execution function.
pub type PassExecuteFn = Box<dyn FnOnce(&mut RenderPass, &HashMap<ResourceId, &Resource>)>;

/// A render pass.
pub struct Pass {
    pub desc: PassDesc,
    pub execute: PassExecuteFn,
}

/// Mini-FrameGraph for organizing render passes.
pub struct FrameGraph {
    resources: HashMap<ResourceId, Resource>,
    passes: HashMap<PassId, Pass>,
    resource_counter: u32,
    pass_counter: u32,
    execution_order: Vec<PassId>,
}

impl FrameGraph {
    pub fn new() -> Self {
        Self {
            resources: HashMap::new(),
            passes: HashMap::new(),
            resource_counter: 0,
            pass_counter: 0,
            execution_order: Vec::new(),
        }
    }

    /// Add a resource to the framegraph.
    pub fn add_resource(&mut self, desc: ResourceDesc) -> ResourceId {
        let id = ResourceId(self.resource_counter);
        self.resource_counter += 1;

        let resource = Resource {
            desc,
            texture: None,
            view: None,
        };

        self.resources.insert(id, resource);
        id
    }

    /// Add a render pass to the framegraph.
    pub fn add_pass(&mut self, desc: PassDesc, execute: PassExecuteFn) -> PassId {
        let id = PassId(self.pass_counter);
        self.pass_counter += 1;

        let pass = Pass { desc, execute };
        self.passes.insert(id, pass);
        id
    }

    /// Compile the framegraph - determine execution order and create resources.
    pub fn compile(&mut self, device: &Device) {
        // Simple execution order: just use insertion order for now
        // A real framegraph would do topological sorting based on dependencies
        self.execution_order.clear();
        self.execution_order.extend(self.passes.keys().copied());

        // Create GPU resources
        for resource in self.resources.values_mut() {
            if resource.texture.is_none() {
                let texture = device.create_texture(&wgpu::TextureDescriptor {
                    label: Some(&resource.desc.label),
                    size: wgpu::Extent3d {
                        width: resource.desc.width,
                        height: resource.desc.height,
                        depth_or_array_layers: 1,
                    },
                    mip_level_count: 1,
                    sample_count: 1,
                    dimension: wgpu::TextureDimension::D2,
                    format: resource.desc.format,
                    usage: resource.desc.usage,
                    view_formats: &[],
                });

                let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
                resource.texture = Some(texture);
                resource.view = Some(view);
            }
        }
    }

    /// Execute the framegraph.
    pub fn execute(&mut self, encoder: &mut CommandEncoder) {
        // In a real framegraph, we'd execute passes in dependency order
        // For now, just demonstrate the concept with a simple approach

        for pass_id in &self.execution_order {
            let pass = self.passes.remove(pass_id).expect("Pass should exist");

            // Create render pass
            let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some(&pass.desc.label),
                color_attachments: &[],
                depth_stencil_attachment: None,
                occlusion_query_set: None,
                timestamp_writes: None,
            });

            // Execute pass with access to resources
            let resource_refs: HashMap<ResourceId, &Resource> =
                self.resources.iter().map(|(id, res)| (*id, res)).collect();

            (pass.execute)(&mut render_pass, &resource_refs);
        }
    }

    /// Get resource by id.
    pub fn get_resource(&self, id: ResourceId) -> Option<&Resource> {
        self.resources.get(&id)
    }
}

impl Default for FrameGraph {
    fn default() -> Self {
        Self::new()
    }
}