use glam::Vec3;
use serde::{Deserialize, Deserializer, de::Error};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ArchetypeError {
    #[error("cannot read params: {0}")]
    Params(#[from] serde_json::Error),
    #[error("{0}")]
    Invalid(String),
}

pub fn parse_vec3(text: &str) -> Option<Vec3> {
    let inner = text.trim().strip_prefix('(')?.strip_suffix(')')?;
    let mut parts = inner.split(',').map(|p| p.trim().parse::<f32>().ok());
    let v = Vec3::new(parts.next()??, parts.next()??, parts.next()??);
    parts.next().is_none().then_some(v)
}

pub fn vec3<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Vec3, D::Error> {
    let text = String::deserialize(deserializer)?;
    parse_vec3(&text).ok_or_else(|| D::Error::custom(format!("`{text}` is not a vec3 like \"(1.0, 2.0, 3.0)\"")))
}

pub fn color<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Vec3, D::Error> {
    vec3(deserializer).map(|c| c.clamp(Vec3::ZERO, Vec3::ONE))
}
