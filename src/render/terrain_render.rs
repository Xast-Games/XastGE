use bytemuck::{Pod, Zeroable};
use flecs_ecs::prelude::*;
use glam::{Mat4, Vec3, Vec4};

use crate::{
    Render, Setup,
    app::RenderSlot,
    math::Transform,
    mesh::{Mesh, Vertex},
    render::{
        RenderDevice, RenderSurface,
        buffer::{BufferHandle, BufferResourceDescriptor},
        camera::{CameraBuffer, PerspectiveCamera},
        include_wgsl,
        lit::{LightSource, LightsUniform, LitEnvironment, LitMaterial, PlacedLight, lights_uniform},
        material::{Material, TerrainMaterial},
        model::Model,
        pass::DrawDescriptor,
        pipeline::{Pipeline, RenderPipelineDescriptor},
        registry::{RenderRegistry, RenderRegistryModule},
        shader_resource::{ShaderResource, ShaderResourceLayout},
        texture::{Texture, TextureDescriptor, TextureResourceDescriptor, TextureResourceUsage},
        types::*,
    },
    terrain::TerrainVertex,
};

const SHADOW_RESOLUTION: u32 = 2048;

#[derive(Component, Debug, Clone, Copy, PartialEq)]
pub struct TerrainLighting {
    pub sun_direction: Vec3,
    pub sun_strength: f32,
    pub shadows: bool,
    pub shadow_extent: f32,
    pub shadow_bias: f32,
    pub normal_offset: f32,
}

impl Default for TerrainLighting {
    fn default() -> Self {
        TerrainLighting {
            sun_direction: Vec3::new(0.45, 0.8, 0.3).normalize(),
            sun_strength: 1.0,
            shadows: true,
            shadow_extent: 3000.0,
            shadow_bias: 0.0015,
            normal_offset: 2.0,
        }
    }
}

#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
pub struct LightUniform {
    pub view_proj: Mat4,
    pub sun: Vec4,
    pub params: Vec4,
}

pub fn light_uniform(lighting: &TerrainLighting, focus: Vec3, resolution: f32) -> LightUniform {
    let dir = lighting.sun_direction.normalize_or(Vec3::Y);
    let half = lighting.shadow_extent.max(10.0);
    let center = Vec3::new(focus.x, 0.0, focus.z);
    let up = if dir.y.abs() > 0.99 { Vec3::Z } else { Vec3::Y };

    let view = Mat4::look_at_rh(dir, Vec3::ZERO, up);
    let texel = 2.0 * half / resolution;
    let p = view.transform_point3(center);
    let (sx, sy) = ((p.x / texel).round() * texel, (p.y / texel).round() * texel);
    let depth = -p.z;
    let reach = half + 4000.0;
    let proj = Mat4::orthographic_rh(sx - half, sx + half, sy - half, sy + half, depth - reach, depth + reach);

    LightUniform {
        view_proj: proj * view,
        sun: dir.extend(lighting.sun_strength),
        params: Vec4::new(
            1.0 / resolution,
            lighting.shadow_bias,
            lighting.normal_offset,
            if lighting.shadows { 1.0 } else { 0.0 },
        ),
    }
}

fn light_layout(device: &RenderDevice) -> ShaderResourceLayout {
    ShaderResourceLayout::builder()
        .with_label("Terrain light")
        .with_buffer(&BufferResourceDescriptor {
            visibility: ShaderStages::VERTEX_FRAGMENT,
            buffer_type: BufferBindingType::Uniform,
        })
        .with_buffer(&BufferResourceDescriptor {
            visibility: ShaderStages::FRAGMENT,
            buffer_type: BufferBindingType::Uniform,
        })
        .build(device)
}

fn shadow_pipeline(device: &RenderDevice, label: &str, vertex_layout: VertexBufferLayout<'static>) -> Pipeline {
    Pipeline::new_render(device, &RenderPipelineDescriptor {
        shader: include_wgsl!("../../assets/shaders/terrain_shadow.wgsl"),
        bindings: &[&light_layout(device)],
        label,
        vertex_layout: Some(vertex_layout),
        surface_formats: &[],
        blend_state: BlendState::REPLACE,
        depth_write_enabled: true,
        depth_compare: CompareFunction::Less,
        front_face: FrontFace::Ccw,
        cull_mode: None,
        topology: PrimitiveTopology::TriangleList,
    })
}

