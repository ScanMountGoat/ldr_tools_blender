use pyo3::prelude::*;

macro_rules! python_enum {
    ($py_ty:ident, $rust_ty:ty, $( $i:ident ),+) => {
        #[pyclass(eq, eq_int)]
        #[derive(Debug, Clone, Copy, PartialEq, Eq)]
        pub enum $py_ty {
            $($i),*
        }

        // These will generate a compile error if variant names don't match.
        impl From<$rust_ty> for $py_ty {
            fn from(value: $rust_ty) -> Self {
                match value {
                    $(<$rust_ty>::$i => Self::$i),*
                }
            }
        }

        impl From<$py_ty> for $rust_ty {
            fn from(value: $py_ty) -> Self {
                match value {
                    $(<$py_ty>::$i => Self::$i),*
                }
            }
        }

        impl ::map_py::MapPy<$rust_ty> for $py_ty {
            fn map_py(self, _py: Python) -> PyResult<$rust_ty> {
                Ok(self.into())
            }
        }

        impl ::map_py::MapPy<$py_ty> for $rust_ty {
            fn map_py(self, _py: Python) -> PyResult<$py_ty> {
                Ok(self.into())
            }
        }
    };
}

python_enum!(
    StudType,
    ldr_tools::StudType,
    Disabled,
    Normal,
    Logo4,
    HighContrast
);

python_enum!(
    PrimitiveResolution,
    ldr_tools::PrimitiveResolution,
    Low,
    Normal,
    High
);

#[pymodule]
mod ldr_tools_py {
    use super::*;

    use std::collections::HashMap;

    use log::info;
    use map_py::{MapPy, TypedList};
    use numpy::{PyArray1, PyArray2, PyArray3};

    #[pymodule_init]
    fn init(_m: &Bound<'_, PyModule>) -> PyResult<()> {
        pyo3_log::init();
        Ok(())
    }

    #[pymodule_export]
    use super::StudType;

    #[pymodule_export]
    use super::PrimitiveResolution;

    #[pyclass(get_all)]
    #[derive(Debug, Clone, MapPy)]
    #[map(ldr_tools::LDrawNode)]
    pub struct LDrawNode {
        name: String,
        transform: Py<PyArray2<f32>>,
        geometry_name: Option<String>,
        current_color: u32,
        children: TypedList<LDrawNode>,
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
        pub geometry_world_transforms: HashMap<(String, u32), Py<PyArray3<f32>>>,
        pub geometry_cache: HashMap<String, LDrawGeometry>,
    }

    #[pyclass(get_all)]
    #[derive(Debug, Clone)]
    pub struct LDrawSceneInstancedPoints {
        pub main_model_name: String,
        pub geometry_point_instances: HashMap<(String, u32), PointInstances>,
        pub geometry_cache: HashMap<String, LDrawGeometry>,
    }

    // Use numpy arrays for reduced overhead.
    #[pyclass(get_all)]
    #[derive(Debug, Clone, MapPy)]
    #[map(ldr_tools::LDrawGeometry)]
    pub struct LDrawGeometry {
        vertices: Py<PyArray2<f32>>,
        vertex_indices: Py<PyArray1<u32>>,
        face_start_indices: Py<PyArray1<u32>>,
        face_sizes: Py<PyArray1<u32>>,
        face_colors: Py<PyArray1<u32>>,
        is_face_stud: Vec<bool>,
        edge_line_indices: Py<PyArray2<u32>>,
        has_grainy_slopes: bool,
        texture_info: Option<LDrawTextureInfo>,
    }

    #[pyclass(get_all)]
    #[derive(Debug, Clone, MapPy)]
    #[map(ldr_tools::LDrawTextureInfo)]
    pub struct LDrawTextureInfo {
        textures: TypedList<Vec<u8>>,
        indices: Py<PyArray1<u8>>,
        uvs: Py<PyArray2<f32>>,
    }

    #[pyclass(get_all)]
    #[derive(Debug, Clone, MapPy)]
    #[map(ldr_tools::LDrawColor)]
    pub struct LDrawColor {
        name: String,
        finish_name: String,
        rgba_linear: [f32; 4],
        speckle_rgba_linear: Option<[f32; 4]>,
    }

