use glam::Vec2;
use glam::Vec3;
use xastge::terrain::shape::*;
use xastge::terrain::*;
use xastge::terrain::erosion::{BaseRegion, ErosionSettings, build_base};

fn smooth_only() -> ShapeSettings {
    ShapeSettings { relief_scale: 0.0, ..Default::default() }
}

#[test]
fn peak_reaches_its_height_and_fades_to_baseline() {
    let anchors = vec![Anchor::Peak { id: "p".into(), position: Vec3::new(0.0, 900.0, 0.0), radius: 500.0, sharpness: 1.0 }];
    let s = AuthoredSource::new(100.0, &anchors, 1, &LandscapeSettings::default(), &smooth_only());

    assert!((s.height(0.0, 0.0) - 900.0).abs() < 1e-3);
    assert!((s.height(800.0, 0.0) - 100.0).abs() < 1e-3);
    assert!(s.height(250.0, 0.0) > 100.0 && s.height(250.0, 0.0) < 900.0);
}

#[test]
fn valley_floor_follows_its_line() {
    let anchors = vec![Anchor::Valley {
        id: "v".into(),
        points: vec![Vec3::new(-1000.0, 40.0, 0.0), Vec3::new(1000.0, 20.0, 0.0)],
        width: 200.0,
    }];
    let s = AuthoredSource::new(100.0, &anchors, 1, &LandscapeSettings::default(), &smooth_only());

    assert!((s.height(0.0, 0.0) - 30.0).abs() < 1e-3);
    assert!((s.height(0.0, 500.0) - 100.0).abs() < 1e-3);
}

#[test]
fn plateau_is_exactly_flat_and_relief_is_suppressed_on_it() {
    let anchors = vec![
        Anchor::Peak { id: "p".into(), position: Vec3::new(0.0, 900.0, 0.0), radius: 900.0, sharpness: 1.0 },
        Anchor::Plateau {
            id: "town".into(),
            position: Vec3::new(200.0, 400.0, 0.0),
            yaw_degrees: 30.0,
            footprint: Footprint::Rect { width: 120.0, depth: 80.0 },
            falloff: 50.0,
        },
    ];
    let s = AuthoredSource::new(100.0, &anchors, 1, &LandscapeSettings::default(), &ShapeSettings::default());

    for (x, z) in [(200.0, 0.0), (220.0, 10.0), (180.0, -15.0)] {
        assert!((s.height(x, z) - 400.0).abs() < 1e-3, "({x}, {z}) = {}", s.height(x, z));
        assert!(s.detail_weight(x, z) < 1e-6);
    }
}

#[test]
fn plateau_survives_erosion() {
    let anchors = vec![
        Anchor::Peak { id: "p".into(), position: Vec3::new(0.0, 700.0, 0.0), radius: 400.0, sharpness: 1.0 },
        Anchor::Plateau {
            id: "town".into(),
            position: Vec3::new(150.0, 300.0, 0.0),
            yaw_degrees: 0.0,
            footprint: Footprint::Circle { radius: 40.0 },
            falloff: 30.0,
        },
    ];
    let s = AuthoredSource::new(100.0, &anchors, 1, &LandscapeSettings::default(), &ShapeSettings::default());
    let region = BaseRegion::covering(Vec2::ZERO, 600.0, 8.0);
    let settings = ErosionSettings { droplets_per_cell: 2.0, ..Default::default() };
    let base = build_base(region, &s, &settings, 5, |_| true).unwrap();

    assert!((base.sample(150.0, 0.0) - 300.0).abs() < 1e-2);
    assert!((base.sample(160.0, 10.0) - 300.0).abs() < 1e-2);
}

#[test]
fn overlapping_peaks_blend_without_a_crease() {
    let anchors = vec![
        Anchor::Peak { id: "a".into(), position: Vec3::new(-300.0, 800.0, 0.0), radius: 600.0, sharpness: 1.0 },
        Anchor::Peak { id: "b".into(), position: Vec3::new(300.0, 800.0, 0.0), radius: 600.0, sharpness: 1.0 },
    ];
    let s = AuthoredSource::new(100.0, &anchors, 1, &LandscapeSettings::default(), &smooth_only());

    let left = s.height(-2.0, 0.0);
    let mid = s.height(0.0, 0.0);
    let right = s.height(2.0, 0.0);
    let curvature = left + right - 2.0 * mid;
    assert!(curvature.abs() < 1.0, "saddle crease {curvature}");
}
