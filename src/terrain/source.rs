use noise::{Fbm, MultiFractal, NoiseFn, Perlin};

pub trait HeightSource: Send + Sync {
    fn height(&self, x: f64, z: f64) -> f32;
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fbm_is_deterministic() {
        let a = NoiseSource::fbm(3, 10.0, 50.0, 100.0, 5);
        let b = NoiseSource::fbm(3, 10.0, 50.0, 100.0, 5);

        for i in -20..20 {
            let (x, z) = (i as f64 * 37.3, i as f64 * -91.1);
            assert_eq!(a.height(x, z), b.height(x, z));
        }
    }

    #[test]
    fn different_seeds_differ() {
        let a = NoiseSource::fbm(1, 0.0, 100.0, 256.0, 5);
        let b = NoiseSource::fbm(2, 0.0, 100.0, 256.0, 5);
        assert_ne!(a.height(123.4, 456.7), b.height(123.4, 456.7));
    }
}
