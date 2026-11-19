use glam::{Vec2, Vec3};
use rayon::prelude::*;

use crate::mesh::{Mesh, Vertex};

use super::{config::TerrainConfig, tile::TileData};

pub fn tile_mesh(config: &TerrainConfig, tile: &TileData) -> Mesh<Vertex> {
    let n = tile.samples;
    let last = n - 1;
    let cell = config.cell_size();
    let height = |i: usize, j: usize| tile.height(config, i, j);

    let vertices: Vec<Vertex> = (0..n * n)
        .into_par_iter()
        .map(|k| {
            let (i, j) = (k % n, k / n);
            let (left, right) = (i.saturating_sub(1), (i + 1).min(last));
            let (down, up) = (j.saturating_sub(1), (j + 1).min(last));
            let dx = (height(right, j) - height(left, j)) / ((right - left) as f32 * cell);
            let dz = (height(i, up) - height(i, down)) / ((up - down) as f32 * cell);

            Vertex {
                position: Vec3::new(i as f32 * cell, height(i, j), j as f32 * cell),
                normal: Vec3::new(-dx, 1.0, -dz).normalize(),
                texcoord: Vec2::new(i as f32 / last as f32, j as f32 / last as f32),
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
