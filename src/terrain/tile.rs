use flecs_ecs::macros::Component;
use glam::{Quat, Vec2, Vec3};
use noise::{NoiseFn, Perlin};
use rayon::prelude::*;

use super::{config::TerrainConfig, source::HeightSource, splat::SplatRules};

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
    pub normals: Box<[Vec3]>,
    pub splat: Box<[[u8; 4]]>,
    pub holes: Option<Box<[u8]>>,
    pub height_bounds: HeightBounds,
    pub objects: Vec<PlacedObject>,
}

impl TileData {
    pub fn generate(config: &TerrainConfig, coord: TileCoord, source: &dyn HeightSource) -> TileData {
        Self::generate_with(config, coord, source, &SplatRules::default())
    }

    pub fn generate_with(
        config: &TerrainConfig,
        coord: TileCoord,
        source: &dyn HeightSource,
        rules: &SplatRules,
    ) -> TileData {
        let samples = config.samples();
        let cell = config.cell_size();
        let reach = ((rules.feature_radius / cell).round() as usize).clamp(1, 64);
        let apron = reach as i64;
        let padded = samples + 2 * reach;
        let cells = config.cells as i64;
        let cell_size = config.tile_size as f64 / config.cells as f64;

        let mut grid = vec![0u16; padded * padded];
        grid.par_chunks_mut(padded).enumerate().for_each(|(pj, row)| {
            let gz = coord.y as i64 * cells + pj as i64 - apron;
            for (pi, sample) in row.iter_mut().enumerate() {
                let gx = coord.x as i64 * cells + pi as i64 - apron;
                *sample = config.quantize(source.height(gx as f64 * cell_size, gz as f64 * cell_size));
            }
        });

        let heights: Vec<u16> = (0..samples)
            .flat_map(|j| {
                let start = (j + reach) * padded + reach;
                grid[start..start + samples].iter().copied()
            })
            .collect();

        let height = |x: usize, y: usize| config.dequantize(grid[y * padded + x]);
        let normals: Vec<Vec3> = (0..samples * samples)
            .into_par_iter()
            .map(|k| {
                let (x, y) = (k % samples + reach, k / samples + reach);
                let dx = (height(x + 1, y) - height(x - 1, y)) / (2.0 * cell);
                let dz = (height(x, y + 1) - height(x, y - 1)) / (2.0 * cell);
                Vec3::new(-dx, 1.0, -dz).normalize()
            })
            .collect();

        let breakup = Perlin::new((config.seed ^ (config.seed >> 32) ^ 0x5EED) as u32);
        let wavelength = rules.breakup_wavelength.max(1.0) as f64;
        let splat: Vec<[u8; 4]> = (0..samples * samples)
            .into_par_iter()
            .map(|k| {
                let (x, y) = (k % samples + reach, k / samples + reach);
                let r = reach;
                let span = r as f32 * cell;
                let h = height(x, y);

                let dx = (height(x + r, y) - height(x - r, y)) / (2.0 * span);
                let dz = (height(x, y + r) - height(x, y - r)) / (2.0 * span);
                let slope = 1.0 - 1.0 / (1.0 + dx * dx + dz * dz).sqrt();

                let around = (height(x + r, y) + height(x - r, y) + height(x, y + r) + height(x, y - r)) * 0.25;
                let concavity = (around - h) / span;

                let gx = (coord.x as i64 * cells + x as i64 - apron) as f64 * cell_size;
                let gz = (coord.y as i64 * cells + y as i64 - apron) as f64 * cell_size;
                let noise = breakup.get([gx / wavelength, gz / wavelength]) as f32;

                rules.weights(h, slope, concavity, noise).pack()
            })
            .collect();

        let mut tile = TileData {
            coord,
            samples,
            heights: heights.into_boxed_slice(),
            normals: normals.into_boxed_slice(),
            splat: splat.into_boxed_slice(),
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
    fn neighbours_share_border_normals() {
        let (config, noise) = (config(), noise());
        let n = config.samples();

        let left = TileData::generate(&config, TileCoord::new(-1, 0), &noise);
        let right = TileData::generate(&config, TileCoord::new(0, 0), &noise);
        let top = TileData::generate(&config, TileCoord::new(0, 1), &noise);

        for j in 0..n {
            assert_eq!(left.normals[j * n + n - 1], right.normals[j * n], "row {j}");
        }
        for i in 0..n {
            assert_eq!(right.normals[(n - 1) * n + i], top.normals[i], "column {i}");
        }
    }

    #[test]
    fn neighbours_share_border_splat() {
        let (config, noise) = (config(), noise());
        let n = config.samples();

        let left = TileData::generate(&config, TileCoord::new(2, -1), &noise);
        let right = TileData::generate(&config, TileCoord::new(3, -1), &noise);

        for j in 0..n {
            assert_eq!(left.splat[j * n + n - 1], right.splat[j * n], "row {j}");
        }
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
