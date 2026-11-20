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

fn faces_point_outward(mesh: &Mesh<Vertex>) {
    for tri in mesh.indices.chunks(3) {
        let [a, b, c] = [tri[0], tri[1], tri[2]].map(|i| mesh.vertices[i as usize].position);
        let face = (b - a).cross(c - a);
        if face.length() > 1e-6 {
            assert!(face.dot((a + b + c) / 3.0) > 0.0, "{a} {b} {c}");
        }
    }
}

#[test]
fn cuboid_spans_its_size_with_outward_faces() {
    let size = Vec3::new(2.0, 1.0, 4.0);
    let cuboid = Mesh::<Vertex>::cuboid(size);

    assert_eq!(cuboid.vertices.len(), 24);
    assert_eq!(cuboid.indices.len(), 36);
    for vertex in &cuboid.vertices {
        assert_eq!(vertex.position.abs(), size * 0.5);
        assert!(vertex.normal.dot(vertex.position) > 0.0);
    }
    faces_point_outward(&cuboid);
}

#[test]
fn ellipsoid_fits_its_size_with_outward_normals() {
    let size = Vec3::new(4.0, 2.0, 6.0);
    let ellipsoid = Mesh::<Vertex>::ellipsoid(size, 16, 8);

    assert_eq!(ellipsoid.vertices.len(), 17 * 9);
    assert_eq!(ellipsoid.indices.len(), 16 * 8 * 6);
    for vertex in &ellipsoid.vertices {
        let p = vertex.position / (size * 0.5);
        assert!((p.length() - 1.0).abs() < 1e-4);
        assert!(vertex.normal.dot(vertex.position) > 0.0);
        assert!((vertex.normal.length() - 1.0).abs() < 1e-4);
    }
    faces_point_outward(&ellipsoid);
}
