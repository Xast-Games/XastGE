use std::path::PathBuf;

use flecs_ecs::prelude::*;
use glam::{Quat, Vec2, Vec3, Vec4};
use serde::Deserialize;
use serde_json::{Value, json};
use xastge::{
    math::Transform,
    scene::kebab::*,
    terrain::{Anchor, Footprint, Landmark, Location, River, Road, TerrainAnchor, WorldPath},
};

fn document(roots: Value, nodes: Value) -> String {
    json!({ "asset": { "version": "2.0" }, "scene": 0, "scenes": [{ "nodes": roots }], "nodes": nodes }).to_string()
}

fn scene(roots: Value, nodes: Value) -> KebabScene {
    KebabScene::parse(&document(roots, nodes), "test/scene.gltf").expect("scene parses")
}

fn kebab(id: &str, archetypes: Value) -> Value {
    json!({ "extras": { "kebab": { "id": id, "archetypes": archetypes } } })
}

fn node(id: &str, translation: [f32; 3], archetypes: Value) -> Value {
    let mut n = kebab(id, archetypes);
    n["name"] = json!(id);
    n["translation"] = json!(translation);
    n
}

fn world() -> World {
    let world = World::new();
    world.import::<KebabModule>();
    world
}

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("xastge-kebab-{}-{name}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("scratch dir");
    dir
}

#[test]
fn vec3_uses_the_editor_text_format() {
    assert_eq!(parse_vec3("(1.0, 2.5, -3.0E-4)"), Some(Vec3::new(1.0, 2.5, -3.0e-4)));
    assert_eq!(parse_vec3("(1.0, 2.0)"), None);
    assert_eq!(parse_vec3("1.0, 2.0, 3.0"), None);
}

#[test]
fn params_deserialize_into_structs() {
    #[derive(Deserialize, PartialEq, Debug)]
    struct Lamp {
        #[serde(deserialize_with = "color")]
        color: Vec3,
        #[serde(deserialize_with = "vec3")]
        offset: Vec3,
        intensity: f32,
        target: Option<String>,
    }

    let instance = ArchetypeInstance::new("lamp")
        .with("color", "(2.0, 0.5, -1.0)")
        .with("offset", "(0.0, 1.5, 0.0)")
        .with("intensity", 2.0)
        .with("target", Value::Null);

    assert_eq!(
        instance.params::<Lamp>().unwrap(),
        Lamp { color: Vec3::new(1.0, 0.5, 0.0), offset: Vec3::new(0.0, 1.5, 0.0), intensity: 2.0, target: None }
    );
    assert!(ArchetypeInstance::new("lamp").with("intensity", 2.0).params::<Lamp>().is_err());
}

#[test]
fn scene_reads_hierarchy_and_world_matrices() {
    let mut parent = node("parent", [10.0, 0.0, 0.0], json!([]));
    parent["scale"] = json!([2.0, 2.0, 2.0]);
    parent["children"] = json!([1]);
    let child = node("child", [1.0, 0.0, 0.0], json!([]));

    let scene = scene(json!([0]), json!([parent, child]));

    assert_eq!(scene.roots, vec![0]);
    assert_eq!(scene.nodes[0].children, vec![1]);
    assert_eq!(scene.nodes[1].parent, Some(0));
    assert_eq!(scene.nodes[1].world.transform_point3(Vec3::ZERO), Vec3::new(12.0, 0.0, 0.0));
    assert_eq!(scene.find("child"), Some(1));
}

#[test]
fn scene_rejects_what_the_editor_rejects() {
    let parse = |nodes: Value, roots: Value| KebabScene::parse(&document(roots, nodes), "bad.gltf");

    assert!(parse(json!([{ "matrix": [1, 0, 0, 0, 0, 1, 0, 0, 0, 0, 1, 0, 0, 0, 0, 1] }]), json!([0])).is_err());
    assert!(parse(json!([{ "scale": [1.0, 2.0, 1.0] }]), json!([0])).is_err());
    assert!(parse(json!([{ "children": [1] }, {}]), json!([0, 1])).is_err());
    assert!(parse(json!([{}]), json!([3])).is_err());
    assert!(parse(json!([{ "extras": { "kebab": { "archetypes": [{ "params": {} }] } } }]), json!([0])).is_err());
}

