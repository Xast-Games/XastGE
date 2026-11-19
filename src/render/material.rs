use bytemuck::{Pod, Zeroable};
use flecs_ecs::core::component_registration::ComponentId;
use flecs_ecs::macros::Component;
use glam::Vec3;
use image::ImageError;
use wgpu::include_wgsl;

use crate::render::RenderDevice;
use crate::render::buffer::{BufferHandle, BufferResourceDescriptor};
use crate::render::mesh::Vertex;
use crate::terrain::TerrainVertex;
use crate::render::texture::{Texture, TextureDescriptor, TextureResourceDescriptor, TextureResourceUsage};
use crate::utils::Color;

use super::{shader_resource::{ShaderResource, ShaderResourceLayout}, registry::RenderRegistry, texture::TextureHandle};
use super::types::*;

pub trait Material: ComponentId {
    fn shader() -> ShaderModuleDescriptor<'static>;

    fn vertex_layout() -> Option<VertexBufferLayout<'static>>;

    fn shader_resource_layout(render_device: &RenderDevice) -> ShaderResourceLayout;

    fn shader_resource(
        &self, 
        render_device: &RenderDevice,
        registry: &RenderRegistry,
    ) -> ShaderResource;

    fn blend_state() -> BlendState {
        BlendState::ALPHA_BLENDING
    }

    fn depth_write_enabled() -> bool {
        true
    }

    fn depth_compare() -> CompareFunction {
        CompareFunction::Less
    }

    fn front_face() -> FrontFace {
        FrontFace::Ccw
    }

    fn cull_mode() -> Option<Face> {
        Some(Face::Back)
    }

    fn topology() -> PrimitiveTopology {
        PrimitiveTopology::TriangleList
    }

    fn load_op() -> LoadOp<GpuColor> {
        LoadOp::Load
    }

    fn store_op() -> StoreOp {
        StoreOp::Store
    }
}

#[derive(Debug, Component)]
pub struct TintedTextureMaterial {
    pub albedo: TextureHandle,
    pub tint: Vec3,
    pub tint_buffer: BufferHandle,
}

impl TintedTextureMaterial {
    pub fn new(
        path: &str, 
        tint: Vec3,
        render_device: &RenderDevice,
        registry: &mut RenderRegistry,
    ) -> Result<TintedTextureMaterial, ImageError> {
        let albedo = registry
            .load_texture(render_device, path, TextureDescriptor::default())?;

        let tint_buffer = registry
            .new_buffer::<Vec3>(render_device, 1, BufferUsages::UNIFORM)
            .and_then_mut(registry, |b| b.fill(render_device, 0, &[tint]));

        Ok(TintedTextureMaterial {
            albedo,
            tint,
            tint_buffer,
        })
    }

    pub fn update_tint(
        &mut self,
        new_tint: Vec3,
        render_device: &RenderDevice,
        registry: &mut RenderRegistry,
    ) {
        self.tint = new_tint;
        registry
            .get_buffer(self.tint_buffer)
            .expect("Cannot update tint buffer")
            .fill_exact(render_device, 0, &[new_tint])
            .expect("Failed to update tint buffer");
    }
}

impl Material for TintedTextureMaterial {
    fn shader() -> ShaderModuleDescriptor<'static> {
        include_wgsl!("../../assets/shaders/basic.wgsl")
    }

    fn vertex_layout() -> Option<VertexBufferLayout<'static>> {
        Some(Vertex::vertex_buffer_layout())
    }

    fn shader_resource_layout(render_device: &RenderDevice) -> ShaderResourceLayout {
        ShaderResourceLayout::builder()
            .with_label("Tinted Texture Material")
            .with_texture(&TextureResourceDescriptor {
                usage: TextureResourceUsage::TEXTURE | TextureResourceUsage::SAMPLER,
                sample_type: Some(TextureSampleType::Float { filterable: true }),
                sampler_binding_type: Some(SamplerBindingType::Filtering),
                dimension: TextureDimension::D2,
                view_dimension: TextureViewDimension::D2,
                format: Texture::DEFAULT_FORMAT,
            })
            .with_buffer(&BufferResourceDescriptor {
                visibility: ShaderStages::FRAGMENT,
                buffer_type: BufferBindingType::Uniform,
            })
            .build(render_device)
    }

    fn shader_resource(
        &self, 
        render_device: &RenderDevice,
        registry: &RenderRegistry,
    ) -> ShaderResource {
        ShaderResource::builder()
            .with_texture(
                registry.get_texture(self.albedo).unwrap(),
                TextureResourceUsage::TEXTURE | TextureResourceUsage::SAMPLER,
            )
            .with_buffer(registry.get_buffer(self.tint_buffer).unwrap())
            .build(
                render_device, 
                &Self::shader_resource_layout(render_device),
            )
    }
}

