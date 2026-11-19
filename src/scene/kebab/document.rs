use std::{
    collections::HashSet,
    path::{Path, PathBuf},
};

use flecs_ecs::prelude::*;
use glam::{Mat4, Quat, Vec3, Vec4};
use serde::de::DeserializeOwned;
use serde_json::{Map, Value};
use thiserror::Error;

use super::value::ArchetypeError;

#[derive(Debug, Clone, PartialEq)]
pub struct ArchetypeInstance {
    pub id: String,
    pub params: Map<String, Value>,
}

impl ArchetypeInstance {
    pub fn new(id: impl Into<String>) -> ArchetypeInstance {
        ArchetypeInstance { id: id.into(), params: Map::new() }
    }

    pub fn with(mut self, name: impl Into<String>, value: impl Into<Value>) -> ArchetypeInstance {
        self.params.insert(name.into(), value.into());
        self
    }

    pub fn params<T: DeserializeOwned>(&self) -> Result<T, ArchetypeError> {
        Ok(serde_json::from_value(Value::Object(self.params.clone()))?)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GreyboxShape {
    Box,
    Sphere,
}

#[derive(Component, Debug, Clone, Copy, PartialEq)]
pub struct Greybox {
    pub shape: GreyboxShape,
    pub size: Vec3,
    pub color: Vec4,
}

impl Greybox {
    pub const DEFAULT_COLOR: Vec4 = Vec4::new(168.0 / 255.0, 170.0 / 255.0, 176.0 / 255.0, 1.0);
}

#[derive(Debug, Clone, PartialEq)]
pub struct KebabNode {
    pub id: Option<String>,
    pub name: Option<String>,
    pub translation: Vec3,
    pub rotation: Quat,
    pub scale: f32,
    pub world: Mat4,
    pub parent: Option<usize>,
    pub children: Vec<usize>,
    pub greybox: Option<Greybox>,
    pub archetypes: Vec<ArchetypeInstance>,
}

impl KebabNode {
    pub fn local(&self) -> Mat4 {
        Mat4::from_scale_rotation_translation(Vec3::splat(self.scale), self.rotation, self.translation)
    }

    pub fn archetype(&self, id: &str) -> Option<&ArchetypeInstance> {
        self.archetypes.iter().find(|a| a.id == id)
    }

    pub fn label(&self) -> String {
        self.name.clone().or_else(|| self.id.clone()).unwrap_or_else(|| "<unnamed>".to_owned())
    }
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct KebabScene {
    pub path: PathBuf,
    pub nodes: Vec<KebabNode>,
    pub roots: Vec<usize>,
}

#[derive(Debug, Error)]
pub enum SceneError {
    #[error("cannot read {path}: {source}")]
    Io { path: PathBuf, source: std::io::Error },
    #[error("{path} is not valid JSON: {source}")]
    Json { path: PathBuf, source: serde_json::Error },
    #[error("{path}: {message}")]
    Format { path: PathBuf, message: String },
}

const UNIFORM_EPSILON: f32 = 1e-6;

struct Reader<'a> {
    path: &'a Path,
    json: &'a [Value],
    visited: HashSet<usize>,
    nodes: Vec<KebabNode>,
}

fn numbers<const N: usize>(value: Option<&Value>, fallback: [f32; N]) -> Option<[f32; N]> {
    let Some(value) = value else {
        return Some(fallback);
    };
    let array = value.as_array().filter(|a| a.len() == N)?;
    let mut out = [0.0; N];
    for (slot, item) in out.iter_mut().zip(array) {
        *slot = item.as_f64()? as f32;
    }
    Some(out)
}

fn greybox(kebab: &Value) -> Option<Greybox> {
    let json = kebab.get("greybox")?.as_object()?;
    let shape = match json.get("shape").and_then(Value::as_str)? {
        "box" => GreyboxShape::Box,
        "sphere" => GreyboxShape::Sphere,
        _ => return None,
    };
    let size = numbers(json.get("size"), [1.0; 3])?;
    let color = numbers(json.get("color"), Greybox::DEFAULT_COLOR.to_array())?;
    Some(Greybox {
        shape,
        size: Vec3::from_array(size),
        color: Vec4::from_array(color).clamp(Vec4::ZERO, Vec4::ONE),
    })
}

impl Reader<'_> {
    fn error(&self, message: impl Into<String>) -> SceneError {
        SceneError::Format { path: self.path.to_owned(), message: message.into() }
    }

    fn floats<const N: usize>(&self, json: &Map<String, Value>, key: &str, fallback: [f32; N], index: usize) -> Result<[f32; N], SceneError> {
        numbers(json.get(key), fallback).ok_or_else(|| self.error(format!("node {index}: `{key}` must have {N} numbers")))
    }

    fn archetypes(&self, kebab: &Value, index: usize) -> Result<Vec<ArchetypeInstance>, SceneError> {
        let Some(list) = kebab.get("archetypes").and_then(Value::as_array) else {
            return Ok(Vec::new());
        };
        list.iter()
            .map(|archetype| {
                let id = archetype
                    .get("id")
                    .and_then(Value::as_str)
                    .ok_or_else(|| self.error(format!("node {index} has an archetype without an id")))?;
                let params = archetype
                    .get("params")
                    .and_then(Value::as_object)
                    .cloned()
                    .unwrap_or_default();
                Ok(ArchetypeInstance { id: id.to_owned(), params })
            })
            .collect()
    }

    fn load(&mut self, index_json: &Value, parent: Option<usize>, parent_world: Mat4) -> Result<usize, SceneError> {
        let index = index_json
            .as_u64()
            .map(|i| i as usize)
            .filter(|i| *i < self.json.len())
            .ok_or_else(|| self.error(format!("invalid node index: {index_json}")))?;

        if !self.visited.insert(index) {
            return Err(self.error(format!("node {index} is referenced more than once")));
        }

        let nodes = self.json;
        let json = nodes[index].as_object().ok_or_else(|| self.error(format!("node {index} is not an object")))?;

        if json.contains_key("matrix") {
            return Err(self.error(format!("node {index} uses a matrix; only translation/rotation/scale are supported")));
        }

        let t = self.floats(json, "translation", [0.0; 3], index)?;
        let r = self.floats(json, "rotation", [0.0, 0.0, 0.0, 1.0], index)?;
        let s = self.floats(json, "scale", [1.0; 3], index)?;
        if (s[0] - s[1]).abs() > UNIFORM_EPSILON || (s[0] - s[2]).abs() > UNIFORM_EPSILON {
            return Err(self.error(format!("node {index} has a non-uniform scale")));
        }

        let kebab = json.get("extras").and_then(|e| e.get("kebab")).cloned().unwrap_or(Value::Null);
        let id = match kebab.get("id") {
            None => None,
            Some(Value::String(id)) if !id.is_empty() => Some(id.clone()),
            Some(other) => return Err(self.error(format!("node {index} has an invalid id: {other}"))),
        };

        let mut node = KebabNode {
            id,
            name: json.get("name").and_then(Value::as_str).map(str::to_owned),
            translation: Vec3::from_array(t),
            rotation: Quat::from_array(r).normalize(),
            scale: s[0],
            world: Mat4::IDENTITY,
            parent,
            children: Vec::new(),
            greybox: greybox(&kebab),
            archetypes: self.archetypes(&kebab, index)?,
        };
        node.world = parent_world * node.local();

        let world = node.world;
        let slot = self.nodes.len();
        self.nodes.push(node);

        if let Some(children) = json.get("children").and_then(Value::as_array) {
            for child in children {
                let child = self.load(child, Some(slot), world)?;
                self.nodes[slot].children.push(child);
            }
        }

        Ok(slot)
    }
}

impl KebabScene {
    pub fn load(path: impl AsRef<Path>) -> Result<KebabScene, SceneError> {
        let path = path.as_ref();
        let text = std::fs::read_to_string(path).map_err(|source| SceneError::Io { path: path.to_owned(), source })?;
        KebabScene::parse(&text, path)
    }

