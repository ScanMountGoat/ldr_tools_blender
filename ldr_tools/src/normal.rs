use glam::Vec3;

pub fn face_normals(
    vertices: &[Vec3],
    vertex_indices: &[u32],
    face_start_indices: &[u32],
    face_sizes: &[u32],
) -> Vec<Vec3> {
    face_start_indices
        .iter()
        .zip(face_sizes)
        .map(|(start, size)| {
            // TODO: Is this the best way to handle non triangular faces?
            let face = &vertex_indices[*start as usize..*start as usize + *size as usize];
            let v1 = vertices[face[0] as usize];
            let v2 = vertices[face[1] as usize];
            let v3 = vertices[face[2] as usize];

            let u = v2 - v1;
            let v = v3 - v1;
            u.cross(v).normalize()
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    use glam::vec3;

    #[test]
    fn normals_single_triangle() {
        let normals = face_normals(
            &[
                vec3(-5f32, 5f32, 1f32),
                vec3(-5f32, 0f32, 1f32),
                vec3(0f32, 0f32, 1f32),
            ],
            &[0, 1, 2],
            &[0],
            &[3],
        );
        assert_eq!(vec![vec3(0.0, 0.0, 1.0)], normals);
    }

    #[test]
    fn normals_single_quad() {
        let normals = face_normals(
            &[
                vec3(-5f32, 5f32, 1f32),
                vec3(-5f32, 0f32, 1f32),
                vec3(0f32, 0f32, 1f32),
                vec3(0f32, 5f32, 1f32),
            ],
            &[0, 1, 2, 3],
            &[0],
            &[4],
        );
        assert_eq!(vec![vec3(0.0, 0.0, 1.0)], normals);
    }
}
