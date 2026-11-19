use glam::Vec3;
use xastge::mesh::*;

#[test]
fn plane_faces_up_and_is_centred() {
    let plane = Mesh::<Vertex>::plane(10.0, 4);

    assert_eq!(plane.vertices.len(), 25);
    assert_eq!(plane.indices.len(), 4 * 4 * 6);
    assert_eq!(plane.vertices[0].position, Vec3::new(-5.0, 0.0, -5.0));
    assert_eq!(plane.vertices[24].position, Vec3::new(5.0, 0.0, 5.0));

    for tri in plane.indices.chunks(3) {
        let a = plane.vertices[tri[0] as usize].position;
        let b = plane.vertices[tri[1] as usize].position;
        let c = plane.vertices[tri[2] as usize].position;
        assert!((b - a).cross(c - a).y > 0.0);
    }
}
