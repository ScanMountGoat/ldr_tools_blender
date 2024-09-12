import bpy
import numpy as np
import mathutils
import math
import struct
import typing
import itertools

from bpy.types import (
    NodesModifier,
    GeometryNodeTree,
    NodeSocketGeometry,
    NodeGroupInput,
    NodeGroupOutput,
    GeometryNodeObjectInfo,
    GeometryNodeInputNamedAttribute,
    GeometryNodeInstanceOnPoints,
    FunctionNodeAxisAngleToRotation,
)

if typing.TYPE_CHECKING:
    import ldr_tools_py
    from ldr_tools_py import LDrawNode, LDrawGeometry, LDrawColor, GeometrySettings
else:
    from . import ldr_tools_py
    from .ldr_tools_py import LDrawNode, LDrawGeometry, LDrawColor, GeometrySettings

from .material import get_material

from .node_dsl import NodeGraph

# TODO: Add type hints for all functions.


def import_ldraw(
    operator: bpy.types.Operator,
    filepath: str,
    ldraw_path: str,
    additional_paths: list[str],
    instance_type: str,
    settings: GeometrySettings,
):
    color_by_code = ldr_tools_py.load_color_table(ldraw_path)

    # TODO: Add an option to make the lowest point have a height of 0 using obj.dimensions?
    if instance_type == "GeometryNodes":
        import_instanced(
            filepath, ldraw_path, additional_paths, color_by_code, settings
        )
    elif instance_type == "LinkedDuplicates":
        import_objects(filepath, ldraw_path, additional_paths, color_by_code, settings)


def import_objects(
    filepath: str,
    ldraw_path: str,
    additional_paths: list[str],
    color_by_code: dict[int, LDrawColor],
    settings: GeometrySettings,
):
    # Create an object for each part in the scene.
    # This still uses instances the mesh data blocks for reduced memory usage.
    blender_mesh_cache = {}

    # Don't scale any coordinates on the Rust side, just change the scale of the parent object
    scale = settings.scene_scale
    settings.scene_scale = 1.0

    scene = ldr_tools_py.load_file(filepath, ldraw_path, additional_paths, settings)

    root_obj = add_nodes(
        scene.root_node, scene.geometry_cache, blender_mesh_cache, color_by_code
    )
    # Account for Blender having a different coordinate system.
    root_obj.rotation_euler = mathutils.Euler((math.radians(-90.0), 0.0, 0.0), "XYZ")
    root_obj.scale = (scale, scale, scale)


def add_nodes(
    node: LDrawNode,
    geometry_cache: dict[str, LDrawGeometry],
    blender_mesh_cache: dict[tuple[str, int], bpy.types.Mesh],
    color_by_code: dict[str, LDrawColor],
):

    if node.geometry_name is not None:
        geometry = geometry_cache[node.geometry_name]

        # Cache meshes to optimize import times and instance mesh data.
        # Linking an existing mesh data block greatly reduces memory usage.
        mesh_key = (node.geometry_name, node.current_color)

        blender_mesh = blender_mesh_cache.get(mesh_key)
        if blender_mesh is None:
            mesh = create_colored_mesh_from_geometry(
                node.name, node.current_color, color_by_code, geometry
            )

            blender_mesh_cache[mesh_key] = mesh
            obj = bpy.data.objects.new(node.name, mesh)
        else:
            # Use an existing mesh data block like with linked duplicates (alt+d).
            obj = bpy.data.objects.new(node.name, blender_mesh)
    else:
        # Create an empty by setting the data to None.
        obj = bpy.data.objects.new(node.name, None)

    # Each node is transformed relative to its parent.
    obj.matrix_local = mathutils.Matrix(node.transform).transposed()
    bpy.context.collection.objects.link(obj)

    for child in node.children:
        child_obj = add_nodes(child, geometry_cache, blender_mesh_cache, color_by_code)
        child_obj.parent = obj

    return obj


