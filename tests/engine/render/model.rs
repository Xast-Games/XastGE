use std::path::PathBuf;

use glam::{Vec2, Vec3, Vec4};
use xastge::render::model::*;

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("xastge-model-{}-{name}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("scratch dir");
    dir
}

fn quad_bin() -> Vec<u8> {
    let positions = [[0.0f32, 0.0, 0.0], [1.0, 0.0, 0.0], [1.0, 0.0, 1.0], [0.0, 0.0, 1.0]];
    let indices = [0u16, 3, 2, 0, 2, 1];

    let mut bytes = Vec::new();
    for p in positions.iter().flatten() {
        bytes.extend_from_slice(&p.to_le_bytes());
    }
    for i in indices {
        bytes.extend_from_slice(&i.to_le_bytes());
    }
    bytes
}

const QUAD_GLTF: &str = r#"{
    "asset": { "version": "2.0" },
    "scene": 0,
    "scenes": [{ "nodes": [0] }],
    "nodes": [
        { "translation": [10.0, 0.0, 0.0], "scale": [2.0, 2.0, 2.0], "children": [1] },
        { "mesh": 0, "translation": [0.0, 1.0, 0.0] }
    ],
    "meshes": [{
        "primitives": [
            { "attributes": { "POSITION": 0 }, "indices": 1, "material": 0 },
            { "attributes": { "POSITION": 0 }, "indices": 1 }
        ]
    }],
    "materials": [{ "name": "Red", "pbrMetallicRoughness": { "baseColorFactor": [1.0, 0.0, 0.0, 1.0], "roughnessFactor": 0.3 } }],
    "buffers": [{ "uri": "quad.bin", "byteLength": 60 }],
    "bufferViews": [
        { "buffer": 0, "byteOffset": 0, "byteLength": 48, "target": 34962 },
        { "buffer": 0, "byteOffset": 48, "byteLength": 12, "target": 34963 }
    ],
    "accessors": [
        { "bufferView": 0, "componentType": 5126, "count": 4, "type": "VEC3", "min": [0.0, 0.0, 0.0], "max": [1.0, 0.0, 1.0] },
        { "bufferView": 1, "componentType": 5123, "count": 6, "type": "SCALAR" }
    ]
}"#;

fn quad(name: &str) -> ModelData {
    let dir = scratch(name);
    std::fs::write(dir.join("quad.bin"), quad_bin()).unwrap();
    std::fs::write(dir.join("quad.gltf"), QUAD_GLTF).unwrap();
    ModelData::load(dir.join("quad.gltf")).expect("quad loads")
}

#[test]
fn primitives_become_parts_baked_into_model_space() {
    let model = quad("parts");

    assert_eq!(model.parts.len(), 2);
    let part = &model.parts[0];
    assert_eq!(part.indices, vec![0, 3, 2, 0, 2, 1]);
    assert!((part.vertices[2].position - Vec3::new(12.0, 2.0, 2.0)).length() < 1e-5);
    assert!((part.vertices[0].position - Vec3::new(10.0, 2.0, 0.0)).length() < 1e-5);
    for vertex in &part.vertices {
        assert!((vertex.normal - Vec3::Y).length() < 1e-5, "{:?}", vertex.normal);
        assert_eq!(vertex.texcoord, Vec2::ZERO);
    }
}

#[test]
fn materials_map_to_surfaces_with_a_fallback() {
    let model = quad("surfaces");

    assert_eq!(model.surfaces.len(), 2);
    assert_eq!(model.parts[0].surface, 0);
    assert_eq!(model.parts[1].surface, 1);

    let red = &model.surfaces[0];
    assert_eq!(red.name.as_deref(), Some("Red"));
    assert_eq!(red.base_color, Vec4::new(1.0, 0.0, 0.0, 1.0));
    assert!((red.roughness - 0.3).abs() < 1e-6);
    assert_eq!(model.surfaces[1], SurfaceData::default());
}

#[test]
fn missing_files_are_errors() {
    let missing = scratch("missing").join("nothing.gltf");
    assert!(matches!(ModelData::load(&missing), Err(ModelError::Gltf { .. })));
}
