use std::collections::{BTreeSet, HashMap, HashSet};

/// Calculate new vertices and indices by splitting the edges in `edges_to_split`.
/// The geometry must be triangulated!
///
/// This works similarly to Blender's "edge split" for calculating normals.
// https://github.com/blender/blender/blob/a32dbb8/source/blender/geometry/intern/mesh_split_edges.cc
pub fn split_edges<T: Copy>(
    vertices: &[T],
    vertex_indices: &[u32],
    face_starts: &[u32],
    face_sizes: &[u32],
    edges_to_split: &[[u32; 2]],
) -> (Vec<T>, Vec<u32>) {
    // TODO: should ldr_tools just store sharp edges?
    let mut should_split_vertex = vec![false; vertices.len()];
    let mut undirected_edges = HashSet::new();
    for [v0, v1] in edges_to_split {
        // Treat edges as undirected.
        undirected_edges.insert([*v0, *v1]);
        undirected_edges.insert([*v1, *v0]);

        // Mark any vertices on an edge to split for duplication.
        should_split_vertex[*v0 as usize] = true;
        should_split_vertex[*v1 as usize] = true;
    }

    let old_adjacent_faces = adjacent_faces(vertices, vertex_indices, face_starts, face_sizes);

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

    reindex_vertices(split_vertex_indices, split_vertices)
}

fn reindex_vertices<T: Copy>(
    split_vertex_indices: Vec<u32>,
    split_vertices: Vec<T>,
) -> (Vec<T>, Vec<u32>) {
    // Reindex to use the indices 0..N.
    // Truncate the split vertices to length N.
    let mut verts = Vec::new();
    let mut indices = Vec::new();
    let mut remapped_indices = HashMap::new();

    // Map each index to a new index.
    // Use this mapping to create the new vertices as well.
    for index in split_vertex_indices {
        if let Some(new_index) = remapped_indices.get(&index) {
            indices.push(*new_index);
        } else {
            let new_index = remapped_indices.len() as u32;
            verts.push(split_vertices[index as usize]);
            indices.push(new_index);
            remapped_indices.insert(index, new_index);
        }
    }

    (verts, indices)
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
    duplicate_edges: HashSet<[u32; 2]>,
    edges_to_split: HashSet<[u32; 2]>,
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
) -> (Vec<T>, Vec<u32>, HashSet<[u32; 2]>) {
    // Split edges by duplicating the vertices.
    // This creates some duplicate edges to be cleaned up later.
    let mut split_vertices = vertices.to_vec();
    let mut split_vertex_indices = vertex_indices.to_vec();

    let mut duplicate_edges = HashSet::new();

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
    use super::*;

    #[test]
    fn split_edges_triangle_no_sharp_edges() {
        // 2
        // | \
        // 0 - 1

        assert_eq!(
            (vec![0.0, 1.0, 2.0], vec![0, 1, 2]),
            split_edges(&[0.0, 1.0, 2.0], &[0, 1, 2], &[0], &[3], &[])
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
            (vec![0.0, 1.0, 2.0, 3.0], indices.clone()),
            split_edges(&[0.0, 1.0, 2.0, 3.0], &indices, &[0, 3], &[3, 3], &[[2, 3]])
        );
    }

    #[test]
    fn split_edges_two_quads() {
        // Two quads of two tris.
        // The topology shouldn't change.
        // 2 - 3 - 5
        // | \ | \ |
        // 0 - 1 - 4

        let indices = vec![0, 1, 2, 2, 1, 3, 3, 1, 4, 3, 4, 5];
        assert_eq!(
            (vec![0.0, 1.0, 2.0, 3.0, 4.0, 5.0], indices.clone()),
            split_edges(
                &[0.0, 1.0, 2.0, 3.0, 4.0, 5.0],
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
        // 2 - 3    4 - 7
        // | \ |    | \ |
        // 0 - 1    5 - 6

        let indices = vec![0, 1, 2, 2, 1, 3, 3, 1, 5, 3, 5, 4];
        assert_eq!(
            (
                vec![0.0, 1.0, 2.0, 3.0, 3.0, 1.0, 5.0, 4.0],
                vec![0, 1, 2, 2, 1, 3, 4, 5, 6, 4, 6, 7]
            ),
            split_edges(
                &[0.0, 1.0, 2.0, 3.0, 4.0, 5.0],
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
        // 3 - 2    7 - 6
        // |   |    |   |
        // 0 - 1    4 - 5

        let indices = vec![0, 1, 2, 3, 1, 4, 5, 2];
        assert_eq!(
            (
                vec![0.0, 1.0, 2.0, 3.0, 1.0, 4.0, 5.0, 2.0],
                vec![0, 1, 2, 3, 4, 5, 6, 7]
            ),
            split_edges(
                &[0.0, 1.0, 2.0, 3.0, 4.0, 5.0],
                &indices,
                &[0, 4],
                &[4, 4],
                &[[1, 2]]
            )
        );
    }

    #[test]
    fn split_edges_split_1_8cyli_dat() {
        // Example taken from p/1-8cyli.dat.
        // 3 - 0 - 4
        // | / | / |
        // 2 - 1 - 5

        // After splitting sharp edges.
        // 3 - 2 - 5
        // | / | / |
        // 0 - 1 - 4

        assert_eq!(
            (
                vec![2.0, 1.0, 0.0, 3.0, 5.0, 4.0],
                vec![0, 1, 2, 3, 0, 2, 1, 4, 5, 2, 1, 5]
            ),
            split_edges(
                &[0.0, 1.0, 2.0, 3.0, 4.0, 5.0],
                &[2, 1, 0, 3, 2, 0, 1, 5, 4, 0, 1, 4],
                &[0, 3, 6, 9],
                &[3, 3, 3, 3],
                &[[2, 1], [0, 3], [1, 5], [4, 0]]
            )
        );
    }
}