fn shadow_layout(device: &RenderDevice) -> ShaderResourceLayout {
    ShaderResourceLayout::builder()
        .with_label("Terrain shadow map")
        .with_texture(&TextureResourceDescriptor {
            usage: TextureResourceUsage::TEXTURE,
            sample_type: Some(TextureSampleType::Depth),
            sampler_binding_type: None,
            dimension: TextureDimension::D2,
            view_dimension: TextureViewDimension::D2,
            format: TextureFormat::Depth32Float,
        })
        .build(device)
}

struct ShadowState {
    texture: Texture,
    pipeline: Pipeline,
    model_pipeline: Pipeline,
    light_buffer: BufferHandle,
    lights_buffer: BufferHandle,
    light_resource: ShaderResource,
    shadow_resource: ShaderResource,
}

impl ShadowState {
    fn new(device: &RenderDevice, registry: &mut RenderRegistry) -> ShadowState {
        let texture = Texture::new(device, TextureDescriptor {
            width: SHADOW_RESOLUTION,
            height: SHADOW_RESOLUTION,
            format: TextureFormat::Depth32Float,
            usage: TextureUsages::RENDER_ATTACHMENT | TextureUsages::TEXTURE_BINDING,
            label: "Terrain shadow map".to_owned(),
            ..Default::default()
        });

        let pipeline = shadow_pipeline(device, "Terrain shadow", TerrainVertex::vertex_buffer_layout());
        let model_pipeline = shadow_pipeline(device, "Model shadow", Vertex::vertex_buffer_layout());

        let light_buffer = registry.new_buffer::<LightUniform>(device, 1, BufferUsages::UNIFORM);
        let initial = light_uniform(&TerrainLighting::default(), Vec3::ZERO, SHADOW_RESOLUTION as f32);
        let buffer = registry.get_buffer(light_buffer).expect("Terrain light buffer was just created");
        if let Err(e) = buffer.fill_exact(device, 0, &[initial]) {
            log::error!("Cannot initialize terrain light buffer: {e}");
        }

        let lights_buffer = registry.new_buffer::<LightsUniform>(device, 1, BufferUsages::UNIFORM);
        let lights = lights_uniform(&LitEnvironment::default(), Vec3::ZERO, &[]);
        let lights_storage = registry.get_buffer(lights_buffer).expect("Lights buffer was just created");
        if let Err(e) = lights_storage.fill_exact(device, 0, &[lights]) {
            log::error!("Cannot initialize lights buffer: {e}");
        }

        let buffer = registry.get_buffer(light_buffer).expect("Terrain light buffer was just created");
        let light_resource = ShaderResource::builder()
            .with_buffer(buffer)
            .with_buffer(registry.get_buffer(lights_buffer).expect("Lights buffer was just created"))
            .build(device, &light_layout(device));
        let shadow_resource = ShaderResource::builder()
            .with_texture(&texture, TextureResourceUsage::TEXTURE)
            .build(device, &shadow_layout(device));

        ShadowState { texture, pipeline, model_pipeline, light_buffer, lights_buffer, light_resource, shadow_resource }
    }
}

#[derive(Component, Default)]
pub struct TerrainRenderer {
    state: Option<ShadowState>,
}

#[derive(Component)]
pub struct TerrainRenderModule;

