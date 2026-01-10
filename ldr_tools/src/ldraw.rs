//! LDraw file format and parser.

// The LDraw representation and parser are based on work done for [weldr](https://github.com/djeedai/weldr).
use log::{debug, error, trace};
use std::{collections::HashMap, path::Path, str};

pub use glam::{Mat4, Vec2, Vec3, Vec4};
pub use parse::parse_commands;

mod parse;

/// RGB color in sRGB color space.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Color {
    pub red: u8,
    pub green: u8,
    pub blue: u8,
}

impl Color {
    /// Construct a new color instance from individual RGB components.
    pub fn new(red: u8, green: u8, blue: u8) -> Self {
        Self { red, green, blue }
    }
}

struct FileRef {
    /// Filename of unresolved source file.
    filename: String,
}

/// Parse a single file and its sub-file references recursively.
///
/// Attempt to load the content of `path` via the given `resolver`, and parse it.
/// Then recursively look for sub-file commands inside that root file, and try to resolve
/// the content of those sub-files and parse them too. All the loaded and parsed files end
/// up populating the given `source_map`, which can be pre-populated manually or from a
/// previous call with already loaded and parsed files.
/// ```rust
/// use ldr_tools::ldraw::{FileRefResolver, parse, SourceMap};
///
/// struct MyCustomResolver;
///
/// impl FileRefResolver for MyCustomResolver {
///   fn resolve<P: AsRef<std::path::Path>>(&self, filename: P) -> Option<Vec<u8>> {
///     Some(Vec::new()) // replace with custom impl
///   }
/// }
///
/// fn main() -> Result<(), Box<dyn std::error::Error>> {
///   let resolver = MyCustomResolver{};
///   let mut source_map = SourceMap::new();
///   let main_model_name = parse("root.ldr", &resolver, &mut source_map);
///   let root_file = source_map.get(&main_model_name).unwrap();
///   Ok(())
/// }
/// ```
pub fn parse<P: AsRef<Path>, R: FileRefResolver>(
    path: P,
    resolver: &R,
    source_map: &mut SourceMap,
) -> String {
    // Use a stack to avoid function recursion in load_file.
    let mut stack: Vec<FileRef> = Vec::new();

    debug!("Processing root file '{:?}'", path.as_ref());
    // The provided path should refer to a file from the resolver.
    // Use the path directly without any normalization.
    let filename = path.as_ref().to_string_lossy().to_string();
    let actual_root = load_file(LDrawPath::new(&filename), resolver, source_map, &mut stack);

    // Recursively load files referenced by the root file.
    while let Some(file) = stack.pop() {
        let filename = &file.filename;
        debug!("Processing sub-file: '{filename}'");
        match source_map.get(filename) {
            Some(_) => trace!("Already parsed; reusing sub-file: {filename}"),
            None => {
                trace!("Not yet parsed; parsing sub-file: {filename}");
                // Normalize file references to subfiles.
                let subfile_ref = LDrawPath::new(filename);
                load_file(subfile_ref, resolver, source_map, &mut stack);
            }
        }
    }

    actual_root
}

fn load_file<R: FileRefResolver>(
    path: LDrawPath,
    resolver: &R,
    source_map: &mut SourceMap,
    stack: &mut Vec<FileRef>,
) -> String {
    // Resolve with the normalized path to work properly on Unix systems.
    let raw_content = resolver.resolve(&path.normalized_name).unwrap_or_else(|| {
        // TODO: Is there a better way to allow partial imports with resolve errors?
        error!("Unable to resolve {path:?}");
        Vec::new()
    });
    let source_file = SourceFile {
        cmds: parse_commands(&raw_content),
    };

    source_map.queue_subfiles(&source_file, stack);
    source_map.insert(path, source_file)
}

/// [Line Type 0](https://www.ldraw.org/article/218.html#lt0) META command:
/// [!CATEGORY language extension](https://www.ldraw.org/article/340.html#category).
#[derive(Debug, PartialEq, Clone)]
pub struct CategoryCmd {
    /// Category name.
    pub category: String,
}

/// [Line Type 0](https://www.ldraw.org/article/218.html#lt0) META command:
/// [!KEYWORDS language extension](https://www.ldraw.org/article/340.html#keywords).
#[derive(Debug, PartialEq, Clone)]
pub struct KeywordsCmd {
    /// List of keywords.
    pub keywords: Vec<String>,
}

/// Finish for color definitions ([!COLOUR language extension](https://www.ldraw.org/article/299.html)).
#[derive(Debug, PartialEq, Clone)]
pub enum ColorFinish {
    Chrome,
    Pearlescent,
    Rubber,
    MatteMetallic,
    Metal,
    Material(MaterialFinish),
}

/// Finish for optional MATERIAL part of color definition
/// ([!COLOUR language extension](https://www.ldraw.org/article/299.html)).
#[derive(Debug, PartialEq, Clone)]
pub enum MaterialFinish {
    Glitter(GlitterMaterial),
    Speckle(SpeckleMaterial),
    Other(String),
}

/// Grain size variants for the optional MATERIAL part of color definition
/// ([!COLOUR language extension](https://www.ldraw.org/article/299.html)).
#[derive(Debug, PartialEq, Clone)]
pub enum GrainSize {
    Size(f32),
    MinMaxSize((f32, f32)),
}

