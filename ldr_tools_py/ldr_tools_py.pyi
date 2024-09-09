from .stub_helpers import (
    UIntArray,
    FloatArray,
    UVec2Array,
    Vec3Array,
    Mat4Array,
    Vec4,
    Mat4,
)

class LDrawNode:
    name: str
    transform: Mat4
    geometry_name: str | None
    current_color: int
    children: list[LDrawNode]

class LDrawGeometry:
    vertices: Vec3Array
    vertex_indices: UIntArray
    face_start_indices: UIntArray
    face_sizes: UIntArray
    face_colors: UIntArray
    is_face_stud: list[bool]
    edge_line_indices: UVec2Array
    has_grainy_slopes: bool

class LDrawColor:
    name: str
    finish_name: str
    rgba_linear: Vec4
    speckle_rgba_linear: Vec4 | None

class GeometrySettings:
    triangulate: bool
    add_gap_between_parts: bool
    stud_type: StudType
    weld_vertices: bool
    primitive_resolution: PrimitiveResolution
    scene_scale: float

class StudType:
    Disabled: StudType
    Normal: StudType
    Logo4: StudType
    HighContrast: StudType

class PrimitiveResolution:
    Low: PrimitiveResolution
    Normal: PrimitiveResolution
    High: PrimitiveResolution

class PointInstances:
    translations: Vec3Array
    rotations_axis: Vec3Array
    rotations_angle: FloatArray
    scales: Vec3Array

class LDrawScene:
    root_node: LDrawNode
    geometry_cache: dict[str, LDrawGeometry]

class LDrawSceneInstanced:
    main_model_name: str
    geometry_world_transforms: dict[tuple[str, int], Mat4Array]
    geometry_cache: dict[str, LDrawGeometry]

class LDrawSceneInstancedPoints:
    main_model_name: str
    geometry_point_instances: dict[tuple[str, int], PointInstances]
    geometry_cache: dict[str, LDrawGeometry]

def load_file(
    path: str, ldraw_path: str, additional_paths: list[str], settings: GeometrySettings
) -> LDrawScene: ...
def load_file_instanced(
    path: str, ldraw_path: str, additional_paths: list[str], settings: GeometrySettings
) -> LDrawSceneInstanced: ...
def load_file_instanced_points(
    path: str, ldraw_path: str, additional_paths: list[str], settings: GeometrySettings
) -> LDrawSceneInstancedPoints: ...
def load_color_table(ldraw_path: str) -> dict[int, LDrawColor]: ...
