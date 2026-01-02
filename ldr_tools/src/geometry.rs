use std::collections::HashMap;

use crate::ldraw::{BfcCommand, Command, SubFileRef, Winding};
use glam::{Mat4, Vec2, Vec3};
use log::warn;

use crate::{
    ColorCode, GeometrySettings, StudType,
    edge_split::split_edges,
    pe_tex_info::{LDrawTextureInfo, PendingStudioTexture, project_texture},
    replace_color,
    slope::is_slope_piece,
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
    pub texture_info: Option<LDrawTextureInfo>,
}

impl LDrawGeometry {
    pub fn texture_info(&mut self) -> &mut LDrawTextureInfo {
        self.texture_info.get_or_insert_with(|| {
            LDrawTextureInfo::new(self.face_start_indices.len(), self.vertex_indices.len())
        })
    }
}

/// Settings that inherit or accumulate when recursing into subfiles.
struct GeometryContext {
    current_color: ColorCode,
    transform: Mat4,
    inverted: bool,
    is_stud: bool,
    is_slope: bool,
    studio_textures: Vec<PendingStudioTexture>,
    subfile_path: Vec<i32>,
    active_texture_index: Option<usize>,
}

impl GeometryContext {
    fn active_texture(&self) -> Option<&PendingStudioTexture> {
        self.studio_textures.get(self.active_texture_index?)
    }
}

#[derive(Debug, PartialEq, Eq, Hash)]
struct VertexKey([u32; 3]);

impl VertexKey {
    fn new(v: Vec3) -> Self {
        // LDraw geometry isn't indexed due to the recursive nature of the file format.
        // Models can have many vertices after generating all the studs.
        // Distance calculations slow down even tree data structures with O(log n) nearest neighbor queries.
        // Rounding the vertex coordinates for comparisons approximates a distance based query.
        // This hack works well in practice for the rounding errors introduced by subfile transforms.
        // Choose enough decimal places in LDraw units to not mess up vertices for patterned parts.
        Self(v.to_array().map(|f| {
            let x = round(f, 3);

            // Make sure that -0.0 is equal to 0.0.
            if x == 0.0 {
                0.0f32.to_bits()
            } else {
                x.to_bits()
            }
        }))
    }
}

fn round(x: f32, decimals: u32) -> f32 {
    let y = 10i32.pow(decimals) as f32;
    (x * y).round() / y
}
struct VertexMap {
    map: HashMap<VertexKey, u32>,
}

impl VertexMap {
    fn new() -> Self {
        Self {
            map: HashMap::new(),
        }
    }

    fn get(&self, v: Vec3) -> Option<u32> {
        self.map.get(&VertexKey::new(v)).copied()
    }

    fn insert(&mut self, i: u32, v: Vec3) -> Option<u32> {
        let key = VertexKey::new(v);
        match self.map.get(&key) {
            Some(index) => Some(*index),
            None => {
                // This vertex isn't in the map yet, so add it.
                self.map.insert(key, i);
                None
            }
        }
    }
}