/// Glitter material definition of a color definition
/// ([!COLOUR language extension](https://www.ldraw.org/article/299.html)).
#[derive(Debug, PartialEq, Clone)]
pub struct GlitterMaterial {
    /// Primary color value of the material.
    pub value: Color,
    /// Optional alpha (opacity) value.
    pub alpha: Option<u8>,
    /// Optional brightness value.
    pub luminance: Option<u8>,
    /// Fraction of the surface using the alternate color.
    pub surface_fraction: f32,
    /// Fraction of the volume using the alternate color.
    pub volume_fraction: f32,
    /// Size of glitter grains.
    pub size: GrainSize,
}

/// Speckle material definition of a color definition
/// ([!COLOUR language extension](https://www.ldraw.org/article/299.html)).
#[derive(Debug, PartialEq, Clone)]
pub struct SpeckleMaterial {
    /// Primary color value of the material.
    pub value: Color,
    /// Optional alpha (opacity) value.
    pub alpha: Option<u8>,
    /// Optional brightness value.
    pub luminance: Option<u8>,
    /// Fraction of the surface using the alternate color.
    pub surface_fraction: f32,
    /// Size of speckle grains.
    pub size: GrainSize,
}

/// [Line Type 0](https://www.ldraw.org/article/218.html#lt0) META command:
/// [!COLOUR language extension](https://www.ldraw.org/article/299.html).
#[derive(Debug, PartialEq, Clone)]
pub struct ColourCmd {
    /// Name of the color.
    pub name: String,
    /// Color code uniquely identifying this color. Codes 16 and 24 are reserved.
    pub code: u32,
    /// Primary value of the color.
    pub value: Color,
    /// Contrasting edge value of the color.
    pub edge: Color,
    /// Optional alpha (opacity) value.
    pub alpha: Option<u8>,
    /// Optional ["brightness for colors that glow"](https://www.ldraw.org/article/299.html#luminance).
    pub luminance: Option<u8>,
    /// Finish/texture of the object for high-fidelity rendering.
    pub finish: Option<ColorFinish>,
}

/// [Line Type 0](https://www.ldraw.org/article/218.html#lt0) comment.
#[derive(Debug, PartialEq, Clone)]
pub struct CommentCmd {
    /// Comment content, excluding the command identififer `0` and the optional comment marker `//`.
    pub text: String,
}

impl CommentCmd {
    pub fn new(text: &str) -> Self {
        Self {
            text: text.to_string(),
        }
    }
}

/// [Line Type 0](https://www.ldraw.org/article/218.html#lt0) FILE start.
/// [MPD Extension[(<https://www.ldraw.org/article/47.html>)
#[derive(Debug, PartialEq, Clone)]
pub struct FileCmd {
    /// The filename for this file.
    pub file: String,
}

/// [Line Type 0](https://www.ldraw.org/article/218.html#lt0) DATA.
/// [MPD Extension[(<https://www.ldraw.org/article/47.html>)
#[derive(Debug, PartialEq, Clone)]
pub struct DataCmd {
    /// The filename for this data file.
    pub file: String,
}

/// [Line Type 0](https://www.ldraw.org/article/218.html#lt0) base64 data chunk.
/// [MPD Extension[(<https://www.ldraw.org/article/47.html>)
#[derive(Debug, PartialEq, Clone)]
pub struct Base64DataCmd {
    /// The decoded base64 data chunk.
    pub data: Vec<u8>,
}

/// The commands for a single LDraw source file.
#[derive(Debug, PartialEq, Clone)]
pub struct SourceFile {
    /// LDraw commands parsed from the raw text content of the file.
    pub cmds: Vec<Command>,
}

/// An LDraw file or submodel name that normalizes
/// case and path separators for hashing and comparison.
#[derive(Debug, Clone)]
pub struct LDrawPath {
    pub name: String,
    pub normalized_name: String,
}

impl LDrawPath {
    pub fn new(s: &str) -> Self {
        // Cache name normalization to improve performance.
        Self {
            name: s.to_string(),
            normalized_name: normalize_subfile_reference(s),
        }
    }
}

impl PartialEq for LDrawPath {
    fn eq(&self, other: &Self) -> bool {
        self.normalized_name == other.normalized_name
    }
}

impl Eq for LDrawPath {}

impl std::hash::Hash for LDrawPath {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.normalized_name.hash(state);
    }
}

fn normalize_subfile_reference(s: &str) -> String {
    // LDraw filenames are not case sensitive.
    // This also includes references to MPD subfiles.
    // Normalize paths to lowercase and forward slashes.
    // The official parts library can be assumed to use lowercase.
    s.to_lowercase().replace('\\', "/").replace("//", "/")
}

/// Collection of [`SourceFile`] accessible from their reference filename.
#[derive(Debug)]
pub struct SourceMap {
    /// Map of filenames to source files.
    source_files: HashMap<LDrawPath, SourceFile>,
}

impl SourceMap {
    /// Construct a new empty source map.
    pub fn new() -> Self {
        Self {
            source_files: HashMap::new(),
        }
    }

    /// Returns a reference to the source file corresponding to `filename`.
    pub fn get(&self, filename: &str) -> Option<(&LDrawPath, &SourceFile)> {
        // TODO: handle normalization and case sensitivity.
        self.source_files.get_key_value(&LDrawPath::new(filename))
    }

