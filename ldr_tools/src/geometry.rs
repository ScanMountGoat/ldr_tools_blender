use glam::{vec3, Mat4, Vec3};
use weldr::Command;

use crate::{replace_color, SCALE};

// TODO: use the edge information to calculate smooth normals?
pub struct LDrawGeometry {
    pub vertices: Vec<Vec3>,
    pub vertex_indices: Vec<u32>,
    pub face_start_indices: Vec<u32>,
    pub face_sizes: Vec<u32>,
    pub face_colors: Vec<u32>, // single element if all faces share a color
}

struct GeometryContext {
    current_color: u32,
    transform: Mat4,
    inverted: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Winding {
    CCW,
    CW,
}

pub fn create_geometry(
    source_file: &weldr::SourceFile,
    source_map: &weldr::SourceMap,
    current_color: u32,
    recursive: bool,
) -> LDrawGeometry {
    let mut geometry = LDrawGeometry {
        vertices: Vec::new(),
        vertex_indices: Vec::new(),
        face_start_indices: Vec::new(),
        face_sizes: Vec::new(),
        face_colors: Vec::new(),
    };

    // Start with inverted set to false since parts should never be inverted.
    // TODO: Is this also correct for geometry within an MPD file?
    let ctx = GeometryContext {
        current_color,
        transform: Mat4::IDENTITY,
        inverted: false,
    };

    append_geometry(&mut geometry, source_file, source_map, ctx, recursive);

    // Optimize the case where all face colors are the same.
    // This reduces overhead when processing data in Python.
    // A single color can be applied per object rather than per face.
    if let Some(color) = geometry.face_colors.first() {
        if geometry.face_colors.iter().all(|c| c == color) {
            geometry.face_colors = vec![*color];
        }
    }

    geometry
}

fn append_geometry(
    geometry: &mut LDrawGeometry,
    source_file: &weldr::SourceFile,
    source_map: &weldr::SourceMap,
    ctx: GeometryContext,
    recursive: bool,
) {
    // BFC Extension: https://www.ldraw.org/article/415.html
    // The default winding can be assumed to be CCW.
    // Winding can be changed within a file.
    // Winding only impacts the current file commands.
    let mut current_winding = Winding::CCW;

    let mut current_inverted = ctx.inverted;
    // Invert if the current transform is "inverted".
    if ctx.transform.determinant() < 0.0 {
        current_inverted = !current_inverted;
    }

    let mut invert_next = false;

    let scale = Mat4::from_scale(vec3(SCALE, SCALE, SCALE));
    let current_transform = scale * ctx.transform;

    for cmd in &source_file.cmds {
        match cmd {
            Command::Comment(c) => {
                // TODO: Add proper parsing to weldr.
                for word in c.text.split_whitespace() {
                    match word {
                        "CCW" => current_winding = Winding::CCW,
                        "CW" => current_winding = Winding::CW,
                        "INVERTNEXT" => invert_next = true,
                        _ => (),
                    }
                }
            }
            Command::Triangle(t) => {
                add_face(
                    geometry,
                    current_transform,
                    &t.vertices,
                    invert_winding(current_winding, current_inverted),
                );

                let color = replace_color(t.color, ctx.current_color);
                geometry.face_colors.push(color);
            }
            Command::Quad(q) => {
                add_face(
                    geometry,
                    current_transform,
                    &q.vertices,
                    invert_winding(current_winding, current_inverted),
                );

                let color = replace_color(q.color, ctx.current_color);
                geometry.face_colors.push(color);
            }
            Command::SubFileRef(subfile_cmd) => {
                if recursive {
                    if let Some(subfile) = source_map.get(&subfile_cmd.file) {
                        // The global scale and determinant are checked in each file.
                        // They should not be included in the child's context.
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

                        append_geometry(geometry, subfile, source_map, child_ctx, recursive);
                    }
                }
            }
            _ => {}
        }
    }
}

fn invert_winding(winding: Winding, invert: bool) -> Winding {
    match (winding, invert) {
        (Winding::CCW, false) => Winding::CCW,
        (Winding::CW, false) => Winding::CW,
        (Winding::CCW, true) => Winding::CW,
        (Winding::CW, true) => Winding::CCW,
    }
}

fn add_face(
    geometry: &mut LDrawGeometry,
    transform: Mat4,
    vertices: &[weldr::Vec3],
    winding: Winding,
) {
    for v in vertices {
        let pos = transform.transform_point3(*v);
        geometry.vertices.push(pos);
    }

    let count = vertices.len() as u32;
    let starting_index = geometry.vertex_indices.len() as u32;

    // TODO: Is it ok to just reverse indices even though this isn't the convention?
    let indices = starting_index..starting_index + count;
    match winding {
        Winding::CCW => geometry.vertex_indices.extend(indices),
        Winding::CW => geometry.vertex_indices.extend(indices.rev()),
    }

    geometry.face_start_indices.push(starting_index);
    geometry.face_sizes.push(count);
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
        fn resolve(&self, filename: &str) -> Result<Vec<u8>, weldr::ResolveError> {
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
        let vertex_count = 3 + 4 + 3 + 3 + 3 + 4 + 3 + 4;
        assert_eq!(vertex_count, geometry.vertices.len());
        assert_eq!(vertex_count, geometry.vertex_indices.len());
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

        let geometry = create_geometry(&source_file, &source_map, 16, true);

        assert_eq!(vec![0, 1, 2, 3, 4, 5], geometry.vertex_indices);
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

        assert_eq!(vec![2, 1, 0, 5, 4, 3], geometry.vertex_indices);
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
            vec![0, 1, 2, 5, 4, 3, 8, 7, 6, 9, 10, 11],
            geometry.vertex_indices
        );
        assert_eq!(vec![3, 3, 3, 3], geometry.face_sizes);
    }

    // TODO: Add tests for BFC certified superfiles.
}
