use std::{
    path::{Path, PathBuf},
    sync::Arc,
};

use flecs_ecs::macros::Component;
use glam::{Mat3, Mat4, Vec2, Vec3, Vec4};
use image::RgbaImage;
use thiserror::Error;

use crate::mesh::{Mesh, Vertex};

use super::{
    RenderDevice,
    lit::{LitMaterial, LitParams},
    registry::RenderRegistry,
    texture::{TextureDescriptor, TextureHandle},
    types::*,
    updated,
};

#[derive(Debug, Error)]
pub enum ModelError {
    #[error("cannot load model {path}: {source}")]
    Gltf { path: PathBuf, source: gltf::Error },
    #[error("model {path}: {message}")]
    Format { path: PathBuf, message: String },
}

#[derive(Debug, Clone, PartialEq)]
pub struct SurfaceData {
    pub name: Option<String>,
    pub base_color: Vec4,
    pub texture: Option<usize>,
    pub roughness: f32,
}

impl Default for SurfaceData {
    fn default() -> Self {
        SurfaceData { name: None, base_color: Vec4::ONE, texture: None, roughness: 0.8 }
    }
}

#[derive(Debug, Clone)]
pub struct PartData {
    pub vertices: Vec<Vertex>,
    pub indices: Vec<u32>,
    pub surface: usize,
}

#[derive(Debug, Clone, Default)]
pub struct ModelData {
    pub parts: Vec<PartData>,
    pub surfaces: Vec<SurfaceData>,
    pub images: Vec<Option<RgbaImage>>,
}

impl ModelData {
    pub fn load(path: impl AsRef<Path>) -> Result<ModelData, ModelError> {
        let path = path.as_ref();
        let (document, buffers, images) =
            gltf::import(path).map_err(|source| ModelError::Gltf { path: path.to_owned(), source })?;

        let scene = document
            .default_scene()
            .or_else(|| document.scenes().next())
            .ok_or_else(|| ModelError::Format { path: path.to_owned(), message: "has no scene".to_owned() })?;

        let mut model = ModelData {
            parts: Vec::new(),
            surfaces: document.materials().map(|m| surface(&m)).collect(),
            images: images.iter().map(rgba).collect(),
        };
        let fallback = model.surfaces.len();
        model.surfaces.push(SurfaceData::default());

        for node in scene.nodes() {
            model.collect(&node, Mat4::IDENTITY, &buffers, fallback, 0);
        }
        Ok(model)
    }

    fn collect(&mut self, node: &gltf::Node, parent: Mat4, buffers: &[gltf::buffer::Data], fallback: usize, depth: usize) {
        if depth > 64 {
            return;
        }

        let world = parent * Mat4::from_cols_array_2d(&node.transform().matrix());
        if let Some(mesh) = node.mesh() {
            for primitive in mesh.primitives() {
                if primitive.mode() != gltf::mesh::Mode::Triangles {
                    continue;
                }
                let surface = primitive.material().index().unwrap_or(fallback);
                if let Some(part) = part(&primitive, buffers, world, surface) {
                    self.parts.push(part);
                }
            }
        }

        for child in node.children() {
            self.collect(&child, world, buffers, fallback, depth + 1);
        }
    }
}

fn surface(material: &gltf::Material) -> SurfaceData {
    let pbr = material.pbr_metallic_roughness();
    SurfaceData {
        name: material.name().map(str::to_owned),
        base_color: Vec4::from_array(pbr.base_color_factor()),
        texture: pbr.base_color_texture().map(|info| info.texture().source().index()),
        roughness: pbr.roughness_factor(),
    }
}

fn rgba(image: &gltf::image::Data) -> Option<RgbaImage> {
    use gltf::image::Format;

    let pixels = &image.pixels;
    let data: Vec<u8> = match image.format {
        Format::R8G8B8A8 => pixels.clone(),
        Format::R8G8B8 => pixels.chunks_exact(3).flat_map(|p| [p[0], p[1], p[2], 255]).collect(),
        Format::R8G8 => pixels.chunks_exact(2).flat_map(|p| [p[0], p[0], p[0], p[1]]).collect(),
        Format::R8 => pixels.iter().flat_map(|&v| [v, v, v, 255]).collect(),
        _ => return None,
    };
    RgbaImage::from_raw(image.width, image.height, data)
}

