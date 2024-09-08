from typing import Callable, TypeVar
from dataclasses import dataclass

from .ldr_tools_py import LDrawColor
from .colors import rgb_peeron_by_code, rgb_ldr_tools_by_code
from .node_dsl import NodeGraph

import bpy

from bpy.types import (
    Material,
    NodeTree,
    ShaderNodeTree,
    NodeSocketFloat,
    NodeSocketVector,
    NodeGroupInput,
    NodeGroupOutput,
    ShaderNodeBevel,
    ShaderNodeTexCoord,
    ShaderNodeTexNoise,
    ShaderNodeBump,
    ShaderNodeMapRange,
)

# Materials are based on the techniques described in the following blog posts.
# This covers how to create lego shaders with realistic surface detailing.
# https://stefanmuller.com/exploring-lego-material-part-1/
# https://stefanmuller.com/exploring-lego-material-part-2/
# https://stefanmuller.com/exploring-lego-material-part-3/


def get_material(
    color_by_code: dict[int, LDrawColor], code: int, is_slope: bool
) -> bpy.types.Material:
    # Cache materials by name.
    # This loads materials lazily to avoid creating unused colors.
    ldraw_color = color_by_code.get(code)

    name = str(code)
    if ldraw_color is not None:
        name = f"{code} {ldraw_color.name}"
        if is_slope:
            name += " slope"

    material = bpy.data.materials.get(name)

    # TODO: Report warnings if a part contains an invalid color code.
    if material is None:
        material = bpy.data.materials.new(name)
        material.use_nodes = True

        # Create the nodes from scratch to ensure the required nodes are present.
        # This avoids hard coding names like "Material Output" that depend on the UI language.
        material.node_tree.nodes.clear()

        nodes = material.node_tree.nodes
        links = material.node_tree.links

        bsdf = nodes.new("ShaderNodeBsdfPrincipled")
        bsdf.location = (-240, 462)
        output_node = nodes.new("ShaderNodeOutputMaterial")
        output_node.location = (60, 462)
        links.new(bsdf.outputs["BSDF"], output_node.inputs["Surface"])

        # TODO: Error if color is missing?
        if ldraw_color is not None:
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

            bsdf.inputs["Base Color"].default_value = [r, g, b, 1.0]

            # Transparent colors specify an alpha of 128 / 255.
            is_transmissive = a <= 0.6

            # RANDOM_WALK is more accurate but has discoloration around thin corners.
            # TODO: This is in Blender units and should depend on scene scale
            bsdf.subsurface_method = "BURLEY"
            # Use a less accurate SSS method instead.
            bsdf.inputs["Subsurface Radius"].default_value = [r, g, b]
            bsdf.inputs["Subsurface Weight"].default_value = 1.0
            bsdf.inputs["Subsurface Scale"].default_value = 0.025

            # Procedural roughness.
            roughness_node = create_node_group(
                material, "ldr_tools_roughness", create_roughness_node_group
            )
            roughness_node.location = (-430, 500)

            links.new(roughness_node.outputs["Roughness"], bsdf.inputs["Roughness"])

            # Normal opaque materials.
            roughness_node.inputs["Min"].default_value = 0.075
            roughness_node.inputs["Max"].default_value = 0.2

            # TODO: Have a case for each finish type?
            if ldraw_color.finish_name == "MatteMetallic":
                bsdf.inputs["Metallic"].default_value = 1.0
            if ldraw_color.finish_name == "Chrome":
                # Glossy metal coating.
                bsdf.inputs["Metallic"].default_value = 1.0
                roughness_node.inputs["Min"].default_value = 0.075
                roughness_node.inputs["Max"].default_value = 0.1
            if ldraw_color.finish_name == "Metal":
                # Rougher metals.
                bsdf.inputs["Metallic"].default_value = 1.0
                roughness_node.inputs["Min"].default_value = 0.15
                roughness_node.inputs["Max"].default_value = 0.3
            elif ldraw_color.finish_name == "Pearlescent":
                bsdf.inputs["Metallic"].default_value = 0.35
                roughness_node.inputs["Min"].default_value = 0.3
                roughness_node.inputs["Max"].default_value = 0.5
            elif ldraw_color.finish_name == "Speckle":
                # TODO: Are all speckled colors metals?
                bsdf.inputs["Metallic"].default_value = 1.0

                speckle_node = create_node_group(
                    material, "ldr_tools_speckle", create_speckle_node_group
                )
                speckle_node.location = (-620, 700)

                # Adjust the thresholds to control speckle size and density.
                speckle_node.inputs["Min"].default_value = 0.5
                speckle_node.inputs["Max"].default_value = 0.6

                # Blend between the two speckle colors.
                mix_rgb = nodes.new("ShaderNodeMixRGB")
                mix_rgb.location = (-430, 700)

                links.new(speckle_node.outputs["Fac"], mix_rgb.inputs["Fac"])
                mix_rgb.inputs[1].default_value = [r, g, b, 1.0]
                speckle_r, speckle_g, speckle_b, _ = ldraw_color.speckle_rgba_linear
                mix_rgb.inputs[2].default_value = [speckle_r, speckle_g, speckle_b, 1.0]

                links.new(mix_rgb.outputs["Color"], bsdf.inputs["Base Color"])

            if is_transmissive:
                bsdf.inputs["Transmission Weight"].default_value = 1.0
                bsdf.inputs["IOR"].default_value = 1.55

                if ldraw_color.finish_name == "Rubber":
                    # Make the transparent rubber appear cloudy.
                    roughness_node.inputs["Min"].default_value = 0.1
                    roughness_node.inputs["Max"].default_value = 0.35
                else:
                    roughness_node.inputs["Min"].default_value = 0.01
                    roughness_node.inputs["Max"].default_value = 0.15

            # Procedural normals.
            normals = create_node_group(
                material, "ldr_tools_normal", create_normals_node_group
            )
            normals.location = (-620, 202)

            if is_slope:
                # Apply grainy normals to faces that aren't vertical or horizontal.
                # Use non transformed normals to not consider object rotation.
                ldr_normals = nodes.new("ShaderNodeAttribute")
                ldr_normals.attribute_name = "ldr_normals"
                ldr_normals.location = (-1600, 400)

                separate = nodes.new("ShaderNodeSeparateXYZ")
                links.new(ldr_normals.outputs["Vector"], separate.inputs["Vector"])
                separate.location = (-1400, 400)

                # Use normal.y to check if the face is horizontal (-1.0 or 1.0) or vertical (0.0).
                # Any values in between are considered "slopes" and use grainy normals.
                absolute = nodes.new("ShaderNodeMath")
                absolute.operation = "ABSOLUTE"
                absolute.location = (-1200, 400)
                links.new(separate.outputs["Y"], absolute.inputs["Value"])

                compare = nodes.new("ShaderNodeMath")
                compare.operation = "COMPARE"
                compare.inputs[1].default_value = 0.5
                compare.inputs[2].default_value = 0.45
                compare.location = (-1000, 400)
                links.new(absolute.outputs["Value"], compare.inputs["Value"])

                slope_normals = create_node_group(
                    material, "ldr_tools_slope_normal", create_slope_normals_node_group
                )
                slope_normals.location = (-630, 100)

                is_stud = nodes.new("ShaderNodeAttribute")
                is_stud.attribute_name = "ldr_is_stud"
                is_stud.location = (-1000, 200)

                # Don't apply the grainy slopes to any faces marked as studs.
                # We use an attribute here to avoid per face material assignment.
                subtract_studs = nodes.new("ShaderNodeMath")
                subtract_studs.operation = "SUBTRACT"
                subtract_studs.location = (-800, 400)
                links.new(compare.outputs["Value"], subtract_studs.inputs[0])
                links.new(is_stud.outputs[2], subtract_studs.inputs[1])

                # Choose between grainy and smooth normals depending on the face.
                mix_normals = nodes.new("ShaderNodeMix")
                mix_normals.data_type = "VECTOR"
                mix_normals.location = (-430, 330)
                links.new(subtract_studs.outputs["Value"], mix_normals.inputs["Factor"])
                links.new(normals.outputs["Normal"], mix_normals.inputs[4])
                links.new(slope_normals.outputs["Normal"], mix_normals.inputs[5])

                # The second output is the vector output.
                links.new(mix_normals.outputs[1], bsdf.inputs["Normal"])
            else:
                links.new(normals.outputs["Normal"], bsdf.inputs["Normal"])

    return material