#[tracing::instrument]
pub fn create_geometry(
    source_file: &crate::ldraw::SourceFile,
    source_map: &crate::ldraw::SourceMap,
    name: &SubFileRef,
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
        has_grainy_slopes: is_slope_piece(&name.name),
        texture_info: None,
    };

    // Start with inverted set to false since parts should never be inverted.
    // TODO: Is this also correct for geometry within an MPD file?
    let ctx = GeometryContext {
        current_color,
        transform: Mat4::IDENTITY,
        inverted: false,
        is_stud: is_stud(&name.name),
        is_slope: is_slope_piece(&name.name),
        studio_textures: Vec::new(),
        subfile_path: Vec::new(),
        active_texture_index: None,
    };

    let mut vertex_map = VertexMap::new();
    let mut hard_edges = Vec::new();

    // TODO: Cache geometry creation for studs?
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

    // Splitting only works if vertices are fully welded.
    if settings.weld_vertices {
        let (split_positions, split_indices) = split_edges(
            &geometry.vertices,
            &geometry.vertex_indices,
            &geometry.face_start_indices,
            &geometry.face_sizes,
            &geometry.edge_line_indices,
        );
        geometry.vertices = split_positions;
        geometry.vertex_indices = split_indices;

        // The edge indices are no longer valid since splitting can change vertices.
        vertex_map = VertexMap::new();
        for (i, v) in geometry.vertices.iter().enumerate() {
            vertex_map.insert(i as u32, *v);
        }
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
        gaps_scale(dimensions) * settings.scene_scale
    } else {
        Vec3::splat(settings.scene_scale)
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
        let i0 = vertex_map.get(*v0);
        let i1 = vertex_map.get(*v1);
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
    source_file: &crate::ldraw::SourceFile,
    source_map: &crate::ldraw::SourceMap,
    mut ctx: GeometryContext,
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

    let mut subfile_index = 0;

    // Track the subfile paths for the next PE_TEX_INFO command.
    let mut current_tex_path = Vec::new();

    for cmd in &source_file.cmds {
        match cmd {
            Command::PeTexPath(pe_tex_path) => {
                // Tex paths are relative to the current file.
                // The single element [-1] refers to the current file.
                current_tex_path = ctx.subfile_path.clone();
                if pe_tex_path.paths != [-1] {
                    current_tex_path.extend_from_slice(&pe_tex_path.paths);
                }
            }
            Command::PeTexInfo(pe_tex_info) => {
                if let Some(tex_info) =
                    PendingStudioTexture::from_cmd(pe_tex_info, &current_tex_path, geometry)
                {
                    ctx.studio_textures.push(tex_info);

                    // Check what texture will be assigned starting with this subfile.
                    ctx.active_texture_index = find_active_texture_index(&ctx, &current_tex_path);
                }
            }
            Command::Bfc(bfc_cmd) => {
                // Ignore clip and certify since we only need to set winding.
                match bfc_cmd {
                    BfcCommand::NoCertify => (),
                    BfcCommand::Certify(winding) => {
                        current_winding = winding.unwrap_or(Winding::Ccw);
                    }
                    BfcCommand::Winding(winding) => {
                        current_winding = *winding;
                    }
                    BfcCommand::NoClip => (),
                    BfcCommand::Clip(winding) => {
                        if let Some(winding) = winding {
                            current_winding = *winding;
                        }
                    }
                    BfcCommand::InvertNext => invert_next = true,
                }
            }
            Command::Triangle(t) => {
                let color = replace_color(t.color, ctx.current_color);
                add_triangle_face(
                    geometry,
                    &ctx,
                    t.vertices,
                    t.uvs,
                    invert_winding(current_winding, current_inverted),
                    vertex_map,
                    color,
                    settings.weld_vertices,
                    ctx.active_texture(),
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
                        q.uvs.map(|[a, b, c, _d]| [a, b, c]),
                        invert_winding(current_winding, current_inverted),
                        vertex_map,
                        color,
                        settings.weld_vertices,
                        ctx.active_texture(),
                    );
                    add_triangle_face(
                        geometry,
                        &ctx,
                        [q.vertices[0], q.vertices[2], q.vertices[3]],
                        q.uvs.map(|[a, _b, c, d]| [a, c, d]),
                        invert_winding(current_winding, current_inverted),
                        vertex_map,
                        color,
                        settings.weld_vertices,
                        ctx.active_texture(),
                    );
                } else {
                    add_face(
                        geometry,
                        ctx.transform,
                        q.vertices,
                        q.uvs,
                        invert_winding(current_winding, current_inverted),
                        vertex_map,
                        settings.weld_vertices,
                        ctx.active_texture(),
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
                if !recursive {
                    continue;
                }
                let subfilename = replace_studs(subfile_cmd, settings.stud_type);
                let Some((subfilename, subfile)) = source_map.get(subfilename) else {
                    continue;
                };
                let subfilename = &subfilename.normalized_name;

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

                let mut subfile_path = ctx.subfile_path.clone();
                subfile_path.push(subfile_index);

                let active_texture_index = find_active_texture_index(&ctx, &subfile_path);

                // The determinant is checked in each file.
                // It should not be included in the child's context.
                let child_ctx = GeometryContext {
                    current_color,
                    transform: ctx.transform * subfile_cmd.transform.to_matrix(),
                    inverted: if invert_next {
                        !ctx.inverted
                    } else {
                        ctx.inverted
                    },
                    is_stud,
                    is_slope,
                    studio_textures: ctx.studio_textures.clone(),
                    subfile_path,
                    active_texture_index,
                };

                // Don't invert additional subfile reference commands.
                invert_next = false;

                // TODO: Cache the processed geometry for studs?
                // TODO: Will studs ever need to be welded to other geometry?
                append_geometry(
                    geometry, hard_edges, vertex_map, subfile, source_map, child_ctx, recursive,
                    settings,
                );

                subfile_index += 1;
            }
            _ => {}
        }
    }
}

fn find_active_texture_index(ctx: &GeometryContext, subfile_path: &[i32]) -> Option<usize> {
    // Check what texture will be assigned starting with this subfile.
    let mut matching_textures: Vec<_> = ctx
        .studio_textures
        .iter()
        .enumerate()
        .filter_map(|(i, t)| {
            if subfile_path.starts_with(&t.path) {
                Some((i, &t.path))
            } else {
                None
            }
        })
        .collect();

    // Find the most specific texture path that this path matches.
    // For example, PE_TEX_PATH 0 0 0 2 should take precedence over PE_TEX_PATH 0.
    matching_textures.sort_by_key(|(_, path)| path.len());
    matching_textures.last().map(|(i, _)| *i)
}

fn replace_studs(subfile_cmd: &crate::ldraw::SubFileRefCmd, stud_type: StudType) -> &str {
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
    vertices: [Vec3; 3],
    uvs: Option<[Vec2; 3]>,
    winding: Winding,
    vertex_map: &mut VertexMap,
    color: u32,
    weld_vertices: bool,
    texture: Option<&PendingStudioTexture>,
) {
    add_face(
        geometry,
        ctx.transform,
        vertices,
        uvs,
        winding,
        vertex_map,
        weld_vertices,
        texture,
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
    vertices: [Vec3; N],
    uvs: Option<[Vec2; N]>,
    winding: Winding,
    vertex_map: &mut VertexMap,
    weld_vertices: bool,
    texture: Option<&PendingStudioTexture>,
) {
    let mut vertices = vertices;
    if winding == Winding::Cw {
        vertices.reverse();
    }

    let texmap = texture.and_then(|t| project_texture(t, transform, vertices, uvs));

    let starting_index = geometry.vertex_indices.len() as u32;
    let indices =
        vertices.map(|v| insert_vertex(geometry, transform, v, vertex_map, weld_vertices));

    geometry.vertex_indices.extend_from_slice(&indices);
    geometry.face_start_indices.push(starting_index);
    geometry.face_sizes.push(N as u32);

    if let Some(texmap) = texmap {
        // Lazily initialize the texture info, because we have actual data to insert.
        let texture_info = geometry.texture_info();
        texture_info.indices.push(texmap.texture_index);
        texture_info.uvs.extend(texmap.uvs);
    } else {
        // Avoid initializing the texture info,
        // as we only need to add placeholder data if the buffers are already there.
        if let Some(texture_info) = &mut geometry.texture_info {
            texture_info.indices.push(u8::MAX); // Sentinel value indicating no texture for this face.
            texture_info.uvs.extend([Vec2::ZERO; N]); // "Padding" so that all vertices get a UV.
        }
    }
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
    } else if let Some(index) = vertex_map.insert(new_index, new_vertex) {
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

    use glam::vec3;
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

    impl crate::ldraw::FileRefResolver for DummyResolver {
        fn resolve<P: AsRef<std::path::Path>>(&self, filename: P) -> Option<Vec<u8>> {
            let filename = filename.as_ref().to_str().unwrap();
            self.files.get(filename).cloned()
        }
    }

    #[test]
    fn create_geometry_mpd() {
        let mut source_map = crate::ldraw::SourceMap::new();

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

        let main_model_name = crate::ldraw::parse("root", &resolver, &mut source_map);
        let (_name, source_file) = source_map.get(&main_model_name).unwrap();

        let geometry = create_geometry(
            source_file,
            &source_map,
            &SubFileRef::new(""),
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
        let mut source_map = crate::ldraw::SourceMap::new();

        let document = indoc! {"
            0 BFC CERTIFY CCW
            3 16 1 0 0 0 1 0 0 0 1
            3 16 1 0 0 0 1 0 0 0 1
        "};

        let mut resolver = DummyResolver::new();
        resolver.files.insert("root", document.as_bytes().to_vec());

        let main_model_name = crate::ldraw::parse("root", &resolver, &mut source_map);
        let (_name, source_file) = source_map.get(&main_model_name).unwrap();

        let geometry = create_geometry(
            source_file,
            &source_map,
            &SubFileRef::new(""),
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
        let mut source_map = crate::ldraw::SourceMap::new();

        let document = indoc! {"
            0 BFC CERTIFY CW
            3 16 1 0 0 0 1 0 0 0 1
            3 16 1 0 0 0 1 0 0 0 1
        "};

        let mut resolver = DummyResolver::new();
        resolver.files.insert("root", document.as_bytes().to_vec());

        let main_model_name = crate::ldraw::parse("root", &resolver, &mut source_map);
        let (_name, source_file) = source_map.get(&main_model_name).unwrap();

        let geometry = create_geometry(
            source_file,
            &source_map,
            &SubFileRef::new(""),
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
    fn create_geometry_invert_next_determinant() {
        let mut source_map = crate::ldraw::SourceMap::new();

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

        let main_model_name = crate::ldraw::parse("root", &resolver, &mut source_map);
        let (_name, source_file) = source_map.get(&main_model_name).unwrap();

        let geometry = create_geometry(
            source_file,
            &source_map,
            &SubFileRef::new(""),
            16,
            true,
            &GeometrySettings {
                weld_vertices: true,
                ..Default::default()
            },
        );

        // Some vertices are repeated from the face normal threshold for edge splitting.
        assert_eq!(
            vec![
                vec3(-1.0, 0.0, 0.0,),
                vec3(0.0, -1.0, 0.0,),
                vec3(0.0, 0.0, -1.0,),
                vec3(0.0, 0.0, 1.0,),
                vec3(0.0, 1.0, 0.0,),
                vec3(1.0, 0.0, 0.0,),
                vec3(0.0, -1.0, 0.0,),
                vec3(0.0, 1.0, 0.0,),
            ],
            geometry.vertices
        );
        assert_eq!(
            vec![0, 1, 2, 3, 4, 5, 2, 6, 0, 5, 7, 3],
            geometry.vertex_indices
        );
        assert_eq!(vec![3, 3, 3, 3], geometry.face_sizes);
    }

    // TODO: Test create geometry with and without welding and triangulate options

    // TODO: Add tests for BFC certified superfiles.

    #[test]
    fn round_vertices() {
        assert_eq!(
            VertexKey::new(vec3(1.0, 2.0, 3.0)),
            VertexKey::new(vec3(1.0, 2.0, 3.0))
        );
        assert_eq!(
            VertexKey::new(vec3(-0.0001, 2.0024, 2.9999)),
            VertexKey::new(vec3(0.0001, 2.0021, 3.0))
        );
        assert!(
            VertexKey::new(vec3(0.002, 2.003, 2.999)) != VertexKey::new(vec3(0.001, 2.0021, 3.01))
        );
    }
}
