use glam::Vec2;
use glam::Vec3;
use xastge::terrain::road::*;
use xastge::terrain::*;

fn straight() -> RoadSpline {
    RoadSpline::new([Vec3::ZERO, Vec3::new(100.0, 0.0, 0.0)]).unwrap()
}

fn curvy() -> RoadSpline {
    RoadSpline::new([
        Vec3::new(0.0, 0.0, 0.0),
        Vec3::new(60.0, 5.0, 10.0),
        Vec3::new(90.0, 10.0, 80.0),
        Vec3::new(40.0, 0.0, 140.0),
        Vec3::new(-20.0, 0.0, 120.0),
    ])
    .unwrap()
}

#[test]
fn needs_two_distinct_points() {
    assert!(RoadSpline::new([]).is_none());
    assert!(RoadSpline::new([Vec3::ONE]).is_none());
    assert!(RoadSpline::new([Vec3::ONE, Vec3::ONE]).is_none());
}

#[test]
fn straight_road_is_a_line() {
    let road = straight();

    assert!((road.length() - 100.0).abs() < 1e-2);
    assert!(road.position(25.0).distance(Vec3::new(25.0, 0.0, 0.0)) < 1e-2);

    let frame = road.frame(50.0);
    assert!((frame.tangent - Vec3::X).length() < 1e-3);
    assert!((frame.right - Vec3::Z).length() < 1e-3);
}

#[test]
fn curve_passes_through_control_points() {
    let road = curvy();

    for (i, point) in road.points().iter().enumerate() {
        let p = road.position(road.control_s(i));
        assert!(p.distance(*point) < 1e-2, "point {i}: {p:?} vs {point:?}");
    }
}

#[test]
fn sampling_is_uniform_in_arc_length() {
    let road = curvy();
    let frames = road.sample(2.0);
    let expected = road.length() / (frames.len() - 1) as f32;

    for pair in frames.windows(2) {
        let d = pair[0].position.distance(pair[1].position);
        assert!((d - expected).abs() < expected * 0.02, "{d} vs {expected}");
    }
}

#[test]
fn right_vector_stays_horizontal() {
    let road = curvy();

    for frame in road.sample(5.0) {
        assert!(frame.right.y.abs() < 1e-5);
        assert!((frame.right.length() - 1.0).abs() < 1e-4);
    }
}

#[test]
fn closest_reports_signed_lateral_distance() {
    let road = straight();

    let left = road.closest(Vec2::new(50.0, -4.0));
    assert!((left.s - 50.0).abs() < 1e-2);
    assert!((left.distance + 4.0).abs() < 1e-3);

    let right = road.closest(Vec2::new(30.0, 3.0));
    assert!((right.s - 30.0).abs() < 1e-2);
    assert!((right.distance - 3.0).abs() < 1e-3);
}

#[test]
fn profile_on_flat_ground_is_flat() {
    let profile = RoadProfile::build(&curvy(), 2.0, 10.0, 0.08, |_| 42.0);

    assert!(profile.heights.iter().all(|h| (h - 42.0).abs() < 1e-4));
    assert!((profile.height(37.3) - 42.0).abs() < 1e-4);
}

#[test]
fn profile_respects_max_grade() {
    let grade = 0.08;
    let ground = |p: Vec2| (p.x * 0.3).sin() * 20.0 + p.y * 0.5;
    let profile = RoadProfile::build(&curvy(), 1.0, 0.0, grade, ground);

    for pair in profile.heights.windows(2) {
        assert!((pair[1] - pair[0]).abs() <= grade * profile.step + 1e-4, "{pair:?}");
    }
}

#[test]
fn profile_stays_within_ground_range() {
    let ground = |p: Vec2| (p.x * 0.2).sin() * 15.0 + 100.0;
    let profile = RoadProfile::build(&straight(), 1.0, 8.0, 0.05, ground);

    for h in &profile.heights {
        assert!((85.0 - 1e-3..=115.0 + 1e-3).contains(h), "{h}");
    }
    assert!((profile.length() - straight().length()).abs() < 1e-3);
}

#[test]
fn smoothing_flattens_a_bump() {
    let ground = |p: Vec2| if (p.x - 50.0).abs() < 2.0 { 10.0 } else { 0.0 };
    let rough = RoadProfile::build(&straight(), 1.0, 0.0, 10.0, ground);
    let smooth = RoadProfile::build(&straight(), 1.0, 10.0, 10.0, ground);

    let peak = |p: &RoadProfile| p.heights.iter().copied().fold(f32::MIN, f32::max);
    assert!(peak(&smooth) < peak(&rough) * 0.5);
}
