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


def importldraw(operator: bpy.types.Operator, filepath: str, use_instancing=True):
    color_by_code = ldr_tools_py.load_color_table()

    # TODO: Create a parameter for whether to use instancing or not.
    if use_instancing:
        import_instanced(filepath, color_by_code)
    else:
        import_objects(filepath, color_by_code)


def import_objects(filepath: str, color_by_code: dict[int, LDrawColor]):
    blender_mesh_cache = {}
    root_node, geometry_cache = ldr_tools_py.load_file(filepath)

    root_obj = add_nodes(root_node, geometry_cache,
                         blender_mesh_cache, color_by_code)
    # Account for Blender having a different coordinate system.
    root_obj.matrix_basis = mathutils.Matrix.Rotation(
        math.radians(-90.0), 4, 'X')


def import_instanced(filepath: str, color_by_code: dict[int, LDrawColor]):
    # Instance each part on the points of a mesh.
    # This avoids overhead from object creation for large scenes.
    geometry_cache, geometry_world_transforms = ldr_tools_py.load_file_instanced(
        filepath)

    # First create all the meshes and materials.
    blender_mesh_cache = {}
    for name, color in geometry_world_transforms:
        geometry = geometry_cache[name]

        mesh = create_mesh_from_geometry(name, geometry)
        assign_materials(mesh, color, color_by_code, geometry)

        blender_mesh_cache[(name, color)] = mesh

    # Instant each unique colored part on the faces of a mesh.
    for (name, color), transforms in geometry_world_transforms.items():
        instancer_mesh = create_instancer_mesh(
            f'{name}_{color}_instancer', transforms)

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
        instance_object.hide_render = True
        bpy.context.collection.objects.link(instance_object)
        # Hide the original instanced object to avoid cluttering the viewport.
        # Make sure the object is in the view layer before hiding.
        instance_object.hide_set(True)

        # Instance the mesh on the faces of the parent object.
        instancer_object.instance_type = 'FACES'
        instancer_object.use_instance_faces_scale = True
        instancer_object.show_instancer_for_render = False
        instancer_object.show_instancer_for_viewport = False


def create_instancer_mesh(name: str, transforms: np.ndarray):
    instancer_mesh = bpy.data.meshes.new(name)

    # Use homogeneous coordinates for 3D points.
    # Use a square with unit area centered at the origin.
    face_vertices = np.array(
        [[-0.5, -0.5, 0.0, 1.0], [0.5, -0.5, 0.0, 1.0], [0.5, 0.5, 0.0, 1.0], [-0.5, 0.5, 0.0, 1.0]])

    # Duplicate the vertices for each face for each transform.
    # Transform each quad face by each of the transforms.
    vertices = face_vertices.reshape((1, 4, 4)).repeat(
        transforms.shape[0], axis=0) @ transforms
    vertices = vertices[:, :, :3].reshape((-1, 3))

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
            mesh = create_mesh_from_geometry(node.name, geometry)

            assign_materials(mesh, node.current_color, color_by_code, geometry)

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


def assign_materials(mesh: bpy.types.Mesh, current_color: int, color_by_code: dict[int, LDrawColor], geometry: LDrawGeometry):
    if geometry.face_colors.size == 1:
        # Geometry is cached with code 16, so also handle color replacement.
        color = current_color if geometry.face_colors[0] == 16 else geometry.face_colors[0]

        # Cache materials by name.
        material = get_material(color_by_code, color)

        mesh.materials.append(material)
    else:
        # Copy the array to avoid modifying the geometry cache.
        # The value 16 must be preserved for parts used in multiple colors.
        replaced_colors = geometry.face_colors.copy()
        replaced_colors[replaced_colors == 16] = current_color

        # Handle the case where not all faces have the same color.
        # This includes patterned (printed) parts and stickers.
        for (face, color) in zip(mesh.polygons, replaced_colors):
            material = get_material(color_by_code, color)
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

    mesh.validate()
    mesh.update()

    bm = bmesh.new()
    bm.from_mesh(mesh)

    # TODO: Faster to move this to Rust?
    bmesh.ops.remove_doubles(bm, verts=bm.verts[:], dist=0.0001)
    # TODO: Calculate normals using the edge information in Rust?

    bm.to_mesh(mesh)
    bm.free()

    return mesh
