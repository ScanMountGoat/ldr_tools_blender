use std::collections::HashSet;

use glam::{Mat4, Vec3};
use rstar::{primitives::GeomWithData, RTree};
use weldr::Command;

use crate::{replace_color, ColorCode, SCENE_SCALE};

// TODO: use the edge information to calculate smooth normals directly in Rust?
// TODO: Document the data layout for these fields.
#[derive(Debug, PartialEq)]
pub struct LDrawGeometry {
    pub vertices: Vec<Vec3>,
    pub vertex_indices: Vec<u32>,
    pub face_start_indices: Vec<u32>,
    pub face_sizes: Vec<u32>,
    /// The colors of each face or a single element if all faces share a color.
    pub face_colors: Vec<FaceColor>,
    pub edges: Vec<[u32; 2]>,
    pub is_edge_sharp: Vec<bool>,
}

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub struct FaceColor {
    pub color: ColorCode,
    pub is_grainy_slope: bool,
}

struct GeometryContext {
    current_color: ColorCode,
    transform: Mat4,
    inverted: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Winding {
    Ccw,
    Cw,
}

struct VertexMap {
    rtree: RTree<rstar::primitives::GeomWithData<[f32; 3], u32>>,
}

impl VertexMap {
    fn new() -> Self {
        Self {
            rtree: RTree::new(),
        }
    }

    fn get_nearest(&self, v: [f32; 3]) -> Option<u32> {
        // TODO: Why do edges require higher tolerances?
        self.rtree.nearest_neighbor(&v).map(|p| p.data)
    }

    fn get(&self, v: [f32; 3]) -> Option<u32> {
        // Return the value already in the map or None.
        // Dimensions in LDUs tend to be large, so use a large threshold.
        let epsilon = 0.001;
        self.rtree
            .locate_within_distance(v, epsilon * epsilon)
            .next()
            .map(|p| p.data)
    }

