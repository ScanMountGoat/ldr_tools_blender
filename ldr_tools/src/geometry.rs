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

    // TODO: Track BFC and inversion.

    append_geometry(
        &mut geometry,
        source_file,
        source_map,
        GeometryContext {
            current_color,
            transform: Mat4::IDENTITY,
        },
        recursive,
    );

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
    // Only apply the scale to the current transform.
    let scale = Mat4::from_scale(vec3(SCALE, SCALE, SCALE));
    let current_transform = scale * ctx.transform;

    for cmd in &source_file.cmds {
        match cmd {
            Command::Triangle(t) => {
                add_face(geometry, current_transform, &t.vertices);

                let color = replace_color(t.color, ctx.current_color);
                geometry.face_colors.push(color);
            }
            Command::Quad(q) => {
                add_face(geometry, current_transform, &q.vertices);

                let color = replace_color(q.color, ctx.current_color);
                geometry.face_colors.push(color);
            }
            Command::SubFileRef(subfile_cmd) => {
                if recursive {
                    if let Some(subfile) = source_map.get(&subfile_cmd.file) {
                        let child_ctx = GeometryContext {
                            current_color: replace_color(subfile_cmd.color, ctx.current_color),
                            transform: ctx.transform * subfile_cmd.matrix(),
                        };

                        append_geometry(geometry, subfile, source_map, child_ctx, recursive);
                    }
                }
            }
            _ => {}
        }
    }
}

fn add_face(geometry: &mut LDrawGeometry, transform: Mat4, vertices: &[weldr::Vec3]) {
    for v in vertices {
        let pos = transform.transform_point3(*v);
        geometry.vertices.push(pos);
    }
    let count = vertices.len() as u32;
    let starting_index = geometry.vertex_indices.len() as u32;
    geometry
        .vertex_indices
        .extend(starting_index..starting_index + count);
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
}
