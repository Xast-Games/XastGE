use std::{
    fs, 
    path::PathBuf, 
    sync::OnceLock,
};
use flecs_ecs::{
    core::{World, utility::traits::IdOperations},
    sys::{ecs_entity_t, ecs_world_t},
};
use libloading::Library;
use parking_lot::Mutex;
use thiserror::Error;

use crate::{RandomNumber, Render, Update};

static PLUGIN_REGISTRY: OnceLock<Mutex<PluginRegistry>> = OnceLock::new();

#[repr(C)]
pub struct XastSched {
    _private: [u8; 0],
}

unsafe extern "C" {
    fn xast_sched_new(world: *mut ecs_world_t, workers: i32) -> *mut XastSched;
    fn xast_sched_new_unit(sched: *mut XastSched) -> u32;
    fn xast_sched_register(sched: *mut XastSched, unit: u32, phase: u32, system: ecs_entity_t);
    fn xast_sched_run_phase(sched: *mut XastSched, phase: u32, dt: f32);
}

struct SchedHandle(*mut XastSched);

// SAFETY: only used while the registry mutex is held, and the scheduler is driven from one thread.
unsafe impl Send for SchedHandle {}

#[derive(Default)]
pub struct PluginRegistry {
    plugins: Vec<Library>,
    sched: Option<SchedHandle>,
}

impl PluginRegistry {
    pub fn push(&mut self, plugin: Library) {
        self.plugins.push(plugin);
    }

    fn sched_for(&mut self, world: *mut ecs_world_t) -> *mut XastSched {
        self.sched
            .get_or_insert_with(|| SchedHandle(unsafe { xast_sched_new(world, 0) }))
            .0
    }
}

#[derive(Debug, Error)]
pub enum LoadPluginError {
    #[error("Plugin file cannot be found: {0}")]
    FileNotFound(PathBuf),
    #[error("Error opening plugin shared library: {0}")]
    SharedLibraryError(libloading::Error),
}

pub trait LoadPluginExt {
    fn load_plugin(&self, name: &str) -> Result<(), LoadPluginError>;
}

impl LoadPluginExt for World {
    fn load_plugin(&self, name: &str) -> Result<(), LoadPluginError> {
        let filename = libloading::library_filename(name);
        let mut path = PathBuf::new();
        path.push("assets");
        path.push("plugins");
        path.push(&filename);

        if !fs::exists(&path).unwrap_or(false) {
            return Err(LoadPluginError::FileNotFound(path));
        }

        let plugin = unsafe {
            Library::new(path)
                .map_err(LoadPluginError::SharedLibraryError)?
        };

        let mut registry = PLUGIN_REGISTRY
            .get_or_init(|| Mutex::new(PluginRegistry::default()))
            .lock();

        unsafe {
            let register_systems = plugin.get::<RegisterSystems>(b"register_systems")
                .map_err(LoadPluginError::SharedLibraryError)?;

            let sched = registry.sched_for(self.ptr_mut());

            let api = PluginApi {
                world: self.ptr_mut(),
                update_label: *self.component::<Update>().id(),
                render_label: *self.component::<Render>().id(),
                random_number: *self.component::<RandomNumber>().id(),
                sched,
                unit: xast_sched_new_unit(sched),
                sched_register: xast_sched_register,
            };

            register_systems(&api);
        }

        registry.push(plugin);

        Ok(())
    }
}

pub type RegisterSystems = unsafe extern "C" fn(*const PluginApi);

#[repr(C)]
pub struct PluginApi {
    // World
    pub world: *mut ecs_world_t,
    // Labels
    pub update_label: ecs_entity_t,
    pub render_label: ecs_entity_t,
    // Components
    pub random_number: ecs_entity_t,
    // Scheduler, owned by the host
    pub sched: *mut XastSched,
    pub unit: u32,
    pub sched_register: unsafe extern "C" fn(*mut XastSched, u32, u32, ecs_entity_t),
}

/// Mirrors the `XastPhase` enum in `cdylib/plugin.c`.
#[repr(u32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum XastPhase {
    Setup = 0,
    Update = 1,
    Render = 2,
}

/// Runs `phase` for every loaded plugin, one plugin after another.
pub fn run_phase(_world: &World, phase: XastPhase, dt: f32) {
    let Some(registry) = PLUGIN_REGISTRY.get() else {
        return;
    };
    let registry = registry.lock();

    if let Some(sched) = &registry.sched {
        unsafe { xast_sched_run_phase(sched.0, phase as u32, dt) };
    }
}