    /// Inserts a new source file into the collection.
    /// Returns a copy of the filename of `source_file`
    /// or the filename of the main file for multi-part documents (MPD).
    pub fn insert(&mut self, filename: LDrawPath, source_file: SourceFile) -> String {
        // The MPD extension allows .ldr or .mpd files to contain multiple files.
        // Add each of these so that they can be resolved by subfile commands later.
        let files = split_mpd_file(&source_file.cmds);

        // Some files are referenced in their entirety even if they have multiple models.
        self.source_files.insert(filename.clone(), source_file);

        // TODO: More cleanly handle the fact that not all files have 0 FILE commands.
        if files.is_empty() {
            filename.name
        } else {
            // The first block is the "main model" of the file.
            let main_model_name = files[0].0.clone();
            for (name, file) in files {
                self.source_files.insert(LDrawPath::new(&name), file);
            }
            main_model_name
        }
    }

    fn queue_subfiles(&self, source_file: &SourceFile, stack: &mut Vec<FileRef>) {
        for cmd in &source_file.cmds {
            if let Command::SubFileRef(sfr_cmd) = cmd {
                // Queue this file for loading if we haven't already.
                if self.get(&sfr_cmd.file).is_none() {
                    trace!("Queuing unresolved subfile ref {}", sfr_cmd.file);
                    stack.push(FileRef {
                        filename: sfr_cmd.file.clone(),
                    });
                }
            }
        }
    }
}

fn split_mpd_file(cmds: &[Command]) -> Vec<(String, SourceFile)> {
    cmds.iter()
        .enumerate()
        .filter_map(|(i, c)| match c {
            Command::File(file_cmd) => Some((i, file_cmd)),
            _ => None,
        })
        .map(|(file_start, file_cmd)| {
            // Each file block starts with a FILE command.
            // The block continues until the next NOFILE or FILE command.
            // TODO: Is there a cleaner way of expressing this?
            let subfile = &cmds[file_start..];
            // Start from 1 to ignore the current file command.
            let subfile_end = subfile
                .iter()
                .skip(1)
                .position(|c| matches!(c, Command::File(_) | Command::NoFile));
            let subfile_cmds = if let Some(subfile_end) = subfile_end {
                // Add one here since we skip the first FILE command.
                subfile[..subfile_end + 1].to_vec()
            } else {
                subfile.to_vec()
            };
            (file_cmd.file.clone(), SourceFile { cmds: subfile_cmds })
        })
        .collect()
}

impl Default for SourceMap {
    fn default() -> Self {
        Self::new()
    }
}

/// A transformation matrix.
#[derive(Debug, PartialEq, Clone)]
pub struct Transform {
    /// Position.
    pub pos: Vec3,
    /// First row of rotation+scaling matrix part.
    pub row0: Vec3,
    /// Second row of rotation+scaling matrix part.
    pub row1: Vec3,
    /// Third row of rotation+scaling matrix part.
    pub row2: Vec3,
}

/// [Line Type 1](https://www.ldraw.org/article/218.html#lt1) LDraw command:
/// Reference a sub-file from the current file.
#[derive(Debug, PartialEq, Clone)]
pub struct SubFileRefCmd {
    /// Color code of the part.
    pub color: u32,
    /// Transform of this part relative to parent.
    pub transform: Transform,
    /// Referenced sub-file.
    pub file: String,
}

/// [Line Type 2](https://www.ldraw.org/article/218.html#lt2) LDraw command:
/// Draw a segment between 2 vertices.
#[derive(Debug, PartialEq, Clone)]
pub struct LineCmd {
    /// Color code of the primitive.
    pub color: u32,
    /// Vertices of the segment.
    pub vertices: [Vec3; 2],
}

/// [Line Type 3](https://www.ldraw.org/article/218.html#lt3) LDraw command:
/// Draw a triangle between 3 vertices.
#[derive(Debug, PartialEq, Clone)]
pub struct TriangleCmd {
    /// Color code of the primitive.
    pub color: u32,
    /// Vertices of the triangle.
    pub vertices: [Vec3; 3],
    /// UV texture coordinates for texture mapping extensions.
    pub uvs: Option<[Vec2; 3]>,
}

/// [Line Type 4](https://www.ldraw.org/article/218.html#lt4) LDraw command:
/// Draw a quad between 4 vertices.
#[derive(Debug, PartialEq, Clone)]
pub struct QuadCmd {
    /// Color code of the primitive.
    pub color: u32,
    /// Vertices of the quad. In theory they are guaranteed to be coplanar according to the LDraw
    /// specification, although no attempt is made to validate this property.
    pub vertices: [Vec3; 4],
    /// UV texture coordinates for texture mapping extensions.
    pub uvs: Option<[Vec2; 4]>,
}

/// [Line Type 5](https://www.ldraw.org/article/218.html#lt5) LDraw command:
/// Draw an optional segment between two vertices, aided by 2 control points.
#[derive(Debug, PartialEq, Clone)]
pub struct OptLineCmd {
    /// Color code of the primitive.
    pub color: u32,
    /// Vertices of the segment.
    pub vertices: [Vec3; 2],
    /// Control points of the segment.
    pub control_points: [Vec3; 2],
}