#[derive(Debug, Clone, Component)]
pub struct SkyboxMaterial {
    pub cubemap: TextureHandle,
}

impl SkyboxMaterial {
    pub fn new(cubemap: TextureHandle) -> Self {
        Self { cubemap }
    }
}

impl Material for SkyboxMaterial {
    fn shader() -> ShaderModuleDescriptor<'static> {
        include_wgsl!("../../assets/shaders/skybox.wgsl")
    }

    fn vertex_layout() -> Option<VertexBufferLayout<'static>> {
        Some(Vertex::vertex_buffer_layout())
    }

    fn shader_resource_layout(render_device: &RenderDevice) -> ShaderResourceLayout {
        ShaderResourceLayout::builder()
            .with_label("Skybox Material")
            .with_texture(&TextureResourceDescriptor {
                usage: TextureResourceUsage::TEXTURE | TextureResourceUsage::SAMPLER,
                sample_type: Some(TextureSampleType::Float { filterable: true }),
                sampler_binding_type: Some(SamplerBindingType::Filtering),
                dimension: TextureDimension::D2,
                view_dimension: TextureViewDimension::Cube,
                format: Texture::DEFAULT_FORMAT,
            })
            .build(render_device)
    }

    fn shader_resource(
        &self,
        render_device: &RenderDevice,
        registry: &RenderRegistry,
    ) -> ShaderResource {
        ShaderResource::builder()
            .with_texture(
                registry.get_texture(self.cubemap).unwrap(),
                TextureResourceUsage::TEXTURE | TextureResourceUsage::SAMPLER,
            )
            .build(
                render_device,
                &Self::shader_resource_layout(render_device),
            )
    }

    fn depth_write_enabled() -> bool {
        false
    }

    fn depth_compare() -> CompareFunction {
        CompareFunction::LessEqual
    }

    fn cull_mode() -> Option<Face> {
        Some(Face::Front)
    }

    fn load_op() -> LoadOp<GpuColor> {
        LoadOp::Clear(GpuColor::BLACK)
    }
}

#[derive(Debug, Component)]
pub struct ColorMaterial {
    color: Color,
    color_buffer: BufferHandle,
    dirty: bool,
}

impl ColorMaterial {
    pub fn new(
        color: Color,
        render_device: &RenderDevice,
        registry: &mut RenderRegistry,
    ) -> ColorMaterial {
        let color_buffer = registry
            .new_buffer::<Vec3>(render_device, 1, BufferUsages::UNIFORM)
            .and_then_mut(registry, |b| b.fill(render_device, 0, &[color.0]));

        ColorMaterial {
            color,
            color_buffer,
            dirty: false,
        }
    }

    pub fn release(&self, registry: &mut RenderRegistry) {
        registry.remove_buffer(self.color_buffer);
    }

    pub fn update_color(
        &mut self,
        new_color: Color,
        render_device: &RenderDevice,
        registry: &RenderRegistry,
    ) {
        self.color = new_color;
        registry
            .get_buffer(self.color_buffer)
            .expect("Cannot update color buffer")
            .fill_exact(render_device, 0, &[new_color.0])
            .expect("Failed to update color buffer");
    }

    // Sets color without updating the inner buffer. Must call `update`
    // afterwards or just use single `update_color` method
    pub fn set_color(&mut self, color: Color) {
        self.color = color;
        self.dirty = true;
    }

    pub fn update(
        &self, 
        registry: &RenderRegistry, 
        render_device: &RenderDevice,
    ) {
        if !self.dirty {
            return;
        }

        registry
            .get_buffer(self.color_buffer)
            .expect("Cannot update color buffer")
            .fill_exact(render_device, 0, &[self.color.0])
            .expect("Failed to update color buffer");
    }
}

