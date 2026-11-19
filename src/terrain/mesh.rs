use bytemuck::{Pod, Zeroable};
use glam::Vec3;
use rayon::prelude::*;

use crate::mesh::Mesh;

use super::{config::TerrainConfig, tile::TileData};

#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Pod, Zeroable)]
pub struct TerrainVertex {
    pub position: Vec3,
    pub normal: Vec3,
    pub splat: [u8; 4],
    pub hints: [u8; 4],
}

#[cfg(feature = "render")]
impl TerrainVertex {
    const ATTRIBS: &[wgpu::VertexAttribute] = &wgpu::vertex_attr_array![
        0 => Float32x3,
        1 => Float32x3,
        2 => Unorm8x4,
        3 => Unorm8x4,
    ];

    pub fn vertex_buffer_layout() -> wgpu::VertexBufferLayout<'static> {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<TerrainVertex>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: Self::ATTRIBS,
        }
    }
}

pub fn tile_mesh(config: &TerrainConfig, tile: &TileData) -> Mesh<TerrainVertex> {
    let n = tile.samples;
    let last = n - 1;
    let cell = config.cell_size();

    let vertices: Vec<TerrainVertex> = (0..n * n)
        .into_par_iter()
        .map(|k| {
            let (i, j) = (k % n, k / n);
            TerrainVertex {
                position: Vec3::new(i as f32 * cell, tile.height(config, i, j), j as f32 * cell),
                normal: tile.normals[k],
                splat: tile.splat[k],
                hints: [tile.hints[k][0], tile.hints[k][1], 0, 0],
            }
        })
        .collect();

    let stride = n as u32;
    let mut indices = Vec::with_capacity(last * last * 6);
    for j in 0..last as u32 {
        for i in 0..last as u32 {
            let i00 = j * stride + i;
            let i10 = i00 + 1;
            let i01 = i00 + stride;
            let i11 = i01 + 1;
            indices.extend_from_slice(&[i00, i01, i10, i10, i01, i11]);
        }
    }

    Mesh::new(vertices, indices)
}
