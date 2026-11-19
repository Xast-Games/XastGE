use glam::Vec2;
use xastge::terrain::tile::*;
use xastge::terrain::*;
use xastge::terrain::source::NoiseSource;
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