#[test]
fn scene_without_scenes_is_empty() {
    let scene = KebabScene::parse(r#"{ "asset": { "version": "2.0" } }"#, "empty.gltf").unwrap();
    assert!(scene.nodes.is_empty());
}

#[test]
fn greybox_is_read_and_malformed_ones_are_dropped() {
    let mut boxed = kebab("a", json!([]));
    boxed["extras"]["kebab"]["greybox"] = json!({ "shape": "box", "size": [2.0, 1.0, 3.0] });
    let mut broken = kebab("b", json!([]));
    broken["extras"]["kebab"]["greybox"] = json!({ "shape": "box", "size": [2.0, 1.0] });

    let scene = scene(json!([0, 1]), json!([boxed, broken]));
    let greybox = scene.nodes[0].greybox.expect("greybox is read");

    assert_eq!(greybox.shape, GreyboxShape::Box);
    assert_eq!(greybox.size, Vec3::new(2.0, 1.0, 3.0));
    assert_eq!(greybox.color, Vec4::new(168.0 / 255.0, 170.0 / 255.0, 176.0 / 255.0, 1.0));
    assert!(scene.nodes[1].greybox.is_none());
}

#[test]
fn spawn_creates_entities_with_components() {
    let mut child = node("crate", [0.0, 1.0, 0.0], json!([{ "id": "collider_box", "params": { "size": "(2.0, 3.0, 4.0)" } }]));
    child["extras"]["kebab"]["greybox"] = json!({ "shape": "sphere" });
    let lamp = json!({ "color": "(1.0, 1.0, 1.0)", "intensity": 1000.0, "range": 10.0, "shadows": false });
    let mut parent = node("room", [5.0, 0.0, 0.0], json!([{ "id": "point_light", "params": lamp }]));
    parent["children"] = json!([1]);

    let scene = scene(json!([0]), json!([parent, child]));
    let world = world();
    let root = Transform { translation: Vec3::new(100.0, 0.0, 0.0), ..Default::default() };
    let report = spawn_scene(&world, &scene, root);

    assert!(report.issues.is_empty(), "{:?}", report.issues);
    assert_eq!(report.entities.len(), 2);

    let room = world.entity_from_id(report.entities[0]);
    let crate_entity = world.entity_from_id(report.entities[1]);

    assert_eq!(room.parent().map(|p| p.id()), Some(report.root));
    assert_eq!(crate_entity.parent().map(|p| p.id()), Some(room.id()));
    assert_eq!(room.try_cloned::<&PointLight>().map(|l| l.intensity), Some(1000.0));
    assert_eq!(crate_entity.try_cloned::<&BoxCollider>().map(|c| c.size), Some(Vec3::new(2.0, 3.0, 4.0)));
    assert_eq!(crate_entity.try_cloned::<&Greybox>().map(|g| g.shape), Some(GreyboxShape::Sphere));

    let at = crate_entity.cloned::<&Transform>().translation;
    assert!((at - Vec3::new(105.0, 1.0, 0.0)).length() < 1e-4);
    assert_eq!(
        crate_entity.cloned::<&KebabNodeInfo>(),
        KebabNodeInfo { id: Some("crate".to_owned()), name: Some("crate".to_owned()) }
    );
}

#[test]
fn spawn_reports_unknown_archetypes_and_bad_params() {
    let scene = scene(
        json!([0, 1]),
        json!([
            node("a", [0.0; 3], json!([{ "id": "dragon" }])),
            node("b", [0.0; 3], json!([{ "id": "collider_sphere", "params": { "radius": "big" } }])),
        ]),
    );

    let world = world();
    let report = spawn_scene(&world, &scene, Transform::default());

    assert_eq!(report.entities.len(), 2);
    assert_eq!(report.issues.len(), 2);
    assert!(report.issues[0].message.contains("unknown archetype `dragon`"));
    assert!(report.issues[1].message.contains("collider_sphere"));
}

#[test]
fn game_archetypes_are_registered_on_the_singleton() {
    #[derive(Component, Deserialize, Clone, Copy, PartialEq, Debug)]
    struct Health {
        max: i32,
    }

    let world = world();
    world.get::<&mut Archetypes>(|archetypes| {
        archetypes.component::<Health>("health");
    });

    let scene = scene(json!([0]), json!([node("guard", [0.0; 3], json!([{ "id": "health", "params": { "max": 80 } }]))]));
    let report = spawn_scene(&world, &scene, Transform::default());

    assert!(report.issues.is_empty(), "{:?}", report.issues);
    assert_eq!(world.entity_from_id(report.entities[0]).try_cloned::<&Health>(), Some(Health { max: 80 }));
}

#[test]
fn handlers_can_resolve_node_refs_to_entities() {
    #[derive(Component, Clone, Copy, PartialEq, Debug)]
    struct Link(Entity);

    #[derive(Deserialize)]
    struct LinkParams {
        to: Option<String>,
    }

    let world = world();
    world.get::<&mut Archetypes>(|archetypes| {
        archetypes.handler("link", |context, entity, instance| {
            if let Some(target) = instance.params::<LinkParams>()?.to.and_then(|id| context.entity(&id)) {
                entity.set(Link(target));
            }
            Ok(())
        });
    });

    let scene = scene(
        json!([0, 1]),
        json!([node("a", [0.0; 3], json!([{ "id": "link", "params": { "to": "b" } }])), node("b", [0.0; 3], json!([]))]),
    );
    let report = spawn_scene(&world, &scene, Transform::default());

    assert!(report.issues.is_empty(), "{:?}", report.issues);
    let a = world.entity_from_id(report.entities[0]);
    assert_eq!(a.try_cloned::<&Link>(), Some(Link(report.entities[1])));
}

#[test]
fn prefabs_spawn_under_their_node_and_cycles_are_reported() {
    let dir = scratch("prefab");
    let house = document(
        json!([0]),
        json!([node("house", [0.0, 0.0, 2.0], json!([{ "id": "prefab_ref", "params": { "path": "street.gltf" } }]))]),
    );
    let street = document(
        json!([0]),
        json!([node("street", [3.0, 0.0, 0.0], json!([{ "id": "prefab_ref", "params": { "path": "house.gltf" } }]))]),
    );
    std::fs::write(dir.join("house.gltf"), house).unwrap();
    std::fs::write(dir.join("street.gltf"), street).unwrap();

    let scene = KebabScene::load(dir.join("house.gltf")).unwrap();
    let world = world();
    let report = spawn_scene(&world, &scene, Transform::default());

    assert_eq!(report.entities.len(), 2);
    let street = world.entity_from_id(report.entities[1]);
    assert_eq!(street.parent().map(|p| p.id()), Some(report.entities[0]));
    assert!((street.cloned::<&Transform>().translation - Vec3::new(3.0, 0.0, 2.0)).length() < 1e-4);
    assert_eq!(report.issues.len(), 1);
    assert!(report.issues[0].message.contains("refers back"));
}

#[test]
fn model_ref_paths_resolve_next_to_the_scene() {
    let scene = scene(json!([0]), json!([node("m", [0.0; 3], json!([{ "id": "model_ref", "params": { "path": "props/barrel.glb" } }]))]));
    let world = world();
    let report = spawn_scene(&world, &scene, Transform::default());

    let model = world.entity_from_id(report.entities[0]).cloned::<&ModelRef>();
    assert_eq!(model.path, PathBuf::from("test/props/barrel.glb"));
}

fn anchors(world: &World) -> Vec<Anchor> {
    let mut out = Vec::new();
    world.query::<&TerrainAnchor>().build().each(|anchor| out.push(anchor.0.clone()));
    out.sort_by(|a, b| a.id().cmp(b.id()));
    out
}

#[test]
fn chains_become_one_anchor_in_world_space() {
    let ridge = |next: &str| json!([{ "id": "terrain_ridge", "params": { "next": next, "width": 50.0, "sharpness": 2.0 } }]);
    let scene = scene(
        json!([0, 1, 2]),
        json!([
            node("a", [0.0, 300.0, 0.0], ridge("b")),
            node("b", [100.0, 320.0, 0.0], ridge("c")),
            node("c", [200.0, 280.0, 0.0], json!([])),
        ]),
    );

    let world = world();
    let root = Transform { translation: Vec3::new(1000.0, 0.0, 0.0), ..Default::default() };
    let report = spawn_scene(&world, &scene, root);

    assert!(report.issues.is_empty(), "{:?}", report.issues);
    assert_eq!(
        anchors(&world),
        vec![Anchor::Ridge {
            id: "a".to_owned(),
            points: vec![Vec3::new(1000.0, 300.0, 0.0), Vec3::new(1100.0, 320.0, 0.0), Vec3::new(1200.0, 280.0, 0.0)],
            width: 50.0,
            sharpness: 2.0,
        }]
    );
}

#[test]
fn closed_chains_and_missing_links_are_reported() {
    let valley = |next: &str| json!([{ "id": "terrain_valley", "params": { "next": next, "width": 80.0 } }]);
    let scene = scene(
        json!([0, 1, 2]),
        json!([node("a", [0.0; 3], valley("b")), node("b", [0.0; 3], valley("a")), node("c", [0.0; 3], valley("nowhere"))]),
    );

    let world = world();
    let report = spawn_scene(&world, &scene, Transform::default());

    assert!(anchors(&world).is_empty());
    assert!(report.issues.iter().any(|i| i.message.contains("missing node `nowhere`")));
    assert_eq!(report.issues.iter().filter(|i| i.message.contains("closed loop")).count(), 2);
}

#[test]
fn point_anchors_take_scale_and_yaw_from_the_node() {
    let mut pad = node(
        "pad",
        [10.0, 120.0, 0.0],
        json!([
            { "id": "terrain_plateau", "params": { "falloff": 40.0 } },
            { "id": "no_volume", "params": {} }
        ]),
    );
    pad["rotation"] = json!(Quat::from_rotation_y(-90f32.to_radians()).to_array());
    pad["extras"]["kebab"]["greybox"] = json!({ "shape": "box", "size": [20.0, 0.5, 10.0] });
    let mut hill = node(
        "hill",
        [0.0, 400.0, 0.0],
        json!([
            { "id": "terrain_peak", "params": { "radius": 400.0, "sharpness": 1.0 } },
            { "id": "terrain_noise", "params": { "radius": 300.0, "amount": 0.5 } }
        ]),
    );
    hill["scale"] = json!([2.0, 2.0, 2.0]);

    let world = world();
    let report = spawn_scene(&world, &scene(json!([0, 1]), json!([pad, hill])), Transform::default());
    assert!(report.issues.is_empty(), "{:?}", report.issues);
    assert!(world.entity_from_id(report.entities[0]).try_cloned::<&Greybox>().is_none());

    let found = anchors(&world);
    assert_eq!(found.len(), 3);

    let plateau = found.iter().find(|a| matches!(a, Anchor::Plateau { .. })).unwrap();
    let Anchor::Plateau { position, yaw_degrees, footprint, falloff, .. } = plateau else { unreachable!() };
    assert!((*position - Vec3::new(10.0, 120.0, 0.0)).length() < 1e-4);
    assert!((yaw_degrees - 90.0).abs() < 1e-3);
    assert!(matches!(footprint, Footprint::Rect { width, depth } if (width - 20.0).abs() < 1e-4 && (depth - 10.0).abs() < 1e-4));
    assert!((falloff - 40.0).abs() < 1e-3);

    assert!(found.iter().any(|a| matches!(a, Anchor::Peak { radius, .. } if (radius - 800.0).abs() < 1e-3)));
    assert!(found.iter().any(|a| matches!(a, Anchor::Detail { radius, amount, .. } if (radius - 600.0).abs() < 1e-3 && *amount == 0.5)));
}

#[test]
fn anchors_inside_prefabs_follow_the_prefab_node() {
    let dir = scratch("anchors");
    let mut pad = node("pad", [0.0, 50.0, 0.0], json!([{ "id": "terrain_plateau", "params": { "falloff": 5.0 } }]));
    pad["extras"]["kebab"]["greybox"] = json!({ "shape": "box", "size": [10.0, 0.5, 10.0] });
    let house = document(json!([0]), json!([pad]));
    let town = document(
        json!([0]),
        json!([node("house", [30.0, 0.0, 0.0], json!([{ "id": "prefab_ref", "params": { "path": "house.gltf" } }]))]),
    );
    std::fs::write(dir.join("house.gltf"), house).unwrap();
    std::fs::write(dir.join("town.gltf"), town).unwrap();

    let world = world();
    let report = spawn_scene(&world, &KebabScene::load(dir.join("town.gltf")).unwrap(), Transform::default());

    assert!(report.issues.is_empty(), "{:?}", report.issues);
    let found = anchors(&world);
    assert_eq!(found.len(), 1);
    assert!(matches!(&found[0], Anchor::Plateau { position, .. } if (*position - Vec3::new(30.0, 50.0, 0.0)).length() < 1e-4));
}

#[test]
fn round_plateaus_come_from_spheres_and_need_a_greybox() {
    let mut round = node("round", [0.0; 3], json!([{ "id": "terrain_plateau", "params": { "falloff": 5.0 } }]));
    round["extras"]["kebab"]["greybox"] = json!({ "shape": "sphere", "size": [30.0, 2.0, 24.0] });
    round["scale"] = json!([2.0, 2.0, 2.0]);
    let bare = node("bare", [0.0; 3], json!([{ "id": "terrain_plateau", "params": { "falloff": 5.0 } }]));

    let world = world();
    let report = spawn_scene(&world, &scene(json!([0, 1]), json!([round, bare])), Transform::default());

    let found = anchors(&world);
    assert_eq!(found.len(), 1);
    assert!(matches!(&found[0], Anchor::Plateau { footprint: Footprint::Circle { radius }, falloff, .. }
        if (radius - 30.0).abs() < 1e-4 && (falloff - 10.0).abs() < 1e-4));
    assert_eq!(report.issues.len(), 1);
    assert!(report.issues[0].message.contains("needs a greybox"));
    assert!(world.entity_from_id(report.entities[0]).try_cloned::<&Greybox>().is_some());
}

#[test]
fn anchors_are_removed_with_their_scene() {
    let scene = scene(json!([0]), json!([node("hill", [0.0; 3], json!([{ "id": "terrain_peak", "params": { "radius": 10.0, "sharpness": 1.0 } }]))]));
    let world = world();
    let report = spawn_scene(&world, &scene, Transform::default());
    assert_eq!(anchors(&world).len(), 1);

    world.entity_from_id(report.root).destruct();
    assert!(anchors(&world).is_empty());
}

#[test]
fn terrain_params_with_missing_fields_are_reported() {
    let scene = scene(json!([0]), json!([node("hill", [0.0; 3], json!([{ "id": "terrain_peak", "params": { "radius": 10.0 } }]))]));
    let world = world();
    let report = spawn_scene(&world, &scene, Transform::default());

    assert!(anchors(&world).is_empty());
    assert!(report.issues[0].message.contains("sharpness"));
}

#[test]
fn locations_take_their_footprint_and_constraints_from_the_node() {
    let mut town = node(
        "Rail Town",
        [300.0, 150.0, 600.0],
        json!([
            {
                "id": "location",
                "params": {
                    "max_slope_degrees": 8.0,
                    "min_elevation": 80.0,
                    "max_elevation": 350.0,
                    "river_within": 0.0,
                    "road_access": true
                }
            },
            { "id": "terrain_plateau", "params": { "falloff": 60.0 } },
            { "id": "no_volume", "params": {} }
        ]),
    );
    town["rotation"] = json!(Quat::from_rotation_y(-20f32.to_radians()).to_array());
    town["extras"]["kebab"]["greybox"] = json!({ "shape": "box", "size": [260.0, 10.0, 180.0] });
    let bare = node(
        "Camp",
        [0.0; 3],
        json!([{
            "id": "location",
            "params": { "max_slope_degrees": 0.0, "min_elevation": 0.0, "max_elevation": 0.0, "river_within": 0.0, "road_access": false }
        }]),
    );

    let world = world();
    let report = spawn_scene(&world, &scene(json!([0, 1]), json!([town, bare])), Transform::default());

    assert_eq!(report.issues.len(), 1, "{:?}", report.issues);
    assert!(report.issues[0].message.contains("needs a greybox"));
    assert!(world.entity_from_id(report.entities[1]).try_cloned::<&Location>().is_none());

    let location = world.entity_from_id(report.entities[0]).cloned::<&Location>();
    assert_eq!(location.name, "Rail Town");
    assert!((location.position - Vec3::new(300.0, 150.0, 600.0)).length() < 1e-3);
    assert!((location.yaw_degrees - 20.0).abs() < 1e-3);
    assert!(matches!(location.footprint, Footprint::Rect { width, depth } if (width - 260.0).abs() < 1e-3 && (depth - 180.0).abs() < 1e-3));
    assert_eq!(location.constraints.max_slope_degrees, Some(8.0));
    assert_eq!(location.constraints.elevation, Some(Vec2::new(80.0, 350.0)));
    assert_eq!(location.constraints.river_within, None);
    assert!(location.constraints.road_access);
    assert!(matches!(&anchors(&world)[..], [Anchor::Plateau { .. }]));
}

#[test]
fn roads_and_rivers_are_chains_and_landmarks_are_points() {
    let road = |next: Value| json!([{ "id": "road", "params": { "next": next, "width": 8.0 } }]);
    let scene = scene(
        json!([0, 1, 2, 3, 4]),
        json!([
            node("a", [0.0, 100.0, 0.0], road(json!("b"))),
            node("b", [100.0, 110.0, 0.0], road(json!("c"))),
            node("c", [200.0, 120.0, 50.0], road(Value::Null)),
            node("spring", [0.0; 3], json!([{ "id": "river", "params": { "next": null, "width": 30.0 } }])),
            node("Grey Peak", [500.0, 900.0, 500.0], json!([{ "id": "landmark", "params": { "kind": "peak" } }])),
        ]),
    );

    let world = world();
    let report = spawn_scene(&world, &scene, Transform::default());

    let mut roads = Vec::new();
    world.query::<&Road>().build().each(|road| roads.push(road.0.clone()));
    assert_eq!(
        roads,
        vec![WorldPath {
            name: "a".to_owned(),
            points: vec![Vec3::new(0.0, 100.0, 0.0), Vec3::new(100.0, 110.0, 0.0), Vec3::new(200.0, 120.0, 50.0)],
            width: 8.0,
        }]
    );

    let mut rivers = 0;
    world.query::<&River>().build().each(|_| rivers += 1);
    assert_eq!(rivers, 0);
    assert_eq!(report.issues.len(), 1, "{:?}", report.issues);
    assert!(report.issues[0].message.contains("at least two nodes"));

    assert_eq!(
        world.entity_from_id(report.entities[4]).cloned::<&Landmark>(),
        Landmark { name: "Grey Peak".to_owned(), kind: "peak".to_owned(), position: Vec3::new(500.0, 900.0, 500.0) }
    );
}

#[cfg(feature = "app")]
#[test]
fn editor_lights_become_light_sources() {
    let color = Vec3::new(1.0, 0.5, 0.25);
    let point = point_light_source(&PointLight { color, intensity: 800.0, range: 6.0, shadows: false });
    assert_eq!((point.color, point.intensity, point.range, point.cone), (color, 800.0, 6.0, None));

    let spot = spot_light_source(&SpotLight {
        color,
        intensity: 1000.0,
        range: 10.0,
        inner_angle: 25.0,
        outer_angle: 35.0,
        shadows: false,
    });
    let cone = spot.cone.expect("spot lights have a cone");
    assert_eq!((cone.inner_degrees, cone.outer_degrees), (25.0, 35.0));
}

#[cfg(feature = "app")]
#[test]
fn greybox_colors_are_linearized_and_meshes_match_the_shape() {
    let linear = srgb_to_linear(Vec4::new(0.0, 0.5, 1.0, 0.7));
    assert_eq!(linear.x, 0.0);
    assert!((linear.y - 0.2140).abs() < 1e-3);
    assert!((linear.z - 1.0).abs() < 1e-6);
    assert_eq!(linear.w, 0.7);

    let cube = Greybox { shape: GreyboxShape::Box, size: Vec3::new(2.0, 3.0, 4.0), color: Greybox::DEFAULT_COLOR };
    assert_eq!(greybox_mesh(&cube).vertices.len(), 24);
    let ball = Greybox { shape: GreyboxShape::Sphere, ..cube };
    assert!(greybox_mesh(&ball).vertices.len() > 24);
}
