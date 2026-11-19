use std::{
    collections::HashMap,
    fmt,
    path::{Path, PathBuf},
    sync::Arc,
};

use flecs_ecs::prelude::*;
use glam::Mat4;
use serde::de::DeserializeOwned;

use crate::math::Transform;

use super::{
    builtin::{self, KebabNodeInfo, KebabSceneRoot, PREFAB_REF},
    document::{ArchetypeInstance, KebabNode, KebabScene},
    value::ArchetypeError,
};

pub struct SpawnContext<'a> {
    pub scene: &'a KebabScene,
    pub node: &'a KebabNode,
    pub index: usize,
    pub transform: Transform,
    root: Mat4,
    entities: &'a HashMap<String, Entity>,
}

impl SpawnContext<'_> {
    pub fn world_matrix(&self, node: &KebabNode) -> Mat4 {
        self.root * node.world
    }

    pub fn path(&self, relative: &str) -> PathBuf {
        self.scene.resolve(relative)
    }

    pub fn entity(&self, node_id: &str) -> Option<Entity> {
        self.entities.get(node_id).copied()
    }
}

type SpawnFn = dyn Fn(&SpawnContext<'_>, EntityView<'_>, &ArchetypeInstance) -> Result<(), ArchetypeError> + Send + Sync;

fn spawn_fn<F>(spawn: F) -> Arc<SpawnFn>
where
    F: Fn(&SpawnContext<'_>, EntityView<'_>, &ArchetypeInstance) -> Result<(), ArchetypeError> + Send + Sync + 'static,
{
    Arc::new(spawn)
}

#[derive(Clone)]
enum Entry {
    Spawn(Arc<SpawnFn>),
    Ignored,
}

#[derive(Component, Clone, Default)]
pub struct Archetypes {
    entries: HashMap<String, Entry>,
}

impl Archetypes {
    pub fn new() -> Archetypes {
        Archetypes::default()
    }

    pub fn with_builtins() -> Archetypes {
        let mut archetypes = Archetypes::new();
        builtin::register(&mut archetypes);
        archetypes
    }

    pub fn handler<F>(&mut self, id: impl Into<String>, spawn: F) -> &mut Archetypes
    where
        F: Fn(&SpawnContext<'_>, EntityView<'_>, &ArchetypeInstance) -> Result<(), ArchetypeError> + Send + Sync + 'static,
    {
        self.entries.insert(id.into(), Entry::Spawn(spawn_fn(spawn)));
        self
    }

    pub fn component<T>(&mut self, id: impl Into<String>) -> &mut Archetypes
    where
        T: ComponentId + DeserializeOwned + 'static,
    {
        self.handler(id, |_, entity, instance| {
            entity.set(instance.params::<T>()?);
            Ok(())
        })
    }

    pub fn ignore(&mut self, id: impl Into<String>) -> &mut Archetypes {
        self.entries.insert(id.into(), Entry::Ignored);
        self
    }

    pub fn contains(&self, id: &str) -> bool {
        id == PREFAB_REF || self.entries.contains_key(id)
    }
}

#[derive(Component)]
pub struct KebabModule;

impl Module for KebabModule {
    fn module(world: &World) {
        world.component::<Archetypes>().add_trait::<flecs::Singleton>();
        world.set(Archetypes::with_builtins());
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct SpawnIssue {
    pub scene: PathBuf,
    pub node: String,
    pub message: String,
}

impl fmt::Display for SpawnIssue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}: {}", self.scene.display(), self.node, self.message)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct SpawnReport {
    pub root: Entity,
    pub entities: Vec<Entity>,
    pub issues: Vec<SpawnIssue>,
}

const MAX_PREFAB_DEPTH: usize = 16;

fn canonical(path: &Path) -> PathBuf {
    path.canonicalize().unwrap_or_else(|_| path.to_owned())
}

fn transform(matrix: Mat4) -> Transform {
    let (scale, rotation, translation) = matrix.to_scale_rotation_translation();
    Transform { translation, rotation, scale }
}