def import_instanced(
    filepath: str,
    ldraw_path: str,
    additional_paths: list[str],
    color_by_code: dict[int, LDrawColor],
    settings: GeometrySettings,
):
    scale = settings.scene_scale
    settings.scene_scale = 1.0

    # Instance each part on the points of a mesh.
    # This avoids overhead from object creation for large scenes.
    scene = ldr_tools_py.load_file_instanced_points(
        filepath, ldraw_path, additional_paths, settings
    )

    # First create all the meshes and materials.
    blender_mesh_cache = {}
    for name, color in scene.geometry_point_instances:
        geometry = scene.geometry_cache[name]

        mesh = create_colored_mesh_from_geometry(name, color, color_by_code, geometry)

        blender_mesh_cache[(name, color)] = mesh

    root_obj = bpy.data.objects.new(scene.main_model_name, None)
    # Account for Blender having a different coordinate system.
    root_obj.rotation_euler = mathutils.Euler((math.radians(-90.0), 0.0, 0.0), "XYZ")
    root_obj.scale = (scale, scale, scale)
    bpy.context.collection.objects.link(root_obj)

    # Instant each unique colored part on the faces of a mesh.
    for (name, color), instances in scene.geometry_point_instances.items():
        instancer_mesh = create_instancer_mesh(f"{name}_{color}_instancer", instances)

        instancer_object = bpy.data.objects.new(
            f"{name}_{color}_instancer", instancer_mesh
        )
        instancer_object.parent = root_obj

        bpy.context.collection.objects.link(instancer_object)

        mesh = blender_mesh_cache[(name, color)]
        instance_object = bpy.data.objects.new(f"{name}_{color}_instance", mesh)
        instance_object.parent = instancer_object
        bpy.context.collection.objects.link(instance_object)

        # Hide the original instanced object to avoid cluttering the viewport.
        # Make sure the object is in the view layer before hiding.
        instance_object.hide_set(True)
        instance_object.hide_render = False

        # Set up geometry nodes for the actual instancing.
        # Geometry nodes are more reliable than instancing on faces.
        # This also avoids performance overhead from object creation.
        create_geometry_node_instancing(instancer_object, instance_object)


def create_geometry_node_instancing(
    instancer_object: bpy.types.Object,
    instance_object: bpy.types.Object,
) -> None:
    modifier = instancer_object.modifiers.new(name="GeometryNodes", type="NODES")
    assert isinstance(modifier, NodesModifier)

    tree = bpy.data.node_groups.new("GeometryNodes", "GeometryNodeTree")  # type: ignore[arg-type]
    assert isinstance(tree, GeometryNodeTree)

    modifier.node_group = tree
    graph = NodeGraph(tree)

    graph.input(NodeSocketGeometry, "Geometry")
    graph.output(NodeSocketGeometry, "Geometry")

    group_input = graph.node(NodeGroupInput, location=(-380, 0))

    # Scale instances from the custom attribute.
    scale_attribute = graph.node(
        GeometryNodeInputNamedAttribute,
        location=(-380, -434),
        data_type="FLOAT_VECTOR",
        inputs={"Name": "instance_scale"},
    )

    # Rotate instances from the custom attributes.
    rot_axis = graph.node(
        GeometryNodeInputNamedAttribute,
        location=(-570, -275),
        data_type="FLOAT_VECTOR",
        inputs={"Name": "instance_rotation_axis"},
    )
    rot_angle = graph.node(
        GeometryNodeInputNamedAttribute,
        location=(-570, -418),
        data_type="FLOAT",
        inputs={"Name": "instance_rotation_angle"},
    )
    rotation = graph.node(
        FunctionNodeAxisAngleToRotation,
        location=(-380, -318),
        inputs=[rot_axis, rot_angle],
    )

    # Set the instance mesh.
    instance_info = graph.node(
        GeometryNodeObjectInfo,
        location=(-380, -91),
        inputs={"Object": instance_object},
    )

    # The instancer mesh's points define the instance translation.
    instance_points = graph.node(
        GeometryNodeInstanceOnPoints,
        location=(-190, 0),
        inputs={
            "Points": group_input,
            "Instance": instance_info["Geometry"],
            "Rotation": rotation,
            "Scale": scale_attribute,
        },
    )

    group_output = graph.node(
        NodeGroupOutput, location=(0, 0), inputs=[instance_points]
    )


def create_instancer_mesh(name: str, instances: ldr_tools_py.PointInstances):
    # Create a vertex at each instance.
    instancer_mesh = bpy.data.meshes.new(name)

    positions = instances.translations
    if positions.shape[0] > 0:
        # Using foreach_set is faster than bmesh or from_pydata.
        # https://devtalk.blender.org/t/alternative-in-2-80-to-create-meshes-from-python-using-the-tessfaces-api/7445/3
        # We can assume the data is already a numpy array.
        instancer_mesh.vertices.add(positions.shape[0])
        instancer_mesh.vertices.foreach_set("co", positions.reshape(-1))

        # Encode rotation and scale into custom attributes.
        # This allows geometry nodes to access the attributes later.
        scale_attribute = instancer_mesh.attributes.new(
            name="instance_scale", type="FLOAT_VECTOR", domain="POINT"
        )
        scale_attribute.data.foreach_set("vector", instances.scales.reshape(-1))

        rot_axis_attribute = instancer_mesh.attributes.new(
            name="instance_rotation_axis", type="FLOAT_VECTOR", domain="POINT"
        )
        rot_axis_attribute.data.foreach_set(
            "vector", instances.rotations_axis.reshape(-1)
        )

        rot_angle_attribute = instancer_mesh.attributes.new(
            name="instance_rotation_angle", type="FLOAT", domain="POINT"
        )
        rot_angle_attribute.data.foreach_set("value", instances.rotations_angle)

    instancer_mesh.validate()
    instancer_mesh.update()
    return instancer_mesh


