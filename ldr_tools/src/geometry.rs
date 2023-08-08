use glam::{Mat4, Vec3};
use rstar::{primitives::GeomWithData, RTree};
use weldr::Command;

use crate::{
    edge_split::split_edges, replace_color, slope::is_slope_piece, ColorCode, GeometrySettings,
    StudType, SCENE_SCALE,
};

// TODO: Document the data layout for these fields.
#[derive(Debug, PartialEq)]
pub struct LDrawGeometry {
    pub vertices: Vec<Vec3>,
    pub vertex_indices: Vec<u32>,
    pub face_start_indices: Vec<u32>,
    pub face_sizes: Vec<u32>,
    /// The colors of each face or a single element if all faces share a color.
    pub face_colors: Vec<ColorCode>,
    pub is_face_stud: Vec<bool>,
    /// Indices for the end points of line type 2 edges.
    pub edge_line_indices: Vec<[u32; 2]>,
    /// `true` if the geometry is part of a slope piece with grainy faces.
    /// Some applications may want to apply a separate texture to faces
    /// based on an angle threshold.
    pub has_grainy_slopes: bool,
}

/// Settings that inherit or accumulate when recursing into subfiles.
struct GeometryContext {
    current_color: ColorCode,
    transform: Mat4,
    inverted: bool,
    is_stud: bool,
    is_slope: bool,
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
    name: &str,
    current_color: ColorCode,
    recursive: bool,
    settings: &GeometrySettings,
) -> LDrawGeometry {
    let mut geometry = LDrawGeometry {
        vertices: Vec::new(),
        vertex_indices: Vec::new(),
        face_start_indices: Vec::new(),
        face_sizes: Vec::new(),
        face_colors: Vec::new(),
        is_face_stud: Vec::new(),
        edge_line_indices: Vec::new(),
        has_grainy_slopes: is_slope_piece(name),
    };

    // Start with inverted set to false since parts should never be inverted.
    // TODO: Is this also correct for geometry within an MPD file?
    let ctx = GeometryContext {
        current_color,
        transform: Mat4::IDENTITY,
        inverted: false,
        is_stud: is_stud(name),
        is_slope: is_slope_piece(name),
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
        settings,
    );

    geometry.edge_line_indices = edge_indices(&hard_edges, &vertex_map);

    // TODO: make this optional.
    // TODO: Should this be disabled when not welding vertices?
    if !geometry.edge_line_indices.is_empty() {
        let (split_positions, split_indices) = split_edges(
            &geometry.vertices,
            &geometry.vertex_indices,
            &geometry.face_start_indices,
            &geometry.face_sizes,
            &geometry.edge_line_indices,
        );
        geometry.vertices = split_positions;
        geometry.vertex_indices = split_indices;
        // TODO: Are the previous edge indices still valid at this point?
        geometry.edge_line_indices = edge_indices(&hard_edges, &vertex_map);
    }

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

    let scale = if settings.add_gap_between_parts {
        gaps_scale(dimensions) * SCENE_SCALE
    } else {
        Vec3::splat(SCENE_SCALE)
    };

    // Apply the scale last to use LDUs as the unit for vertex welding.
    // This avoids small floating point comparisons for small scene scales.
    for vertex in &mut geometry.vertices {
        *vertex *= scale;
    }

    geometry
}

fn is_stud(name: &str) -> bool {
    // TODO: find a more accurate way to check this.
    name.contains("stu")
}

fn gaps_scale(dimensions: Vec3) -> Vec3 {
    // TODO: Avoid applying this on chains, ropes, etc?
    // TODO: Weld ropes into a single piece?
    // Convert a distance between parts to a scale factor.
    // This gap is in LDUs since we haven't scaled the part yet.
    let gap_distance = 0.1;
    if dimensions.length_squared() > 0.0 {
        ((2.0 * gap_distance - dimensions) / dimensions).abs()
    } else {
        Vec3::ONE
    }
}

