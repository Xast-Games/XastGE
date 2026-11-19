pub const LAYERS: [&str; 4] = ["Grass", "Soil", "Rocky", "Cliff"];

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Band {
    pub start: f32,
    pub end: f32,
}

impl Band {
    pub const fn new(start: f32, end: f32) -> Self {
        Band { start, end }
    }

    pub fn ramp(self, value: f32) -> f32 {
        if self.end == self.start {
            return if value >= self.end { 1.0 } else { 0.0 };
        }
        let t = ((value - self.start) / (self.end - self.start)).clamp(0.0, 1.0);
        t * t * (3.0 - 2.0 * t)
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SplatWeights {
    pub grass: f32,
    pub soil: f32,
    pub rocky: f32,
    pub cliff: f32,
}

impl SplatWeights {
    pub fn sum(self) -> f32 {
        self.grass + self.soil + self.rocky + self.cliff
    }

    pub fn pack(self) -> [u8; 4] {
        let byte = |w: f32| (w.clamp(0.0, 1.0) * 255.0).round() as u8;
        [byte(self.grass), byte(self.soil), byte(self.rocky), byte(self.cliff)]
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct SplatRules {
    pub rocky_slope: Band,
    pub rocky_height: Band,
    pub cliff_slope: Band,
    pub soil_concavity: Band,
    pub soil_moisture: Band,
    pub feature_radius: f32,
    pub breakup: f32,
    pub breakup_wavelength: f32,
}

impl Default for SplatRules {
    fn default() -> Self {
        SplatRules {
            rocky_slope: Band::new(0.07, 0.16),
            rocky_height: Band::new(800.0, 1100.0),
            cliff_slope: Band::new(0.18, 0.30),
            soil_concavity: Band::new(0.02, 0.08),
            soil_moisture: Band::new(0.35, 0.85),
            feature_radius: 16.0,
            breakup: 60.0,
            breakup_wavelength: 300.0,
        }
    }
}

impl SplatRules {
    pub fn weights(&self, height: f32, slope: f32, concavity: f32, moisture: f32, noise: f32) -> SplatWeights {
        let tilted = slope + noise * 0.03;
        let cliff = self.cliff_slope.ramp(tilted);
        let rocky = self.rocky_slope.ramp(tilted).max(self.rocky_height.ramp(height + noise * self.breakup));
        let soil = self.soil_concavity.ramp(concavity + noise * 0.01)
            .max(self.soil_moisture.ramp(moisture + noise * 0.05));

        SplatWeights {
            grass: (1.0 - soil) * (1.0 - rocky) * (1.0 - cliff),
            soil: soil * (1.0 - rocky) * (1.0 - cliff),
            rocky: rocky * (1.0 - cliff),
            cliff,
        }
    }
}
