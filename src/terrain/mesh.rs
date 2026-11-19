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
}

#[cfg(feature = "render")]
impl TerrainVertex {
    const ATTRIBS: &[wgpu::VertexAttribute] = &wgpu::vertex_attr_array![
        0 => Float32x3,
        1 => Float32x3,
        2 => Unorm8x4,
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::terrain::{source::{FlatSource, HeightSource}, tile::TileCoord};

    struct Ramp;

    impl HeightSource for Ramp {
        fn height(&self, x: f64, _z: f64) -> f32 {
            x as f32
        }
    }

    struct Steep;

    impl HeightSource for Steep {
        fn height(&self, x: f64, _z: f64) -> f32 {
            x as f32 * 3.0
        }
    }

    fn config() -> TerrainConfig {
        TerrainConfig { cells: 16, tile_size: 64.0, lod_levels: 3, ..Default::default() }
    }

    #[test]
    fn flat_tile_has_upward_normals_and_faces_up() {
        let config = config();
        let tile = TileData::generate(&config, TileCoord::new(0, 0), &FlatSource(12.0));
        let mesh = tile_mesh(&config, &tile);

        assert_eq!(mesh.vertices.len(), 17 * 17);
        assert_eq!(mesh.indices.len(), 16 * 16 * 6);

        for v in &mesh.vertices {
            assert!((v.position.y - 12.0).abs() < 0.05);
            assert!((v.normal - Vec3::Y).length() < 1e-5);
            assert_eq!(v.splat, [255, 0, 0, 0]);
        }

        for tri in mesh.indices.chunks(3) {
            let a = mesh.vertices[tri[0] as usize].position;
            let b = mesh.vertices[tri[1] as usize].position;
            let c = mesh.vertices[tri[2] as usize].position;
            assert!((b - a).cross(c - a).y > 0.0);
        }
    }

    #[test]
    fn slope_tilts_normals_against_the_gradient() {
        let config = config();
        let tile = TileData::generate(&config, TileCoord::new(0, 0), &Ramp);
        let mesh = tile_mesh(&config, &tile);
        let expected = Vec3::new(-1.0, 1.0, 0.0).normalize();

        for v in &mesh.vertices {
            assert!((v.normal - expected).length() < 1e-2, "{:?}", v.normal);
        }
    }

    #[test]
    fn steep_slope_is_painted_as_cliff() {
        let config = config();
        let tile = TileData::generate(&config, TileCoord::new(0, 0), &Steep);
        let mesh = tile_mesh(&config, &tile);

        assert!(mesh.vertices.iter().all(|v| v.splat[3] > 200), "{:?}", mesh.vertices[0].splat);
    }

    #[test]
    fn positions_are_local_to_the_tile() {
        let config = config();
        let tile = TileData::generate(&config, TileCoord::new(-3, 5), &FlatSource(0.0));
        let mesh = tile_mesh(&config, &tile);

        let first = mesh.vertices.first().unwrap().position;
        let last = mesh.vertices.last().unwrap().position;
        assert_eq!((first.x, first.z), (0.0, 0.0));
        assert_eq!((last.x, last.z), (config.tile_size, config.tile_size));
    }
}
