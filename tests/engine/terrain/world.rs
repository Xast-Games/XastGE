use glam::{Vec2, Vec3};
use xastge::terrain::*;

const SAMPLE: &str = r#"
    WorldDefinition(
        name: "Test valley",
        scene: "valley.gltf",
        terrain: WorldTerrain(tile_size: 1024.0, cells: 256, height_min: -100.0, height_max: 1900.0, seed: 3, baseline: 120.0),
    )
"#;

fn town() -> Location {
    Location {
        name: "Town".to_owned(),
        position: Vec3::new(0.0, 100.0, 0.0),
        yaw_degrees: 30.0,
        footprint: Footprint::Rect { width: 200.0, depth: 120.0 },
        constraints: Constraints {
            max_slope_degrees: Some(6.0),
            elevation: Some(Vec2::new(50.0, 300.0)),
            river_within: Some(300.0),
            road_access: true,
        },
    }
}

fn path(name: &str, points: &[[f32; 3]], width: f32) -> WorldPath {
    WorldPath { name: name.to_owned(), points: points.iter().map(|p| Vec3::from_array(*p)).collect(), width }
}

fn roads() -> Vec<WorldPath> {
    vec![path("main_road", &[[-900.0, 0.0, 0.0], [0.0, 0.0, 10.0]], 8.0)]
}

fn rivers() -> Vec<WorldPath> {
    vec![path("river", &[[-900.0, 0.0, 200.0], [900.0, 0.0, 250.0]], 20.0)]
}

#[test]
fn definition_names_its_scene_and_terrain() {
    let world = WorldDefinition::parse(SAMPLE).expect("sample parses");
    assert_eq!(world.scene, "valley.gltf");
    assert_eq!(world.terrain.baseline, 120.0);
}

#[test]
fn footprint_distance_is_zero_inside_and_grows_outside() {
    let rect = Footprint::Rect { width: 10.0, depth: 4.0 };
    assert_eq!(rect.distance(Vec2::new(1.0, 1.0)), 0.0);
    assert!((rect.distance(Vec2::new(8.0, 0.0)) - 3.0).abs() < 1e-5);

    let circle = Footprint::Circle { radius: 5.0 };
    assert!((circle.distance(Vec2::new(0.0, 9.0)) - 4.0).abs() < 1e-5);
}

#[test]
fn local_points_undo_the_yaw() {
    let town = town();
    for local in [Vec2::new(30.0, -10.0), Vec2::new(-80.0, 40.0)] {
        assert!((town.to_local(town.to_world(local)) - local).length() < 1e-3);
    }
}

#[test]
fn footprints_contain_their_interior() {
    let rect = Footprint::Rect { width: 10.0, depth: 4.0 };
    assert!(rect.contains(Vec2::new(4.9, 1.9)));
    assert!(!rect.contains(Vec2::new(5.1, 0.0)));

    let circle = Footprint::Circle { radius: 5.0 };
    assert!(circle.contains(Vec2::new(3.0, 3.9)));
    assert!(!circle.contains(Vec2::new(3.6, 3.6)));
}

#[test]
fn flat_ground_satisfies_the_town() {
    assert!(town().check(&roads(), &rivers(), 8.0, |_| 120.0).is_empty());
}

#[test]
fn steep_low_ground_is_reported() {
    let violations = town().check(&roads(), &rivers(), 8.0, |p| p.x * 0.5 - 100.0);

    assert!(violations.iter().any(|v| matches!(v, Violation::TooSteep { .. })));
    assert!(violations.iter().any(|v| matches!(v, Violation::TooLow { .. })));
}

#[test]
fn missing_road_and_river_are_reported() {
    let violations = town().check(&[], &[], 8.0, |_| 120.0);

    assert!(violations.contains(&Violation::NoRoadAccess));
    assert!(violations.iter().any(|v| matches!(v, Violation::NoRiverNearby { .. })));
}