    #[pyclass(get_all, set_all)]
    #[derive(Debug, Clone, MapPy)]
    #[map(ldr_tools::GeometrySettings)]
    pub struct GeometrySettings {
        triangulate: bool,
        add_gap_between_parts: bool,
        stud_type: StudType,
        weld_vertices: bool,
        primitive_resolution: PrimitiveResolution,
        scene_scale: f32,
    }

    #[pymethods]
    impl GeometrySettings {
        #[new]
        fn new(py: Python) -> PyResult<Self> {
            ldr_tools::GeometrySettings::default().map_py(py)
        }
    }

    #[pyclass(get_all, set_all)]
    #[derive(Debug, Clone, MapPy)]
    #[map(ldr_tools::PointInstances)]
    pub struct PointInstances {
        translations: Py<PyArray2<f32>>,
        rotations_axis: Py<PyArray2<f32>>,
        rotations_angle: Py<PyArray1<f32>>,
        scales: Py<PyArray2<f32>>,
    }

    #[pyfunction]
    fn load_file(
        py: Python,
        path: String,
        ldraw_path: String,
        additional_paths: Vec<String>,
        settings: GeometrySettings,
    ) -> PyResult<LDrawScene> {
        // TODO: This timing code doesn't need to be here.
        let start = std::time::Instant::now();
        let scene =
            ldr_tools::load_file(&path, &ldraw_path, &additional_paths, &settings.map_py(py)?);

        let geometry_cache = scene
            .geometry_cache
            .into_iter()
            .map(|(k, v)| Ok((k, v.map_py(py)?)))
            .collect::<PyResult<_>>()?;
        info!("load_file: {:?}", start.elapsed());

        Ok(LDrawScene {
            root_node: scene.root_node.map_py(py)?,
            geometry_cache,
        })
    }

    #[pyfunction]
    fn load_file_instanced(
        py: Python,
        path: String,
        ldraw_path: String,
        additional_paths: Vec<String>,
        settings: GeometrySettings,
    ) -> PyResult<LDrawSceneInstanced> {
        let start = std::time::Instant::now();
        let scene = ldr_tools::load_file_instanced(
            &path,
            &ldraw_path,
            &additional_paths,
            &settings.map_py(py)?,
        );

        let geometry_cache = scene
            .geometry_cache
            .into_iter()
            .map(|(k, v)| Ok((k, v.map_py(py)?)))
            .collect::<PyResult<_>>()?;

        let geometry_world_transforms = scene
            .geometry_world_transforms
            .into_iter()
            .map(|(k, v)| {
                let transforms = v.map_py(py)?;
                Ok((k, transforms))
            })
            .collect::<PyResult<_>>()?;

        info!("load_file_instanced: {:?}", start.elapsed());

        Ok(LDrawSceneInstanced {
            main_model_name: scene.main_model_name,
            geometry_world_transforms,
            geometry_cache,
        })
    }

    #[pyfunction]
    fn load_file_instanced_points(
        py: Python,
        path: String,
        ldraw_path: String,
        additional_paths: Vec<String>,
        settings: GeometrySettings,
    ) -> PyResult<LDrawSceneInstancedPoints> {
        let start = std::time::Instant::now();
        let scene = ldr_tools::load_file_instanced_points(
            &path,
            &ldraw_path,
            &additional_paths,
            &settings.map_py(py)?,
        );

        let geometry_cache = scene
            .geometry_cache
            .into_iter()
            .map(|(k, v)| Ok((k, v.map_py(py)?)))
            .collect::<PyResult<_>>()?;

        let geometry_point_instances = scene
            .geometry_point_instances
            .into_iter()
            .map(|(k, v)| Ok((k, v.map_py(py)?)))
            .collect::<PyResult<_>>()?;

        info!("load_file_instanced_points: {:?}", start.elapsed());

        Ok(LDrawSceneInstancedPoints {
            main_model_name: scene.main_model_name,
            geometry_point_instances,
            geometry_cache,
        })
    }

    #[pyfunction]
    fn load_color_table(py: Python, ldraw_path: &str) -> PyResult<HashMap<u32, LDrawColor>> {
        ldr_tools::load_color_table(ldraw_path)
            .into_iter()
            .map(|(k, v)| Ok((k, v.map_py(py)?)))
            .collect()
    }
}
