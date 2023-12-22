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

#[pyclass(get_all)]
#[derive(Debug, Clone)]
pub struct LDrawScene {
    pub root_node: LDrawNode,
    pub geometry_cache: HashMap<String, LDrawGeometry>,
}

#[pyclass(get_all)]
#[derive(Debug, Clone)]
pub struct LDrawSceneInstanced {
    pub main_model_name: String,
    pub geometry_world_transforms: HashMap<(String, u32), PyObject>,
    pub geometry_cache: HashMap<String, LDrawGeometry>,
}

#[pyclass(get_all)]
#[derive(Debug, Clone)]
pub struct LDrawSceneInstancedPoints {
    pub main_model_name: String,
    pub geometry_point_instances: HashMap<(String, u32), PointInstances>,
    pub geometry_cache: HashMap<String, LDrawGeometry>,
}

// Use numpy arrays (PyObject) for reduced overhead.
#[pyclass(get_all)]
#[derive(Debug, Clone)]
pub struct LDrawGeometry {
    vertices: PyObject,
    vertex_indices: PyObject,
    face_start_indices: PyObject,
    face_sizes: PyObject,
    face_colors: PyObject,
    is_face_stud: Vec<bool>,
    edge_line_indices: PyObject,
    has_grainy_slopes: bool,
}

impl LDrawGeometry {
    fn from_geometry(py: Python, geometry: ldr_tools::LDrawGeometry) -> Self {
        let sharp_edge_count = geometry.edge_line_indices.len();

        // This flatten will be optimized in Release mode.
        // This avoids needing unsafe code.
        Self {
            vertices: pyarray_vec3(py, geometry.vertices),
            vertex_indices: geometry.vertex_indices.into_pyarray(py).into(),
            face_start_indices: geometry.face_start_indices.into_pyarray(py).into(),
            face_sizes: geometry.face_sizes.into_pyarray(py).into(),
            face_colors: geometry.face_colors.into_pyarray(py).into(),
            is_face_stud: geometry.is_face_stud,
            edge_line_indices: geometry
                .edge_line_indices
                .into_iter()
                .flatten()
                .collect::<Vec<u32>>()
                .into_pyarray(py)
                .reshape((sharp_edge_count, 2))
                .unwrap()
                .into(),
            has_grainy_slopes: geometry.has_grainy_slopes,
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
    scene_scale: f32,
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
            scene_scale: value.scene_scale,
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
            scene_scale: value.scene_scale,
        }
    }
}

#[pyclass(get_all, set_all)]
#[derive(Debug, Clone)]
pub struct PointInstances {
    translations: PyObject,
    rotations_axis: PyObject,
    rotations_angle: PyObject,
    scales: PyObject,
}

impl PointInstances {
    fn from_instances(py: Python, instances: ldr_tools::PointInstances) -> Self {
        Self {
            translations: pyarray_vec3(py, instances.translations),
            rotations_axis: pyarray_vec3(py, instances.rotations_axis),
            rotations_angle: instances.rotations_angle.into_pyarray(py).into(),
            scales: pyarray_vec3(py, instances.scales),
        }
    }
}

#[pyfunction]
fn load_file(
    py: Python,
    path: &str,
    ldraw_path: &str,
    additional_paths: Vec<&str>,
    settings: &GeometrySettings,
) -> PyResult<LDrawScene> {
    // TODO: This timing code doesn't need to be here.
    let start = std::time::Instant::now();
    let scene = ldr_tools::load_file(path, ldraw_path, &additional_paths, &settings.into());

    let geometry_cache = scene
        .geometry_cache
        .into_iter()
        .map(|(k, v)| (k, LDrawGeometry::from_geometry(py, v)))
        .collect();
    println!("load_file: {:?}", start.elapsed());

    Ok(LDrawScene {
        root_node: scene.root_node.into(),
        geometry_cache,
    })
}

#[pyfunction]
fn load_file_instanced(
    py: Python,
    path: &str,
    ldraw_path: &str,
    additional_paths: Vec<&str>,
    settings: &GeometrySettings,
) -> PyResult<LDrawSceneInstanced> {
    let start = std::time::Instant::now();
    let scene =
        ldr_tools::load_file_instanced(path, ldraw_path, &additional_paths, &settings.into());

    let geometry_cache = scene
        .geometry_cache
        .into_iter()
        .map(|(k, v)| (k, LDrawGeometry::from_geometry(py, v)))
        .collect();

    let geometry_world_transforms = scene
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

    Ok(LDrawSceneInstanced {
        main_model_name: scene.main_model_name,
        geometry_world_transforms,
        geometry_cache,
    })
}

#[pyfunction]
fn load_file_instanced_points(
    py: Python,
    path: &str,
    ldraw_path: &str,
    additional_paths: Vec<&str>,
    settings: &GeometrySettings,
) -> PyResult<LDrawSceneInstancedPoints> {
    let start = std::time::Instant::now();
    let scene = ldr_tools::load_file_instanced_points(
        path,
        ldraw_path,
        &additional_paths,
        &settings.into(),
    );

    let geometry_cache = scene
        .geometry_cache
        .into_iter()
        .map(|(k, v)| (k, LDrawGeometry::from_geometry(py, v)))
        .collect();

    let geometry_point_instances = scene
        .geometry_point_instances
        .into_iter()
        .map(|(k, v)| (k, PointInstances::from_instances(py, v)))
        .collect();

    println!("load_file_instanced_points: {:?}", start.elapsed());

    Ok(LDrawSceneInstancedPoints {
        main_model_name: scene.main_model_name,
        geometry_point_instances,
        geometry_cache,
    })
}

#[pyfunction]
fn load_color_table(ldraw_path: &str) -> PyResult<HashMap<u32, LDrawColor>> {
    Ok(ldr_tools::load_color_table(ldraw_path)
        .into_iter()
        .map(|(k, v)| (k, v.into()))
        .collect())
}

fn pyarray_vec3(py: Python, values: Vec<ldr_tools::glam::Vec3>) -> PyObject {
    // This flatten will be optimized in Release mode.
    // This avoids needing unsafe code.
    let count = values.len();
    values
        .into_iter()
        .flat_map(|v| [v.x, v.y, v.z])
        .collect::<Vec<f32>>()
        .into_pyarray(py)
        .reshape((count, 3))
        .unwrap()
        .into()
}

#[pymodule]
fn ldr_tools_py(_py: Python<'_>, m: &PyModule) -> PyResult<()> {
    m.add_class::<LDrawNode>()?;
    m.add_class::<LDrawGeometry>()?;
    m.add_class::<LDrawColor>()?;
    m.add_class::<GeometrySettings>()?;
    m.add_class::<PointInstances>()?;

    m.add_function(wrap_pyfunction!(load_file, m)?)?;
    m.add_function(wrap_pyfunction!(load_file_instanced, m)?)?;
    m.add_function(wrap_pyfunction!(load_file_instanced_points, m)?)?;
    m.add_function(wrap_pyfunction!(load_color_table, m)?)?;

    Ok(())
}
