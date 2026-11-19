use glam::Vec2;
use xastge::terrain::erosion::*;
use xastge::terrain::*;
use xastge::terrain::source::FlatSource;

struct Cone;

impl HeightSource for Cone {
    fn height(&self, x: f64, z: f64) -> f32 {
        (200.0 - (x * x + z * z).sqrt() * 0.5).max(0.0) as f32
    }
}

fn region() -> BaseRegion {
    BaseRegion::covering(Vec2::ZERO, 512.0, 8.0)
}

#[test]
fn region_is_centred() {
    let r = region();
    let half = (r.size - 1) as f32 * r.spacing * 0.5;
    assert_eq!(r.origin, Vec2::splat(-half));
    assert_eq!(r.size, 65);
}

#[test]
fn sampling_hits_the_grid_points() {
    let base = BaseMap::generate(region(), &Cone);
    let r = base.region;

    for (i, j) in [(0usize, 0usize), (10, 20), (32, 32), (64, 64)] {
        let x = r.origin.x as f64 + i as f64 * r.spacing as f64;
        let z = r.origin.y as f64 + j as f64 * r.spacing as f64;
        assert!((base.sample(x, z) - base.heights[j * r.size + i]).abs() < 1e-3);
    }
}

#[test]
fn flat_ground_stays_flat() {
    let base = build_base(region(), &FlatSource(50.0), &ErosionSettings::default(), 1, |_| true).unwrap();
    assert!(base.heights.iter().all(|h| (h - 50.0).abs() < 1e-3));
}

#[test]
fn erosion_is_deterministic_and_changes_slopes() {
    let settings = ErosionSettings { droplets_per_cell: 2.0, ..Default::default() };
    let plain = BaseMap::generate(region(), &Cone);
    let a = build_base(region(), &Cone, &settings, 7, |_| true).unwrap();
    let b = build_base(region(), &Cone, &settings, 7, |_| true).unwrap();

    assert_eq!(a.heights, b.heights);
    assert!(a.heights.iter().zip(&plain.heights).any(|(x, y)| (x - y).abs() > 0.01));
    assert!(a.heights.iter().all(|h| h.is_finite()));
}

#[test]
fn erosion_records_wet_and_deposited_areas() {
    let settings = ErosionSettings { droplets_per_cell: 2.0, ..Default::default() };
    let base = build_base(region(), &Cone, &settings, 3, |_| true).unwrap();

    assert!(base.moisture.iter().all(|m| (0.0..=1.0).contains(m)));
    assert!(base.moisture.iter().any(|m| *m > 0.5));
    assert!(base.sediment.iter().any(|s| *s > 0.0));
}

#[test]
fn thermal_erosion_conserves_material() {
    let n = 32;
    let mut map: Vec<f32> = (0..n * n).map(|k| if k == n * 16 + 16 { 10.0 } else { 0.0 }).collect();
    let before: f32 = map.iter().sum();

    erode_thermal(&mut map, n, 20, 0.1, 0.5);

    let after: f32 = map.iter().sum();
    assert!((before - after).abs() < 1e-3);
    assert!(map[n * 16 + 16] < 10.0);
}

#[test]
fn cancelled_build_returns_nothing() {
    assert!(build_base(region(), &Cone, &ErosionSettings::default(), 1, |_| false).is_none());
}
