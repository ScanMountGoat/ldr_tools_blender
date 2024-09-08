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
    ShaderNodeBsdfPrincipled,
    ShaderNodeMixRGB,
    ShaderNodeAttribute,
    ShaderNodeMath,
    ShaderNodeMix,
    ShaderNodeOutputMaterial,
    ShaderNodeSeparateXYZ,
)

# Materials are based on the techniques described in the following blog posts.
# This covers how to create lego shaders with realistic surface detailing.
# https://stefanmuller.com/exploring-lego-material-part-1/
# https://stefanmuller.com/exploring-lego-material-part-2/
# https://stefanmuller.com/exploring-lego-material-part-3/


def get_material(
    color_by_code: dict[int, LDrawColor], code: int, is_slope: bool
) -> Material:
    # Cache materials by name.
    # This loads materials lazily to avoid creating unused colors.
    ldraw_color = color_by_code.get(code)

    name = str(code)
    if ldraw_color is not None:
        name = f"{code} {ldraw_color.name}"
        if is_slope:
            name += " slope"

    material = bpy.data.materials.get(name)
    if material is not None:
        return material

    # TODO: Report warnings if a part contains an invalid color code.
    material = bpy.data.materials.new(name)
    material.use_nodes = True

    # Create the nodes from scratch to ensure the required nodes are present.
    # This avoids hard coding names like "Material Output" that depend on the UI language.
    material.node_tree.nodes.clear()

    graph = NodeGraph(material.node_tree)

    # TODO: Error if color is missing?
    r, g, b, a = 1.0, 1.0, 1.0, 1.0
    if ldraw_color is not None:
        r, g, b, a = ldraw_color.rgba_linear

    bsdf = graph.node(
        ShaderNodeBsdfPrincipled,
        location=(-240, 462),
        # RANDOM_WALK is more accurate but has discoloration around thin corners.
        # TODO: This is in Blender units and should depend on scene scale
        subsurface_method="BURLEY",
        inputs={
            # Alpha is specified using transmission instead.
            "Base Color": (r, g, b, 1.0),
            # Use a less accurate SSS method instead.
            "Subsurface Radius": (r, g, b),
            "Subsurface Weight": 1.0,
            "Subsurface Scale": 0.025,
        },
    )

    output = graph.node(
        ShaderNodeOutputMaterial,
        location=(60, 462),
        inputs={"Surface": bsdf},
    )

    # Set the color in the viewport.
    # This can use the default LDraw color for familiarity.
    material.diffuse_color = (r, g, b, a)

    # Partially complete alternatives to LDraw colors for better realism.
    if code in rgb_ldr_tools_by_code:
        r, g, b = rgb_ldr_tools_by_code[code]
    elif code in rgb_peeron_by_code:
        r, g, b = rgb_peeron_by_code[code]

    # Procedural roughness.
    roughness_node = create_node_group(
        material, "ldr_tools_roughness", create_roughness_node_group
    )
    roughness_node.location = (-430, 500)

    bsdf["Roughness"] = roughness_node

    # Normal opaque materials.
    metal = 0.0
    rough = (0.075, 0.2)

    finish_name = "" if ldraw_color is None else ldraw_color.finish_name

    # TODO: Have a case for each finish type?
    match finish_name:
        case "MatteMetallic":
            metal = 1.0
        case "Chrome":
            # Glossy metal coating.
            metal = 1.0
            rough = (0.075, 0.1)
        case "Metal":
            # Rougher metals.
            metal = 1.0
            rough = (0.15, 0.3)
        case "Pearlescent":
            metal = 0.35
            rough = (0.3, 0.5)
        case "Speckle":
            # TODO: Are all speckled colors metals?
            metal = 1.0

            speckle_node = create_node_group(
                material, "ldr_tools_speckle", create_speckle_node_group
            )
            speckle_node.location = (-620, 700)

            # Adjust the thresholds to control speckle size and density.
            speckle_node.inputs["Min"].default_value = 0.5
            speckle_node.inputs["Max"].default_value = 0.6

            speckle_r, speckle_g, speckle_b, _ = ldraw_color.speckle_rgba_linear

            # Blend between the two speckle colors.
            mix_rgb = graph.node(
                ShaderNodeMix,
                data_type="RGBA",
                location=(-430, 700),
                inputs={
                    "Factor": speckle_node,
                    "A": (r, g, b, 1.0),
                    "B": (speckle_r, speckle_g, speckle_b, 1.0),
                },
            )

            bsdf["Base Color"] = mix_rgb

    # Transparent colors specify an alpha of 128 / 255.
    if a <= 0.6:
        bsdf["Transmission Weight"] = 1.0
        bsdf["IOR"] = 1.55

        if ldraw_color.finish_name == "Rubber":
            # Make the transparent rubber appear cloudy.
            rough = (0.1, 0.35)
        else:
            rough = (0.01, 0.15)

    roughness_node.inputs["Min"].default_value = rough[0]
    roughness_node.inputs["Max"].default_value = rough[1]
    bsdf["Metallic"] = metal

    # Procedural normals.
    normals = create_node_group(material, "ldr_tools_normal", create_normals_node_group)
    normals.location = (-620, 202)

    if is_slope:
        # Apply grainy normals to faces that aren't vertical or horizontal.
        # Use non transformed normals to not consider object rotation.
        ldr_normals = graph.node(
            ShaderNodeAttribute, location=(-1600, 400), attribute_name="ldr_normals"
        )

        separate = graph.node(
            ShaderNodeSeparateXYZ, location=(-1400, 400), inputs=[ldr_normals["Vector"]]
        )

        # Use normal.y to check if the face is horizontal (-1.0 or 1.0) or vertical (0.0).
        # Any values in between are considered "slopes" and use grainy normals.
        absolute = graph.node(
            ShaderNodeMath,
            location=(-1200, 400),
            operation="ABSOLUTE",
            inputs=[separate["Y"]],
        )

        compare = graph.node(
            ShaderNodeMath,
            location=(-1000, 400),
            operation="COMPARE",
            inputs=[absolute, 0.5, 0.45],
        )

        slope_normals = create_node_group(
            material, "ldr_tools_slope_normal", create_slope_normals_node_group
        )
        slope_normals.location = (-630, 100)

        is_stud = graph.node(
            ShaderNodeAttribute, location=(-1000, 200), attribute_name="ldr_is_stud"
        )

        # Don't apply the grainy slopes to any faces marked as studs.
        # We use an attribute here to avoid per face material assignment.
        subtract_studs = graph.node(
            ShaderNodeMath,
            location=(-800, 400),
            operation="SUBTRACT",
            inputs=[compare, is_stud["Fac"]],
        )

        # Choose between grainy and smooth normals depending on the face.
        mix_normals = graph.node(
            ShaderNodeMix,
            location=(-430, 330),
            data_type="VECTOR",
            inputs={
                "Factor": subtract_studs,
                "A": normals,
                "B": slope_normals,
            },
        )

        # The second output is the vector output.
        bsdf["Normal"] = mix_normals
    else:
        bsdf["Normal"] = normals

    return material


def create_node_group(
    material: Material,
    name: str,
    create_group: Callable[[str], NodeTree],
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