impl Material for ColorMaterial {
    fn shader() -> ShaderModuleDescriptor<'static> {
        include_wgsl!("../../assets/shaders/color.wgsl")
    }

    fn vertex_layout() -> Option<VertexBufferLayout<'static>> {
        Some(Vertex::vertex_buffer_layout())
    }

    fn shader_resource_layout(render_device: &RenderDevice) -> ShaderResourceLayout {
        ShaderResourceLayout::builder()
            .with_label("Color Material")
            .with_buffer(&BufferResourceDescriptor {
                visibility: ShaderStages::FRAGMENT,
                buffer_type: BufferBindingType::Uniform,
            })
            .build(render_device)
    }

    fn shader_resource(
        &self, 
        render_device: &RenderDevice,
        registry: &RenderRegistry,
    ) -> ShaderResource {
        ShaderResource::builder()
            .with_buffer(registry.get_buffer(self.color_buffer).unwrap())
            .build(
                render_device, 
                &Self::shader_resource_layout(render_device),
            )
    }
}

#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Pod, Zeroable)]
pub struct TerrainPalette {
    pub grass: [f32; 4],
    pub soil: [f32; 4],
    pub rocky: [f32; 4],
    pub cliff: [f32; 4],
    pub sky: [f32; 4],
    pub sharpness: f32,
    pub macro_strength: f32,
    pub macro_wavelength: f32,
    pub fog_density: f32,
    pub texture_scale: f32,
    pub border_noise: f32,
    pub height_blend: f32,
    pub detail_distance: f32,
    pub far_distance: f32,
    pub far_scale: f32,
    pub stochastic: f32,
    pub padding: f32,
}

impl Default for TerrainPalette {
    fn default() -> Self {
        TerrainPalette {
            grass: [1.0; 4],
            soil: [1.0; 4],
            rocky: [1.0; 4],
            cliff: [1.0; 4],
            sky: [0.55, 0.68, 0.85, 1.0],
            sharpness: 1.5,
            macro_strength: 0.25,
            macro_wavelength: 300.0,
            fog_density: 0.00012,
            texture_scale: 4.0,
            border_noise: 0.4,
            height_blend: 0.25,
            detail_distance: 150.0,
            far_distance: 2500.0,
            far_scale: 8.0,
            stochastic: 1.0,
            padding: 0.0,
        }
    }
}

pub struct TerrainLayerFiles<'a> {
    pub color: &'a str,
    pub normal: &'a str,
    pub height: &'a str,
    pub occlusion: &'a str,
}

#[derive(Debug, Clone, Copy)]
pub struct TerrainTextures {
    pub albedo: TextureHandle,
    pub normal: TextureHandle,
}

#[derive(Debug, thiserror::Error)]
pub enum TerrainTextureError {
    #[error("cannot load `{path}`: {source}")]
    Image { path: String, source: ImageError },
    #[error("terrain needs at least one layer")]
    NoLayers,
}

fn open_rgba(path: &str, size: Option<(u32, u32)>) -> Result<image::RgbaImage, TerrainTextureError> {
    let image = image::open(path)
        .map_err(|source| TerrainTextureError::Image { path: path.to_owned(), source })?
        .to_rgba8();

    Ok(match size {
        Some((w, h)) if image.dimensions() != (w, h) => {
            image::imageops::resize(&image, w, h, image::imageops::FilterType::Triangle)
        }
        _ => image,
    })
}

fn pack_layer(files: &TerrainLayerFiles) -> Result<(image::RgbaImage, image::RgbaImage), TerrainTextureError> {
    let mut albedo = open_rgba(files.color, None)?;
    let size = Some(albedo.dimensions());
    let height = open_rgba(files.height, size)?;
    let mut normal = open_rgba(files.normal, size)?;
    let occlusion = open_rgba(files.occlusion, size)?;

    for (pixel, h) in albedo.pixels_mut().zip(height.pixels()) {
        pixel.0[3] = h.0[0];
    }
    for (pixel, ao) in normal.pixels_mut().zip(occlusion.pixels()) {
        pixel.0[3] = ao.0[0];
    }

    Ok((albedo, normal))
}

