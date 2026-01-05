use std::collections::{BTreeMap, BTreeSet};

use glam::Vec3;

use crate::normal::face_normals;

/// Calculate new vertices and indices by splitting the edges in `edges_to_split`.
/// The geometry must be triangulated!
///
/// This works similarly to Blender's "edge split" for calculating normals.
///
/// The current implementation hardcodes a normal angle threshold of 89 degrees to split sharp edges.
// https://github.com/blender/blender/blob/a32dbb8/source/blender/geometry/intern/mesh_split_edges.cc
pub fn split_edges(
    vertices: &[Vec3],
    vertex_indices: &[u32],
    face_starts: &[u32],
    face_sizes: &[u32],
    edges_to_split: &[[u32; 2]],
) -> (Vec<Vec3>, Vec<u32>) {
    let old_adjacent_faces = adjacent_faces(vertices, vertex_indices, face_starts, face_sizes);

    let mut edges_to_split = edges_to_split.to_vec();

    // Find sharp edges based on an angle threshold.
    let normals = face_normals(vertices, vertex_indices, face_starts, face_sizes);

    add_sharp_edges(
        &mut edges_to_split,
        vertex_indices,
        face_starts,
        face_sizes,
        &old_adjacent_faces,
        normals,
        89f32.to_radians(),
    );

    let mut should_split_vertex = vec![false; vertices.len()];
    let mut undirected_edges = BTreeSet::new();
    for [v0, v1] in &edges_to_split {
        // Treat edges as undirected.
        undirected_edges.insert([*v0, *v1]);
        undirected_edges.insert([*v1, *v0]);

        // Mark any vertices on an edge to split for duplication.
        should_split_vertex[*v0 as usize] = true;
        should_split_vertex[*v1 as usize] = true;
    }

    let (split_vertices, mut split_vertex_indices, duplicate_edges) = split_face_verts(
        vertices,
        vertex_indices,
        face_starts,
        face_sizes,
        &old_adjacent_faces,
        &should_split_vertex,
    );

    // Keep track of the new vertex adjacency while merging edges.
    let mut new_adjacent_faces = adjacent_faces(
        &split_vertices,
        &split_vertex_indices,
        face_starts,
        face_sizes,
    );

    merge_duplicate_edges(
        &mut split_vertex_indices,
        vertex_indices,
        face_starts,
        face_sizes,
        duplicate_edges,
        undirected_edges,
        &old_adjacent_faces,
        &mut new_adjacent_faces,
    );

    // Reindex and keep only unique vertices to remove loose vertices.
    // TODO: Why are there loose vertices?
    remove_loose_vertices(&split_vertices, &split_vertex_indices)
}

fn add_sharp_edges(
    edges_to_split: &mut Vec<[u32; 2]>,
    vertex_indices: &[u32],
    face_starts: &[u32],
    face_sizes: &[u32],
    adjacent_faces: &[BTreeSet<usize>],
    normals: Vec<Vec3>,
    angle_threshold: f32,
) {
    for i in 0..face_starts.len() {
        let face = face_indices(i, vertex_indices, face_starts, face_sizes);
        for j in 0..face.len().saturating_sub(1) {
            let v0 = face[j];
            let v1 = face[(j + 1) % face.len()];
            // Assume vertices are fully welded.
            let v0_faces = &adjacent_faces[v0 as usize];
            let v1_faces = &adjacent_faces[v1 as usize];

            let mut faces = v0_faces.intersection(v1_faces).copied();
            if let (Some(f0), Some(f1)) = (faces.next(), faces.next())
                && normals[f0].angle_between(normals[f1]) >= angle_threshold
            {
                edges_to_split.push([v0, v1]);
            }
        }
    }
}

fn remove_loose_vertices<T: Copy>(vertices: &[T], vertex_indices: &[u32]) -> (Vec<T>, Vec<u32>) {
    // Collect unique indices in sorted order.
    let indices: BTreeSet<u32> = vertex_indices.iter().copied().collect();
    let old_to_new_index: BTreeMap<u32, u32> = indices
        .iter()
        .enumerate()
        .map(|(i, index)| (*index, i as u32))
        .collect();

    // Map indices to a consecutive range to remove unused vertices.
    let new_vertices = indices.iter().map(|i| vertices[*i as usize]).collect();
    let new_indices = vertex_indices.iter().map(|i| old_to_new_index[i]).collect();

    (new_vertices, new_indices)
}

