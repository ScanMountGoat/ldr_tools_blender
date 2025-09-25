use std::{
    collections::HashMap,
    fs::File,
    io::{BufReader, Read},
    path::{Path, PathBuf},
};

use geometry::create_geometry;
use glam::{Mat4, Vec3, vec4};
use ldraw::{Command, FileRefResolver};
use log::error;
use rayon::prelude::*;
use zip::ZipArchive;

pub use color::{LDrawColor, load_color_table};
pub use geometry::LDrawGeometry;
pub use glam;
pub use ldraw::Color;
pub use pe_tex_info::LDrawTextureInfo;

pub type ColorCode = u32;

// Special color code that "inherits" the existing color.
const CURRENT_COLOR: ColorCode = 16;

mod color;
mod edge_split;
mod geometry;
pub mod ldraw;
mod normal;
mod pe_tex_info;
mod slope;

#[derive(Debug, PartialEq)]
pub struct LDrawNode {
    pub name: String,
    pub transform: Mat4,
    /// The name of the geometry in [geometry_cache](struct.LDrawScene.html#structfield.geometry_cache)
    /// or `None` for internal nodes.
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
    fn new_from_library<P: AsRef<Path>>(
        catalog_path: P,
        additional_paths: impl IntoIterator<Item = P>,
        resolution: PrimitiveResolution,
    ) -> Self {
        let catalog_path = catalog_path.as_ref().to_owned();
        let mut base_paths = vec![
            catalog_path.join("p"),
            catalog_path.join("parts"),
            catalog_path.join("parts").join("s"),
            // Studio unoffical part folders.
            catalog_path.join("UnOfficial").join("p"),
            catalog_path.join("UnOfficial").join("parts"),
            catalog_path.join("UnOfficial").join("parts").join("s"),
        ];
        // Insert at the front since earlier elements take priority.
        match resolution {
            PrimitiveResolution::Low => base_paths.insert(0, catalog_path.join("p").join("8")),
            PrimitiveResolution::Normal => (),
            PrimitiveResolution::High => base_paths.insert(0, catalog_path.join("p").join("48")),
        }

        // Users may want to specify additional folders for parts.
        for path in additional_paths {
            base_paths.push(path.as_ref().to_owned());
        }

        Self { base_paths }
    }
}

impl FileRefResolver for DiskResolver {
    fn resolve<P: AsRef<Path>>(&self, filename: P) -> Vec<u8> {
        let filename = filename.as_ref();

        // Find the first folder that contains the given file.
        let contents = self
            .base_paths
            .iter()
            .find_map(|prefix| std::fs::read(prefix.join(filename)).ok());

        match contents {
            Some(contents) => contents,
            None => {
                // TODO: Is there a better way to allow partial imports with resolve errors?
                error!("Unable to resolve {filename:?}");
                Vec::new()
            }
        }
    }
}

struct IoFileResolver {
    io_path: String,
    model_ldr: Vec<u8>,
    resolver: DiskResolver,
}

impl FileRefResolver for IoFileResolver {
    fn resolve<P: AsRef<Path>>(&self, filename: P) -> Vec<u8> {
        if filename.as_ref() == Path::new(&self.io_path) {
            self.model_ldr.clone()
        } else {
            self.resolver.resolve(filename)
        }
    }
}

impl IoFileResolver {
    fn new(io_path: String, resolver: DiskResolver) -> Result<Self, zip::result::ZipError> {
        let zip_file = File::open(&io_path)?;
        let mut archive = ZipArchive::new(BufReader::new(zip_file))?;
        let mut ldr_file = archive.by_name("model.ldr")?;

        let mut buffer = Vec::with_capacity(ldr_file.size() as usize);

        // skip a BOM, if present
        ldr_file.by_ref().take(3).read_to_end(&mut buffer)?;
        if buffer == "\u{FEFF}".as_bytes() {
            buffer.clear();
        }

        ldr_file.read_to_end(&mut buffer)?;

        // TODO: read custom parts from the file?

        Ok(Self {
            io_path,
            model_ldr: buffer,
            resolver,
        })
    }
}