impl TerrainTextures {
    pub fn load(
        layers: &[TerrainLayerFiles],
        render_device: &RenderDevice,
        registry: &mut RenderRegistry,
    ) -> Result<TerrainTextures, TerrainTextureError> {
        use rayon::prelude::*;

        let packed: Vec<_> = layers.par_iter().map(pack_layer).collect::<Result<_, _>>()?;
        let size = packed.first().ok_or(TerrainTextureError::NoLayers)?.0.dimensions();

        let resize = |image: &image::RgbaImage| {
            if image.dimensions() == size {
                image.clone()
            } else {
                image::imageops::resize(image, size.0, size.1, image::imageops::FilterType::Triangle)
            }
        };
        let albedo: Vec<_> = packed.iter().map(|(a, _)| resize(a)).collect();
        let normal: Vec<_> = packed.iter().map(|(_, n)| resize(n)).collect();

        let descriptor = |label: &str, format| TextureDescriptor {
            label: label.to_owned(),
            format,
            filter: FilterMode::Linear,
            mipmap_filter: FilterMode::Linear,
            address_mode: AddressMode::Repeat,
            anisotropy: 8,
            ..Default::default()
        };

        let albedo = registry
            .new_texture_array(render_device, &albedo, descriptor("Terrain albedo", TextureFormat::Rgba8UnormSrgb))
            .ok_or(TerrainTextureError::NoLayers)?;
        let normal = registry
            .new_texture_array(render_device, &normal, descriptor("Terrain normal", TextureFormat::Rgba8Unorm))
            .ok_or(TerrainTextureError::NoLayers)?;

        Ok(TerrainTextures { albedo, normal })
    }
}

#[derive(Debug, Clone, Component)]
pub struct TerrainMaterial {
    palette_buffer: BufferHandle,
    textures: TerrainTextures,
}

impl TerrainMaterial {
    pub fn new(
        palette: &TerrainPalette,
        textures: TerrainTextures,
        render_device: &RenderDevice,
        registry: &mut RenderRegistry,
    ) -> TerrainMaterial {
        let palette_buffer = registry
            .new_buffer::<TerrainPalette>(render_device, 1, BufferUsages::UNIFORM)
            .and_then_mut(registry, |b| b.fill(render_device, 0, std::slice::from_ref(palette)));

        TerrainMaterial { palette_buffer, textures }
    }

    pub fn write(&self, palette: &TerrainPalette, render_device: &RenderDevice, registry: &RenderRegistry) {
        registry
            .get_buffer(self.palette_buffer)
            .expect("Cannot update terrain palette buffer")
            .fill_exact(render_device, 0, std::slice::from_ref(palette))
            .expect("Failed to update terrain palette buffer");
    }

    pub fn release(&self, registry: &mut RenderRegistry) {
        registry.remove_buffer(self.palette_buffer);
    }
}

fn layer_array(format: TextureFormat) -> TextureResourceDescriptor {
    TextureResourceDescriptor {
        usage: TextureResourceUsage::TEXTURE | TextureResourceUsage::SAMPLER,
        sample_type: Some(TextureSampleType::Float { filterable: true }),
        sampler_binding_type: Some(SamplerBindingType::Filtering),
        dimension: TextureDimension::D2,
        view_dimension: TextureViewDimension::D2Array,
        format,
    }
}

impl Material for TerrainMaterial {
    fn shader() -> ShaderModuleDescriptor<'static> {
        include_wgsl!("../../assets/shaders/terrain.wgsl")
    }

    fn vertex_layout() -> Option<VertexBufferLayout<'static>> {
        Some(TerrainVertex::vertex_buffer_layout())
    }

    fn shader_resource_layout(render_device: &RenderDevice) -> ShaderResourceLayout {
        ShaderResourceLayout::builder()
            .with_label("Terrain Material")
            .with_buffer(&BufferResourceDescriptor {
                visibility: ShaderStages::FRAGMENT,
                buffer_type: BufferBindingType::Uniform,
            })
            .with_texture(&layer_array(TextureFormat::Rgba8UnormSrgb))
            .with_texture(&layer_array(TextureFormat::Rgba8Unorm))
            .build(render_device)
    }

    fn shader_resource(
        &self,
        render_device: &RenderDevice,
        registry: &RenderRegistry,
    ) -> ShaderResource {
        let usage = TextureResourceUsage::TEXTURE | TextureResourceUsage::SAMPLER;
        ShaderResource::builder()
            .with_buffer(registry.get_buffer(self.palette_buffer).unwrap())
            .with_texture(registry.get_texture(self.textures.albedo).unwrap(), usage)
            .with_texture(registry.get_texture(self.textures.normal).unwrap(), usage)
            .build(
                render_device,
                &Self::shader_resource_layout(render_device),
            )
    }
}
