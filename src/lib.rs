#[cfg(feature = "app")]
pub mod app;
#[cfg(feature = "render")]
pub mod render;
#[cfg(feature = "physics")]
pub mod physics;
#[cfg(feature = "ui")]
pub mod ui;
#[cfg(feature = "render")]
pub mod error;
pub mod utils;
#[cfg(feature = "plugin")]
pub mod plugin;
pub mod math;
pub mod mesh;
pub mod time;
#[cfg(feature = "gamepad")]
pub mod gamepad;
#[cfg(feature = "scene")]
pub mod scene;
pub mod terrain;

use flecs_ecs::macros::Component;

#[derive(Component)]
pub struct Setup;

#[derive(Component)]
pub struct Update;

#[derive(Component)]
pub struct Render;

/// Runs after native and plugin `Render` systems.
#[derive(Component)]
pub struct RenderOverlay;

#[derive(Component)]
#[repr(C)]
pub struct RandomNumber {
    pub value: u16
}