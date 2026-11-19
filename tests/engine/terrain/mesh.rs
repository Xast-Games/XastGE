use glam::Vec3;
use xastge::terrain::mesh::*;
use xastge::terrain::*;
use xastge::terrain::{source::{FlatSource, HeightSource}, tile::TileCoord};

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
