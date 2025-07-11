//! LDraw file format and parser.

// The LDraw representation and parser are based on work done for [weldr](https://github.com/djeedai/weldr).
use std::{collections::HashMap, path::Path, str};

pub use glam::{Mat4, Vec2, Vec3, Vec4};

pub mod error;

mod parse;

pub use error::{Error, ResolveError};
use log::{debug, trace};

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

/// Parse raw LDR content without sub-file resolution.
///
/// Parse the given LDR data passed in `ldr_content` and return the list of parsed commands.
/// Sub-file references (Line Type 1) are not resolved, and returned as [`Command::SubFileRef`].
///
/// The input LDR content must comply to the LDraw standard. In particular this means:
/// - UTF-8 encoded, without Byte Order Mark (BOM)
/// - Both DOS/Windows `<CR><LF>` and Unix `<LF>` line termination accepted
///
/// ```rust
/// use ldr_tools::ldraw::{parse_raw, Command, CommentCmd, LineCmd, Vec3};
///
/// let cmd0 = Command::Comment(CommentCmd::new("this is a comment"));
/// let cmd1 = Command::Line(LineCmd{
///   color: 16,
///   vertices: [
///     Vec3{ x: 0.0, y: 0.0, z: 0.0 },
///     Vec3{ x: 1.0, y: 1.0, z: 1.0 }
///   ]
/// });
/// assert_eq!(parse_raw(b"0 this is a comment\n2 16 0 0 0 1 1 1").unwrap(), vec![cmd0, cmd1]);
/// ```
pub fn parse_raw(ldr_content: &[u8]) -> Result<Vec<Command>, Error> {
    parse::parse_raw(ldr_content)
}

struct FileRef {
    /// Filename of unresolved source file.
    filename: String,
}

fn load_and_parse_single_file<P: AsRef<Path>, R: FileRefResolver>(
    filename: P,
    resolver: &R,
) -> Result<SourceFile, Error> {
    let raw_content = resolver.resolve(filename)?;
    let cmds = parse::parse_raw(&raw_content)?;
    Ok(SourceFile { cmds })
}

/// Parse a single file and its sub-file references recursively.
///
/// Attempt to load the content of `path` via the given `resolver`, and parse it.
/// Then recursively look for sub-file commands inside that root file, and try to resolve
/// the content of those sub-files and parse them too. All the loaded and parsed files end
/// up populating the given `source_map`, which can be pre-populated manually or from a
/// previous call with already loaded and parsed files.
/// ```rust
/// use ldr_tools::ldraw::{ FileRefResolver, parse, ResolveError, SourceMap };
///
/// struct MyCustomResolver;
///
/// impl FileRefResolver for MyCustomResolver {
///   fn resolve<P: AsRef<std::path::Path>>(&self, filename: P) -> Result<Vec<u8>, ResolveError> {
///     Ok(vec![]) // replace with custom impl
///   }
/// }
///
/// fn main() -> Result<(), Box<dyn std::error::Error>> {
///   let resolver = MyCustomResolver{};
///   let mut source_map = SourceMap::new();
///   let main_model_name = parse("root.ldr", &resolver, &mut source_map)?;
///   let root_file = source_map.get(&main_model_name).unwrap();
///   Ok(())
/// }
/// ```
pub fn parse<P: AsRef<Path>, R: FileRefResolver>(
    path: P,
    resolver: &R,
    source_map: &mut SourceMap,
) -> Result<String, Error> {
    // Use a stack to avoid function recursion in load_file.
    let mut stack: Vec<FileRef> = Vec::new();

    debug!("Processing root file '{:?}'", path.as_ref());
    // The provided path should refer to a file from the resolver.
    // Use the path directly without any normalization.
    let filename = path.as_ref().to_string_lossy().to_string();
    let actual_root = load_file(path, &filename, resolver, source_map, &mut stack)?;

    // Recursively load files referenced by the root file.
    while let Some(file) = stack.pop() {
        let filename = &file.filename;
        debug!("Processing sub-file: '{filename}'");
        match source_map.get(filename) {
            Some(_) => trace!("Already parsed; reusing sub-file: {filename}"),
            None => {
                trace!("Not yet parsed; parsing sub-file: {filename}");
                // Normalize file references to subfiles.
                let subfile_ref = SubFileRef::new(filename);
                load_subfile(subfile_ref, resolver, source_map, &mut stack)?;
            }
        }
    }

    Ok(actual_root)
}

