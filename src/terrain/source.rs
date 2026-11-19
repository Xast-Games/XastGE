use noise::{Fbm, MultiFractal, NoiseFn, Perlin};

use super::splat::Band;

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct SurfaceHints {
    pub moisture: f32,
    pub sediment: f32,
}

pub trait HeightSource: Send + Sync {
    fn height(&self, x: f64, z: f64) -> f32;

    fn surface(&self, _x: f64, _z: f64) -> SurfaceHints {
        SurfaceHints::default()
    }

    fn pin(&self, _x: f64, _z: f64, height: f32) -> f32 {
        height
    }

    fn detail_weight(&self, _x: f64, _z: f64) -> f32 {
        1.0
    }
}

pub struct FlatSource(pub f32);

impl HeightSource for FlatSource {
    fn height(&self, _x: f64, _z: f64) -> f32 {
        self.0
    }
}

pub struct NoiseSource<N> {
    pub noise: N,
    pub base: f32,
    pub amplitude: f32,
}

impl<N: NoiseFn<f64, 2> + Send + Sync> HeightSource for NoiseSource<N> {
    fn height(&self, x: f64, z: f64) -> f32 {
        self.base + self.amplitude * self.noise.get([x, z]) as f32
    }
}

impl NoiseSource<Fbm<Perlin>> {
    pub fn fbm(seed: u64, base: f32, amplitude: f32, wavelength: f64, octaves: u32) -> Self {
        let noise = Fbm::<Perlin>::new((seed ^ (seed >> 32)) as u32)
            .set_octaves(octaves.clamp(1, 16) as usize)
            .set_frequency(1.0 / wavelength.max(1e-3));

        NoiseSource { noise, base, amplitude }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct LandscapeSettings {
    pub base_height: f32,
    pub plains_amplitude: f32,
    pub plains_wavelength: f32,
    pub plains_curve: f32,
    pub mountain_height: f32,
    pub mountain_wavelength: f32,
    pub mountain_curve: f32,
    pub ridge_softness: f32,
    pub octaves: u32,
    pub persistence: f32,
    pub lacunarity: f32,
    pub mask_wavelength: f32,
    pub mask: Band,
    pub warp_strength: f32,
    pub warp_wavelength: f32,
}

impl Default for LandscapeSettings {
    fn default() -> Self {
        LandscapeSettings {
            base_height: 120.0,
            plains_amplitude: 40.0,
            plains_wavelength: 1800.0,
            plains_curve: 1.6,
            mountain_height: 1100.0,
            mountain_wavelength: 3500.0,
            mountain_curve: 1.3,
            ridge_softness: 0.15,
            octaves: 7,
            persistence: 0.45,
            lacunarity: 2.0,
            mask_wavelength: 9000.0,
            mask: Band::new(0.45, 0.7),
            warp_strength: 400.0,
            warp_wavelength: 2500.0,
        }
    }
}

pub struct LandscapeSource {
    settings: LandscapeSettings,
    plains: Fbm<Perlin>,
    ridges: Vec<Perlin>,
    mask: Perlin,
    warp_x: Perlin,
    warp_z: Perlin,
}

impl LandscapeSource {
    pub fn new(seed: u64, settings: &LandscapeSettings) -> Self {
        let base = (seed ^ (seed >> 32)) as u32;
        let octaves = settings.octaves.clamp(1, 16) as usize;
        let persistence = settings.persistence as f64;
        let lacunarity = settings.lacunarity as f64;

        let plains = Fbm::<Perlin>::new(base)
            .set_octaves(octaves.min(5))
            .set_frequency(1.0 / settings.plains_wavelength.max(1.0) as f64)
            .set_persistence(persistence)
            .set_lacunarity(lacunarity);

        let ridges = (0..octaves as u32).map(|i| Perlin::new(base.wrapping_add(10 + i))).collect();

        LandscapeSource {
            settings: settings.clone(),
            plains,
            ridges,
            mask: Perlin::new(base.wrapping_add(2)),
            warp_x: Perlin::new(base.wrapping_add(3)),
            warp_z: Perlin::new(base.wrapping_add(4)),
        }
    }
}

impl LandscapeSource {
    fn ridged(&self, x: f64, z: f64) -> f32 {
        let s = &self.settings;
        let k = s.ridge_softness.max(0.0);
        let mut frequency = 1.0 / s.mountain_wavelength.max(1.0) as f64;
        let mut amplitude = 1.0;
        let mut weight = 1.0f32;
        let mut sum = 0.0;
        let mut norm = 0.0;

        for octave in &self.ridges {
            let n = octave.get([x * frequency, z * frequency]) as f32;
            let ridge = (1.0 + k - (n * n + k * k).sqrt()).clamp(0.0, 1.0);
            let signal = ridge * ridge * weight;

            sum += signal * amplitude;
            norm += amplitude;
            weight = signal.clamp(0.0, 1.0);
            amplitude *= s.persistence;
            frequency *= s.lacunarity as f64;
        }

        if norm > 0.0 { sum / norm } else { 0.0 }
    }
}

fn unit(value: f64) -> f32 {
    ((value as f32 + 1.0) * 0.5).clamp(0.0, 1.0)
}

impl LandscapeSource {
    fn warp(&self, x: f64, z: f64) -> (f64, f64) {
        let s = &self.settings;
        let ww = s.warp_wavelength.max(1.0) as f64;
        (
            x + self.warp_x.get([x / ww, z / ww]) * s.warp_strength as f64,
            z + self.warp_z.get([x / ww + 5.2, z / ww + 1.3]) * s.warp_strength as f64,
        )
    }

    pub fn relief(&self, x: f64, z: f64) -> f32 {
        let s = &self.settings;
        let (wx, wz) = self.warp(x, z);
        let plains = unit(self.plains.get([wx, wz])).powf(s.plains_curve.max(0.1)) * s.plains_amplitude;
        let ridges = self.ridged(wx, wz).powf(s.mountain_curve.max(0.1)) * s.mountain_height;
        plains + ridges
    }
}

impl HeightSource for LandscapeSource {
    fn height(&self, x: f64, z: f64) -> f32 {
        let s = &self.settings;
        let (wx, wz) = self.warp(x, z);

        let mw = s.mask_wavelength.max(1.0) as f64;
        let mask = s.mask.ramp(unit(self.mask.get([wx / mw, wz / mw])));

        let plains = unit(self.plains.get([wx, wz])).powf(s.plains_curve.max(0.1)) * s.plains_amplitude;
        let ridges = self.ridged(wx, wz).powf(s.mountain_curve.max(0.1)) * s.mountain_height;

        s.base_height + plains + ridges * mask
    }
}
