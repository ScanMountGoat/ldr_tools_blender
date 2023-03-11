from .ldr_tools_py import LDrawColor

import bpy


def get_material(color_by_code: dict[int, LDrawColor], color: int):
    # Cache materials by name.
    # This loads materials lazily to avoid creating unused colors.
    material = bpy.data.materials.get(str(color))
    if material is None:
        material = bpy.data.materials.new(str(color))
        material.use_nodes = True
        bsdf = material.node_tree.nodes["Principled BSDF"]

        ldraw_color = color_by_code.get(color)
        if color in color_by_code:
            # LDraw colors don't specify an alpha value.
            r, g, b = ldraw_color.rgba_linear
            bsdf.inputs['Base Color'].default_value = [r, g, b, 1.0]

            bsdf.inputs['Subsurface Color'].default_value = [r, g, b, 1.0]
            bsdf.inputs['Subsurface Radius'].default_value = [
                0.001, 0.001, 0.001]  # TODO: should depend on scene scale
            bsdf.inputs['Subsurface'].default_value = 0.0 if ldraw_color.is_transmissive else 1.0

            # TODO: is it easier to just create a table for this in Python?
            bsdf.inputs['Metallic'].default_value = 1.0 if ldraw_color.is_metallic else 0.0
            bsdf.inputs['Transmission'].default_value = 1.0 if ldraw_color.is_transmissive else 0.0
            bsdf.inputs['Transmission Roughness'].default_value = 0.2

            # Procedural roughness.
            roughness_node_tree = bpy.data.node_groups.get('ldr_tools_roughness')
            if roughness_node_tree is None:
                roughness_node_tree = create_roughness_node_group()

            roughness_node = material.node_tree.nodes.new(type='ShaderNodeGroup')
            roughness_node.node_tree = roughness_node_tree

            material.node_tree.links.new(roughness_node.outputs['Roughness'], bsdf.inputs['Roughness'])

            # Procedural normals.
            normals_node_tree = bpy.data.node_groups.get('ldr_tools_normal')
            if normals_node_tree is None:
                normals_node_tree = create_normals_node_group()

            normals_node = material.node_tree.nodes.new(type='ShaderNodeGroup')
            normals_node.node_tree = normals_node_tree

            material.node_tree.links.new(normals_node.outputs['Normal'], bsdf.inputs['Normal'])

            # Set the color in the viewport.
            material.diffuse_color = [r, g, b, 1.0]

    return material


def create_roughness_node_group() -> bpy.types.NodeTree:
    node_group_node_tree = bpy.data.node_groups.new(
        'ldr_tools_roughness', 'ShaderNodeTree')

    node_group_node_tree.outputs.new('NodeSocketFloat', 'Roughness')

    inner_nodes = node_group_node_tree.nodes
    inner_links = node_group_node_tree.links

    # TODO: Create frame called "smudges" or at least name the nodes.
    noise = inner_nodes.new('ShaderNodeTexNoise')
    noise.inputs['Scale'].default_value = 5.0
    noise.inputs['Detail'].default_value = 2.0
    noise.inputs['Roughness'].default_value = 0.5
    noise.inputs['Distortion'].default_value = 0.0

    ramp = inner_nodes.new('ShaderNodeValToRGB')
    ramp.color_ramp.elements[0].color = (0.075, 0.075, 0.075, 1.0)
    ramp.color_ramp.elements[1].color = (0.3, 0.3, 0.3, 1.0)

    output_node = inner_nodes.new('NodeGroupOutput')

    inner_links.new(noise.outputs['Fac'], ramp.inputs['Fac'])
    inner_links.new(ramp.outputs['Color'], output_node.inputs['Roughness'])

    return node_group_node_tree


def create_normals_node_group() -> bpy.types.NodeTree:
    node_group_node_tree = bpy.data.node_groups.new(
        'ldr_tools_normal', 'ShaderNodeTree')

    node_group_node_tree.outputs.new('NodeSocketVector', 'Normal')

    inner_nodes = node_group_node_tree.nodes
    inner_links = node_group_node_tree.links

    output_node = inner_nodes.new('NodeGroupOutput')

    bevel = inner_nodes.new('ShaderNodeBevel')
    bevel.inputs['Radius'].default_value = 0.01

    # TODO: Create frame called "unevenness" or at least name the nodes.
    unevenness_noise = inner_nodes.new('ShaderNodeTexNoise')
    unevenness_noise.inputs['Scale'].default_value = 5.0
    unevenness_noise.inputs['Detail'].default_value = 0.0
    unevenness_noise.inputs['Roughness'].default_value = 0.0
    unevenness_noise.inputs['Distortion'].default_value = 0.0

    unevenness_bump = inner_nodes.new('ShaderNodeBump')
    unevenness_bump.inputs['Strength'].default_value = 0.15
    unevenness_bump.inputs['Distance'].default_value = 0.15

    # TODO: Create frame called "micronoise" or at least name the nodes.
    micro_noise = inner_nodes.new('ShaderNodeTexNoise')
    micro_noise.inputs['Scale'].default_value = 100.0
    micro_noise.inputs['Detail'].default_value = 15.0
    micro_noise.inputs['Roughness'].default_value = 1.0
    micro_noise.inputs['Distortion'].default_value = 0.0

    micro_bump = inner_nodes.new('ShaderNodeBump')
    micro_bump.inputs['Strength'].default_value = 0.1
    micro_bump.inputs['Distance'].default_value = 0.1

    tex_coord = inner_nodes.new('ShaderNodeTexCoord')

    inner_links.new(bevel.outputs['Normal'], unevenness_bump.inputs['Normal'])

    inner_links.new(tex_coord.outputs['Object'],
                    unevenness_noise.inputs['Vector'])
    inner_links.new(
        unevenness_noise.outputs['Fac'], unevenness_bump.inputs['Height'])
    inner_links.new(
        unevenness_bump.outputs['Normal'], micro_bump.inputs['Normal'])

    inner_links.new(tex_coord.outputs['Object'], micro_noise.inputs['Vector'])
    inner_links.new(micro_noise.outputs['Fac'], micro_bump.inputs['Height'])
    inner_links.new(micro_bump.outputs['Normal'], output_node.inputs['Normal'])

    return node_group_node_tree