fn adjacent_faces<T>(
    vertices: &[T],
    vertex_indices: &[u32],
    face_starts: &[u32],
    face_sizes: &[u32],
) -> Vec<BTreeSet<usize>> {
    // TODO: Function and tests for this since it's shared with normals?
    // Assume the position indices are fully welded.
    // This simplifies calculating the adjacent face indices for each vertex.
    let mut adjacent_faces = vec![BTreeSet::new(); vertices.len()];
    for i in 0..face_starts.len() {
        for vi in face_indices(i, vertex_indices, face_starts, face_sizes) {
            adjacent_faces[*vi as usize].insert(i);
        }
    }
    adjacent_faces
}

fn merge_duplicate_edges(
    split_vertex_indices: &mut [u32],
    vertex_indices: &[u32],
    face_starts: &[u32],
    face_sizes: &[u32],
    duplicate_edges: BTreeSet<[u32; 2]>,
    edges_to_split: BTreeSet<[u32; 2]>,
    old_adjacent_faces: &[BTreeSet<usize>],
    new_adjacent_faces: &mut [BTreeSet<usize>],
) {
    // The splitting step can create lots of duplicate vertices.
    // Merge any of the duplicated edges that is not an edge to split.
    for [v0, v1] in duplicate_edges
        .into_iter()
        .filter(|e| !edges_to_split.contains(e))
    {
        // Find the faces indicent to this edge before splitting.
        let v0_faces = &old_adjacent_faces[v0 as usize];
        let v1_faces = &old_adjacent_faces[v1 as usize];
        let mut faces = v0_faces.intersection(v1_faces).copied();

        if let (Some(f0), Some(f1)) = (faces.next(), faces.next()) {
            merge_verts_in_faces(
                v0,
                v1,
                f0,
                f1,
                vertex_indices,
                face_starts,
                face_sizes,
                split_vertex_indices,
                new_adjacent_faces,
            );
        }
    }
}

fn merge_verts_in_faces(
    v0: u32,
    v1: u32,
    f0: usize,
    f1: usize,
    vertex_indices: &[u32],
    face_starts: &[u32],
    face_sizes: &[u32],
    split_vertex_indices: &mut [u32],
    new_adjacent_faces: &mut [BTreeSet<usize>],
) {
    // Merge an edge by merging both pairs of vertices.
    // We can find the matching vertices using the old indexing.
    // Merging each vertex pair also merges the adjacent faces.
    let v0_f0 = find_old_vertex_in_face(
        v0,
        f0,
        vertex_indices,
        split_vertex_indices,
        face_starts,
        face_sizes,
    );
    let v0_f1 = find_old_vertex_in_face(
        v0,
        f1,
        vertex_indices,
        split_vertex_indices,
        face_starts,
        face_sizes,
    );
    new_adjacent_faces[v0_f0 as usize].extend(new_adjacent_faces[v0_f1 as usize].clone());

    let v1_f0 = find_old_vertex_in_face(
        v1,
        f0,
        vertex_indices,
        split_vertex_indices,
        face_starts,
        face_sizes,
    );
    let v1_f1 = find_old_vertex_in_face(
        v1,
        f1,
        vertex_indices,
        split_vertex_indices,
        face_starts,
        face_sizes,
    );
    new_adjacent_faces[v1_f0 as usize].extend(new_adjacent_faces[v1_f1 as usize].clone());

    // Update the verts in each of the adjacent faces to use the f0 verts.
    // Use the new adjacency to keep track of what has already been merged.
    let v0_faces = &new_adjacent_faces[v0_f0 as usize];
    let v1_faces = &new_adjacent_faces[v1_f0 as usize];
    for adjacent_face in v0_faces.iter().chain(v1_faces.iter()) {
        let start = face_starts[*adjacent_face] as usize;
        let size = face_sizes[*adjacent_face] as usize;
        for i in start..start + size {
            if vertex_indices[i] == v0 {
                split_vertex_indices[i] = v0_f0;
            }
            if vertex_indices[i] == v1 {
                split_vertex_indices[i] = v1_f0;
            }
        }
    }
}

fn face_indices<'a>(
    face_index: usize,
    vertex_indices: &'a [u32],
    face_starts: &[u32],
    face_sizes: &[u32],
) -> &'a [u32] {
    let start = face_starts[face_index] as usize;
    let size = face_sizes[face_index] as usize;
    &vertex_indices[start..start + size]
}

fn face_indices_mut<'a>(
    face_index: usize,
    vertex_indices: &'a mut [u32],
    face_starts: &[u32],
    face_sizes: &[u32],
) -> &'a mut [u32] {
    let start = face_starts[face_index] as usize;
    let size = face_sizes[face_index] as usize;
    &mut vertex_indices[start..start + size]
}

