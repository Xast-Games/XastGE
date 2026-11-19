use xastge::terrain::config::*;
use xastge::terrain::*;

#[test]
fn default_config_is_valid() {
    assert_eq!(TerrainConfig::default().validate(), Ok(()));
}

#[test]
fn rejects_bad_cells() {
    for cells in [0, 1, 3, 100, 513] {
        let config = TerrainConfig { cells, ..Default::default() };
        assert_eq!(config.validate(), Err(ConfigError::Cells(cells)), "cells = {cells}");
    }
}

#[test]
fn rejects_inverted_height_range() {
    let config = TerrainConfig { height_min: 10.0, height_max: 10.0, ..Default::default() };
    assert_eq!(config.validate(), Err(ConfigError::HeightRange));
}

#[test]
fn rejects_too_many_lods() {
    let config = TerrainConfig { cells: 16, lod_levels: 6, ..Default::default() };
    assert_eq!(config.validate(), Err(ConfigError::LodLevels { cells: 16, max: 5 }));
}

#[test]
fn rejects_unload_inside_load_radius() {
    let config = TerrainConfig { load_radius: 4, unload_radius: 3, ..Default::default() };
    assert_eq!(config.validate(), Err(ConfigError::Radii));
}

#[test]
fn quantization_round_trips_within_half_a_step() {
    let config = TerrainConfig::default();
    let half_step = config.height_precision() * 0.5 + 1e-4;

    for height in [-100.0, -42.5, 0.0, 123.456, 999.9, 1900.0] {
        let back = config.dequantize(config.quantize(height));
        assert!((back - height).abs() <= half_step, "{height} -> {back}");
    }
}

#[test]
fn quantization_clamps_out_of_range_heights() {
    let config = TerrainConfig::default();
    assert_eq!(config.quantize(-5000.0), 0);
    assert_eq!(config.quantize(9000.0), u16::MAX);
}