def create_node_group(
    material: bpy.types.Material,
    name: str,
    create_group: Callable[[str], bpy.types.NodeTree],
):
    node_tree = bpy.data.node_groups.get(name)
    if node_tree is None:
        node_tree = create_group(name)

    node = material.node_tree.nodes.new(type="ShaderNodeGroup")
    node.node_tree = node_tree
    return node


def create_roughness_node_group(name: str) -> bpy.types.NodeTree:
    graph = NodeGraph(_node_tree(ShaderNodeTree, name))

    graph.input(NodeSocketFloat, "Min")
    graph.input(NodeSocketFloat, "Max")
    graph.output(NodeSocketFloat, "Roughness")

    input = graph.node(NodeGroupInput, location=(-480, -300))

    # TODO: Create frame called "smudges" or at least name the nodes.
    noise = graph.node(
        ShaderNodeTexNoise,
        location=(-480, 0),
        inputs={
            "Scale": 4.0,
            "Detail": 2.0,
            "Roughness": 0.5,
            "Distortion": 0.0,
        },
    )

    # Easier to configure than a color ramp since the input is 1D.
    map_range = graph.node(
        ShaderNodeMapRange,
        location=(-240, 0),
        inputs={
            "Value": noise["Fac"],
            "To Min": input["Min"],
            "To Max": input["Max"],
        },
    )

    graph.node(NodeGroupOutput, location=(0, 0), inputs=[map_range])
    return graph.tree


