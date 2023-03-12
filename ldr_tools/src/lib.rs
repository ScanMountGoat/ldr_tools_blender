use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};

use geometry::create_geometry;
use glam::{vec4, Mat4};
use path_slash::PathBufExt;
use rayon::prelude::*;
use weldr::{Command, FileRefResolver, ResolveError};

pub use color::{load_color_table, LDrawColor};
pub use geometry::LDrawGeometry;
pub use weldr::Color;

pub type ColorCode = u32;

// Special color code that "inherits" the existing color.
const CURRENT_COLOR: ColorCode = 16;

mod color;
mod geometry;

const SCENE_SCALE: f32 = 0.01;

pub struct LDrawNode {
    pub name: String,
    pub transform: Mat4,
    pub geometry_name: Option<String>, // TODO: Better way to share geometry?
    /// The current color set for this node.
    /// Overrides colors in the geometry if present.
    pub current_color: ColorCode,
    pub children: Vec<LDrawNode>,
}

struct DiskResolver {
    base_paths: Vec<PathBuf>,
}

impl DiskResolver {
    fn new_from_catalog<P: AsRef<Path>>(catalog_path: P) -> Self {
        let catalog_path = std::fs::canonicalize(catalog_path).unwrap();
        let base_paths = vec![
            // TODO: Is it necessary to specify both p and hi-res primitives in p/48?
            catalog_path.join("p"),
            // catalog_path.join("p").join("48"),
            catalog_path.join("parts"),
            catalog_path.join("parts").join("s"),
        ];
        Self { base_paths }
    }
}

impl FileRefResolver for DiskResolver {
    fn resolve(&self, filename: &str) -> Result<Vec<u8>, ResolveError> {
        // TODO: Where to handle stud replacement.
        // TODO: Make this configurable?
        // https://wiki.ldraw.org/wiki/Studs_with_Logos
        let filename = match filename {
            "stud.dat" => "stud-logo4.dat",
            "stud2.dat" => "stud2-logo4.dat",
            _ => filename,
        };

        // TODO: This doesn't work with relative paths as the main ldr path?
        self.base_paths
            .iter()
            .find_map(|prefix| {
                // The file's path separator may not match the current OS.
                // Don't assume the current file system and try both separators.
                // TODO: Split by / or \ and use collect to normalize separators?
                // TODO: replace separators and use collect() to normalize
                let forward_path = prefix.join(PathBuf::from_slash(filename));
                let backward_path = prefix.join(PathBuf::from_backslash(filename));
                std::fs::read(prefix.join(forward_path))
                    .or_else(|_| std::fs::read(prefix.join(backward_path)))
                    .ok()
            })
            .ok_or(ResolveError::new(
                filename.to_string(),
                std::io::Error::from(std::io::ErrorKind::NotFound),
            ))
    }
}

pub struct LDrawScene {
    pub root_node: LDrawNode,
    pub geometry_cache: HashMap<String, LDrawGeometry>,
}

pub struct LDrawSceneInstanced {
    pub geometry_world_transforms: HashMap<(String, ColorCode), Vec<Mat4>>,
    pub geometry_cache: HashMap<String, LDrawGeometry>,
}

fn replace_color(color: ColorCode, current_color: ColorCode) -> ColorCode {
    if color == CURRENT_COLOR {
        current_color
    } else {
        color
    }
}

struct GeometryInitDescriptor<'a> {
    source_file: &'a weldr::SourceFile,
    current_color: ColorCode,
    recursive: bool,
}

// TODO: Add global scale parameters.
// Adjust the draw ctx for iter to set a "global scale"?
// Also add a per part gap scale matrix.
pub fn load_file(path: &str) -> LDrawScene {
    let resolver = DiskResolver::new_from_catalog(r"C:\Users\Public\Documents\LDraw");
    let mut source_map = weldr::SourceMap::new();

    let main_model_name = weldr::parse(path, &resolver, &mut source_map).unwrap();
    let source_file = source_map.get(&main_model_name).unwrap();

    // Collect the scene hierarchy and geometry descriptors.
    let mut geometry_descriptors = HashMap::new();
    let root_node = load_node(
        source_file,
        &Mat4::IDENTITY,
        &source_map,
        &mut geometry_descriptors,
        CURRENT_COLOR,
    );

    let geometry_cache = create_geometry_cache(geometry_descriptors, &source_map);

    LDrawScene {
        root_node,
        geometry_cache,
    }
}

fn load_node<'a>(
    source_file: &'a weldr::SourceFile,
    transform: &Mat4,
    source_map: &'a weldr::SourceMap,
    geometry_descriptors: &mut HashMap<String, GeometryInitDescriptor<'a>>,
    current_color: ColorCode,
) -> LDrawNode {
    let mut children = Vec::new();
    let mut geometry = None;

    if is_part(source_file) || has_geometry(source_file) {
        // Create geometry if the node is a part.
        // Use the special color code to reuse identical parts in different colors.
        geometry_descriptors
            .entry(source_file.filename.clone())
            .or_insert_with(|| GeometryInitDescriptor {
                source_file,
                current_color: CURRENT_COLOR,
                recursive: true,
            });

        geometry = Some(source_file.filename.clone());
    } else if has_geometry(source_file) {
        // Just add geometry for this node.
        // Use the current color at this node since this geometry might not be referenced elsewhere.
        geometry_descriptors
            .entry(source_file.filename.clone())
            .or_insert_with(|| GeometryInitDescriptor {
                source_file,
                current_color,
                recursive: false,
            });

        geometry = Some(source_file.filename.clone());
    } else {
        for cmd in &source_file.cmds {
            if let Command::SubFileRef(sfr_cmd) = cmd {
                if let Some(subfile) = source_map.get(&sfr_cmd.file) {
                    // Don't apply node transforms to preserve the scene hierarchy.
                    // Applications should handle combining the transforms.
                    let child_transform = sfr_cmd.matrix();

                    // Handle replacing colors.
                    let child_color = replace_color(sfr_cmd.color, current_color);

                    let child_node = load_node(
                        subfile,
                        &child_transform,
                        source_map,
                        geometry_descriptors,
                        child_color,
                    );
                    children.push(child_node);
                }
            }
        }
    }

    let transform = scaled_transform(transform);

    LDrawNode {
        name: source_file.filename.clone(),
        transform,
        geometry_name: geometry,
        current_color,
        children,
    }
}