    fn insert(&mut self, i: u32, v: [f32; 3]) -> Option<u32> {
        match self.get(v) {
            Some(index) => Some(index),
            None => {
                // This vertex isn't in the map yet, so add it.
                self.rtree.insert(GeomWithData::new(v, i));
                None
            }
        }
    }
}

pub fn create_geometry(
    source_file: &weldr::SourceFile,
    source_map: &weldr::SourceMap,
    current_color: ColorCode,
    recursive: bool,
) -> LDrawGeometry {
    let mut geometry = LDrawGeometry {
        vertices: Vec::new(),
        vertex_indices: Vec::new(),
        face_start_indices: Vec::new(),
        face_sizes: Vec::new(),
        face_colors: Vec::new(),
        edges: Vec::new(),
        is_edge_sharp: Vec::new(),
    };

    // Start with inverted set to false since parts should never be inverted.
    // TODO: Is this also correct for geometry within an MPD file?
    let ctx = GeometryContext {
        current_color,
        transform: Mat4::IDENTITY,
        inverted: false,
    };

    let mut vertex_map = VertexMap::new();
    let mut hard_edges = Vec::new();

    append_geometry(
        &mut geometry,
        &mut hard_edges,
        &mut vertex_map,
        source_file,
        source_map,
        ctx,
        recursive,
    );

    geometry.is_edge_sharp = get_sharp_edges(&geometry.edges, &hard_edges, &vertex_map);

    // Optimize the case where all face colors are the same.
    // This reduces overhead when processing data in Python.
    // A single color can be applied per object rather than per face.
    if let Some(color) = geometry.face_colors.first() {
        if geometry.face_colors.iter().all(|c| c == color) {
            geometry.face_colors = vec![*color];
        }
    }

    let min = geometry
        .vertices
        .iter()
        .copied()
        .reduce(Vec3::min)
        .unwrap_or_default();
    let max = geometry
        .vertices
        .iter()
        .copied()
        .reduce(Vec3::max)
        .unwrap_or_default();
    let dimensions = max - min;

    // TODO: Avoid applying this on chains, ropes, etc?
    // TODO: Weld ropes into a single piece?
    // Convert a distance between parts to a scale factor.
    // This gap is in LDUs since we haven't scaled the part yet.
    let gap_distance = 0.1;
    let gaps_scale = if dimensions.length_squared() > 0.0 {
        (2.0 * gap_distance - dimensions) / dimensions
    } else {
        Vec3::ONE
    };

    // Apply the scale last to use LDUs as the unit for vertex welding.
    // This avoids small floating point comparisons for small scene scales.
    for vertex in &mut geometry.vertices {
        *vertex *= gaps_scale.abs() * SCENE_SCALE;
    }

    geometry
}

fn get_sharp_edges(
    edges: &[[u32; 2]],
    hard_edges: &[[Vec3; 2]],
    vertex_map: &VertexMap,
) -> Vec<bool> {
    // Find the edges marked as edges in the LDraw geometry.
    // These edges can be split by consuming applications later.
    let mut hard_edge_indices = HashSet::new();
    for [v0, v1] in hard_edges.iter() {
        // TODO: Why is get_nearest not enough to find some indices?
        let i0 = vertex_map.get_nearest(v0.to_array());
        let i1 = vertex_map.get_nearest(v1.to_array());
        if let (Some(i0), Some(i1)) = (i0, i1) {
            hard_edge_indices.insert((i0, i1));
            hard_edge_indices.insert((i1, i0));
        }
    }

    edges
        .iter()
        .map(|[v0, v1]| hard_edge_indices.contains(&(*v0, *v1)))
        .collect()
}

fn append_geometry(
    geometry: &mut LDrawGeometry,
    hard_edges: &mut Vec<[Vec3; 2]>,
    vertex_map: &mut VertexMap,
    source_file: &weldr::SourceFile,
    source_map: &weldr::SourceMap,
    ctx: GeometryContext,
    recursive: bool,
) {
    // BFC Extension: https://www.ldraw.org/article/415.html
    // The default winding can be assumed to be CCW.
    // Winding can be changed within a file.
    // Winding only impacts the current file commands.
    let mut current_winding = Winding::Ccw;

    let mut current_inverted = ctx.inverted;
    // Invert if the current transform is "inverted".
    if ctx.transform.determinant() < 0.0 {
        current_inverted = !current_inverted;
    }

    let mut invert_next = false;

    for cmd in &source_file.cmds {
        match cmd {
            Command::Comment(c) => {
                // TODO: Add proper parsing to weldr.
                for word in c.text.split_whitespace() {
                    match word {
                        "CCW" => current_winding = Winding::Ccw,
                        "CW" => current_winding = Winding::Cw,
                        "INVERTNEXT" => invert_next = true,
                        _ => (),
                    }
                }
            }
            Command::Triangle(t) => {
                add_face(
                    geometry,
                    ctx.transform,
                    t.vertices,
                    invert_winding(current_winding, current_inverted),
                    vertex_map,
                );

                let face_color = FaceColor {
                    color: replace_color(t.color, ctx.current_color),
                    is_grainy_slope: false,
                };
                geometry.face_colors.push(face_color);
            }
            Command::Quad(q) => {
                add_face(
                    geometry,
                    ctx.transform,
                    q.vertices,
                    invert_winding(current_winding, current_inverted),
                    vertex_map,
                );

                let face_color = FaceColor {
                    color: replace_color(q.color, ctx.current_color),
                    is_grainy_slope: false,
                };
                geometry.face_colors.push(face_color);
            }
            Command::Line(line_cmd) => {
                let edge = line_cmd.vertices.map(|v| ctx.transform.transform_point3(v));
                hard_edges.push(edge);
            }
            Command::SubFileRef(subfile_cmd) => {
                if recursive {
                    if let Some(subfile) = source_map.get(&subfile_cmd.file) {
                        // The determinant is checked in each file.
                        // It should not be included in the child's context.
                        let child_ctx = GeometryContext {
                            current_color: replace_color(subfile_cmd.color, ctx.current_color),
                            transform: ctx.transform * subfile_cmd.matrix(),
                            inverted: if invert_next {
                                !ctx.inverted
                            } else {
                                ctx.inverted
                            },
                        };

                        // Don't invert additional subfile reference commands.
                        invert_next = false;

                        append_geometry(
                            geometry, hard_edges, vertex_map, subfile, source_map, child_ctx,
                            recursive,
                        );
                    }
                }
            }
            _ => {}
        }
    }
}

fn invert_winding(winding: Winding, invert: bool) -> Winding {
    match (winding, invert) {
        (Winding::Ccw, false) => Winding::Ccw,
        (Winding::Cw, false) => Winding::Cw,
        (Winding::Ccw, true) => Winding::Cw,
        (Winding::Cw, true) => Winding::Ccw,
    }
}

fn add_face<const N: usize>(
    geometry: &mut LDrawGeometry,
    transform: Mat4,
    vertices: [weldr::Vec3; N],
    winding: Winding,
    vertex_map: &mut VertexMap,
) {
    let starting_index = geometry.vertex_indices.len() as u32;
    let mut indices = vertices.map(|v| insert_vertex(geometry, transform, v, vertex_map));

    // TODO: Is it ok to just reverse indices even though this isn't the convention?
    if winding == Winding::Cw {
        indices.reverse();
    }

    geometry.vertex_indices.extend_from_slice(&indices);
    for i in 0..indices.len() {
        // A face (0,1,2) will have edges (0,1), (1,2), (2,0).
        geometry.edges.push([indices[i], indices[(i + 1) % N]]);
    }
    geometry.face_start_indices.push(starting_index);
    geometry.face_sizes.push(N as u32);
}

fn insert_vertex(
    geometry: &mut LDrawGeometry,
    transform: Mat4,
    vertex: Vec3,
    vertex_map: &mut VertexMap,
) -> u32 {
    let new_vertex = transform.transform_point3(vertex);
    let new_index = geometry.vertices.len() as u32;
    if let Some(index) = vertex_map.insert(new_index, new_vertex.to_array()) {
        index
    } else {
        geometry.vertices.push(new_vertex);
        new_index
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;

    use indoc::indoc;

    struct DummyResolver {
        files: HashMap<&'static str, Vec<u8>>,
    }

    impl DummyResolver {
        fn new() -> Self {
            Self {
                files: HashMap::new(),
            }
        }
    }

    impl weldr::FileRefResolver for DummyResolver {
        fn resolve<P: AsRef<std::path::Path>>(
            &self,
            filename: P,
        ) -> Result<Vec<u8>, weldr::ResolveError> {
            let filename = filename.as_ref().to_str().unwrap();
            self.files
                .get(filename)
                .cloned()
                .ok_or(weldr::ResolveError {
                    filename: filename.to_owned(),
                    resolve_error: None,
                })
        }
    }

    #[test]
    fn create_geometry_mpd() {
        let mut source_map = weldr::SourceMap::new();

        // Create a basic MPD file to test transforms and color resolution.
        // TODO: Test recursive and non recursive parsing.
        let document = indoc! {"
            0 FILE main.ldr
            1 16 0 0 0 1 0 0 0 1 0 0 0 1 a.ldr
            1 1 0 0 0 1 0 0 0 1 0 0 0 1 b.ldr
            1 16 0 0 0 1 0 0 0 1 0 0 0 1 c.ldr
            3 16 1 0 0 0 1 0 0 0 1
            4 8 -1 -1 0 -1 1 0 -1 1 0 1 1 0
            
            0 FILE a.ldr
            3 16 1 0 0 0 1 0 0 0 1
            4 2 -1 -1 0 -1 1 0 -1 1 0 1 1 0
            
            0 FILE b.ldr
            3 3 1 0 0 0 1 0 0 0 1
            3 16 1 0 0 0 1 0 0 0 1
            
            0 FILE c.ldr
            3 4 1 0 0 0 1 0 0 0 1
            4 5 -1 -1 0 -1 1 0 -1 1 0 1 1 0
        "};

        let mut resolver = DummyResolver::new();
        resolver.files.insert("root", document.as_bytes().to_vec());

        let main_model_name = weldr::parse("root", &resolver, &mut source_map).unwrap();
        let source_file = source_map.get(&main_model_name).unwrap();

        let geometry = create_geometry(&source_file, &source_map, 7, true);

        // TODO: Also test vertex positions and transforms.
        assert_eq!(6, geometry.vertices.len());
        assert_eq!(3 + 4 + 3 + 3 + 3 + 4 + 3 + 4, geometry.vertex_indices.len());
        assert_eq!(vec![3, 4, 3, 3, 3, 4, 3, 4], geometry.face_sizes);
        assert_eq!(
            vec![0, 3, 7, 10, 13, 16, 20, 23],
            geometry.face_start_indices
        );
        assert_eq!(
            vec![
                FaceColor {
                    color: 7,
                    is_grainy_slope: false
                },
                FaceColor {
                    color: 2,
                    is_grainy_slope: false
                },
                FaceColor {
                    color: 3,
                    is_grainy_slope: false
                },
                FaceColor {
                    color: 1,
                    is_grainy_slope: false
                },
                FaceColor {
                    color: 4,
                    is_grainy_slope: false
                },
                FaceColor {
                    color: 5,
                    is_grainy_slope: false
                },
                FaceColor {
                    color: 7,
                    is_grainy_slope: false
                },
                FaceColor {
                    color: 8,
                    is_grainy_slope: false
                },
            ],
            geometry.face_colors
        );
    }

    #[test]
    fn create_geometry_ccw() {
        let mut source_map = weldr::SourceMap::new();

        let document = indoc! {"
            0 BFC CERTIFY CCW
            3 16 1 0 0 0 1 0 0 0 1
            3 16 1 0 0 0 1 0 0 0 1
        "};

        let mut resolver = DummyResolver::new();
        resolver.files.insert("root", document.as_bytes().to_vec());

        let main_model_name = weldr::parse("root", &resolver, &mut source_map).unwrap();
        let source_file = source_map.get(&main_model_name).unwrap();

        let geometry = create_geometry(&source_file, &source_map, 16, true);

        assert_eq!(vec![0, 1, 2, 0, 1, 2], geometry.vertex_indices);
        assert_eq!(vec![3, 3], geometry.face_sizes);
    }

    #[test]
    fn create_geometry_cw() {
        let mut source_map = weldr::SourceMap::new();

        let document = indoc! {"
            0 BFC CERTIFY CW
            3 16 1 0 0 0 1 0 0 0 1
            3 16 1 0 0 0 1 0 0 0 1
        "};

        let mut resolver = DummyResolver::new();
        resolver.files.insert("root", document.as_bytes().to_vec());

        let main_model_name = weldr::parse("root", &resolver, &mut source_map).unwrap();
        let source_file = source_map.get(&main_model_name).unwrap();

        let geometry = create_geometry(&source_file, &source_map, 16, true);

        assert_eq!(vec![2, 1, 0, 2, 1, 0], geometry.vertex_indices);
        assert_eq!(vec![3, 3], geometry.face_sizes);
    }

    #[test]
    fn create_geometry_invert_next_determinant() {
        let mut source_map = weldr::SourceMap::new();

        // Check handling of the accumulated matrix determinant.
        // The INVERTNEXT command should flip the entire subfile reference.
        let document = indoc! {"
            0 FILE main.ldr
            0 BFC CCW
            0 BFC INVERTNEXT
            1 16 0 0 0 -1 0 0 0 -1 0 0 0 -1 a.ldr
            1 16 0 0 0 -1 0 0 0 -1 0 0 0 -1 a.ldr

            0 FILE a.ldr
            3 16 1 0 0 0 1 0 0 0 1
            1 16 0 0 0 -1 0 0 0 -1 0 0 0 -1 b.ldr

            0 FILE b.ldr
            3 16 1 0 0 0 1 0 0 0 1
        "};

        let mut resolver = DummyResolver::new();
        resolver.files.insert("root", document.as_bytes().to_vec());

        let main_model_name = weldr::parse("root", &resolver, &mut source_map).unwrap();
        let source_file = source_map.get(&main_model_name).unwrap();

        let geometry = create_geometry(&source_file, &source_map, 16, true);

        assert_eq!(
            vec![0, 1, 2, 5, 4, 3, 2, 1, 0, 3, 4, 5],
            geometry.vertex_indices
        );
        assert_eq!(vec![3, 3, 3, 3], geometry.face_sizes);
    }

    // TODO: Add tests for BFC certified superfiles.
}