    pub fn parse(json: &str, path: impl Into<PathBuf>) -> Result<KebabScene, SceneError> {
        let path = path.into();
        let document: Value = serde_json::from_str(json).map_err(|source| SceneError::Json { path: path.clone(), source })?;

        let mut scene = KebabScene { path, nodes: Vec::new(), roots: Vec::new() };
        let Some(scenes) = document.get("scenes").and_then(Value::as_array).filter(|s| !s.is_empty()) else {
            return Ok(scene);
        };

        let scene_index = document.get("scene").and_then(Value::as_u64).unwrap_or(0) as usize;
        let empty = Vec::new();
        let nodes = document.get("nodes").and_then(Value::as_array).unwrap_or(&empty);

        let mut reader = Reader { path: &scene.path, json: nodes, visited: HashSet::new(), nodes: Vec::new() };
        let Some(selected) = scenes.get(scene_index) else {
            return Err(reader.error(format!("scene {scene_index} does not exist")));
        };

        let mut roots = Vec::new();
        if let Some(indices) = selected.get("nodes").and_then(Value::as_array) {
            for index in indices {
                roots.push(reader.load(index, None, Mat4::IDENTITY)?);
            }
        }

        scene.nodes = reader.nodes;
        scene.roots = roots;
        Ok(scene)
    }

    pub fn directory(&self) -> &Path {
        self.path.parent().unwrap_or(Path::new("."))
    }

    pub fn resolve(&self, relative: &str) -> PathBuf {
        self.directory().join(relative)
    }

    pub fn find(&self, id: &str) -> Option<usize> {
        self.nodes.iter().position(|n| n.id.as_deref() == Some(id))
    }
}
