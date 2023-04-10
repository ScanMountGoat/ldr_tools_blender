use std::collections::HashMap;

use ldr_tools::StudType;
use numpy::IntoPyArray;
use pyo3::prelude::*;

#[pyclass(get_all)]
#[derive(Debug, Clone)]
pub struct LDrawNode {
    name: String,
    transform: [[f32; 4]; 4],
    geometry_name: Option<String>,
    current_color: u32,
    children: Vec<LDrawNode>,
}

impl From<ldr_tools::LDrawNode> for LDrawNode {
    fn from(node: ldr_tools::LDrawNode) -> Self {
        Self {
            name: node.name,
            transform: node.transform.to_cols_array_2d(),
            geometry_name: node.geometry_name,
            current_color: node.current_color,
            children: node.children.into_iter().map(|c| c.into()).collect(),
        }
    }
}

// Use numpy arrays for reduced overhead.
#[pyclass(get_all)]
#[derive(Debug, Clone)]
pub struct LDrawGeometry {
    vertices: PyObject,
    vertex_indices: PyObject,
    face_start_indices: PyObject,
    face_sizes: PyObject,
    face_colors: Vec<FaceColor>,
    edges: PyObject,
    is_edge_sharp: Vec<bool>,
}

impl LDrawGeometry {
    fn from_geometry(py: Python, geometry: ldr_tools::LDrawGeometry) -> Self {
        let vertex_count = geometry.positions.len();
        let edge_count = geometry.edge_position_indices.len();

        // This flatten will be optimized in Release mode.
        // This avoids needing unsafe code.
        Self {
            vertices: geometry
                .positions
                .into_iter()
                .flat_map(|v| [v.x, v.y, v.z])
                .collect::<Vec<f32>>()
                .into_pyarray(py)
                .reshape((vertex_count, 3))
                .unwrap()
                .into(),
            vertex_indices: geometry.position_indices.into_pyarray(py).into(),
            face_start_indices: geometry.face_start_indices.into_pyarray(py).into(),
            face_sizes: geometry.face_sizes.into_pyarray(py).into(),
            face_colors: geometry.face_colors.into_iter().map(Into::into).collect(),
            edges: geometry
                .edge_position_indices
                .into_iter()
                .flatten()
                .collect::<Vec<u32>>()
                .into_pyarray(py)
                .reshape((edge_count, 2))
                .unwrap()
                .into(),
            is_edge_sharp: geometry.is_edge_sharp,
        }
    }
}

#[pyclass(get_all)]
#[derive(Debug, Clone)]
pub struct FaceColor {
    color: u32,
    is_grainy_slope: bool,
}

impl From<ldr_tools::FaceColor> for FaceColor {
    fn from(f: ldr_tools::FaceColor) -> Self {
        Self {
            color: f.color,
            is_grainy_slope: f.is_grainy_slope,
        }
    }
}

#[pyclass(get_all)]
#[derive(Debug, Clone)]
pub struct LDrawColor {
    name: String,
    finish_name: String,
    rgba_linear: [f32; 4],
    speckle_rgba_linear: Option<[f32; 4]>,
}

impl From<ldr_tools::LDrawColor> for LDrawColor {
    fn from(c: ldr_tools::LDrawColor) -> Self {
        Self {
            name: c.name,
            rgba_linear: c.rgba_linear,
            finish_name: c.finish_name,
            speckle_rgba_linear: c.speckle_rgba_linear,
        }
    }
}

// TODO: make a proper enum for the stud type.
#[pyclass(get_all, set_all)]
#[derive(Debug, Clone)]
pub struct GeometrySettings {
    triangulate: bool,
    add_gap_between_parts: bool,
    logo_on_studs: bool,
    weld_vertices: bool,
}

#[pymethods]
impl GeometrySettings {
    #[new]
    fn new() -> Self {
        ldr_tools::GeometrySettings::default().into()
    }
}

impl From<ldr_tools::GeometrySettings> for GeometrySettings {
    fn from(value: ldr_tools::GeometrySettings) -> Self {
        Self {
            triangulate: value.triangulate,
            add_gap_between_parts: value.add_gap_between_parts,
            logo_on_studs: value.stud_type == StudType::Logo4,
            weld_vertices: value.weld_vertices,
        }
    }
}

impl From<&GeometrySettings> for ldr_tools::GeometrySettings {
    fn from(value: &GeometrySettings) -> Self {
        Self {
            triangulate: value.triangulate,
            add_gap_between_parts: value.add_gap_between_parts,
            stud_type: if value.logo_on_studs {
                StudType::Logo4
            } else {
                StudType::Normal
            },
            weld_vertices: value.weld_vertices,
            primitive_resolution: ldr_tools::PrimitiveResolution::Normal,
        }
    }
}