def create_speckle_node_group(name: str) -> bpy.types.NodeTree:
    graph = NodeGraph(_node_tree(ShaderNodeTree, name))

    graph.input(NodeSocketFloat, "Min")
    graph.input(NodeSocketFloat, "Max")
    graph.output(NodeSocketFloat, "Fac")

    input = graph.node(NodeGroupInput, location=(-480, -300))

    noise = graph.node(
        ShaderNodeTexNoise,
        location=(-480, 0),
        inputs={
            "Scale": 15.0,
            "Detail": 6.0,
            "Roughness": 1.0,
            "Distortion": 0.0,
        },
    )

    # Easier to configure than a color ramp since the input is 1D.
    map_range = graph.node(
        ShaderNodeMapRange,
        location=(-240, 0),
        inputs={
            "Value": noise["Fac"],
            "From Min": input["Min"],
            "From Max": input["Max"],
        },
    )

    graph.node(NodeGroupOutput, location=(0, 0), inputs=[map_range])
    return graph.tree


def create_normals_node_group(name: str) -> bpy.types.NodeTree:
    graph = NodeGraph(_node_tree(ShaderNodeTree, name))

    graph.output(NodeSocketVector, "Normal")

    bevel = graph.node(ShaderNodeBevel, location=(-480, -300), inputs={"Radius": 0.01})
    tex_coord = graph.node(ShaderNodeTexCoord, location=(-720, 0))

    # Faces of bricks are never perfectly flat.
    # Create a very low frequency noise to break up highlights
    noise = graph.node(
        ShaderNodeTexNoise,
        location=(-480, 0),
        inputs={
            "Scale": 0.01,  # TODO: scene scale?
            "Detail": 1.0,
            "Roughness": 1.0,
            # "Distortion": 0.0, # already the default
            "Vector": tex_coord["Object"],
        },
    )

    bump = graph.node(
        ShaderNodeBump,
        location=(-240, 0),
        inputs={
            "Strength": 1.0,
            "Distance": 0.01,
            "Height": noise["Fac"],
            "Normal": bevel,
        },
    )

    graph.node(NodeGroupOutput, location=(0, 0), inputs=[bump])
    return graph.tree


def create_slope_normals_node_group(name: str) -> ShaderNodeTree:
    graph = NodeGraph(_node_tree(ShaderNodeTree, name))

    graph.output(NodeSocketVector, "Normal")

    bevel = graph.node(ShaderNodeBevel, location=(-480, -300), inputs={"Radius": 0.01})
    tex_coord = graph.node(ShaderNodeTexCoord, location=(-720, 0))

    noise = graph.node(
        ShaderNodeTexNoise,
        location=(-480, 0),
        inputs={
            "Scale": 2.5,
            "Detail": 3.0,
            "Roughness": 0.5,
            # "Lacunarity": 2.0, # already the default
            "Vector": tex_coord["Object"],
        },
    )

    bump = graph.node(
        ShaderNodeBump,
        location=(-240, 0),
        inputs={
            "Strength": 0.5,
            "Distance": 0.005,
            "Height": noise["Fac"],
            "Normal": bevel,
        },
    )

    graph.node(NodeGroupOutput, location=(0, 0), inputs=[bump])

    return graph.tree


T = TypeVar("T", bound=NodeTree)


def _node_tree(tree_type: type[T], name: str) -> T:
    tree = bpy.data.node_groups.new(name, tree_type.__name__)  # type: ignore[arg-type]
    assert isinstance(tree, tree_type)
    return tree