#[derive(Debug, PartialEq)]
pub struct LDrawScene {
    pub root_node: LDrawNode,
    pub geometry_cache: HashMap<String, LDrawGeometry>,
}

#[derive(Debug, PartialEq)]
pub struct LDrawSceneInstanced {
    pub main_model_name: String,
    pub geometry_world_transforms: HashMap<(String, ColorCode), Vec<Mat4>>,
    pub geometry_cache: HashMap<String, LDrawGeometry>,
}

#[derive(Debug, PartialEq)]
pub struct LDrawSceneInstancedPoints {
    pub main_model_name: String,
    /// Decomposed instance transforms for unique part and color.
    pub geometry_point_instances: HashMap<(String, ColorCode), PointInstances>,
    pub geometry_cache: HashMap<String, LDrawGeometry>,
}

#[derive(Debug, PartialEq)]
pub struct PointInstances {
    pub translations: Vec<Vec3>,
    pub rotations_axis: Vec<Vec3>,
    /// The angle of the rotation in radians.
    pub rotations_angle: Vec<f32>,
    pub scales: Vec<Vec3>,
}

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum StudType {
    /// Removes all visible and internal studs.
    Disabled,
    /// The default stud model and quality.
    Normal,
    /// A higher quality modeled logo suitable for realistic rendering.
    Logo4,
    /// Studs with black sides similar to official LEGO instructions.
    HighContrast,
}

impl Default for StudType {
    fn default() -> Self {
        Self::Normal
    }
}

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum PrimitiveResolution {
    /// Primitives in the `p/8` folder.
    Low,
    /// The standard primitive resolution
    Normal,
    /// Primitives in the `p/48` folder.
    High,
}

impl Default for PrimitiveResolution {
    fn default() -> Self {
        Self::Normal
    }
}

// TODO: Come up with a better name.
#[derive(Debug)]
pub struct GeometrySettings {
    pub triangulate: bool,
    pub add_gap_between_parts: bool,
    pub stud_type: StudType,
    pub weld_vertices: bool, // TODO: default to true?
    pub primitive_resolution: PrimitiveResolution,
    pub scene_scale: f32,
}

impl Default for GeometrySettings {
    fn default() -> Self {
        Self {
            triangulate: Default::default(),
            add_gap_between_parts: Default::default(),
            stud_type: Default::default(),
            weld_vertices: Default::default(),
            primitive_resolution: Default::default(),
            scene_scale: 1.0,
        }
    }
}

fn replace_color(color: ColorCode, current_color: ColorCode) -> ColorCode {
    if color == CURRENT_COLOR {
        current_color
    } else {
        color
    }
}

#[derive(Debug)]
struct GeometryInitDescriptor<'a> {
    source_file: &'a ldraw::SourceFile,
    current_color: ColorCode,
    recursive: bool,
}

// TODO: Add tests for this using files from models?
#[tracing::instrument]
pub fn load_file(
    path: &str,
    ldraw_path: &str,
    additional_paths: &[String],
    settings: &GeometrySettings,
) -> LDrawScene {
    let (source_map, main_model_name) = parse_file(path, ldraw_path, additional_paths, settings);
    let source_file = source_map.get(&main_model_name).unwrap();

    // Collect the scene hierarchy and geometry descriptors.
    let mut geometry_descriptors = HashMap::new();
    let root_node = load_node(
        source_file,
        &main_model_name,
        &Mat4::IDENTITY,
        &source_map,
        &mut geometry_descriptors,
        CURRENT_COLOR,
        settings,
    );

    let geometry_cache = create_geometry_cache(geometry_descriptors, &source_map, settings);

    LDrawScene {
        root_node,
        geometry_cache,
    }
}