fn create_geometry_cache(
    geometry_descriptors: HashMap<String, GeometryInitDescriptor>,
    source_map: &weldr::SourceMap,
) -> HashMap<String, LDrawGeometry> {
    // Create the actual geometry in parallel to improve performance.
    let geometry_cache = geometry_descriptors
        .into_par_iter()
        .map(|(name, descriptor)| {
            let GeometryInitDescriptor {
                source_file,
                current_color,
                recursive,
            } = descriptor;
            (
                name,
                create_geometry(source_file, &source_map, current_color, recursive),
            )
        })
        .collect();
    geometry_cache
}

fn scaled_transform(transform: &Mat4) -> Mat4 {
    // Only scale the translation so that the scale doesn't accumulate.
    // TODO: Is this the best way to handle scale?
    let mut transform = *transform;
    transform.w_axis *= vec4(SCENE_SCALE, SCENE_SCALE, SCENE_SCALE, 1.0);
    transform
}

// TODO: Create another function that can generate the mesh for duplifaces?

/// Find the world transforms for each geometry.
/// This allows applications to more easily use instancing.
pub fn load_file_instanced(path: &str) -> LDrawSceneInstanced {
    let resolver = DiskResolver::new_from_catalog(r"C:\Users\Public\Documents\LDraw");
    let mut source_map = weldr::SourceMap::new();

    let main_model_name = weldr::parse(path, &resolver, &mut source_map).unwrap();
    let source_file = source_map.get(&main_model_name).unwrap();

    // Find the world transforms for each geometry.
    // This allows applications to more easily use instancing.
    let mut geometry_descriptors = HashMap::new();
    let mut geometry_world_transforms = HashMap::new();
    load_node_instanced(
        source_file,
        &Mat4::IDENTITY,
        &source_map,
        &mut geometry_descriptors,
        &mut geometry_world_transforms,
        CURRENT_COLOR,
    );

    let geometry_cache = create_geometry_cache(geometry_descriptors, &source_map);

    LDrawSceneInstanced {
        geometry_world_transforms,
        geometry_cache,
    }
}

// TODO: Share code with the non instanced function?
fn load_node_instanced<'a>(
    source_file: &'a weldr::SourceFile,
    world_transform: &Mat4,
    source_map: &'a weldr::SourceMap,
    geometry_descriptors: &mut HashMap<String, GeometryInitDescriptor<'a>>,
    geometry_world_transforms: &mut HashMap<(String, ColorCode), Vec<Mat4>>,
    current_color: ColorCode,
) {
    // TODO: Find a way to avoid repetition.
    let is_part = is_part(source_file);
    if is_part {
        // Create geometry if the node is a part.
        // Use the special color code to reuse identical parts in different colors.
        geometry_descriptors
            .entry(source_file.filename.clone())
            .or_insert_with(|| GeometryInitDescriptor {
                source_file,
                current_color: CURRENT_COLOR,
                recursive: true,
            });

        // Add another instance of the current geometry.
        // Also key by the color in case a part appears in multiple colors.
        geometry_world_transforms
            .entry((source_file.filename.clone(), current_color))
            .or_insert(Vec::new())
            .push(scaled_transform(world_transform));
    } else if has_geometry(source_file) {
        // Just add geometry for this node.
        // Use the current color at this node since this geometry might not be referenced elsewhere.
        geometry_descriptors
            .entry(source_file.filename.clone())
            .or_insert_with(|| GeometryInitDescriptor {
                source_file,
                current_color,
                recursive: false,
            });

        // Add another instance of the current geometry.
        // Also key by the color in case a part appears in multiple colors.
        geometry_world_transforms
            .entry((source_file.filename.clone(), current_color))
            .or_insert(Vec::new())
            .push(scaled_transform(world_transform));
    }

    // Recursion is already handled for parts.
    if !is_part {
        for cmd in &source_file.cmds {
            if let Command::SubFileRef(sfr_cmd) = cmd {
                if let Some(subfile) = source_map.get(&sfr_cmd.file) {
                    // Accumulate transforms.
                    let child_transform = *world_transform * sfr_cmd.matrix();

                    // Handle replacing colors.
                    let child_color = replace_color(sfr_cmd.color, current_color);

                    load_node_instanced(
                        subfile,
                        &child_transform,
                        source_map,
                        geometry_descriptors,
                        geometry_world_transforms,
                        child_color,
                    );
                }
            }
        }
    }
}

fn is_part(source_file: &weldr::SourceFile) -> bool {
    // TODO: Check the part type rather than the extension.
    source_file.filename.ends_with(".dat")
}

fn has_geometry(source_file: &weldr::SourceFile) -> bool {
    // Some files have subfile ref commands but also define parts inline.
    // This includes tube segments on the Volkswagen Beetle.mpd
    source_file
        .cmds
        .iter()
        .any(|c| matches!(c, Command::Triangle(_) | Command::Quad(_)))
}
