from typing import Callable

from .ldr_tools_py import LDrawColor
from .colors import rgb_peeron_by_code, rgb_ldr_tools_by_code

import bpy

# Materials are based on the techniques described in the following blog posts.
# This covers how to create lego shaders with realistic surface detailing.
# https://stefanmuller.com/exploring-lego-material-part-1/
# https://stefanmuller.com/exploring-lego-material-part-2/
# https://stefanmuller.com/exploring-lego-material-part-3/


def get_material(color_by_code: dict[int, LDrawColor], code: int):
    # Cache materials by name.
    # This loads materials lazily to avoid creating unused colors.
    ldraw_color = color_by_code.get(code)

    name = f'{code} {ldraw_color.name}' if ldraw_color is not None else str(
        code)
    material = bpy.data.materials.get(name)

    # TODO: Report warnings if a part contains an invalid color code.
    if material is None and ldraw_color is not None:
        material = bpy.data.materials.new(name)
        material.use_nodes = True
        bsdf = material.node_tree.nodes["Principled BSDF"]

        # Alpha is specified using transmission instead.
        r, g, b, a = ldraw_color.rgba_linear

        # Set the color in the viewport.
        # This can use the default LDraw color for familiarity.
        material.diffuse_color = [r, g, b, a]

        # Partially complete alternatives to LDraw colors for better realism.
        if code in rgb_ldr_tools_by_code:
            r, g, b = rgb_ldr_tools_by_code[code]
        elif code in rgb_peeron_by_code:
            r, g, b = rgb_peeron_by_code[code]

        bsdf.inputs['Base Color'].default_value = [r, g, b, 1.0]

        # Transparent colors specify an alpha of 128 / 255.
        is_transmissive = a <= 0.6

        # RANDOM_WALK_FIXED_RADIUS is more accurate but appears white on edges.
        # Use a less accurate SSS method instead.
        bsdf.subsurface_method = 'BURLEY'
        bsdf.inputs['Subsurface Color'].default_value = [r, g, b, 1.0]
        # TODO: This is in Blender units and should depend on scene scale
        bsdf.inputs['Subsurface Radius'].default_value = [0.02, 0.02, 0.02]
        bsdf.inputs['Subsurface'].default_value = 1.0

        # Procedural roughness.
        roughness_node = create_node_group(
            material, 'ldr_tools_roughness', create_roughness_node_group)

        material.node_tree.links.new(
            roughness_node.outputs['Roughness'], bsdf.inputs['Roughness'])

        # Normal opaque materials.
        roughness_node.inputs['Min'].default_value = 0.075
        roughness_node.inputs['Max'].default_value = 0.2

        # TODO: Have a case for each finish type?
        if ldraw_color.finish_name == 'MatteMetallic':
            bsdf.inputs['Metallic'].default_value = 1.0
        if ldraw_color.finish_name == 'Chrome':
            # Glossy metal coating.
            bsdf.inputs['Metallic'].default_value = 1.0
            roughness_node.inputs['Min'].default_value = 0.075
            roughness_node.inputs['Max'].default_value = 0.1
        if ldraw_color.finish_name == 'Metal':
            # Rougher metals.
            bsdf.inputs['Metallic'].default_value = 1.0
            roughness_node.inputs['Min'].default_value = 0.15
            roughness_node.inputs['Max'].default_value = 0.3
        elif ldraw_color.finish_name == 'Pearlescent':
            bsdf.inputs['Metallic'].default_value = 0.35
            roughness_node.inputs['Min'].default_value = 0.3
            roughness_node.inputs['Max'].default_value = 0.5
        elif ldraw_color.finish_name == 'Speckle':
            # TODO: Are all speckled colors metals?
            bsdf.inputs['Metallic'].default_value = 1.0

            speckle_node = create_node_group(
                material, 'ldr_tools_speckle', create_speckle_node_group)

            # Adjust the thresholds to control speckle size and density.
            speckle_node.inputs['Min'].default_value = 0.5
            speckle_node.inputs['Max'].default_value = 0.6

            # Blend between the two speckle colors.
            mix_rgb = material.node_tree.nodes.new('ShaderNodeMixRGB')

            material.node_tree.links.new(
                speckle_node.outputs['Fac'], mix_rgb.inputs['Fac'])
            mix_rgb.inputs[1].default_value = [r, g, b, 1.0]
            speckle_r, speckle_g, speckle_b, _ = ldraw_color.speckle_rgba_linear
            mix_rgb.inputs[2].default_value = [
                speckle_r, speckle_g, speckle_b, 1.0]

            material.node_tree.links.new(
                mix_rgb.outputs['Color'], bsdf.inputs['Base Color'])

        if is_transmissive:
            bsdf.inputs['Transmission'].default_value = 1.0
            bsdf.inputs['IOR'].default_value = 1.55

            if ldraw_color.finish_name == 'Rubber':
                # Make the transparent rubber appear cloudy.
                roughness_node.inputs['Min'].default_value = 0.1
                roughness_node.inputs['Max'].default_value = 0.35
                bsdf.inputs['Transmission Roughness'].default_value = 0.25
            else:
                roughness_node.inputs['Min'].default_value = 0.01
                roughness_node.inputs['Max'].default_value = 0.02
                bsdf.inputs['Transmission Roughness'].default_value = 0.1

            # Disable shadow casting for transparent materials.
            # This avoids making transparent parts too dark.
            make_shadows_transparent(material, bsdf)

        # Procedural normals.
        if not is_transmissive:
            normals_node = create_node_group(
                material, 'ldr_tools_normal', create_normals_node_group)

            material.node_tree.links.new(
                normals_node.outputs['Normal'], bsdf.inputs['Normal'])

    return material


