// Reverse engineered from C# DLLs for the Unity app for Bricklink Studio.

use crate::LDrawGeometry;
use base64::prelude::*;
use glam::{Mat4, Vec2, Vec3, Vec3Swizzles};

#[derive(Debug, PartialEq)]
pub struct LDrawTextureInfo {
    /// PNG-encoded images from PE_TEX_INFO commands.
    pub textures: Vec<Vec<u8>>,
    /// Per-face indices into `textures`. 0xFF indicates no texture for the face.
    /// Eight-bit indices save memory, especially for the untextured majority of parts.
    pub indices: Vec<u8>,
    /// Per-vertex UV coordinates for the entire mesh, even non-textured faces.
    pub uvs: Vec<Vec2>,
}

impl LDrawTextureInfo {
    pub fn new(num_faces: usize, num_vertices: usize) -> Self {
        // "Catch up" with the mesh that we had optimistically assumed would have no textures
        // by filling in the arrays "up to this point" with sentinel/placeholder data.
        Self {
            textures: vec![],
            indices: vec![u8::MAX; num_faces],
            uvs: vec![Vec2::ZERO; num_vertices],
        }
    }
}

fn init_texture_transform(texture_matrix: Mat4, part_matrix: Mat4) -> (Mat4, Vec3) {
    let (scale, rot, tr) = (part_matrix * texture_matrix).to_scale_rotation_translation();
    let mut mirroring = scale.signum();
    mirroring.z *= -1.0;
    let box_extents = scale.abs() / 2.0;
    let rhs = Mat4::from_scale_rotation_translation(mirroring, rot, tr);
    let matrix = part_matrix.inverse() * rhs;
    (matrix, box_extents)
}

pub fn project_texture<const N: usize>(
    texture: &PendingStudioTexture,
    transform: Mat4,
    vertices: [Vec3; N],
    uvs: Option<[Vec2; N]>,
) -> Option<TextureMap<N>> {
    let texture_index = texture.index;

    if let Some(uvs) = uvs {
        return Some(TextureMap { texture_index, uvs });
    }

    // if there are neither vertex UVs on the face
    // nor a projection matrix on the texture,
    // then the texture is not drawn on this face
    let tex_location = texture.location?;

    let (matrix, box_extents) = init_texture_transform(tex_location.transform, transform);
    let inverse = matrix.inverse();
    let vertices = vertices.map(|v| inverse.transform_point3(v));

    if !intersect_poly_box(&vertices, box_extents) {
        return None;
    }

    let min = tex_location.point_min;
    let diff = tex_location.point_max - tex_location.point_min;

    let uvs = vertices.map(|v| (v.xz() - min) / diff);
    Some(TextureMap { texture_index, uvs })
}

#[derive(Clone)]
pub struct PendingStudioTexture {
    pub index: u8,
    pub location: Option<TextureLocation>,
    pub path: Vec<i32>,
}

#[derive(Copy, Clone)]
pub struct TextureLocation {
    pub transform: Mat4,
    pub point_min: Vec2,
    pub point_max: Vec2,
}

#[derive(Debug, PartialEq)]
pub struct TextureMap<const N: usize> {
    pub texture_index: u8,
    pub uvs: [Vec2; N],
}

impl PendingStudioTexture {
    // TODO: the images probably need names based on their file of origin
    pub fn parse(line: &str, path: &[i32], geometry: &mut LDrawGeometry) -> Option<Self> {
        let words = line.split_whitespace().collect::<Vec<_>>();
        if words.first() != Some(&"PE_TEX_INFO") {
            return None;
        }

        let image: &str;
        let mut location = None::<TextureLocation>;
        if let Some((cells, [img])) = words[1..].split_at_checked(16) {
            let mut iter = cells.iter().filter_map(|c| c.parse::<f32>().ok());
            let [x, y, z, a, b, c, d, e, f, g, h, i] = next_array(&mut iter)?;
            let transform = Mat4::from_cols_array_2d(&[
                [a, d, g, 0.0],
                [b, e, h, 0.0],
                [c, f, i, 0.0],
                [x, y, z, 1.0],
            ]);

            location = Some(TextureLocation {
                transform,
                point_min: Vec2::from(next_array(&mut iter)?),
                point_max: Vec2::from(next_array(&mut iter)?),
            });

            image = img;
        } else if words.len() == 2 {
            image = words[1];
        } else {
            return None;
        }

        let image = BASE64_STANDARD.decode(image).ok()?;

        // Avoid lazily initializing the texture info until everything else has succeeded.
        let tex_info = geometry.texture_info();

        if tex_info.textures.len() >= u8::MAX as usize {
            // Why would a single part ever have 256 or more different textures?
            eprintln!("Texture limit exceeded!");
            return None;
        }

        let index = tex_info.textures.len() as u8;
        tex_info.textures.push(image);
        let path = path.to_owned();
        Some(Self {
            index,
            location,
            path,
        })
    }
}

fn intersect_poly_box(polygon: &[Vec3], r: Vec3) -> bool {
    match *polygon {
        [a, b, c] => intersect_tri_box([a, b, c], r),
        [a, b, c, d] => intersect_tri_box([a, b, c], r) || intersect_tri_box([c, d, a], r),
        _ => unimplemented!(),
    }
}

fn intersect_tri_box(triangle: [Vec3; 3], box_extents: Vec3) -> bool {
    let edges = {
        let [a, b, c] = triangle;
        [b - a, c - b, a - c]
    };

    let normal = edges[0].cross(edges[1]);

    // AABB triangle intersection using Separating Axis Theorem (SAT).
    // TODO: Find a clearer way to write this.
    let be = box_extents;
    for e in edges {
        for (rhs, num) in [
            ((0.0, -e.z, e.y).into(), be.y * e.z.abs() + be.z * e.y.abs()),
            ((e.z, 0.0, -e.x).into(), be.x * e.z.abs() + be.z * e.x.abs()),
            ((-e.y, e.x, 0.0).into(), be.x * e.y.abs() + be.y * e.x.abs()),
        ] {
            let dot_products = triangle.map(|v| v.dot(rhs));
            let (min, max) = min_max(&dot_products);
            if f32::max(-max, min) > num {
                return false;
            }
        }
    }

    for dim in 0..3 {
        let coords = triangle.map(|v| v[dim]);
        let (min, max) = min_max(&coords);
        if max < -box_extents[dim] || min > box_extents[dim] {
            return false;
        }
    }

    normal.dot(triangle[0]) <= normal.abs().dot(box_extents)
}

fn min_max(values: &[f32]) -> (f32, f32) {
    let (mut min, mut max) = (f32::MAX, f32::MIN);
    for &n in values {
        min = min.min(n);
        max = max.max(n);
    }
    (min, max)
}

fn next_array<T: Default, const N: usize>(mut iter: impl Iterator<Item = T>) -> Option<[T; N]> {
    let mut arr = std::array::from_fn(|_| T::default());
    for elem in &mut arr {
        *elem = iter.next()?;
    }
    Some(arr)
}
