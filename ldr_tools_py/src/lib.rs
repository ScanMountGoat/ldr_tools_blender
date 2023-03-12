use std::collections::HashMap;

use numpy::IntoPyArray;
use pyo3::prelude::*;

// TODO: Is it worth supporting mutability in lists using PyList?
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
    face_colors: PyObject,
}

impl LDrawGeometry {
    fn from_geometry(py: Python, geometry: ldr_tools::LDrawGeometry) -> Self {
        let vertex_count = geometry.vertices.len();

        // This flatten will be optimized in Release mode.
        // This avoids needing unsafe code.
        Self {
            vertices: geometry
                .vertices
                .into_iter()
                .flat_map(|v| [v.x, v.y, v.z])
                .collect::<Vec<f32>>()
                .into_pyarray(py)
                .reshape((vertex_count, 3))
                .unwrap()
                .into(),
            vertex_indices: geometry.vertex_indices.into_pyarray(py).into(),
            face_start_indices: geometry.face_start_indices.into_pyarray(py).into(),
            face_sizes: geometry.face_sizes.into_pyarray(py).into(),
            face_colors: geometry.face_colors.into_pyarray(py).into(),
        }
    }
}

#[pyclass(get_all)]
#[derive(Debug, Clone)]
pub struct LDrawColor {
    name: String,
    rgba_linear: [f32; 4],
    finish_name: String,
}

impl From<ldr_tools::LDrawColor> for LDrawColor {
    fn from(c: ldr_tools::LDrawColor) -> Self {
        Self {
            name: c.name,
            rgba_linear: c.rgba_linear,
            finish_name: c.finish_name,
        }
    }
}

// TODO: Is it worth creating the scene structs here as well?
#[pyfunction]
fn load_file(py: Python, path: &str) -> PyResult<(LDrawNode, HashMap<String, LDrawGeometry>)> {
    // TODO: This timing code doesn't need to be here.
    let start = std::time::Instant::now();
    let scene = ldr_tools::load_file(path);

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
) -> PyResult<(
    HashMap<String, LDrawGeometry>,
    HashMap<(String, u32), PyObject>,
)> {
    let start = std::time::Instant::now();
    let scene = ldr_tools::load_file_instanced(path);

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
fn load_color_table() -> PyResult<HashMap<u32, LDrawColor>> {
    Ok(ldr_tools::load_color_table()
        .into_iter()
        .map(|(k, v)| (k, v.into()))
        .collect())
}

#[pymodule]
fn ldr_tools_py(_py: Python<'_>, m: &PyModule) -> PyResult<()> {
    m.add_class::<LDrawNode>()?;
    m.add_class::<LDrawGeometry>()?;
    m.add_class::<LDrawColor>()?;

    m.add_function(wrap_pyfunction!(load_file, m)?)?;
    m.add_function(wrap_pyfunction!(load_file_instanced, m)?)?;
    m.add_function(wrap_pyfunction!(load_color_table, m)?)?;

    Ok(())
}