fn find_old_vertex_in_face(
    old_vertex_index: u32,
    face_index: usize,
    old_indices: &[u32],
    new_indices: &[u32],
    face_starts: &[u32],
    face_sizes: &[u32],
) -> u32 {
    // Find the corresponding vertex index in the new face.
    face_indices(face_index, old_indices, face_starts, face_sizes)
        .iter()
        .zip(face_indices(
            face_index,
            new_indices,
            face_starts,
            face_sizes,
        ))
        .find_map(|(old, new)| {
            if *old == old_vertex_index {
                Some(*new)
            } else {
                None
            }
        })
        .unwrap()
}

fn split_face_verts<T: Copy>(
    vertices: &[T],
    vertex_indices: &[u32],
    face_starts: &[u32],
    face_sizes: &[u32],
    adjacent_faces: &[BTreeSet<usize>],
    should_split_vertex: &[bool],
) -> (Vec<T>, Vec<u32>, BTreeSet<[u32; 2]>) {
    // Split edges by duplicating the vertices.
    // This creates some duplicate edges to be cleaned up later.
    let mut split_vertices = vertices.to_vec();
    let mut split_vertex_indices = vertex_indices.to_vec();

    let mut duplicate_edges = BTreeSet::new();

    // Iterate over all the indices of marked vertices.
    for vertex_index in should_split_vertex
        .iter()
        .enumerate()
        .filter_map(|(v, split)| split.then_some(v))
    {
        for (i, f) in adjacent_faces[vertex_index].iter().enumerate() {
            let face = face_indices_mut(*f, &mut split_vertex_indices, face_starts, face_sizes);

            // Duplicate the vertex in all faces except the first.
            // The first face can just use the original index.
            if i > 0 {
                for face_vert in face.iter_mut() {
                    if *face_vert == vertex_index as u32 {
                        *face_vert = split_vertices.len() as u32;
                        split_vertices.push(split_vertices[vertex_index]);
                    }
                }
            }

            // Find any edges that may need to be merged later.
            let original_face = face_indices(*f, vertex_indices, face_starts, face_sizes);
            let (e0, e1) = find_incident_edges(original_face, vertex_index);

            duplicate_edges.insert(e0);
            duplicate_edges.insert(e1);
        }
    }

    (split_vertices, split_vertex_indices, duplicate_edges)
}

fn find_incident_edges(face: &[u32], vertex_index: usize) -> ([u32; 2], [u32; 2]) {
    // Assume edges are [0,1], ..., [N-1,0] for N vertices.
    let i = face.iter().position(|v| *v == vertex_index as u32).unwrap();
    let prev = if i > 0 { i - 1 } else { face.len() - 1 };
    let next = (i + 1) % face.len();
    let mut e0 = [face[i], face[prev]];
    let mut e1 = [face[i], face[next]];

    // Edges are undirected, so normalize the direction for each edge.
    // This avoids redundant merge operations later.
    e0.sort();
    e1.sort();

    (e0, e1)
}

#[cfg(test)]
mod tests {
    use glam::vec3;

    use super::*;

    fn v3(f: f32) -> Vec3 {
        Vec3::splat(f)
    }

    #[test]
    fn split_edges_triangle_no_sharp_edges() {
        // 2
        // | \
        // 0 - 1

        assert_eq!(
            (vec![v3(0.0), v3(1.0), v3(2.0)], vec![0, 1, 2]),
            split_edges(&[v3(0.0), v3(1.0), v3(2.0)], &[0, 1, 2], &[0], &[3], &[])
        );
    }

    #[test]
    fn split_edges_quad() {
        // Quad of two tris and one sharp edge.
        // The topology shouldn't change since 2-3 is already a boundary.
        // 2 - 3
        // | \ |
        // 0 - 1

        let indices = vec![0, 1, 2, 2, 1, 3];
        assert_eq!(
            (
                vec![v3(0.0), v3(1.0), v3(2.0), v3(3.0)],
                vec![0, 1, 2, 2, 1, 3]
            ),
            split_edges(
                &[v3(0.0), v3(1.0), v3(2.0), v3(3.0)],
                &indices,
                &[0, 3],
                &[3, 3],
                &[[2, 3]]
            )
        );
    }

    #[test]
    fn split_edges_two_quads() {
        // Two quads of two tris.
        // The topology shouldn't change for splitting boundaries.
        // 2 - 3 - 5
        // | \ | \ |
        // 0 - 1 - 4

        let indices = vec![0, 1, 2, 2, 1, 3, 3, 1, 4, 3, 4, 5];
        assert_eq!(
            (
                vec![v3(0.0), v3(1.0), v3(2.0), v3(3.0), v3(4.0), v3(5.0)],
                vec![0, 1, 2, 2, 1, 3, 3, 1, 4, 3, 4, 5]
            ),
            split_edges(
                &[v3(0.0), v3(1.0), v3(2.0), v3(3.0), v3(4.0), v3(5.0)],
                &indices,
                &[0, 3, 6, 9],
                &[3, 3, 3, 3],
                &[[2, 3], [3, 5], [0, 1], [1, 4]]
            )
        );
    }