fn smooth_normals(positions: &[Vec3], indices: &[u32]) -> Vec<Vec3> {
    let mut normals = vec![Vec3::ZERO; positions.len()];
    for tri in indices.chunks_exact(3) {
        let [a, b, c] = [tri[0] as usize, tri[1] as usize, tri[2] as usize];
        let face = (positions[b] - positions[a]).cross(positions[c] - positions[a]);
        normals[a] += face;
        normals[b] += face;
        normals[c] += face;
    }
    normals.into_iter().map(|n| n.normalize_or(Vec3::Y)).collect()
}

fn part(primitive: &gltf::Primitive, buffers: &[gltf::buffer::Data], world: Mat4, surface: usize) -> Option<PartData> {
    let reader = primitive.reader(|buffer| buffers.get(buffer.index()).map(|data| &data.0[..]));

    let positions: Vec<Vec3> = reader.read_positions()?.map(Vec3::from_array).collect();
    let mut indices: Vec<u32> = match reader.read_indices() {
        Some(indices) => indices.into_u32().collect(),
        None => (0..positions.len() as u32).collect(),
    };
    indices.truncate(indices.len() / 3 * 3);
    if indices.is_empty() || indices.iter().any(|&i| i as usize >= positions.len()) {
        return None;
    }

    let normals = reader
        .read_normals()
        .map(|normals| normals.map(Vec3::from_array).collect::<Vec<_>>())
        .filter(|normals| normals.len() == positions.len())
        .unwrap_or_else(|| smooth_normals(&positions, &indices));
    let texcoords: Vec<Vec2> = reader
        .read_tex_coords(0)
        .map(|coords| coords.into_f32().map(Vec2::from_array).collect())
        .unwrap_or_default();

    let normal_matrix = Mat3::from_mat4(world).inverse().transpose();
    if world.determinant() < 0.0 {
        for tri in indices.chunks_exact_mut(3) {
            tri.swap(1, 2);
        }
    }

    let vertices = positions
        .iter()
        .zip(&normals)
        .enumerate()
        .map(|(i, (position, normal))| Vertex {
            position: world.transform_point3(*position),
            normal: (normal_matrix * *normal).normalize_or(Vec3::Y),
            texcoord: texcoords.get(i).copied().unwrap_or(Vec2::ZERO),
        })
        .collect();

    Some(PartData { vertices, indices, surface })
}

#[derive(Debug, Clone)]
pub struct ModelPart {
    pub mesh: Mesh<Vertex>,
    pub material: LitMaterial,
}

#[derive(Component, Debug, Clone)]
pub struct Model {
    pub parts: Arc<Vec<ModelPart>>,
}

fn texture_descriptor() -> TextureDescriptor {
    TextureDescriptor {
        label: "Model".to_owned(),
        filter: FilterMode::Linear,
        mipmap_filter: FilterMode::Linear,
        address_mode: AddressMode::Repeat,
        anisotropy: 8,
        ..Default::default()
    }
}

impl Model {
    pub fn new(parts: Vec<ModelPart>) -> Model {
        Model { parts: Arc::new(parts) }
    }

    pub fn upload(data: &ModelData, render_device: &mut RenderDevice, registry: &mut RenderRegistry) -> Model {
        let mut textures: Vec<Option<TextureHandle>> = Vec::with_capacity(data.images.len());
        for image in &data.images {
            textures.push(image.as_ref().map(|image| registry.new_texture_2d(render_device, image, texture_descriptor())));
        }

        let mut materials = Vec::with_capacity(data.surfaces.len());
        for surface in &data.surfaces {
            let albedo = match surface.texture.and_then(|i| textures.get(i).copied().flatten()) {
                Some(texture) => texture,
                None => registry.white_texture(render_device),
            };
            let params = LitParams::new(surface.base_color, surface.roughness);
            materials.push(LitMaterial::new(params, albedo, render_device, registry));
        }

        let mut parts = Vec::with_capacity(data.parts.len());
        for part in &data.parts {
            let Some(material) = materials.get(part.surface) else {
                continue;
            };
            let mesh = Mesh::new(part.vertices.clone(), part.indices.clone());
            parts.push(ModelPart { mesh: updated(mesh, render_device, registry), material: material.clone() });
        }

        Model::new(parts)
    }
}