fn edge_indices(edges: &[[Vec3; 2]], vertex_map: &VertexMap) -> Vec<[u32; 2]> {
    // Find the edges marked as edges in the LDraw geometry.
    // These edges can be split by consuming applications later.
    let mut edge_indices = Vec::new();
    for [v0, v1] in edges.iter() {
        // TODO: Why is get_nearest not enough to find some indices?
        let i0 = vertex_map.get_nearest(v0.to_array());
        let i1 = vertex_map.get_nearest(v1.to_array());
        if let (Some(i0), Some(i1)) = (i0, i1) {
            edge_indices.push([i0, i1]);
        }
    }

    edge_indices
}

// TODO: simplify the parameters on these functions.
fn append_geometry(
    geometry: &mut LDrawGeometry,
    hard_edges: &mut Vec<[Vec3; 2]>,
    vertex_map: &mut VertexMap,
    source_file: &weldr::SourceFile,
    source_map: &weldr::SourceMap,
    ctx: GeometryContext,
    recursive: bool,
    settings: &GeometrySettings,
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
                let color = replace_color(t.color, ctx.current_color);
                add_triangle_face(
                    geometry,
                    &ctx,
                    t.vertices,
                    current_winding,
                    current_inverted,
                    vertex_map,
                    color,
                    settings.weld_vertices,
                );
            }
            Command::Quad(q) => {
                let color = replace_color(q.color, ctx.current_color);

                // TODO: Avoid repetition
                if settings.triangulate {
                    // TODO: How to properly triangulate a quad?
                    add_triangle_face(
                        geometry,
                        &ctx,
                        [q.vertices[0], q.vertices[1], q.vertices[2]],
                        current_winding,
                        current_inverted,
                        vertex_map,
                        color,
                        settings.weld_vertices,
                    );
                    add_triangle_face(
                        geometry,
                        &ctx,
                        [q.vertices[0], q.vertices[2], q.vertices[3]],
                        current_winding,
                        current_inverted,
                        vertex_map,
                        color,
                        settings.weld_vertices,
                    );
                } else {
                    add_face(
                        geometry,
                        ctx.transform,
                        q.vertices,
                        invert_winding(current_winding, current_inverted),
                        vertex_map,
                        settings.weld_vertices,
                    );

                    let face_color = replace_color(q.color, ctx.current_color);
                    geometry.face_colors.push(face_color);
                    geometry.is_face_stud.push(ctx.is_stud);
                }
            }
            Command::Line(line_cmd) => {
                let edge = line_cmd.vertices.map(|v| ctx.transform.transform_point3(v));
                hard_edges.push(edge);
            }
            Command::SubFileRef(subfile_cmd) => {
                if recursive {
                    let subfilename = replace_studs(subfile_cmd, settings.stud_type);

                    if let Some(subfile) = source_map.get(subfilename) {
                        // Subfiles of slopes or studs are still slopes or studs.
                        let is_stud = ctx.is_stud || is_stud(subfilename);
                        let is_slope = ctx.is_slope || is_slope_piece(subfilename);

                        // Set the walls of high contrast studs to black.
                        // TODO: Create custom stud files for better accuracy.
                        let current_color = if is_stud
                            && settings.stud_type == StudType::HighContrast
                            && subfilename.contains("cyli.dat")
                        {
                            0
                        } else {
                            replace_color(subfile_cmd.color, ctx.current_color)
                        };

                        // The determinant is checked in each file.
                        // It should not be included in the child's context.
                        let child_ctx = GeometryContext {
                            current_color,
                            transform: ctx.transform * subfile_cmd.matrix(),
                            inverted: if invert_next {
                                !ctx.inverted
                            } else {
                                ctx.inverted
                            },
                            is_stud,
                            is_slope,
                        };

                        // Don't invert additional subfile reference commands.
                        invert_next = false;

                        append_geometry(
                            geometry, hard_edges, vertex_map, subfile, source_map, child_ctx,
                            recursive, settings,
                        );
                    }
                }
            }
            _ => {}
        }
    }
}