// TODO: Is it worth creating the scene structs here as well?
#[pyfunction]
fn load_file(
    py: Python,
    path: &str,
    ldraw_path: &str,
    settings: &GeometrySettings,
) -> PyResult<(LDrawNode, HashMap<String, LDrawGeometry>)> {
    // TODO: This timing code doesn't need to be here.
    let start = std::time::Instant::now();
    let scene = ldr_tools::load_file(path, ldraw_path, &settings.into());

    let geometry_cache_py = scene
        .geometry_cache
        .into_iter()
        .map(|(k, v)| (k, LDrawGeometry::from_geometry(py, v)))
        .collect();
    println!("load_file: {:?}", start.elapsed());
    Ok((scene.root_node.into(), geometry_cache_py))
}

#[pyfunction]
fn load_file_instanced(
    py: Python,
    path: &str,
    ldraw_path: &str,
    settings: &GeometrySettings,
) -> PyResult<(
    HashMap<String, LDrawGeometry>,
    HashMap<(String, u32), PyObject>,
)> {
    let start = std::time::Instant::now();
    let scene = ldr_tools::load_file_instanced(path, ldraw_path, &settings.into());

    let geometry_cache_py = scene
        .geometry_cache
        .into_iter()
        .map(|(k, v)| (k, LDrawGeometry::from_geometry(py, v)))
        .collect();

    let geometry_world_transforms_py = scene
        .geometry_world_transforms
        .into_iter()
        .map(|(k, v)| {
            // Create a single numpy array of transforms for each geometry.
            // This means Python code can avoid overhead from for loops.
            // This flatten will be optimized in Release mode.
            // This avoids needing unsafe code.
            let transform_count = v.len();
            let transforms = v
                .into_iter()
                .flat_map(|v| v.to_cols_array())
                .collect::<Vec<f32>>()
                .into_pyarray(py)
                .reshape((transform_count, 4, 4))
                .unwrap()
                .into();

            (k, transforms)
        })
        .collect();

    println!("load_file_instanced: {:?}", start.elapsed());

    Ok((geometry_cache_py, geometry_world_transforms_py))
}

#[pyfunction]
fn load_file_instanced_faces(
    py: Python,
    path: &str,
    ldraw_path: &str,
    settings: &GeometrySettings,
) -> PyResult<(
    HashMap<String, LDrawGeometry>,
    HashMap<(String, u32), PyObject>,
)> {
    let start = std::time::Instant::now();
    let scene = ldr_tools::load_file_instanced_faces(path, ldraw_path, &settings.into());

    let geometry_cache_py = scene
        .geometry_cache
        .into_iter()
        .map(|(k, v)| (k, LDrawGeometry::from_geometry(py, v)))
        .collect();

    let geometry_world_transforms_py = scene
        .geometry_face_instances
        .into_iter()
        .map(|(k, v)| {
            // Create a single numpy array of vertices for each part.
            // This means Python code can avoid overhead from for loops.
            // This flatten will be optimized in Release mode.
            // This avoids needing unsafe code.
            let vertex_count = v.len() * 4;
            let transforms = v
                .into_iter()
                .flat_map(|[v0, v1, v2, v3]| {
                    [v0.to_array(), v1.to_array(), v2.to_array(), v3.to_array()]
                })
                .flatten()
                .collect::<Vec<f32>>()
                .into_pyarray(py)
                .reshape((vertex_count, 3))
                .unwrap()
                .into();

            (k, transforms)
        })
        .collect();

    println!("load_file_instanced_faces: {:?}", start.elapsed());

    Ok((geometry_cache_py, geometry_world_transforms_py))
}

#[pyfunction]
fn load_color_table(ldraw_path: &str) -> PyResult<HashMap<u32, LDrawColor>> {
    Ok(ldr_tools::load_color_table(ldraw_path)
        .into_iter()
        .map(|(k, v)| (k, v.into()))
        .collect())
}

#[pymodule]
fn ldr_tools_py(_py: Python<'_>, m: &PyModule) -> PyResult<()> {
    m.add_class::<LDrawNode>()?;
    m.add_class::<LDrawGeometry>()?;
    m.add_class::<FaceColor>()?;
    m.add_class::<LDrawColor>()?;
    m.add_class::<GeometrySettings>()?;

    m.add_function(wrap_pyfunction!(load_file, m)?)?;
    m.add_function(wrap_pyfunction!(load_file_instanced, m)?)?;
    m.add_function(wrap_pyfunction!(load_file_instanced_faces, m)?)?;
    m.add_function(wrap_pyfunction!(load_color_table, m)?)?;

    Ok(())
}