#[tracing::instrument]
fn parse_file(
    path: &str,
    ldraw_path: &str,
    additional_paths: &[String],
    settings: &GeometrySettings,
) -> (ldraw::SourceMap, String) {
    let mut resolver = DiskResolver::new_from_library(
        ldraw_path,
        additional_paths.iter().map(|s| s.as_str()),
        settings.primitive_resolution,
    );
    // Resolve paths relative to the current file.
    if let Some(parent) = Path::new(path).parent() {
        resolver.base_paths.insert(0, parent.to_owned());
    }

    let mut source_map = ldraw::SourceMap::new();
    ensure_studs(settings, &resolver, &mut source_map);

    let is_io = Path::new(path).extension() == Some("io".as_ref());

    let main_model_name = if is_io {
        // TODO: Avoid unwrap?
        let io_resolver = IoFileResolver::new(path.to_owned(), resolver).unwrap();
        ldraw::parse(path, &io_resolver, &mut source_map)
    } else {
        ldraw::parse(path, &resolver, &mut source_map)
    };

    (source_map, main_model_name)
}

fn ensure_studs(
    settings: &GeometrySettings,
    resolver: &DiskResolver,
    source_map: &mut ldraw::SourceMap,
) {
    // The replaced studs likely won't be referenced by existing files.
    // Make sure the selected stud type is in the source map.
    if settings.stud_type == StudType::Logo4 {
        ldraw::parse("stud-logo4.dat", resolver, source_map);
        ldraw::parse("stud2-logo4.dat", resolver, source_map);
    }
}

fn load_node<'a>(
    source_file: &'a ldraw::SourceFile,
    filename: &str,
    transform: &Mat4,
    source_map: &'a ldraw::SourceMap,
    geometry_descriptors: &mut HashMap<String, GeometryInitDescriptor<'a>>,
    current_color: ColorCode,
    settings: &GeometrySettings,
) -> LDrawNode {
    let mut children = Vec::new();
    let mut geometry_name = None;

    if is_part(source_file, filename) || has_geometry(source_file) {
        // Create geometry if the node is a part.
        // Use the special color code to reuse identical parts in different colors.
        geometry_descriptors
            .entry(filename.to_lowercase())
            .or_insert_with(|| GeometryInitDescriptor {
                source_file,
                current_color: CURRENT_COLOR,
                recursive: true,
            });

        geometry_name = Some(filename.to_lowercase());
    } else if has_geometry(source_file) {
        // Just add geometry for this node.
        // Use the current color at this node since this geometry might not be referenced elsewhere.
        geometry_descriptors
            .entry(filename.to_lowercase())
            .or_insert_with(|| GeometryInitDescriptor {
                source_file,
                current_color,
                recursive: false,
            });

        geometry_name = Some(filename.to_lowercase());
    } else {
        for cmd in &source_file.cmds {
            if let Command::SubFileRef(sfr_cmd) = cmd {
                if let Some(subfile) = source_map.get(&sfr_cmd.file) {
                    // Don't apply node transforms to preserve the scene hierarchy.
                    // Applications should handle combining the transforms.
                    let child_transform = sfr_cmd.transform.to_matrix();

                    // Handle replacing colors.
                    let child_color = replace_color(sfr_cmd.color, current_color);

                    let child_node = load_node(
                        subfile,
                        &sfr_cmd.file,
                        &child_transform,
                        source_map,
                        geometry_descriptors,
                        child_color,
                        settings,
                    );
                    children.push(child_node);
                }
            }
        }
    }

    let transform = scaled_transform(transform, settings.scene_scale);

    LDrawNode {
        name: filename.to_string(),
        transform,
        geometry_name,
        current_color,
        children,
    }
}

#[tracing::instrument]
fn create_geometry_cache(
    geometry_descriptors: HashMap<String, GeometryInitDescriptor>,
    source_map: &ldraw::SourceMap,
    settings: &GeometrySettings,
) -> HashMap<String, LDrawGeometry> {
    // Create the actual geometry in parallel to improve performance.
    // TODO: The workload is incredibly uneven across threads.
    geometry_descriptors
        .into_par_iter()
        .map(|(name, descriptor)| {
            let GeometryInitDescriptor {
                source_file,
                current_color,
                recursive,
            } = descriptor;

            let geometry = create_geometry(
                source_file,
                source_map,
                &name,
                current_color,
                recursive,
                settings,
            );

            (name, geometry)
        })
        .collect()
}

