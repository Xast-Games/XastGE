use bytemuck::{Pod, Zeroable};
use flecs_ecs::macros::Component;
use glam::{Quat, UVec4, Vec3, Vec4};

use crate::mesh::Vertex;

use super::{
    RenderDevice,
    buffer::{BufferHandle, BufferResourceDescriptor},
    include_wgsl,
    material::Material,
    registry::RenderRegistry,
    shader_resource::{ShaderResource, ShaderResourceLayout},
    texture::{Texture, TextureHandle, TextureResourceDescriptor, TextureResourceUsage},
    types::*,
};

pub const MAX_LIGHTS: usize = 32;
pub const INTENSITY_SCALE: f32 = 0.004;

#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Pod, Zeroable)]
pub struct LitParams {
    pub base_color: Vec4,
    pub roughness: f32,
    pub specular: f32,
    padding: [f32; 2],
}

impl LitParams {
    pub fn new(base_color: Vec4, roughness: f32) -> LitParams {
        LitParams {
            base_color,
            roughness: roughness.clamp(0.05, 1.0),
            specular: 0.5,
            padding: [0.0; 2],
        }
    }
}

impl Default for LitParams {
    fn default() -> Self {
        LitParams::new(Vec4::ONE, 0.8)
    }
}

#[derive(Debug, Clone, Component)]
pub struct LitMaterial {
    params: BufferHandle,
    albedo: TextureHandle,
}

impl LitMaterial {
    pub fn new(
        params: LitParams,
        albedo: TextureHandle,
        render_device: &RenderDevice,
        registry: &mut RenderRegistry,
    ) -> LitMaterial {
        let buffer = registry
            .new_buffer::<LitParams>(render_device, 1, BufferUsages::UNIFORM)
            .and_then_mut(registry, |b| b.fill(render_device, 0, std::slice::from_ref(&params)));

        LitMaterial { params: buffer, albedo }
    }

    pub fn write(&self, params: &LitParams, render_device: &RenderDevice, registry: &RenderRegistry) {
        if let Some(buffer) = registry.get_buffer(self.params)
            && let Err(e) = buffer.fill_exact(render_device, 0, std::slice::from_ref(params))
        {
            log::error!("Cannot update lit material: {e}");
        }
    }

    pub fn release(&self, registry: &mut RenderRegistry) {
        registry.remove_buffer(self.params);
    }
}

impl Material for LitMaterial {
    fn shader() -> ShaderModuleDescriptor<'static> {
        include_wgsl!("../../assets/shaders/lit.wgsl")
    }

    fn vertex_layout() -> Option<VertexBufferLayout<'static>> {
        Some(Vertex::vertex_buffer_layout())
    }

    fn shader_resource_layout(render_device: &RenderDevice) -> ShaderResourceLayout {
        ShaderResourceLayout::builder()
            .with_label("Lit Material")
            .with_buffer(&BufferResourceDescriptor {
                visibility: ShaderStages::FRAGMENT,
                buffer_type: BufferBindingType::Uniform,
            })
            .with_texture(&TextureResourceDescriptor {
                usage: TextureResourceUsage::TEXTURE | TextureResourceUsage::SAMPLER,
                sample_type: Some(TextureSampleType::Float { filterable: true }),
                sampler_binding_type: Some(SamplerBindingType::Filtering),
                dimension: TextureDimension::D2,
                view_dimension: TextureViewDimension::D2,
                format: Texture::DEFAULT_FORMAT,
            })
            .build(render_device)
    }

    fn shader_resource(&self, render_device: &RenderDevice, registry: &RenderRegistry) -> ShaderResource {
        ShaderResource::builder()
            .with_buffer(registry.get_buffer(self.params).unwrap())
            .with_texture(
                registry.get_texture(self.albedo).unwrap(),
                TextureResourceUsage::TEXTURE | TextureResourceUsage::SAMPLER,
            )
            .build(render_device, &Self::shader_resource_layout(render_device))
    }

    fn blend_state() -> BlendState {
        BlendState::REPLACE
    }

    fn cull_mode() -> Option<Face> {
        None
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Cone {
    pub inner_degrees: f32,
    pub outer_degrees: f32,
}

#[derive(Component, Debug, Clone, Copy, PartialEq)]
pub struct LightSource {
    pub color: Vec3,
    pub intensity: f32,
    pub range: f32,
    pub cone: Option<Cone>,
}

#[derive(Component, Debug, Clone, Copy, PartialEq)]
pub struct LitEnvironment {
    pub sky: Vec3,
    pub ground: Vec3,
    pub ambient: f32,
    pub fog_color: Vec3,
    pub fog_density: f32,
}

impl Default for LitEnvironment {
    fn default() -> Self {
        LitEnvironment {
            sky: Vec3::new(0.55, 0.68, 0.85),
            ground: Vec3::new(0.25, 0.22, 0.18),
            ambient: 0.4,
            fog_color: Vec3::new(0.55, 0.68, 0.85),
            fog_density: 0.00012,
        }
    }
}

#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Pod, Zeroable)]
pub struct GpuLight {
    pub position: Vec4,
    pub color: Vec4,
    pub direction: Vec4,
    pub cone: Vec4,
}

#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Pod, Zeroable)]
pub struct LightsUniform {
    pub sky: Vec4,
    pub ground: Vec4,
    pub fog: Vec4,
    pub count: UVec4,
    pub lights: [GpuLight; MAX_LIGHTS],
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PlacedLight {
    pub position: Vec3,
    pub rotation: Quat,
    pub light: LightSource,
}

fn gpu_light(placed: &PlacedLight) -> GpuLight {
    let light = &placed.light;
    let cone = match light.cone {
        Some(cone) => {
            let outer = cone.outer_degrees.clamp(0.5, 89.5);
            let inner = cone.inner_degrees.clamp(0.0, outer - 0.25);
            Vec4::new(inner.to_radians().cos(), outer.to_radians().cos(), 1.0, 0.0)
        }
        None => Vec4::ZERO,
    };

    GpuLight {
        position: placed.position.extend(light.range),
        color: light.color.extend(light.intensity * INTENSITY_SCALE),
        direction: (placed.rotation * Vec3::NEG_Z).normalize_or(Vec3::NEG_Y).extend(0.0),
        cone,
    }
}

pub fn lights_uniform(environment: &LitEnvironment, focus: Vec3, lights: &[PlacedLight]) -> LightsUniform {
    let mut nearest: Vec<&PlacedLight> = lights
        .iter()
        .filter(|l| l.light.range > 0.0 && l.light.intensity > 0.0)
        .collect();
    let reach = |l: &PlacedLight| l.position.distance(focus) - l.light.range;
    nearest.sort_by(|a, b| reach(a).total_cmp(&reach(b)));
    nearest.truncate(MAX_LIGHTS);

    let mut uniform = LightsUniform::zeroed();
    uniform.sky = environment.sky.extend(environment.ambient);
    uniform.ground = environment.ground.extend(0.0);
    uniform.fog = environment.fog_color.extend(environment.fog_density);
    uniform.count = UVec4::new(nearest.len() as u32, 0, 0, 0);
    for (slot, light) in uniform.lights.iter_mut().zip(nearest) {
        *slot = gpu_light(light);
    }
    uniform
}