    #[test]
    fn split_edges_split_two_triangulated_quads() {
        // Two quads of two tris and one sharp edge.
        // 2 - 3 - 4
        // | \ | \ |
        // 0 - 1 - 5

        // The edge 1-3 splits the quads in two.
        // 2 - 3    7 - 4
        // | \ |    | \ |
        // 0 - 1    6 - 5

        let indices = vec![0, 1, 2, 2, 1, 3, 3, 1, 5, 3, 5, 4];
        assert_eq!(
            (
                vec![
                    v3(0.0),
                    v3(1.0),
                    v3(2.0),
                    v3(3.0),
                    v3(4.0),
                    v3(5.0),
                    v3(1.0),
                    v3(3.0)
                ],
                vec![0, 1, 2, 2, 1, 3, 7, 6, 5, 7, 5, 4]
            ),
            split_edges(
                &[v3(0.0), v3(1.0), v3(2.0), v3(3.0), v3(4.0), v3(5.0)],
                &indices,
                &[0, 3, 6, 9],
                &[3, 3, 3, 3],
                &[[1, 3]]
            )
        );
    }

    #[test]
    fn split_edges_split_two_quads() {
        // Two quads and one sharp edge.
        // 3 - 2 - 5
        // |   |   |
        // 0 - 1 - 4

        // The edge 1-2 splits the quads in two.
        // 3 - 2    7 - 5
        // |   |    |   |
        // 0 - 1    6 - 4

        let indices = vec![0, 1, 2, 3, 1, 4, 5, 2];
        assert_eq!(
            (
                vec![
                    v3(0.0),
                    v3(1.0),
                    v3(2.0),
                    v3(3.0),
                    v3(4.0),
                    v3(5.0),
                    v3(1.0),
                    v3(2.0)
                ],
                vec![0, 1, 2, 3, 6, 4, 5, 7]
            ),
            split_edges(
                &[v3(0.0), v3(1.0), v3(2.0), v3(3.0), v3(4.0), v3(5.0)],
                &indices,
                &[0, 4],
                &[4, 4],
                &[[1, 2]]
            )
        );
    }

    #[test]
    fn split_edges_split_1_8cyli_dat() {
        // TODO: Is this right?
        // Example taken from p/1-8cyli.dat.
        // 3 - 0 - 4
        // | / | / |
        // 2 - 1 - 5

        // 4 - 1 - 5
        // | / | / |
        // 3 - 2 - 0
        assert_eq!(
            (
                vec![v3(0.0), v3(2.0), v3(3.0), v3(4.0), v3(5.0), v3(1.0)],
                vec![1, 5, 0, 2, 1, 0, 5, 4, 3, 0, 5, 3]
            ),
            split_edges(
                &[v3(0.0), v3(1.0), v3(2.0), v3(3.0), v3(4.0), v3(5.0)],
                &[2, 1, 0, 3, 2, 0, 1, 5, 4, 0, 1, 4],
                &[0, 3, 6, 9],
                &[3, 3, 3, 3],
                &[[2, 1], [0, 3], [1, 5], [4, 0]]
            )
        );
    }

    #[test]
    fn split_edges_normals_tetrahedron() {
        // TODO: Make this more mathematically precise
        // The angle threshold should split all faces.
        assert_eq!(
            (
                vec![
                    vec3(0.0, -0.707, -1.0),
                    vec3(0.866025, -0.707, 0.5),
                    vec3(-0.866025, -0.707, 0.5),
                    vec3(0.0, 0.707, 0.0),
                    vec3(0.0, -0.707, -1.0),
                    vec3(0.866025, -0.707, 0.5),
                    vec3(0.866025, -0.707, 0.5),
                    vec3(-0.866025, -0.707, 0.5),
                    vec3(0.0, 0.707, 0.0),
                    vec3(0.0, 0.707, 0.0)
                ],
                vec![0, 3, 1, 4, 5, 2, 6, 8, 7, 2, 9, 4]
            ),
            split_edges(
                &[
                    vec3(0.000000, -0.707000, -1.000000),
                    vec3(0.866025, -0.707000, 0.500000),
                    vec3(-0.866025, -0.707000, 0.500000),
                    vec3(0.000000, 0.707000, 0.000000),
                ],
                &[0, 3, 1, 0, 1, 2, 1, 3, 2, 2, 3, 0],
                &[0, 3, 6, 9],
                &[3, 3, 3, 3],
                &[]
            )
        );
    }

    // TODO: test normal threshold and hard edges together.
}
