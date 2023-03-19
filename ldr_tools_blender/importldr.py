import bpy
import numpy as np
import mathutils
import math
import bmesh

# TODO: Create a pyi type stub file?
from . import ldr_tools_py

from .ldr_tools_py import LDrawNode, LDrawGeometry, LDrawColor

from .material import get_material

# TODO: Add type hints for all functions.


def importldraw(operator: bpy.types.Operator, filepath: str, ldraw_path: str, use_instancing: bool):
    color_by_code = ldr_tools_py.load_color_table(ldraw_path)

    # TODO: Add an option to make the lowest point have a height of 0 using obj.dimensions?
    if use_instancing:
        import_instanced(filepath, ldraw_path, color_by_code)
    else:
        import_objects(filepath, ldraw_path, color_by_code)


def import_objects(filepath: str, ldraw_path: str, color_by_code: dict[int, LDrawColor]):
    blender_mesh_cache = {}
    root_node, geometry_cache = ldr_tools_py.load_file(filepath, ldraw_path)

    root_obj = add_nodes(root_node, geometry_cache,
                         blender_mesh_cache, color_by_code)
    # Account for Blender having a different coordinate system.
    root_obj.matrix_basis = mathutils.Matrix.Rotation(
        math.radians(-90.0), 4, 'X')


def import_instanced(filepath: str, ldraw_path: str, color_by_code: dict[int, LDrawColor]):
    # Instance each part on the points of a mesh.
    # This avoids overhead from object creation for large scenes.
    geometry_cache, geometry_face_instances = ldr_tools_py.load_file_instanced_faces(
        filepath, ldraw_path)

    # First create all the meshes and materials.
    blender_mesh_cache = {}
    for name, color in geometry_face_instances:
        geometry = geometry_cache[name]

        mesh = create_colored_mesh_from_geometry(
            name, color, color_by_code, geometry)

        blender_mesh_cache[(name, color)] = mesh

    # Instant each unique colored part on the faces of a mesh.
    for (name, color), faces in geometry_face_instances.items():
        instancer_mesh = create_instancer_mesh(
            f'{name}_{color}_instancer', faces)

        instancer_object = bpy.data.objects.new(
            f'{name}_{color}_instancer', instancer_mesh)

        # Account for Blender having a different coordinate system.
        instancer_object.matrix_basis = mathutils.Matrix.Rotation(
            math.radians(-90.0), 4, 'X')

        bpy.context.collection.objects.link(instancer_object)

        mesh = blender_mesh_cache[(name, color)]
        instance_object = bpy.data.objects.new(
            f'{name}_{color}_instance', mesh)
        instance_object.parent = instancer_object
        bpy.context.collection.objects.link(instance_object)
        # Hide the original instanced object to avoid cluttering the viewport.
        # Make sure the object is in the view layer before hiding.
        instance_object.hide_set(True)
        instance_object.hide_render = False

        # Instance the mesh on the faces of the parent object.
        instancer_object.instance_type = 'FACES'
        instancer_object.use_instance_faces_scale = True
        instancer_object.show_instancer_for_render = False
        instancer_object.show_instancer_for_viewport = False


def create_instancer_mesh(name: str, vertices: np.ndarray):
    # Create a quad face for each part instance.
    # The face's position, normal, and area encode the transform.
    instancer_mesh = bpy.data.meshes.new(name)

    vertex_indices = np.arange(vertices.shape[0], dtype=np.uint32)
    loop_start = np.arange(0, vertex_indices.shape[0], 4, dtype=np.int32)
    loop_total = np.full(loop_start.shape[0], 4, dtype=np.int32)

    if vertices.shape[0] > 0:
        # Using foreach_set is faster than bmesh or from_pydata.
        # https://devtalk.blender.org/t/alternative-in-2-80-to-create-meshes-from-python-using-the-tessfaces-api/7445/3
        # We can assume the data is already a numpy array.
        instancer_mesh.vertices.add(vertices.shape[0])
        instancer_mesh.vertices.foreach_set('co', vertices.reshape(-1))

        instancer_mesh.loops.add(vertex_indices.size)
        instancer_mesh.loops.foreach_set(
            'vertex_index', vertex_indices)

        instancer_mesh.polygons.add(loop_start.size)
        instancer_mesh.polygons.foreach_set(
            'loop_start', loop_start)
        instancer_mesh.polygons.foreach_set('loop_total', loop_total)

    instancer_mesh.validate()
    instancer_mesh.update()
    return instancer_mesh


