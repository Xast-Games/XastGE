use std::collections::HashMap;

use flecs_ecs::prelude::*;
use glam::Vec2;
use image::{Rgba as ImageRgba, RgbaImage};

use crate::{
    RenderOverlay, Setup,
    app::RenderSlot,
    render::{
        RenderDevice, RenderSurface,
        buffer::BufferStorage,
        include_wgsl,
        pipeline::{Pipeline, RenderPipelineDescriptor},
        registry::RenderRegistry,
        shader_resource::{ShaderResource, ShaderResourceLayout},
        texture::{Texture, TextureDescriptor, TextureHandle, TextureResourceDescriptor, TextureResourceUsage},
        types::*,
    },
};

use super::tree::{UiQuadVertex, UiTree};

impl UiQuadVertex {
    const ATTRIBS: &[wgpu::VertexAttribute] = &wgpu::vertex_attr_array![
        0 => Float32x2,
        1 => Float32x2,
        2 => Float32x4,
        3 => Uint32,
    ];

    pub fn vertex_buffer_layout() -> wgpu::VertexBufferLayout<'static> {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<UiQuadVertex>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: Self::ATTRIBS,
        }
    }
}

struct TreeBuffers {
    vertices: BufferStorage,
    indices: BufferStorage,
    version: u64,
}

struct RendererState {
    pipeline: Pipeline,
    layout: ShaderResourceLayout,
    /// Bound for solid quads.
    white: ShaderResource,
    _white_texture: Texture,
    textures: HashMap<TextureHandle, ShaderResource>,
    trees: HashMap<Entity, TreeBuffers>,
}

#[derive(Component, Default)]
pub struct UiRenderer {
    state: Option<RendererState>,
}

impl RendererState {
    fn new(device: &RenderDevice) -> Self {
        let layout = ShaderResourceLayout::builder()
            .with_label("UI")
            .with_texture(&TextureResourceDescriptor {
                usage: TextureResourceUsage::TEXTURE | TextureResourceUsage::SAMPLER,
                sample_type: Some(TextureSampleType::Float { filterable: true }),
                sampler_binding_type: Some(SamplerBindingType::Filtering),
                dimension: TextureDimension::D2,
                view_dimension: TextureViewDimension::D2,
                format: Texture::DEFAULT_FORMAT,
            })
            .build(device);

        let pipeline = Pipeline::new_render(device, &RenderPipelineDescriptor {
            shader: include_wgsl!("../../assets/shaders/ui.wgsl"),
            bindings: &[&layout],
            label: "UI",
            vertex_layout: Some(UiQuadVertex::vertex_buffer_layout()),
            surface_formats: &[device.surface_format()],
            blend_state: BlendState::ALPHA_BLENDING,
            depth_write_enabled: false,
            depth_compare: CompareFunction::Always,
            front_face: FrontFace::Ccw,
            cull_mode: None,
            topology: PrimitiveTopology::TriangleList,
        });

        let white_texture = Texture::new(device, TextureDescriptor {
            label: "UI white".to_owned(),
            ..Default::default()
        });
        white_texture.fill(device, &RgbaImage::from_pixel(1, 1, ImageRgba([255, 255, 255, 255])));

        let white = ShaderResource::builder()
            .with_texture(&white_texture, TextureResourceUsage::TEXTURE | TextureResourceUsage::SAMPLER)
            .build(device, &layout);

        RendererState {
            pipeline,
            layout,
            white,
            _white_texture: white_texture,
            textures: HashMap::new(),
            trees: HashMap::new(),
        }
    }
}

pub(crate) fn register(world: &World) {
    world.component::<UiRenderer>().add_trait::<flecs::Singleton>();
    world.set(UiRenderer::default());

    world.system::<(&mut UiRenderer, &RenderSlot)>()
        .kind(Setup)
        .each(|(renderer, slot)| {
            renderer.state = Some(RendererState::new(slot.device()));
        });

    let trees = world.query::<&UiTree>().build();

    world.system::<(&mut UiRenderer, &mut RenderSlot, &RenderRegistry)>()
        .kind(RenderOverlay)
        .each(move |(renderer, slot, registry)| {
            let Some(state) = renderer.state.as_mut() else {
                return;
            };

            let (device, canvas, ctx) = slot.parts_mut();

            let mut order = Vec::new();
            trees.each_entity(|e, tree| {
                let id = e.id();
                order.push((tree.z_index, id, tree.geometry.batches.clone()));

                let geometry = &tree.geometry;
                let buffers = state.trees.entry(id).or_insert_with(|| TreeBuffers {
                    vertices: BufferStorage::new::<UiQuadVertex>(device, geometry.vertices.len().max(4), BufferUsages::VERTEX),
                    indices: BufferStorage::new::<u32>(device, geometry.indices.len().max(6), BufferUsages::INDEX),
                    version: u64::MAX,
                });

                if buffers.version != tree.version {
                    buffers.vertices.fill(device, 0, &geometry.vertices);
                    buffers.indices.fill(device, 0, &geometry.indices);
                    buffers.version = tree.version;
                }

                for batch in &geometry.batches {
                    if let Some(handle) = batch.texture
                        && !state.textures.contains_key(&handle)
                        && let Some(texture) = registry.get_texture(handle)
                    {
                        let resource = ShaderResource::builder()
                            .with_texture(texture, TextureResourceUsage::TEXTURE | TextureResourceUsage::SAMPLER)
                            .build(device, &state.layout);
                        state.textures.insert(handle, resource);
                    }
                }
            });

            state.trees.retain(|id, _| order.iter().any(|(_, e, _)| e == id));
            order.sort_by_key(|(z, _, _)| *z);

            let size = device.size();
            let viewport = Vec2::new(size.x as f32, size.y as f32);
            let canvases: &[&dyn RenderSurface] = &[canvas];
            let mut pass = ctx.render_pass(canvases, device.depth_texture(), Operations {
                load: LoadOp::Load,
                store: StoreOp::Store,
            });

            pass.set_pipeline(&state.pipeline);
            pass.set_push_constants(&viewport);

            for (_, id, batches) in order {
                let Some(buffers) = state.trees.get(&id) else {
                    continue;
                };

                for batch in &batches {
                    let resource = match batch.texture {
                        None => &state.white,
                        Some(handle) => match state.textures.get(&handle) {
                            Some(r) => r,
                            None => continue,
                        },
                    };

                    let x0 = batch.scissor.x.max(0.0).min(viewport.x);
                    let y0 = batch.scissor.y.max(0.0).min(viewport.y);
                    let x1 = (batch.scissor.x + batch.scissor.w).max(0.0).min(viewport.x);
                    let y1 = (batch.scissor.y + batch.scissor.h).max(0.0).min(viewport.y);
                    if x1 - x0 < 1.0 || y1 - y0 < 1.0 {
                        continue;
                    }

                    pass.set_scissor(x0 as u32, y0 as u32, (x1 - x0) as u32, (y1 - y0) as u32);
                    pass.set_shader_resource(0, resource);
                    pass.draw_indexed_buffers(
                        &buffers.vertices,
                        &buffers.indices,
                        batch.start..batch.start + batch.count,
                    );
                }
            }
        });
}