def create_colored_mesh_from_geometry(
    name: str, color: int, color_by_code: dict[int, LDrawColor], geometry: LDrawGeometry
):
    mesh = create_mesh_from_geometry(name, geometry)

    assign_materials(mesh, color, color_by_code, geometry)

    # TODO: Why does this need to be done here to avoid messing up face colors?
    # TODO: Can blender adjust faces in these calls?
    mesh.validate()
    mesh.update()

    # Add attributes needed to render grainy slopes properly.
    if geometry.has_grainy_slopes:
        # Get custom normals now that everything has been initialized.
        # This won't include any object transforms.
        # mesh.calc_normals_split()
        loop_normals = np.zeros(len(mesh.loops) * 3)
        mesh.loops.foreach_get("normal", loop_normals)

        normals = mesh.attributes.new(
            name="ldr_normals", type="FLOAT_VECTOR", domain="CORNER"
        )
        normals.data.foreach_set("vector", loop_normals)

    return mesh


def load_png(data: bytes, name: str = "img") -> bpy.types.Image:
    # TODO: pass image names up from the Rust side
    w, h = struct.unpack(b">LL", data[16:24])
    img = bpy.data.images.new(name, w, h)
    img.use_fake_user = True
    img.pack(data=data, data_len=len(data))  # type: ignore[arg-type]
    img.source = "FILE"  # ?
    return img


def assign_materials(
    mesh: bpy.types.Mesh,
    current_color: int,
    color_by_code: dict[int, LDrawColor],
    geometry: LDrawGeometry,
):
    if len(geometry.face_colors) == 1 and not geometry.textures:
        # Geometry is cached with code 16, so also handle color replacement.
        face_color = geometry.face_colors[0]
        color = current_color if face_color == 16 else face_color

        # Cache materials by name.
        material = get_material(color_by_code, color, geometry.has_grainy_slopes)
        mesh.materials.append(material)
        return

    images = [load_png(t) for t in geometry.textures]

    if len(geometry.face_colors) > 1:
        assert len(geometry.face_colors) == len(mesh.polygons)

    if geometry.texmaps:
        assert len(geometry.texmaps) == len(mesh.polygons)

    for i, face in enumerate(mesh.polygons):
        # determine color
        color_index = i if len(geometry.face_colors) > 1 else 0
        face_color = geometry.face_colors[color_index]
        color = current_color if face_color == 16 else face_color

        # determine texture
        image = None
        if geometry.texmaps:
            if texmap := geometry.texmaps[i]:
                image = images[texmap.texture_index]

        material = get_material(color_by_code, color, geometry.has_grainy_slopes, image)
        if mesh.materials.get(material.name) is None:
            mesh.materials.append(material)

        face.material_index = mesh.materials.find(material.name)


def create_mesh_from_geometry(name: str, geometry: LDrawGeometry):
    mesh = bpy.data.meshes.new(name)
    if geometry.vertices.shape[0] == 0:
        return mesh

    # Using foreach_set is faster than bmesh or from_pydata.
    # https://devtalk.blender.org/t/alternative-in-2-80-to-create-meshes-from-python-using-the-tessfaces-api/7445/3
    # We can assume the data is already a numpy array.
    mesh.vertices.add(geometry.vertices.shape[0])
    mesh.vertices.foreach_set("co", geometry.vertices.reshape(-1))

    mesh.loops.add(geometry.vertex_indices.size)
    mesh.loops.foreach_set("vertex_index", geometry.vertex_indices)

    mesh.polygons.add(geometry.face_sizes.size)
    mesh.polygons.foreach_set("loop_start", geometry.face_start_indices)
    mesh.polygons.foreach_set("loop_total", geometry.face_sizes)

    # TODO: Enable autosmooth to handle some cases where edges aren't split.
    # TODO: Just do this in ldr_tools and set custom normals?
    # mesh.use_auto_smooth = True
    # mesh.auto_smooth_angle = math.radians(89.0)
    mesh.polygons.foreach_set("use_smooth", [True] * len(mesh.polygons))

    # Add attributes needed to render grainy slopes properly.
    if geometry.has_grainy_slopes:
        is_stud = mesh.attributes.new(name="ldr_is_stud", type="FLOAT", domain="FACE")
        is_stud.data.foreach_set("value", geometry.is_face_stud)

    uvs = []

    assert len(geometry.texmaps) == len(
        mesh.polygons
    ), f"{len(geometry.texmaps)} {len(mesh.polygons)}"

    for polygon, texmap in zip(mesh.polygons, geometry.texmaps):
        if texmap is None:
            for _ in range(polygon.loop_total):
                uvs.extend((0.0, 0.0))
        else:
            assert len(texmap.uvs) == polygon.loop_total
            for uv in texmap.uvs:
                uvs.extend(uv)

    uv_layer = mesh.uv_layers.new()
    uv_layer.data.foreach_set("uv", uvs)

    return mesh