fn scaled_transform(transform: &Mat4, scale: f32) -> Mat4 {
    // Only scale the translation so that the scale doesn't accumulate.
    // TODO: Is this the best way to handle scale?
    let mut transform = *transform;
    transform.w_axis *= vec4(scale, scale, scale, 1.0);
    transform
}

#[tracing::instrument]
pub fn load_file_instanced_points(
    path: &str,
    ldraw_path: &str,
    additional_paths: &[String],
    settings: &GeometrySettings,
) -> LDrawSceneInstancedPoints {
    let scene = load_file_instanced(path, ldraw_path, additional_paths, settings);

    let geometry_point_instances = scene
        .geometry_world_transforms
        .into_par_iter()
        .map(|(k, transforms)| {
            let instances = geometry_point_instances(transforms);
            (k, instances)
        })
        .collect();

    LDrawSceneInstancedPoints {
        main_model_name: scene.main_model_name,
        geometry_point_instances,
        geometry_cache: scene.geometry_cache,
    }
}

#[tracing::instrument]
fn geometry_point_instances(transforms: Vec<Mat4>) -> PointInstances {
    let mut translations = Vec::new();
    let mut rotations_axis = Vec::new();
    let mut rotations_angle = Vec::new();
    let mut scales = Vec::new();

    for transform in transforms {
        let (s, r, t) = transform.to_scale_rotation_translation();

        translations.push(t);

        // Decomposing to euler seems to not always work.
        // Just use an axis and angle since this better represents the quaternion.
        let (axis, angle) = r.to_axis_angle();
        rotations_axis.push(axis);
        rotations_angle.push(angle);

        scales.push(s);
    }

    PointInstances {
        translations,
        rotations_axis,
        rotations_angle,
        scales,
    }
}

// TODO: Also instance studs to reduce memory usage?
/// Find the world transforms for each geometry.
/// This allows applications to more easily use instancing.
// TODO: Take AsRef<Path> instead?
#[tracing::instrument]
pub fn load_file_instanced(
    path: &str,
    ldraw_path: &str,
    additional_paths: &[String],
    settings: &GeometrySettings,
) -> LDrawSceneInstanced {
    let (source_map, main_model_name) = parse_file(path, ldraw_path, additional_paths, settings);
    let source_file = source_map.get(&main_model_name).unwrap();

    // Find the world transforms for each geometry.
    // This allows applications to more easily use instancing.
    let mut geometry_descriptors = HashMap::new();
    let mut geometry_world_transforms = HashMap::new();
    load_node_instanced(
        source_file,
        &main_model_name,
        &Mat4::IDENTITY,
        &source_map,
        &mut geometry_descriptors,
        &mut geometry_world_transforms,
        CURRENT_COLOR,
        settings,
    );

    let geometry_cache = create_geometry_cache(geometry_descriptors, &source_map, settings);

    LDrawSceneInstanced {
        main_model_name,
        geometry_world_transforms,
        geometry_cache,
    }
}