fn replace_studs(subfile_cmd: &weldr::SubFileRefCmd, stud_type: StudType) -> &str {
    // https://wiki.ldraw.org/wiki/Studs_with_Logos
    match stud_type {
        StudType::Disabled => {
            if is_stud(&subfile_cmd.file) {
                // TODO: is there a better way to empty out files?
                ""
            } else {
                subfile_cmd.file.as_str()
            }
        }
        StudType::Normal => &subfile_cmd.file,
        StudType::Logo4 => match subfile_cmd.file.as_str() {
            "stud.dat" => "stud-logo4.dat",
            "stud2.dat" => "stud2-logo4.dat",
            "stud20.dat" => "stud20-logo4.dat",
            _ => subfile_cmd.file.as_str(),
        },
        StudType::HighContrast => &subfile_cmd.file,
    }
}

fn add_triangle_face(
    geometry: &mut LDrawGeometry,
    ctx: &GeometryContext,
    vertices: [weldr::Vec3; 3],
    current_winding: Winding,
    current_inverted: bool,
    vertex_map: &mut VertexMap,
    color: u32,
    weld_vertices: bool,
) {
    add_face(
        geometry,
        ctx.transform,
        vertices,
        invert_winding(current_winding, current_inverted),
        vertex_map,
        weld_vertices,
    );

    geometry.face_colors.push(color);
    geometry.is_face_stud.push(ctx.is_stud);
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
    weld_vertices: bool,
) {
    let starting_index = geometry.vertex_indices.len() as u32;
    let mut indices =
        vertices.map(|v| insert_vertex(geometry, transform, v, vertex_map, weld_vertices));

    // TODO: Is it ok to just reverse indices even though this isn't the convention?
    if winding == Winding::Cw {
        indices.reverse();
    }

    geometry.vertex_indices.extend_from_slice(&indices);
    geometry.face_start_indices.push(starting_index);
    geometry.face_sizes.push(N as u32);
}

fn insert_vertex(
    geometry: &mut LDrawGeometry,
    transform: Mat4,
    vertex: Vec3,
    vertex_map: &mut VertexMap,
    weld_vertices: bool,
) -> u32 {
    let new_vertex = transform.transform_point3(vertex);
    let new_index = geometry.vertices.len() as u32;

    if !weld_vertices {
        geometry.vertices.push(new_vertex);
        new_index
    } else if let Some(index) = vertex_map.insert(new_index, new_vertex.to_array()) {
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

        let geometry = create_geometry(
            &source_file,
            &source_map,
            "",
            7,
            true,
            &GeometrySettings {
                weld_vertices: true,
                ..Default::default()
            },
        );

        // TODO: Also test vertex positions and transforms.
        assert_eq!(6, geometry.vertices.len());
        assert_eq!(3 + 4 + 3 + 3 + 3 + 4 + 3 + 4, geometry.vertex_indices.len());
        assert_eq!(vec![3, 4, 3, 3, 3, 4, 3, 4], geometry.face_sizes);
        assert_eq!(
            vec![0, 3, 7, 10, 13, 16, 20, 23],
            geometry.face_start_indices
        );
        assert_eq!(vec![7, 2, 3, 1, 4, 5, 7, 8,], geometry.face_colors);
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

        let geometry = create_geometry(
            &source_file,
            &source_map,
            "",
            16,
            true,
            &GeometrySettings {
                weld_vertices: true,
                ..Default::default()
            },
        );

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

        let geometry = create_geometry(
            &source_file,
            &source_map,
            "",
            16,
            true,
            &GeometrySettings {
                weld_vertices: true,
                ..Default::default()
            },
        );

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

        let geometry = create_geometry(
            &source_file,
            &source_map,
            "",
            16,
            true,
            &GeometrySettings {
                weld_vertices: true,
                ..Default::default()
            },
        );

        assert_eq!(
            vec![0, 1, 2, 5, 4, 3, 2, 1, 0, 3, 4, 5],
            geometry.vertex_indices
        );
        assert_eq!(vec![3, 3, 3, 3], geometry.face_sizes);
    }

    // TODO: Test create geometry with and without welding and triangulate options

    // TODO: Add tests for BFC certified superfiles.
}
