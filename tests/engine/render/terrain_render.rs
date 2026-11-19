use glam::Vec3;
use xastge::render::terrain_render::*;

fn project(u: &LightUniform, p: Vec3) -> Vec3 {
    let clip = u.view_proj * p.extend(1.0);
    clip.truncate() / clip.w
}

#[test]
fn focus_lands_in_the_middle_of_the_shadow_map() {
    let lighting = TerrainLighting::default();
    let u = light_uniform(&lighting, Vec3::new(1234.0, 50.0, -777.0), 2048.0);
    let ndc = project(&u, Vec3::new(1234.0, 0.0, -777.0));

    assert!(ndc.x.abs() < 0.01 && ndc.y.abs() < 0.01, "{ndc:?}");
    assert!((0.0..1.0).contains(&ndc.z), "{ndc:?}");
}

#[test]
fn points_toward_the_sun_are_closer_to_the_light() {
    let lighting = TerrainLighting::default();
    let u = light_uniform(&lighting, Vec3::ZERO, 2048.0);

    let ground = project(&u, Vec3::ZERO);
    let raised = project(&u, lighting.sun_direction * 500.0);
    assert!(raised.z < ground.z);
}

#[test]
fn snapping_keeps_the_focus_within_one_texel() {
    let lighting = TerrainLighting::default();
    let texel_ndc = 2.0 / 2048.0;

    for x in [0.0, 13.7, 999.9, -4321.2] {
        let u = light_uniform(&lighting, Vec3::new(x, 0.0, x * 0.5), 2048.0);
        let ndc = project(&u, Vec3::new(x, 0.0, x * 0.5));
        assert!(ndc.x.abs() <= texel_ndc && ndc.y.abs() <= texel_ndc, "{ndc:?}");
    }
}
