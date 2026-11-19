use flecs_ecs::macros::Component;
use glam::{Quat, Vec2, Vec3};
use rayon::prelude::*;

use super::{config::TerrainConfig, source::HeightSource};

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct TileCoord {
    pub x: i32,
    pub y: i32,
}

impl TileCoord {
    pub const fn new(x: i32, y: i32) -> Self {
        TileCoord { x, y }
    }

    pub fn offset(self, dx: i32, dy: i32) -> Self {
        TileCoord { x: self.x + dx, y: self.y + dy }
    }

    pub fn containing(position: Vec2, tile_size: f32) -> Self {
        TileCoord {
            x: (position.x / tile_size).floor() as i32,
            y: (position.y / tile_size).floor() as i32,
        }
    }

    pub fn origin(self, tile_size: f32) -> Vec2 {
        Vec2::new(self.x as f32 * tile_size, self.y as f32 * tile_size)
    }

    pub fn chebyshev_distance(self, other: TileCoord) -> u32 {
        self.x.abs_diff(other.x).max(self.y.abs_diff(other.y))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Edge {
    MinX,
    MaxX,
    MinY,
    MaxY,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PlacedObject {
    pub kind: u32,
    pub position: Vec3,
    pub rotation: Quat,
    pub scale: f32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct HeightBounds {
    pub min: f32,
    pub max: f32,
}

#[derive(Debug, Clone)]
pub struct TileData {
    pub coord: TileCoord,
    pub samples: usize,
    pub heights: Box<[u16]>,
    pub splat: Box<[[u8; 4]]>,
    pub holes: Option<Box<[u8]>>,
    pub height_bounds: HeightBounds,
    pub objects: Vec<PlacedObject>,
}

impl TileData {
    pub fn generate(config: &TerrainConfig, coord: TileCoord, source: &dyn HeightSource) -> TileData {
        let samples = config.samples();
        let cells = config.cells as i64;
        let cell_size = config.tile_size as f64 / config.cells as f64;

        let mut heights = vec![0u16; samples * samples];
        heights.par_chunks_mut(samples).enumerate().for_each(|(j, row)| {
            let gz = coord.y as i64 * cells + j as i64;
            for (i, sample) in row.iter_mut().enumerate() {
                let gx = coord.x as i64 * cells + i as i64;
                *sample = config.quantize(source.height(gx as f64 * cell_size, gz as f64 * cell_size));
            }
        });

        let mut tile = TileData {
            coord,
            samples,
            heights: heights.into_boxed_slice(),
            splat: vec![[255, 0, 0, 0]; samples * samples].into_boxed_slice(),
            holes: None,
            height_bounds: HeightBounds { min: config.height_min, max: config.height_min },
            objects: Vec::new(),
        };
        tile.height_bounds = tile.compute_bounds(config);
        tile
    }

    pub fn compute_bounds(&self, config: &TerrainConfig) -> HeightBounds {
        let min = self.heights.iter().copied().min().unwrap_or(0);
        let max = self.heights.iter().copied().max().unwrap_or(0);
        HeightBounds { min: config.dequantize(min), max: config.dequantize(max) }
    }

    pub fn raw(&self, i: usize, j: usize) -> u16 {
        self.heights[j * self.samples + i]
    }

    pub fn height(&self, config: &TerrainConfig, i: usize, j: usize) -> f32 {
        config.dequantize(self.raw(i, j))
    }

    pub fn edge(&self, edge: Edge) -> Vec<u16> {
        let n = self.samples;
        match edge {
            Edge::MinX => (0..n).map(|j| self.raw(0, j)).collect(),
            Edge::MaxX => (0..n).map(|j| self.raw(n - 1, j)).collect(),
            Edge::MinY => (0..n).map(|i| self.raw(i, 0)).collect(),
            Edge::MaxY => (0..n).map(|i| self.raw(i, n - 1)).collect(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::terrain::source::NoiseSource;
    use noise::{Fbm, Perlin};

    fn config() -> TerrainConfig {
        TerrainConfig { cells: 32, tile_size: 100.0, lod_levels: 3, ..Default::default() }
    }

    fn noise() -> NoiseSource<Fbm<Perlin>> {
        NoiseSource::fbm(7, 400.0, 300.0, 40.0, 4)
    }

    #[test]
    fn horizontal_neighbours_share_their_border() {
        let (config, noise) = (config(), noise());

        for x in [-3, -1, 0, 5] {
            let left = TileData::generate(&config, TileCoord::new(x, 2), &noise);
            let right = TileData::generate(&config, TileCoord::new(x + 1, 2), &noise);
            assert_eq!(left.edge(Edge::MaxX), right.edge(Edge::MinX), "x = {x}");
        }
    }

    #[test]
    fn vertical_neighbours_share_their_border() {
        let (config, noise) = (config(), noise());

        for y in [-4, -1, 0, 3] {
            let top = TileData::generate(&config, TileCoord::new(1, y), &noise);
            let bottom = TileData::generate(&config, TileCoord::new(1, y + 1), &noise);
            assert_eq!(top.edge(Edge::MaxY), bottom.edge(Edge::MinY), "y = {y}");
        }
    }

    #[test]
    fn four_tiles_agree_on_the_shared_corner() {
        let (config, noise) = (config(), noise());
        let last = config.samples() - 1;

        let corners = [
            TileData::generate(&config, TileCoord::new(-1, -1), &noise).raw(last, last),
            TileData::generate(&config, TileCoord::new(0, -1), &noise).raw(0, last),
            TileData::generate(&config, TileCoord::new(-1, 0), &noise).raw(last, 0),
            TileData::generate(&config, TileCoord::new(0, 0), &noise).raw(0, 0),
        ];

        assert!(corners.iter().all(|c| *c == corners[0]), "{corners:?}");
    }

    #[test]
    fn samples_match_the_source_at_world_coordinates() {
        let (config, noise) = (config(), noise());
        let coord = TileCoord::new(-2, 1);
        let tile = TileData::generate(&config, coord, &noise);
        let cell = config.tile_size as f64 / config.cells as f64;

        for (i, j) in [(0usize, 0usize), (5, 9), (31, 17), (32, 32)] {
            let x = (coord.x as i64 * config.cells as i64 + i as i64) as f64 * cell;
            let z = (coord.y as i64 * config.cells as i64 + j as i64) as f64 * cell;
            assert_eq!(tile.raw(i, j), config.quantize(noise.height(x, z)), "({i}, {j})");
        }
    }

    #[test]
    fn bounds_cover_every_sample() {
        let (config, noise) = (config(), noise());
        let tile = TileData::generate(&config, TileCoord::new(0, 0), &noise);
        let HeightBounds { min, max } = tile.height_bounds;

        assert!(min <= max);
        for j in 0..tile.samples {
            for i in 0..tile.samples {
                let h = tile.height(&config, i, j);
                assert!(h >= min && h <= max);
            }
        }
    }

    #[test]
    fn containing_handles_negative_positions() {
        assert_eq!(TileCoord::containing(Vec2::new(-0.5, 0.0), 100.0), TileCoord::new(-1, 0));
        assert_eq!(TileCoord::containing(Vec2::new(-100.0, 99.9), 100.0), TileCoord::new(-1, 0));
        assert_eq!(TileCoord::containing(Vec2::new(100.0, -100.1), 100.0), TileCoord::new(1, -2));
    }

    #[test]
    fn tile_buffers_have_the_expected_size() {
        let (config, noise) = (config(), noise());
        let tile = TileData::generate(&config, TileCoord::new(0, 0), &noise);
        let n = config.samples() * config.samples();

        assert_eq!(tile.heights.len(), n);
        assert_eq!(tile.splat.len(), n);
        assert_eq!(config.tile_bytes(), n * 6);
    }
}
