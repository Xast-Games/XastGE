use flecs_ecs::macros::Component;
use thiserror::Error;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum ConfigError {
    #[error("tile size must be positive and finite")]
    TileSize,
    #[error("cells per tile must be a power of two, at least 2 (got {0})")]
    Cells(u32),
    #[error("height range must satisfy min < max")]
    HeightRange,
    #[error("LOD levels must be between 1 and {max} for {cells} cells")]
    LodLevels { cells: u32, max: u8 },
    #[error("unload radius must not be smaller than load radius")]
    Radii,
    #[error("splat layers must be between 1 and 4")]
    SplatLayers,
}

#[derive(Component, Debug, Clone, PartialEq)]
pub struct TerrainConfig {
    pub tile_size: f32,
    pub cells: u32,
    pub height_min: f32,
    pub height_max: f32,
    pub lod_levels: u8,
    pub load_radius: u32,
    pub unload_radius: u32,
    pub splat_layers: u8,
    pub seed: u64,
}

impl Default for TerrainConfig {
    fn default() -> Self {
        TerrainConfig {
            tile_size: 512.0,
            cells: 256,
            height_min: -100.0,
            height_max: 1900.0,
            lod_levels: 5,
            load_radius: 3,
            unload_radius: 4,
            splat_layers: 4,
            seed: 0,
        }
    }
}

impl TerrainConfig {
    pub fn samples(&self) -> usize {
        self.cells as usize + 1
    }

    pub fn cell_size(&self) -> f32 {
        self.tile_size / self.cells as f32
    }

    pub fn height_range(&self) -> f32 {
        self.height_max - self.height_min
    }

    pub fn height_precision(&self) -> f32 {
        self.height_range() / u16::MAX as f32
    }

    pub fn tile_bytes(&self) -> usize {
        let samples = self.samples() * self.samples();
        samples * std::mem::size_of::<u16>() + samples * 4
    }

    pub fn max_lod_levels(&self) -> u8 {
        if self.cells < 2 {
            return 0;
        }
        (self.cells.ilog2() as u8).saturating_add(1)
    }

    pub fn validate(&self) -> Result<(), ConfigError> {
        if !(self.tile_size.is_finite() && self.tile_size > 0.0) {
            return Err(ConfigError::TileSize);
        }
        if self.cells < 2 || !self.cells.is_power_of_two() {
            return Err(ConfigError::Cells(self.cells));
        }
        if !(self.height_min.is_finite() && self.height_max.is_finite() && self.height_min < self.height_max) {
            return Err(ConfigError::HeightRange);
        }
        let max = self.max_lod_levels();
        if self.lod_levels == 0 || self.lod_levels > max {
            return Err(ConfigError::LodLevels { cells: self.cells, max });
        }
        if self.unload_radius < self.load_radius {
            return Err(ConfigError::Radii);
        }
        if self.splat_layers == 0 || self.splat_layers > 4 {
            return Err(ConfigError::SplatLayers);
        }
        Ok(())
    }

    pub fn quantize(&self, height: f32) -> u16 {
        let t = ((height - self.height_min) / self.height_range()).clamp(0.0, 1.0);
        (t * u16::MAX as f32).round() as u16
    }

    pub fn dequantize(&self, value: u16) -> f32 {
        self.height_min + value as f32 / u16::MAX as f32 * self.height_range()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