def add_nodes(node: LDrawNode,
              geometry_cache: dict[str, LDrawGeometry],
              blender_mesh_cache: dict[tuple[str, int], bpy.types.Mesh],
              color_by_code: dict[str, LDrawColor]):

    if node.geometry_name is not None:
        geometry = geometry_cache[node.geometry_name]

        # Cache meshes to optimize import times and instance mesh data.
        # Linking an existing mesh data block greatly reduces memory usage.
        mesh_key = (node.geometry_name, node.current_color)

        blender_mesh = blender_mesh_cache.get(mesh_key)
        if blender_mesh is None:
            mesh = create_colored_mesh_from_geometry(
                node.name, node.current_color, color_by_code, geometry)

            blender_mesh_cache[mesh_key] = mesh
            obj = bpy.data.objects.new(node.name, mesh)
        else:
            obj = bpy.data.objects.new(node.name, blender_mesh)
    else:
        # Create an empty by setting the data to None.
        obj = bpy.data.objects.new(node.name, None)

    # Each node is transformed relative to its parent.
    obj.matrix_local = mathutils.Matrix(node.transform).transposed()
    bpy.context.collection.objects.link(obj)

    for child in node.children:
        child_obj = add_nodes(child, geometry_cache,
                              blender_mesh_cache, color_by_code)
        child_obj.parent = obj

    return obj


def create_colored_mesh_from_geometry(name: str, color: int, color_by_code: dict[int, LDrawColor], geometry: LDrawGeometry):
    mesh = create_mesh_from_geometry(name, geometry)

    assign_materials(mesh, color, color_by_code, geometry)

    # TODO: Why does this need to be done here to avoid messing up face colors?
    # TODO: Can blender adjust faces in these calls?
    mesh.validate()
    mesh.update()

    split_hard_edges(mesh)

    return mesh


def split_hard_edges(mesh):
    bm = bmesh.new()
    bm.from_mesh(mesh)

    # The edge smooth state is set when creating the mesh geometry.
    bmesh.ops.split_edges(bm, edges=[e for e in bm.edges if not e.smooth])

    bm.to_mesh(mesh)
    bm.free()


def assign_materials(mesh: bpy.types.Mesh, current_color: int, color_by_code: dict[int, LDrawColor], geometry: LDrawGeometry):
    if len(geometry.face_colors) == 1:
        # Geometry is cached with code 16, so also handle color replacement.
        face_color = geometry.face_colors[0]
        color = current_color if face_color.color == 16 else face_color.color

        # Cache materials by name.
        material = get_material(color_by_code, color, face_color.is_grainy_slope)

        mesh.materials.append(material)
    else:
        # Handle the case where not all faces have the same color.
        # This includes patterned (printed) parts and stickers.
        for (face, face_color) in zip(mesh.polygons, geometry.face_colors):
            color = current_color if face_color.color == 16 else face_color.color

            material = get_material(color_by_code, color, face_color.is_grainy_slope)
            if mesh.materials.get(material.name) is None:
                mesh.materials.append(material)
            face.material_index = mesh.materials.find(material.name)


def create_mesh_from_geometry(name: str, geometry: LDrawGeometry):
    mesh = bpy.data.meshes.new(name)
    if geometry.vertices.shape[0] > 0:
        # Using foreach_set is faster than bmesh or from_pydata.
        # https://devtalk.blender.org/t/alternative-in-2-80-to-create-meshes-from-python-using-the-tessfaces-api/7445/3
        # We can assume the data is already a numpy array.
        mesh.vertices.add(geometry.vertices.shape[0])
        mesh.vertices.foreach_set('co', geometry.vertices.reshape(-1))

        mesh.loops.add(geometry.vertex_indices.size)
        mesh.loops.foreach_set('vertex_index', geometry.vertex_indices)

        mesh.polygons.add(geometry.face_sizes.size)
        mesh.polygons.foreach_set(
            'loop_start', geometry.face_start_indices)
        mesh.polygons.foreach_set('loop_total', geometry.face_sizes)

        mesh.edges.add(geometry.edges.shape[0])
        mesh.edges.foreach_set('vertices', geometry.edges.reshape(-1))
        mesh.edges.foreach_set('use_edge_sharp', geometry.is_edge_sharp)

        # Enable autosmooth to handle some cases where edges aren't split.
        mesh.use_auto_smooth = True
        mesh.auto_smooth_angle = math.radians(60.0)
        mesh.polygons.foreach_set('use_smooth', [True] * len(mesh.polygons))

    return mesh