struct Spawner<'w> {
    world: &'w World,
    archetypes: Archetypes,
    stack: Vec<PathBuf>,
    entities: Vec<Entity>,
    issues: Vec<SpawnIssue>,
}

impl Spawner<'_> {
    fn issue(&mut self, scene: &KebabScene, node: &KebabNode, message: impl Into<String>) {
        self.issues.push(SpawnIssue { scene: scene.path.clone(), node: node.label(), message: message.into() });
    }

    fn spawn(&mut self, scene: &KebabScene, root: Mat4, parent: Entity) {
        let world = self.world;
        let mut created: Vec<Entity> = Vec::with_capacity(scene.nodes.len());
        let mut ids = HashMap::new();

        for node in &scene.nodes {
            let parent = node.parent.map_or(parent, |p| created[p]);
            let entity = world
                .entity()
                .child_of(parent)
                .set(transform(root * node.world))
                .set(KebabNodeInfo { id: node.id.clone(), name: node.name.clone() });
            if let Some(greybox) = node.greybox {
                entity.set(greybox);
            }
            if let Some(id) = &node.id {
                ids.insert(id.clone(), entity.id());
            }
            created.push(entity.id());
            self.entities.push(entity.id());
        }

        for (index, node) in scene.nodes.iter().enumerate() {
            let context = SpawnContext {
                scene,
                node,
                index,
                transform: transform(root * node.world),
                root,
                entities: &ids,
            };

            for instance in &node.archetypes {
                if instance.id == PREFAB_REF {
                    self.prefab(scene, node, instance, root * node.world, created[index]);
                    continue;
                }

                let entry = self.archetypes.entries.get(&instance.id).cloned();
                match entry {
                    None => self.issue(scene, node, format!("unknown archetype `{}`", instance.id)),
                    Some(Entry::Ignored) => {}
                    Some(Entry::Spawn(spawn)) => {
                        if let Err(e) = spawn(&context, world.entity_from_id(created[index]), instance) {
                            self.issue(scene, node, format!("`{}`: {e}", instance.id));
                        }
                    }
                }
            }
        }
    }

    fn prefab(&mut self, scene: &KebabScene, node: &KebabNode, instance: &ArchetypeInstance, root: Mat4, entity: Entity) {
        let path = match instance.params::<builtin::RefPath>() {
            Ok(prefab) if !prefab.path.is_empty() => scene.resolve(&prefab.path),
            Ok(_) => return self.issue(scene, node, "prefab_ref has no path"),
            Err(e) => return self.issue(scene, node, e.to_string()),
        };

        let key = canonical(&path);
        if self.stack.contains(&key) {
            return self.issue(scene, node, format!("prefab refers back to itself: {}", path.display()));
        }
        if self.stack.len() >= MAX_PREFAB_DEPTH {
            return self.issue(scene, node, format!("prefabs are nested deeper than {MAX_PREFAB_DEPTH}"));
        }

        match KebabScene::load(&path) {
            Err(e) => self.issue(scene, node, e.to_string()),
            Ok(prefab) => {
                self.stack.push(key);
                self.spawn(&prefab, root, entity);
                self.stack.pop();
            }
        }
    }
}

pub fn spawn_scene(world: &World, scene: &KebabScene, root: Transform) -> SpawnReport {
    let archetypes = world.try_cloned::<&Archetypes>().unwrap_or_else(Archetypes::with_builtins);
    let root_entity = world.entity().set(root).set(KebabSceneRoot { path: scene.path.clone() }).id();

    let mut spawner = Spawner {
        world,
        archetypes,
        stack: vec![canonical(&scene.path)],
        entities: Vec::new(),
        issues: Vec::new(),
    };
    spawner.spawn(scene, root.to_matrix(), root_entity);

    SpawnReport { root: root_entity, entities: spawner.entities, issues: spawner.issues }
}