def create_node_group(material: bpy.types.Material, name: str, create_group: Callable[[str], bpy.types.NodeTree]):
    node_tree = bpy.data.node_groups.get(name)
    if node_tree is None:
        node_tree = create_group(name)

    node = material.node_tree.nodes.new(type='ShaderNodeGroup')
    node.node_tree = node_tree
    return node


def make_shadows_transparent(material, bsdf):
    mix_shader = material.node_tree.nodes.new('ShaderNodeMixShader')
    light_path = material.node_tree.nodes.new('ShaderNodeLightPath')
    transparent_bsdf = material.node_tree.nodes.new(
        'ShaderNodeBsdfTransparent')
    output_node = material.node_tree.nodes.get('Material Output')

    material.node_tree.links.new(
        light_path.outputs['Is Shadow Ray'], mix_shader.inputs['Fac'])
    material.node_tree.links.new(
        bsdf.outputs['BSDF'], mix_shader.inputs[1])
    material.node_tree.links.new(
        transparent_bsdf.outputs['BSDF'], mix_shader.inputs[2])

    material.node_tree.links.new(
        mix_shader.outputs['Shader'], output_node.inputs['Surface'])


def create_roughness_node_group(name: str) -> bpy.types.NodeTree:
    node_group_node_tree = bpy.data.node_groups.new(
        name, 'ShaderNodeTree')

    node_group_node_tree.outputs.new('NodeSocketFloat', 'Roughness')

    inner_nodes = node_group_node_tree.nodes
    inner_links = node_group_node_tree.links

    input_node = inner_nodes.new('NodeGroupInput')
    node_group_node_tree.inputs.new('NodeSocketFloat', 'Min')
    node_group_node_tree.inputs.new('NodeSocketFloat', 'Max')

    # TODO: Create frame called "smudges" or at least name the nodes.
    noise = inner_nodes.new('ShaderNodeTexNoise')
    noise.inputs['Scale'].default_value = 5.0
    noise.inputs['Detail'].default_value = 2.0
    noise.inputs['Roughness'].default_value = 0.5
    noise.inputs['Distortion'].default_value = 0.0

    # Easier to configure than a color ramp since the input is 1D.
    map_range = inner_nodes.new('ShaderNodeMapRange')
    inner_links.new(noise.outputs['Fac'], map_range.inputs['Value'])
    inner_links.new(input_node.outputs['Min'], map_range.inputs['To Min'])
    inner_links.new(input_node.outputs['Max'], map_range.inputs['To Max'])

    output_node = inner_nodes.new('NodeGroupOutput')

    inner_links.new(map_range.outputs['Result'],
                    output_node.inputs['Roughness'])

    return node_group_node_tree


def create_speckle_node_group(name: str) -> bpy.types.NodeTree:
    node_group_node_tree = bpy.data.node_groups.new(
        name, 'ShaderNodeTree')

    node_group_node_tree.outputs.new('NodeSocketFloat', 'Fac')

    inner_nodes = node_group_node_tree.nodes
    inner_links = node_group_node_tree.links

    input_node = inner_nodes.new('NodeGroupInput')
    node_group_node_tree.inputs.new('NodeSocketFloat', 'Min')
    node_group_node_tree.inputs.new('NodeSocketFloat', 'Max')

    noise = inner_nodes.new('ShaderNodeTexNoise')
    noise.inputs['Scale'].default_value = 15.0
    noise.inputs['Detail'].default_value = 6.0
    noise.inputs['Roughness'].default_value = 1.0
    noise.inputs['Distortion'].default_value = 0.0

    # Easier to configure than a color ramp since the input is 1D.
    map_range = inner_nodes.new('ShaderNodeMapRange')
    inner_links.new(noise.outputs['Fac'], map_range.inputs['Value'])
    inner_links.new(input_node.outputs['Min'], map_range.inputs['From Min'])
    inner_links.new(input_node.outputs['Max'], map_range.inputs['From Max'])

    output_node = inner_nodes.new('NodeGroupOutput')

    inner_links.new(map_range.outputs['Result'], output_node.inputs['Fac'])

    return node_group_node_tree


def create_normals_node_group(name: str) -> bpy.types.NodeTree:
    node_group_node_tree = bpy.data.node_groups.new(name, 'ShaderNodeTree')

    node_group_node_tree.outputs.new('NodeSocketVector', 'Normal')

    inner_nodes = node_group_node_tree.nodes
    inner_links = node_group_node_tree.links

    output_node = inner_nodes.new('NodeGroupOutput')

    bevel = inner_nodes.new('ShaderNodeBevel')
    bevel.inputs['Radius'].default_value = 0.01

    # TODO: Set node positions.
    unevenness_noise = inner_nodes.new('ShaderNodeTexNoise')
    unevenness_noise.inputs['Scale'].default_value = 3.0
    unevenness_noise.inputs['Detail'].default_value = 0.0
    unevenness_noise.inputs['Roughness'].default_value = 0.0
    unevenness_noise.inputs['Distortion'].default_value = 0.0

    unevenness_bump = inner_nodes.new('ShaderNodeBump')
    unevenness_bump.inputs['Strength'].default_value = 1.0
    unevenness_bump.inputs['Distance'].default_value = 0.01

    micro_noise = inner_nodes.new('ShaderNodeTexMusgrave')
    micro_noise.inputs['Scale'].default_value = 400.0
    micro_noise.inputs['Detail'].default_value = 2.0
    micro_noise.inputs['Dimension'].default_value = 2.0
    micro_noise.inputs['Lacunarity'].default_value = 2.0

    micro_bump = inner_nodes.new('ShaderNodeBump')
    micro_bump.inputs['Strength'].default_value = 0.5
    micro_bump.inputs['Distance'].default_value = 0.0002

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
