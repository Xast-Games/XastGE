use glam::{Quat, Vec3};
use xastge::render::lit::*;

fn point(position: Vec3, range: f32) -> PlacedLight {
    PlacedLight {
        position,
        rotation: Quat::IDENTITY,
        light: LightSource { color: Vec3::ONE, intensity: 1000.0, range, cone: None },
    }
}

#[test]
fn environment_and_lights_are_packed() {
    let environment = LitEnvironment::default();
    let uniform = lights_uniform(&environment, Vec3::ZERO, &[point(Vec3::new(1.0, 2.0, 3.0), 10.0)]);

    assert_eq!(uniform.count.x, 1);
    assert_eq!(uniform.sky, environment.sky.extend(environment.ambient));
    assert_eq!(uniform.fog, environment.fog_color.extend(environment.fog_density));
    assert_eq!(uniform.lights[0].position, Vec3::new(1.0, 2.0, 3.0).extend(10.0));
    assert_eq!(uniform.lights[0].color.w, 1000.0 * INTENSITY_SCALE);
    assert_eq!(uniform.lights[0].cone.z, 0.0);
}

#[test]
fn only_the_nearest_lights_fit() {
    let far: Vec<_> = (0..MAX_LIGHTS).map(|i| point(Vec3::new(500.0 + i as f32, 0.0, 0.0), 10.0)).collect();
    let mut lights = far.clone();
    lights.push(point(Vec3::new(5.0, 0.0, 0.0), 10.0));
    lights.push(point(Vec3::ZERO, 0.0));

    let uniform = lights_uniform(&LitEnvironment::default(), Vec3::ZERO, &lights);

    assert_eq!(uniform.count.x as usize, MAX_LIGHTS);
    assert_eq!(uniform.lights[0].position.x, 5.0);
    assert!(uniform.lights.iter().all(|l| l.position.w > 0.0));
}

#[test]
fn spot_lights_point_down_their_negative_z() {
    let mut spot = point(Vec3::ZERO, 10.0);
    spot.rotation = Quat::from_rotation_x(-90f32.to_radians());
    spot.light.cone = Some(Cone { inner_degrees: 25.0, outer_degrees: 35.0 });

    let uniform = lights_uniform(&LitEnvironment::default(), Vec3::ZERO, &[spot]);
    let gpu = uniform.lights[0];

    assert!((gpu.direction.truncate() - Vec3::NEG_Y).length() < 1e-5);
    assert_eq!(gpu.cone.z, 1.0);
    assert!((gpu.cone.x - 25f32.to_radians().cos()).abs() < 1e-6);
    assert!((gpu.cone.y - 35f32.to_radians().cos()).abs() < 1e-6);
}