fn load_file<P: AsRef<Path>, R: FileRefResolver>(
    path: P,
    filename: &str,
    resolver: &R,
    source_map: &mut SourceMap,
    stack: &mut Vec<FileRef>,
) -> Result<String, Error> {
    let source_file = load_and_parse_single_file(path, resolver)?;
    source_map.queue_subfiles(&source_file, stack);
    Ok(source_map.insert(filename, source_file))
}

fn load_subfile<R: FileRefResolver>(
    filename: SubFileRef,
    resolver: &R,
    source_map: &mut SourceMap,
    stack: &mut Vec<FileRef>,
) -> Result<String, Error> {
    load_file(&filename.0, &filename.0, resolver, source_map, stack)
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

#[derive(Debug, PartialEq, Eq, Hash)]
struct SubFileRef(String);

impl SubFileRef {
    fn new(s: &str) -> Self {
        Self(normalize_subfile_reference(s))
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
    source_files: HashMap<SubFileRef, SourceFile>,
}

impl SourceMap {
    /// Construct a new empty source map.
    pub fn new() -> Self {
        Self {
            source_files: HashMap::new(),
        }
    }

    /// Returns a reference to the source file corresponding to `filename`.
    pub fn get(&self, filename: &str) -> Option<&SourceFile> {
        // TODO: handle normalization and case sensitivity.
        self.source_files.get(&SubFileRef::new(filename))
    }

    /// Returns a mutable reference to the source file corresponding to `filename`.
    pub fn get_mut(&mut self, filename: &str) -> Option<&mut SourceFile> {
        self.source_files.get_mut(&SubFileRef::new(filename))
    }

    /// Inserts a new source file into the collection.
    /// Returns a copy of the filename of `source_file`
    /// or the filename of the main file for multi-part documents (MPD).
    pub fn insert(&mut self, filename: &str, source_file: SourceFile) -> String {
        // The MPD extension allows .ldr or .mpd files to contain multiple files.
        // Add each of these so that they can be resolved by subfile commands later.
        let files = split_mpd_file(&source_file.cmds);

        // Some files are referenced in their entirety even if they have multiple models.
        self.source_files
            .insert(SubFileRef::new(filename), source_file);

        // TODO: More cleanly handle the fact that not all files have 0 FILE commands.
        if files.is_empty() {
            filename.to_string()
        } else {
            // The first block is the "main model" of the file.
            let main_model_name = files[0].0.clone();
            for (name, file) in files {
                self.source_files.insert(SubFileRef::new(&name), file);
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
    fn resolve<P: AsRef<Path>>(&self, filename: P) -> Result<Vec<u8>, ResolveError>;
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

    #[test]
    fn test_split_mpd_files() {
        let commands = vec![
            Command::File(FileCmd {
                file: "a".to_string(),
            }),
            Command::SubFileRef(SubFileRefCmd {
                color: 16,
                transform: Transform {
                    pos: Vec3::new(0.0, 0.0, 0.0),
                    row0: Vec3::new(1.0, 0.0, 0.0),
                    row1: Vec3::new(0.0, 1.0, 0.0),
                    row2: Vec3::new(0.0, 0.0, 1.0),
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
                    pos: Vec3::new(0.0, 0.0, 0.0),
                    row0: Vec3::new(1.0, 0.0, 0.0),
                    row1: Vec3::new(0.0, 1.0, 0.0),
                    row2: Vec3::new(0.0, 0.0, 1.0),
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
                    pos: Vec3::new(0.0, 0.0, 0.0),
                    row0: Vec3::new(1.0, 0.0, 0.0),
                    row1: Vec3::new(0.0, 1.0, 0.0),
                    row2: Vec3::new(0.0, 0.0, 1.0),
                },
                file: "1.dat".to_string(),
            }),
            Command::File(FileCmd {
                file: "b".to_string(),
            }),
            Command::SubFileRef(SubFileRefCmd {
                color: 16,
                transform: Transform {
                    pos: Vec3::new(0.0, 0.0, 0.0),
                    row0: Vec3::new(1.0, 0.0, 0.0),
                    row1: Vec3::new(0.0, 1.0, 0.0),
                    row2: Vec3::new(0.0, 0.0, 1.0),
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
    fn test_parse_raw_mpd() {
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

        let commands = parse_raw(ldr_contents).unwrap();
        // TODO: Check the actual commands.
        assert_eq!(28, commands.len());
    }

    #[test]
    fn test_parse_raw() {
        let cmd0 = Command::Comment(CommentCmd::new("this is a comment"));
        let cmd1 = Command::Line(LineCmd {
            color: 16,
            vertices: [Vec3::new(0.0, 0.0, 0.0), Vec3::new(1.0, 1.0, 1.0)],
        });
        assert_eq!(
            parse_raw(b"0 this is a comment\n2 16 0 0 0 1 1 1").unwrap(),
            vec![cmd0, cmd1]
        );

        let cmd0 = Command::Comment(CommentCmd::new("this doesn't matter"));
        let cmd1 = Command::SubFileRef(SubFileRefCmd {
            color: 16,
            transform: Transform {
                pos: Vec3::new(0.0, 0.0, 0.0),
                row0: Vec3::new(1.0, 0.0, 0.0),
                row1: Vec3::new(0.0, 1.0, 0.0),
                row2: Vec3::new(0.0, 0.0, 1.0),
            },
            file: "aa/aaaaddd".to_string(),
        });
        assert_eq!(
            parse_raw(b"\n0 this doesn't matter\n\n1 16 0 0 0 1 0 0 0 1 0 0 0 1 aa/aaaaddd")
                .unwrap(),
            vec![cmd0, cmd1]
        );

        let cmd0 = Command::Comment(CommentCmd::new("this doesn't \"matter\""));
        let cmd1 = Command::SubFileRef(SubFileRefCmd {
            color: 16,
            transform: Transform {
                pos: Vec3::new(0.0, 0.0, 0.0),
                row0: Vec3::new(1.0, 0.0, 0.0),
                row1: Vec3::new(0.0, 1.0, 0.0),
                row2: Vec3::new(0.0, 0.0, 1.0),
            },
            file: "aa/aaaaddd".to_string(),
        });
        assert_eq!(
            parse_raw(
                b"\r\n0 this doesn't \"matter\"\r\n\r\n1 16 0 0 0 1 0 0 0 1 0 0 0 1 aa/aaaaddd\n"
            )
            .unwrap(),
            vec![cmd0, cmd1]
        );

        let cmd0 = Command::SubFileRef(SubFileRefCmd {
            color: 16,
            transform: Transform {
                pos: Vec3::new(0.0, 0.0, 0.0),
                row0: Vec3::new(1.0, 0.0, 0.0),
                row1: Vec3::new(0.0, 1.0, 0.0),
                row2: Vec3::new(0.0, 0.0, 1.0),
            },
            file: "aa/aaaaddd".to_string(),
        });
        let cmd1 = Command::SubFileRef(SubFileRefCmd {
            color: 16,
            transform: Transform {
                pos: Vec3::new(0.0, 0.0, 0.0),
                row0: Vec3::new(1.0, 0.0, 0.0),
                row1: Vec3::new(0.0, 1.0, 0.0),
                row2: Vec3::new(0.0, 0.0, 1.0),
            },
            file: "aa/aaaaddd".to_string(),
        });
        assert_eq!(
            parse_raw(
                b"1 16 0 0 0 1 0 0 0 1 0 0 0 1 aa/aaaaddd\n1 16 0 0 0 1 0 0 0 1 0 0 0 1 aa/aaaaddd"
            )
            .unwrap(),
            vec![cmd0, cmd1]
        );
    }

    #[test]
    fn test_source_map_normalization() {
        let mut source_map = SourceMap::new();
        source_map.insert("p\\part.dat", SourceFile { cmds: Vec::new() });
        assert!(source_map.get("p/part.DAT").is_some());

        source_map.insert("TEST.LDR", SourceFile { cmds: Vec::new() });
        assert!(source_map.get("test.LDR").is_some());

        source_map.insert("a//b\\\\c//d.dat", SourceFile { cmds: Vec::new() });
        assert!(source_map.get("a/b/c/d.dat").is_some());
    }
}