/// [Line Type 0](https://www.ldraw.org/article/218.html#lt0) META command:
/// [BFC language extension](https://www.ldraw.org/article/415)
#[derive(Debug, PartialEq, Clone)]
pub enum BfcCommand {
    /// Disable BFC commands for this file.
    NoCertify,
    /// Certify this file as BFC compatiple and set winding.
    /// Winding is assumed to be [Winding::Ccw] if not set.
    Certify(Option<Winding>),
    /// Set the winding for this file.
    Winding(Winding),
    /// Disable backface culling.
    NoClip,
    /// Enable backface culling and set winding.
    Clip(Option<Winding>),
    /// Invert the winding of the next subfile command.
    InvertNext,
}

/// The ordering of vertices in a face.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Winding {
    /// Countr-clockwise winding
    Ccw,
    /// Clockwise winding
    Cw,
}

/// [Line Type 0](https://www.ldraw.org/article/218.html#lt0) META command: PE_TEX_PATH
/// Bricklink Studio texture extension
#[derive(Debug, PartialEq, Clone)]
pub struct PeTexPathCmd {
    /// Indices for [SubFileRefCmd] starting from the current file to assign the current [PeTexInfoCmd].
    ///
    /// The paths `[0, 1]` would assign the [PeTexInfoCmd]
    /// to `file.subfiles[0].subfiles[1]` in pseudo code.
    /// The paths `[-1]` is a special case that assigns to the current file.  
    pub paths: Vec<i32>,
}

/// [Line Type 0](https://www.ldraw.org/article/218.html#lt0) META command: PE_TEX_INFO
/// Bricklink Studio texture extension
#[derive(Debug, PartialEq, Clone)]
pub struct PeTexInfoCmd {
    /// Transform for projecting vertex positions to texture coordinates.
    pub transform: Option<PeTexInfoTransform>,
    /// The decoded base64 image data chunk.
    pub data: Vec<u8>,
}

/// Projection transform for creating texture coordinates from vertex positions.
#[derive(Debug, PartialEq, Clone)]
pub struct PeTexInfoTransform {
    pub transform: Transform,
    pub point_min: Vec2,
    pub point_max: Vec2,
}

/// Types of commands contained in a LDraw file.
#[derive(Debug, PartialEq, Clone)]
pub enum Command {
    /// [Line Type 0](https://www.ldraw.org/article/218.html#lt0) META command:
    /// [!CATEGORY language extension](https://www.ldraw.org/article/340.html#category).
    Category(CategoryCmd),
    /// [Line Type 0](https://www.ldraw.org/article/218.html#lt0) META command:
    /// [!KEYWORDS language extension](https://www.ldraw.org/article/340.html#keywords).
    Keywords(KeywordsCmd),
    /// [Line Type 0](https://www.ldraw.org/article/218.html#lt0) META command:
    /// [!COLOUR language extension](https://www.ldraw.org/article/299.html).
    Colour(ColourCmd),
    /// [Line Type 0](https://www.ldraw.org/article/218.html#lt0) META command:
    /// [MPD language extension](https://www.ldraw.org/article/47.html).
    File(FileCmd),
    /// [Line Type 0](https://www.ldraw.org/article/218.html#lt0) META command:
    /// [MPD language extension](https://www.ldraw.org/article/47.html).
    NoFile,
    /// [Line Type 0](https://www.ldraw.org/article/218.html#lt0) META command:
    /// [MPD language extension](https://www.ldraw.org/article/47.html).
    Data(DataCmd),
    /// [Line Type 0](https://www.ldraw.org/article/218.html#lt0) META command:
    /// [MPD language extension](https://www.ldraw.org/article/47.html).
    Base64Data(Base64DataCmd),
    /// [Line Type 0](https://www.ldraw.org/article/218.html#lt0) comment.
    /// Note: any line type 0 not otherwise parsed as a known meta-command is parsed as a generic comment.
    Comment(CommentCmd),
    /// [Line Type 1](https://www.ldraw.org/article/218.html#lt1) sub-file reference.
    SubFileRef(SubFileRefCmd),
    /// [Line Type 2](https://www.ldraw.org/article/218.html#lt2) segment.
    Line(LineCmd),
    /// [Line Type 3](https://www.ldraw.org/article/218.html#lt3) triangle.
    Triangle(TriangleCmd),
    /// [Line Type 4](https://www.ldraw.org/article/218.html#lt4) quadrilateral.
    Quad(QuadCmd),
    /// [Line Type 5](https://www.ldraw.org/article/218.html#lt5) optional line.
    OptLine(OptLineCmd),
    /// [Line Type 0](https://www.ldraw.org/article/218.html#lt0) META command:
    /// [BFC language extension](https://www.ldraw.org/article/415)
    Bfc(BfcCommand),
    /// [Line Type 0](https://www.ldraw.org/article/218.html#lt0) META command:
    /// Bricklink Studio texture extension
    PeTexPath(PeTexPathCmd),
    /// [Line Type 0](https://www.ldraw.org/article/218.html#lt0) META command:
    /// Bricklink Studio texture extension
    PeTexInfo(PeTexInfoCmd),
}

