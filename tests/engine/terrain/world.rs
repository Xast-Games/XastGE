use glam::Vec2;
use glam::Vec3;
use xastge::terrain::world::*;
use xastge::terrain::*;

const SAMPLE: &str = r#"
    WorldDefinition(
        name: "Test valley",
        terrain: WorldTerrain(tile_size: 1024.0, cells: 256, height_min: -100.0, height_max: 1900.0, seed: 3, baseline: 120.0),
        regions: [
            Region(
                id: "valley",
                name: "Valley",
                bounds: Bounds(min: (-1000.0, -1000.0), max: (1000.0, 1000.0)),
                locations: [
                    Location(
                        id: "town",
                        name: "Town",
                        position: (0.0, 100.0, 0.0),
                        yaw_degrees: 30.0,
                        footprint: Rect(width: 200.0, depth: 120.0),
                        constraints: Constraints(max_slope_degrees: Some(6.0), elevation: Some((50.0, 300.0)), road_access: true, river_within: Some(300.0)),
                    ),
                ],
                roads: [WorldPath(id: "main_road", points: [(-900.0, 0.0, 0.0), (0.0, 0.0, 10.0)], width: 8.0, connects: ["town"])],
                rivers: [WorldPath(id: "river", points: [(-900.0, 0.0, 200.0), (900.0, 0.0, 250.0)], width: 20.0)],
                landmarks: [Landmark(id: "peak", name: "Old Peak", kind: Peak, position: (500.0, 900.0, 500.0))],
                anchors: [
                    Peak(id: "old_peak", position: (500.0, 900.0, 500.0), radius: 400.0, sharpness: 2.0),
                    Ridge(id: "spine", points: [(400.0, 700.0, 300.0), (700.0, 800.0, 700.0)], width: 200.0),
                    Valley(id: "river_bed", points: [(-900.0, 60.0, 200.0), (900.0, 50.0, 250.0)], width: 150.0),
                    Plateau(id: "yard", position: (-500.0, 140.0, -500.0), footprint: Circle(radius: 50.0), falloff: 40.0),
                    Detail(id: "calm", position: (0.0, 0.0, 0.0), radius: 300.0, amount: -0.5),
                ],
            ),
        ],
    )
"#;

fn world() -> WorldDefinition {
    WorldDefinition::parse(SAMPLE).expect("sample parses")
}

#[test]
fn sample_parses_and_is_valid() {
    let world = world();
    assert_eq!(world.regions.len(), 1);
    assert_eq!(world.locations().count(), 1);
    assert!(world.validate().is_empty(), "{:?}", world.validate());
}

#[test]
fn validation_reports_duplicates_and_strays() {
    let mut world = world();
    let region = &mut world.regions[0];
    let mut copy = region.locations[0].clone();
    copy.position = Vec3::new(5000.0, 0.0, 0.0);
    region.locations.push(copy);
    region.roads[0].connects.push("nowhere".into());

    let issues = world.validate();
    assert!(issues.contains(&WorldIssue::DuplicateId("town".into())));
    assert!(issues.iter().any(|i| matches!(i, WorldIssue::OutsideRegion { item, .. } if item == "town")));
    assert!(issues.iter().any(|i| matches!(i, WorldIssue::UnknownConnection { location, .. } if location == "nowhere")));
}

#[test]
fn anchors_parse_with_defaults() {
    let world = world();
    let anchors = &world.regions[0].anchors;
    assert_eq!(anchors.len(), 5);
    assert!(matches!(&anchors[1], Anchor::Ridge { sharpness, .. } if *sharpness == 1.0));
    assert_eq!(world.terrain.baseline, 120.0);
}

#[test]
fn flattened_locations_become_plateau_anchors() {
    let mut world = world();
    assert_eq!(world.anchors().len(), 5);

    world.regions[0].locations[0].constraints.flatten = true;
    world.regions[0].locations[0].constraints.falloff = 0.0;
    let anchors = world.anchors();

    assert_eq!(anchors.len(), 6);
    assert_eq!(
        anchors[5],
        Anchor::Plateau {
            id: "town".into(),
            position: Vec3::new(0.0, 100.0, 0.0),
            yaw_degrees: 30.0,
            footprint: Footprint::Rect { width: 200.0, depth: 120.0 },
            falloff: 1.0,
        }
    );
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
    let world = world();
    let (_, town) = world.locations().next().unwrap();
    for local in [Vec2::new(30.0, -10.0), Vec2::new(-80.0, 40.0)] {
        assert!((town.to_local(town.to_world(local)) - local).length() < 1e-3);
    }
}

#[test]
fn footprints_contain_their_interior() {
    let rect = Footprint::Rect { width: 10.0, depth: 4.0 };
    assert!(rect.contains(Vec2::new(4.9, 1.9)));
    assert!(!rect.contains(Vec2::new(5.1, 0.0)));

    let triangle = Footprint::Polygon { points: vec![Vec2::new(0.0, 0.0), Vec2::new(10.0, 0.0), Vec2::new(0.0, 10.0)] };
    assert!(triangle.contains(Vec2::new(2.0, 2.0)));
    assert!(!triangle.contains(Vec2::new(8.0, 8.0)));
}

#[test]
fn flat_ground_satisfies_the_town() {
    let world = world();
    let (region, town) = world.locations().next().unwrap();
    assert!(town.check(region, 8.0, |_| 120.0).is_empty());
}

#[test]
fn steep_low_ground_is_reported() {
    let world = world();
    let (region, town) = world.locations().next().unwrap();
    let violations = town.check(region, 8.0, |p| p.x * 0.5 - 100.0);

    assert!(violations.iter().any(|v| matches!(v, Violation::TooSteep { .. })));
    assert!(violations.iter().any(|v| matches!(v, Violation::TooLow { .. })));
}

#[test]
fn missing_road_and_river_are_reported() {
    let mut world = world();
    world.regions[0].roads.clear();
    world.regions[0].rivers.clear();
    let (region, town) = world.locations().next().unwrap();
    let violations = town.check(region, 8.0, |_| 120.0);

    assert!(violations.contains(&Violation::NoRoadAccess));
    assert!(violations.iter().any(|v| matches!(v, Violation::NoRiverNearby { .. })));
}