impl Module for TerrainRenderModule {
    fn module(world: &World) {
        world.import::<RenderRegistryModule>();

        world.component::<TerrainLighting>().add_trait::<flecs::Singleton>();
        world.set(TerrainLighting::default());

        world.component::<TerrainRenderer>().add_trait::<flecs::Singleton>();
        world.set(TerrainRenderer::default());

        world.component::<LitEnvironment>().add_trait::<flecs::Singleton>();
        world.set(LitEnvironment::default());

        world.system::<(&mut TerrainRenderer, &mut RenderRegistry, &RenderSlot)>()
            .kind(Setup)
            .each(|(renderer, registry, slot)| {
                let device = slot.device();
                let globals = [&CameraBuffer::layout(device), &light_layout(device), &shadow_layout(device)];
                registry.register_material::<TerrainMaterial>(device, &globals);
                registry.register_material::<LitMaterial>(device, &globals);
                renderer.state = Some(ShadowState::new(device, registry));
            });

        let meshes = world.query::<(&Mesh<TerrainVertex>, &TerrainMaterial, &Transform)>().build();
        let models = world.query::<(&Model, &Transform)>().build();
        let sources = world.query::<(&LightSource, &Transform)>().build();
        let cameras = world.query::<(&CameraBuffer, &Transform)>()
            .with(PerspectiveCamera::id())
            .build();

        world.system::<(&TerrainRenderer, &mut RenderSlot, &RenderRegistry, &TerrainLighting, &LitEnvironment)>()
            .kind(Render)
            .each(move |(renderer, slot, registry, lighting, environment)| {
                let Some(state) = renderer.state.as_ref() else {
                    return;
                };

                let mut focus = None;
                cameras.each(|(_, t)| {
                    focus.get_or_insert(t.translation);
                });
                let focus = focus.unwrap_or(Vec3::ZERO);
                let uniform = light_uniform(lighting, focus, SHADOW_RESOLUTION as f32);

                let mut placed = Vec::new();
                sources.each(|(light, t)| {
                    placed.push(PlacedLight { position: t.translation, rotation: t.rotation, light: *light });
                });
                let lights = lights_uniform(environment, focus, &placed);

                let (device, canvas, ctx) = slot.parts_mut();
                if let Some(buffer) = registry.get_buffer(state.light_buffer)
                    && let Err(e) = buffer.fill_exact(device, 0, &[uniform])
                {
                    log::error!("Cannot update terrain light buffer: {e}");
                }
                if let Some(buffer) = registry.get_buffer(state.lights_buffer)
                    && let Err(e) = buffer.fill_exact(device, 0, &[lights])
                {
                    log::error!("Cannot update lights buffer: {e}");
                }

                if lighting.shadows {
                    let no_color: &[&dyn RenderSurface] = &[];
                    let mut pass = ctx.render_pass(no_color, &state.texture, Operations {
                        load: LoadOp::Load,
                        store: StoreOp::Store,
                    });
                    pass.set_pipeline(&state.pipeline);
                    pass.set_shader_resource(0, &state.light_resource);

                    meshes.each(|(mesh, _, t)| {
                        let vertices = mesh.vertex_buffer.and_then(|h| registry.get_buffer(h));
                        let indices = mesh.index_buffer.and_then(|h| registry.get_buffer(h));
                        if let (Some(vertices), Some(indices)) = (vertices, indices) {
                            pass.set_push_constants(&t.to_matrix());
                            pass.draw_indexed_buffers(vertices, indices, 0..mesh.indices.len() as u32);
                        }
                    });

                    pass.set_pipeline(&state.model_pipeline);
                    pass.set_shader_resource(0, &state.light_resource);
                    models.each(|(model, t)| {
                        let matrix = t.to_matrix();
                        for part in model.parts.iter() {
                            let vertices = part.mesh.vertex_buffer.and_then(|h| registry.get_buffer(h));
                            let indices = part.mesh.index_buffer.and_then(|h| registry.get_buffer(h));
                            if let (Some(vertices), Some(indices)) = (vertices, indices) {
                                pass.set_push_constants(&matrix);
                                pass.draw_indexed_buffers(vertices, indices, 0..part.mesh.indices.len() as u32);
                            }
                        }
                    });
                }

                let canvases: &[&dyn RenderSurface] = &[canvas];
                let mut pass = ctx.render_pass(canvases, device.depth_texture(), Operations {
                    load: TerrainMaterial::load_op(),
                    store: TerrainMaterial::store_op(),
                });

                cameras.each(|(camera, _)| {
                    meshes.each(|(mesh, material, t)| {
                        pass.draw(device, registry, DrawDescriptor {
                            drawable: Some(mesh),
                            instance_data: Some(t),
                            global_shader_resources: &[camera.resource(), &state.light_resource, &state.shadow_resource],
                            material,
                        });
                    });

                    models.each(|(model, t)| {
                        for part in model.parts.iter() {
                            pass.draw(device, registry, DrawDescriptor {
                                drawable: Some(&part.mesh),
                                instance_data: Some(t),
                                global_shader_resources: &[camera.resource(), &state.light_resource, &state.shadow_resource],
                                material: &part.material,
                            });
                        }
                    });
                });
            });
    }
}