// TODO: Share code with the non instanced function?
fn load_node_instanced<'a>(
    source_file: &'a ldraw::SourceFile,
    filename: &str,
    world_transform: &Mat4,
    source_map: &'a ldraw::SourceMap,
    geometry_descriptors: &mut HashMap<String, GeometryInitDescriptor<'a>>,
    geometry_world_transforms: &mut HashMap<(String, ColorCode), Vec<Mat4>>,
    current_color: ColorCode,
    settings: &GeometrySettings,
) {
    // TODO: Find a way to avoid repetition.
    let is_part = is_part(source_file, filename);
    if is_part {
        // Create geometry if the node is a part.
        // Use the special color code to reuse identical parts in different colors.
        geometry_descriptors
            .entry(filename.to_lowercase())
            .or_insert_with(|| GeometryInitDescriptor {
                source_file,
                current_color: CURRENT_COLOR,
                recursive: true,
            });

        // Add another instance of the current geometry.
        // Also key by the color in case a part appears in multiple colors.
        geometry_world_transforms
            .entry((filename.to_lowercase(), current_color))
            .or_default()
            .push(scaled_transform(world_transform, settings.scene_scale));
    } else if has_geometry(source_file) {
        // Just add geometry for this node.
        // Use the current color at this node since this geometry might not be referenced elsewhere.
        geometry_descriptors
            .entry(filename.to_lowercase())
            .or_insert_with(|| GeometryInitDescriptor {
                source_file,
                current_color,
                recursive: false,
            });

        // Add another instance of the current geometry.
        // Also key by the color in case a part appears in multiple colors.
        geometry_world_transforms
            .entry((filename.to_lowercase(), current_color))
            .or_default()
            .push(scaled_transform(world_transform, settings.scene_scale));
    }

    // Recursion is already handled for parts.
    if !is_part {
        for cmd in &source_file.cmds {
            if let Command::SubFileRef(sfr_cmd) = cmd {
                if let Some(subfile) = source_map.get(&sfr_cmd.file) {
                    // Accumulate transforms.
                    let child_transform = *world_transform * sfr_cmd.transform.to_matrix();

                    // Handle replacing colors.
                    let child_color = replace_color(sfr_cmd.color, current_color);

                    load_node_instanced(
                        subfile,
                        &sfr_cmd.file,
                        &child_transform,
                        source_map,
                        geometry_descriptors,
                        geometry_world_transforms,
                        child_color,
                        settings,
                    );
                }
            }
        }
    }
}

fn is_part(_source_file: &ldraw::SourceFile, filename: &str) -> bool {
    // TODO: Check the part type rather than the extension.
    filename.to_lowercase().ends_with(".dat")
}

fn has_geometry(source_file: &ldraw::SourceFile) -> bool {
    // Some files have subfile ref commands but also define parts inline.
    // This includes tube segments on the Volkswagen Beetle.mpd
    source_file
        .cmds
        .iter()
        .any(|c| matches!(c, Command::Triangle(_) | Command::Quad(_)))
}

#[cfg(test)]
mod tests {
    use approx::assert_relative_eq;
    use glam::vec3;

    use super::*;

    #[test]
    fn geometry_point_instances_flip() {
        // Some LDraw models use negative scaling.
        // Test that decomposed transforms are correct.
        // This used to break with instance on faces.
        // Point instances seem to not need special handling for now.
        let transforms = vec![
            Mat4::from_cols_array_2d(&[
                [0.0, 0.0, -1.0, 1.0],
                [0.0, 1.0, 0.0, 2.0],
                [1.0, 0.0, 0.0, 3.0],
                [0.0, 0.0, 0.0, 1.0],
            ])
            .transpose(),
            Mat4::from_cols_array_2d(&[
                [0.0, 0.0, 1.0, 1.0],
                [0.0, 1.0, 0.0, 2.0],
                [1.0, 0.0, 0.0, 3.0],
                [0.0, 0.0, 0.0, 1.0],
            ])
            .transpose(),
        ];

        let instances = geometry_point_instances(transforms);

        assert_relative_eq!(instances.rotations_axis[0].to_array()[..], [0.0, 1.0, 0.0]);
        assert_relative_eq!(instances.rotations_axis[1].to_array()[..], [0.0, 1.0, 0.0]);

        assert_relative_eq!(instances.rotations_angle[0], 4.712389);
        assert_relative_eq!(instances.rotations_angle[1], 1.5707964);

        assert_eq!(
            instances.scales,
            vec![vec3(1.0, 1.0, 1.0), vec3(-1.0, 1.0, 1.0)]
        );
    }
}
