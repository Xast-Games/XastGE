use xastge::terrain::source::*;
use xastge::terrain::*;

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

#[test]
fn landscape_stays_within_its_parts() {
    let settings = LandscapeSettings::default();
    let land = LandscapeSource::new(9, &settings);
    let top = settings.base_height + settings.plains_amplitude + settings.mountain_height;

    for i in -50..50 {
        let h = land.height(i as f64 * 731.0, i as f64 * -419.0);
        assert!(h >= settings.base_height - 1e-3 && h <= top + 1e-3, "{h}");
    }
}

#[test]
fn landscape_is_deterministic() {
    let settings = LandscapeSettings::default();
    let a = LandscapeSource::new(4, &settings);
    let b = LandscapeSource::new(4, &settings);
    assert_eq!(a.height(1234.5, -987.6), b.height(1234.5, -987.6));
}