/// Resolver trait for sub-file references ([Line Type 1](https://www.ldraw.org/article/218.html#lt1) LDraw command).
///
/// An implementation of this trait must be passed to [`parse()`] to allow resolving sub-file references recursively,
/// and parsing all dependent sub-files of the top-level file provided.
///
/// When loading parts and primitives from the official LDraw catalog, implementations are free to decide how to retrieve
/// the file content, but must ensure that all canonical paths are in scope, as sub-file references can be relative to
/// any of those:
/// - `/p/`       - Parts primitives
/// - `/p/48/`    - High-resolution primitives
/// - `/parts/`   - Main catalog of parts
/// - `/parts/s/` - Catalog of sub-parts commonly used
pub trait FileRefResolver {
    /// Resolve the given file reference `filename`, given as it appears in a sub-file reference, and return
    /// the content of the file as a UTF-8 encoded buffer of bytes, without BOM. Line ending can be indifferently
    /// Unix style `\n` or Windows style `\r\n`.
    ///
    /// See [`parse()`] for usage.
    fn resolve<P: AsRef<Path>>(&self, filename: P) -> Option<Vec<u8>>;
}

impl Transform {
    /// Get the 4x4 transformation matrix applied to the subfile.
    pub fn to_matrix(&self) -> Mat4 {
        Mat4::from_cols(
            self.row0.extend(self.pos.x),
            self.row1.extend(self.pos.y),
            self.row2.extend(self.pos.z),
            Vec4::new(0.0, 0.0, 0.0, 1.0),
        )
        .transpose()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use glam::vec3;
    use pretty_assertions::assert_eq;

    #[test]
    fn test_split_mpd_files() {
        let commands = vec![
            Command::File(FileCmd {
                file: "a".to_string(),
            }),
            Command::SubFileRef(SubFileRefCmd {
                color: 16,
                transform: Transform {
                    pos: vec3(0.0, 0.0, 0.0),
                    row0: vec3(1.0, 0.0, 0.0),
                    row1: vec3(0.0, 1.0, 0.0),
                    row2: vec3(0.0, 0.0, 1.0),
                },
                file: "1.dat".to_string(),
            }),
            Command::NoFile,
            Command::File(FileCmd {
                file: "b".to_string(),
            }),
            Command::SubFileRef(SubFileRefCmd {
                color: 16,
                transform: Transform {
                    pos: vec3(0.0, 0.0, 0.0),
                    row0: vec3(1.0, 0.0, 0.0),
                    row1: vec3(0.0, 1.0, 0.0),
                    row2: vec3(0.0, 0.0, 1.0),
                },
                file: "2.dat".to_string(),
            }),
            Command::NoFile,
        ];
        let subfiles = split_mpd_file(&commands);
        assert_eq!(
            vec![
                (
                    "a".to_string(),
                    SourceFile {
                        cmds: commands[0..2].to_vec()
                    }
                ),
                (
                    "b".to_string(),
                    SourceFile {
                        cmds: commands[3..5].to_vec()
                    }
                )
            ],
            subfiles
        );
    }

    #[test]
    fn test_split_mpd_files_just_file_commands() {
        let commands = vec![
            Command::File(FileCmd {
                file: "a".to_string(),
            }),
            Command::SubFileRef(SubFileRefCmd {
                color: 16,
                transform: Transform {
                    pos: vec3(0.0, 0.0, 0.0),
                    row0: vec3(1.0, 0.0, 0.0),
                    row1: vec3(0.0, 1.0, 0.0),
                    row2: vec3(0.0, 0.0, 1.0),
                },
                file: "1.dat".to_string(),
            }),
            Command::File(FileCmd {
                file: "b".to_string(),
            }),
            Command::SubFileRef(SubFileRefCmd {
                color: 16,
                transform: Transform {
                    pos: vec3(0.0, 0.0, 0.0),
                    row0: vec3(1.0, 0.0, 0.0),
                    row1: vec3(0.0, 1.0, 0.0),
                    row2: vec3(0.0, 0.0, 1.0),
                },
                file: "2.dat".to_string(),
            }),
        ];

        let subfiles = split_mpd_file(&commands);
        assert_eq!(
            vec![
                (
                    "a".to_string(),
                    SourceFile {
                        cmds: commands[0..2].to_vec()
                    }
                ),
                (
                    "b".to_string(),
                    SourceFile {
                        cmds: commands[2..].to_vec()
                    }
                )
            ],
            subfiles
        );
    }

    #[test]
    fn test_parse_commands_mpd() {
        // Test various language extensions.
        // Example taken from https://www.ldraw.org/article/47.html
        let ldr_contents = b"0 FILE main.ldr
        1 7 0 0 0 1 0 0 0 1 0 0 0 1 819.dat
        1 4 80 -8 70 1 0 0 0 1 0 0 0 1 house.ldr
        1 4 -70 -8 20 0 0 -1 0 1 0 1 0 0 house.ldr
        1 4 50 -8 -20 0 0 -1 0 1 0 1 0 0 house.ldr
        1 4 0 -8 -30 1 0 0 0 1 0 0 0 1 house.ldr
        1 4 -20 -8 70 1 0 0 0 1 0 0 0 1 house.ldr
        
        0 FILE house.ldr
        1 16 0 0 0 1 0 0 0 1 0 0 0 1 3023.dat
        1 16 0 -24 0 1 0 0 0 1 0 0 0 1 3065.dat
        1 16 0 -48 0 1 0 0 0 1 0 0 0 1 3065.dat
        1 16 0 -72 0 0 0 -1 0 1 0 1 0 0 3044b.dat
        1 4 0 -22 -10 1 0 0 0 0 -1 0 1 0 sticker.ldr
        
        0 FILE sticker.ldr
        0 UNOFFICIAL PART
        0 BFC CERTIFY CCW
        1 16   0 -0.25 0   20 0 0   0 0.25 0   0 0 30   box5.dat
        0 !TEXMAP START PLANAR   -20 -0.25 30   20 -0.25 30   -20 -0.25 -30   sticker.png
        4 16   -20 -0.25 30   -20 -0.25 -30   20 -0.25 -30   20 -0.25 30
        0 !TEXMAP END
        
        0 !DATA sticker.png
        0 !: iVBORw0KGgoAAAANSUhEUgAAAFAAAAB4CAIAAADqjOKhAAAAAXNSR0IArs4c6QAAAARnQU1BAACx
        0 !: jwv8YQUAAAAJcEhZcwAADsMAAA7DAcdvqGQAAAEUSURBVHhe7du9DcIwFABhk5WgQLSsQM0UjMEU
        0 !: 1BQsQIsoYAt6NkAYxQV/JQ7WvfuKkFTR6UmOFJzR9bJLkXTlNwyD6QymM5ju5Tl8m67KGUt3XJcz
        0 !: J/yY8HZ/6C8BFvNZPoaesMF0BtMZTGcwncF0BtMZTGcwncF0BtMZTGcwnf8t0bmLh85gOoPpDKYz
        0 !: mM5gOoPpDKYzmM5gunDBf3tN+/zqNKt367cbOeGUTstxf1nJZHPOx68T/u3XB5/7/zMXLTqD6Qym
        0 !: M5jOYDqD6QymM5jOYDqD6QymM5jOYDqD6QymM5jOYLpwwW3t8ajBXTxtTHgwLlp0BtMZTGcwncF0
        0 !: BtMZTNfKZzyDiT3hCFy06IIFp3QH/CBMh66aBy4AAAAASUVORK5CYII=
        ";

        let commands = parse_commands(ldr_contents);
        assert_eq!(
            vec![
                Command::File(FileCmd {
                    file: "main.ldr".to_string()
                }),
                Command::SubFileRef(SubFileRefCmd {
                    color: 7,
                    transform: Transform {
                        pos: vec3(0.0, 0.0, 0.0),
                        row0: vec3(1.0, 0.0, 0.0),
                        row1: vec3(0.0, 1.0, 0.0),
                        row2: vec3(0.0, 0.0, 1.0)
                    },
                    file: "819.dat".to_string()
                }),
                Command::SubFileRef(SubFileRefCmd {
                    color: 4,
                    transform: Transform {
                        pos: vec3(80.0, -8.0, 70.0),
                        row0: vec3(1.0, 0.0, 0.0),
                        row1: vec3(0.0, 1.0, 0.0),
                        row2: vec3(0.0, 0.0, 1.0)
                    },
                    file: "house.ldr".to_string()
                }),
                Command::SubFileRef(SubFileRefCmd {
                    color: 4,
                    transform: Transform {
                        pos: vec3(-70.0, -8.0, 20.0),
                        row0: vec3(0.0, 0.0, -1.0),
                        row1: vec3(0.0, 1.0, 0.0),
                        row2: vec3(1.0, 0.0, 0.0)
                    },
                    file: "house.ldr".to_string()
                }),
                Command::SubFileRef(SubFileRefCmd {
                    color: 4,
                    transform: Transform {
                        pos: vec3(50.0, -8.0, -20.0),
                        row0: vec3(0.0, 0.0, -1.0),
                        row1: vec3(0.0, 1.0, 0.0),
                        row2: vec3(1.0, 0.0, 0.0)
                    },
                    file: "house.ldr".to_string()
                }),
                Command::SubFileRef(SubFileRefCmd {
                    color: 4,
                    transform: Transform {
                        pos: vec3(0.0, -8.0, -30.0),
                        row0: vec3(1.0, 0.0, 0.0),
                        row1: vec3(0.0, 1.0, 0.0),
                        row2: vec3(0.0, 0.0, 1.0)
                    },
                    file: "house.ldr".to_string()
                }),
                Command::SubFileRef(SubFileRefCmd {
                    color: 4,
                    transform: Transform {
                        pos: vec3(-20.0, -8.0, 70.0),
                        row0: vec3(1.0, 0.0, 0.0),
                        row1: vec3(0.0, 1.0, 0.0),
                        row2: vec3(0.0, 0.0, 1.0)
                    },
                    file: "house.ldr".to_string()
                }),
                Command::File(FileCmd {
                    file: "house.ldr".to_string()
                }),
                Command::SubFileRef(SubFileRefCmd {
                    color: 16,
                    transform: Transform {
                        pos: vec3(0.0, 0.0, 0.0),
                        row0: vec3(1.0, 0.0, 0.0),
                        row1: vec3(0.0, 1.0, 0.0),
                        row2: vec3(0.0, 0.0, 1.0)
                    },
                    file: "3023.dat".to_string()
                }),
                Command::SubFileRef(SubFileRefCmd {
                    color: 16,
                    transform: Transform {
                        pos: vec3(0.0, -24.0, 0.0),
                        row0: vec3(1.0, 0.0, 0.0),
                        row1: vec3(0.0, 1.0, 0.0),
                        row2: vec3(0.0, 0.0, 1.0)
                    },
                    file: "3065.dat".to_string()
                }),
                Command::SubFileRef(SubFileRefCmd {
                    color: 16,
                    transform: Transform {
                        pos: vec3(0.0, -48.0, 0.0),
                        row0: vec3(1.0, 0.0, 0.0),
                        row1: vec3(0.0, 1.0, 0.0),
                        row2: vec3(0.0, 0.0, 1.0)
                    },
                    file: "3065.dat".to_string()
                }),
                Command::SubFileRef(SubFileRefCmd {
                    color: 16,
                    transform: Transform {
                        pos: vec3(0.0, -72.0, 0.0),
                        row0: vec3(0.0, 0.0, -1.0),
                        row1: vec3(0.0, 1.0, 0.0),
                        row2: vec3(1.0, 0.0, 0.0)
                    },
                    file: "3044b.dat".to_string()
                }),
                Command::SubFileRef(SubFileRefCmd {
                    color: 4,
                    transform: Transform {
                        pos: vec3(0.0, -22.0, -10.0),
                        row0: vec3(1.0, 0.0, 0.0),
                        row1: vec3(0.0, 0.0, -1.0),
                        row2: vec3(0.0, 1.0, 0.0)
                    },
                    file: "sticker.ldr".to_string()
                }),
                Command::File(FileCmd {
                    file: "sticker.ldr".to_string()
                }),
                Command::Comment(CommentCmd {
                    text: "UNOFFICIAL PART".to_string()
                }),
                Command::Bfc(BfcCommand::Certify(Some(Winding::Ccw))),
                Command::SubFileRef(SubFileRefCmd {
                    color: 16,
                    transform: Transform {
                        pos: vec3(0.0, -0.25, 0.0),
                        row0: vec3(20.0, 0.0, 0.0),
                        row1: vec3(0.0, 0.25, 0.0),
                        row2: vec3(0.0, 0.0, 30.0)
                    },
                    file: "box5.dat".to_string()
                }),
                Command::Comment(
                    CommentCmd {
                        text: "!TEXMAP START PLANAR   -20 -0.25 30   20 -0.25 30   -20 -0.25 -30   sticker.png".to_string()
                    },
                ),
                Command::Quad(QuadCmd {
                    color: 16,
                    vertices: [
                        vec3(-20.0, -0.25, 30.0),
                        vec3(-20.0, -0.25, -30.0),
                        vec3(20.0, -0.25, -30.0),
                        vec3(20.0, -0.25, 30.0)
                    ],
                    uvs: None
                }),
                Command::Comment(CommentCmd {
                    text: "!TEXMAP END".to_string()
                }),
                Command::Data(DataCmd {
                    file: "sticker.png".to_string()
                }),
                Command::Base64Data(Base64DataCmd {
                    data: vec![
                        137, 80, 78, 71, 13, 10, 26, 10, 0, 0, 0, 13, 73, 72, 68, 82, 0, 0, 0, 80,
                        0, 0, 0, 120, 8, 2, 0, 0, 0, 234, 140, 226, 161, 0, 0, 0, 1, 115, 82, 71,
                        66, 0, 174, 206, 28, 233, 0, 0, 0, 4, 103, 65, 77, 65, 0, 0, 177
                    ]
                }),
                Command::Base64Data(Base64DataCmd {
                    data: vec![
                        143, 11, 252, 97, 5, 0, 0, 0, 9, 112, 72, 89, 115, 0, 0, 14, 195, 0, 0, 14,
                        195, 1, 199, 111, 168, 100, 0, 0, 1, 20, 73, 68, 65, 84, 120, 94, 237, 219,
                        189, 13, 194, 48, 20, 0, 97, 147, 149, 160, 64, 180, 172, 64, 205, 20, 140,
                        193, 20
                    ]
                }),
                Command::Base64Data(Base64DataCmd {
                    data: vec![
                        212, 20, 44, 64, 139, 40, 96, 11, 122, 54, 64, 24, 197, 5, 127, 37, 14,
                        214, 189, 251, 138, 144, 84, 209, 233, 73, 142, 20, 156, 209, 245, 178, 75,
                        145, 116, 229, 55, 12, 131, 233, 12, 166, 51, 152, 238, 229, 57, 124, 155,
                        174, 202, 25, 75, 119, 92, 151, 51
                    ]
                }),
                Command::Base64Data(Base64DataCmd {
                    data: vec![
                        39, 252, 152, 240, 118, 127, 232, 47, 1, 22, 243, 89, 62, 134, 158, 176,
                        193, 116, 6, 211, 25, 76, 103, 48, 157, 193, 116, 6, 211, 25, 76, 103, 48,
                        157, 193, 116, 6, 211, 25, 76, 103, 48, 157, 255, 45, 209, 185, 139, 135,
                        206, 96, 58, 131, 233, 12, 166, 51
                    ]
                }),
                Command::Base64Data(Base64DataCmd {
                    data: vec![
                        152, 206, 96, 58, 131, 233, 12, 166, 51, 152, 206, 96, 186, 112, 193, 127,
                        123, 77, 251, 252, 234, 52, 171, 119, 235, 183, 27, 57, 225, 148, 78, 203,
                        113, 127, 89, 201, 100, 115, 206, 199, 175, 19, 254, 237, 215, 7, 159, 251,
                        255, 51, 23, 45, 58, 131, 233, 12, 166
                    ]
                }),
                Command::Base64Data(Base64DataCmd {
                    data: vec![
                        51, 152, 206, 96, 58, 131, 233, 12, 166, 51, 152, 206, 96, 58, 131, 233,
                        12, 166, 51, 152, 206, 96, 58, 131, 233, 12, 166, 51, 152, 206, 96, 186,
                        112, 193, 109, 237, 241, 168, 193, 93, 60, 109, 76, 120, 48, 46, 90, 116,
                        6, 211, 25, 76, 103, 48, 157, 193, 116
                    ]
                }),
                Command::Base64Data(Base64DataCmd {
                    data: vec![
                        6, 211, 25, 76, 215, 202, 103, 60, 131, 137, 61, 225, 8, 92, 180, 232, 130,
                        5, 167, 116, 7, 252, 32, 76, 135, 174, 154, 7, 46, 0, 0, 0, 0, 73, 69, 78,
                        68, 174, 66, 96, 130
                    ]
                })
            ],
            commands
        );
    }

    #[test]
    fn test_parse_commands() {
        let cmd0 = Command::Comment(CommentCmd::new("this is a comment"));
        let cmd1 = Command::Line(LineCmd {
            color: 16,
            vertices: [vec3(0.0, 0.0, 0.0), vec3(1.0, 1.0, 1.0)],
        });
        assert_eq!(
            parse_commands(b"0 this is a comment\n2 16 0 0 0 1 1 1"),
            vec![cmd0, cmd1]
        );

        let cmd0 = Command::Comment(CommentCmd::new("this doesn't matter"));
        let cmd1 = Command::SubFileRef(SubFileRefCmd {
            color: 16,
            transform: Transform {
                pos: vec3(0.0, 0.0, 0.0),
                row0: vec3(1.0, 0.0, 0.0),
                row1: vec3(0.0, 1.0, 0.0),
                row2: vec3(0.0, 0.0, 1.0),
            },
            file: "aa/aaaaddd".to_string(),
        });
        assert_eq!(
            parse_commands(b"\n0 this doesn't matter\n\n1 16 0 0 0 1 0 0 0 1 0 0 0 1 aa/aaaaddd"),
            vec![cmd0, cmd1]
        );

        let cmd0 = Command::Comment(CommentCmd::new("this doesn't \"matter\""));
        let cmd1 = Command::SubFileRef(SubFileRefCmd {
            color: 16,
            transform: Transform {
                pos: vec3(0.0, 0.0, 0.0),
                row0: vec3(1.0, 0.0, 0.0),
                row1: vec3(0.0, 1.0, 0.0),
                row2: vec3(0.0, 0.0, 1.0),
            },
            file: "aa/aaaaddd".to_string(),
        });
        assert_eq!(
            parse_commands(
                b"\r\n0 this doesn't \"matter\"\r\n\r\n1 16 0 0 0 1 0 0 0 1 0 0 0 1 aa/aaaaddd\n"
            ),
            vec![cmd0, cmd1]
        );

        let cmd0 = Command::SubFileRef(SubFileRefCmd {
            color: 16,
            transform: Transform {
                pos: vec3(0.0, 0.0, 0.0),
                row0: vec3(1.0, 0.0, 0.0),
                row1: vec3(0.0, 1.0, 0.0),
                row2: vec3(0.0, 0.0, 1.0),
            },
            file: "aa/aaaaddd".to_string(),
        });
        let cmd1 = Command::SubFileRef(SubFileRefCmd {
            color: 16,
            transform: Transform {
                pos: vec3(0.0, 0.0, 0.0),
                row0: vec3(1.0, 0.0, 0.0),
                row1: vec3(0.0, 1.0, 0.0),
                row2: vec3(0.0, 0.0, 1.0),
            },
            file: "aa/aaaaddd".to_string(),
        });
        assert_eq!(
            parse_commands(
                b"1 16 0 0 0 1 0 0 0 1 0 0 0 1 aa/aaaaddd\n1 16 0 0 0 1 0 0 0 1 0 0 0 1 aa/aaaaddd"
            ),
            vec![cmd0, cmd1]
        );
    }

    #[test]
    fn test_source_map_normalization() {
        let mut source_map = SourceMap::new();
        source_map.insert(
            LDrawPath::new("p\\part.dat"),
            SourceFile { cmds: Vec::new() },
        );
        assert!(source_map.get("p/part.DAT").is_some());

        source_map.insert(LDrawPath::new("TEST.LDR"), SourceFile { cmds: Vec::new() });
        assert!(source_map.get("test.LDR").is_some());

        source_map.insert(
            LDrawPath::new("a//b\\\\c//d.dat"),
            SourceFile { cmds: Vec::new() },
        );
        assert!(source_map.get("a/b/c/d.dat").is_some());
    }
}
