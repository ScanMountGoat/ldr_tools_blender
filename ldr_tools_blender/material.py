import typing
from typing import Callable, TypeVar

if typing.TYPE_CHECKING:
    from ldr_tools_py import LDrawColor
else:
    from .ldr_tools_py import LDrawColor

from .colors import rgb_peeron_by_code, rgb_ldr_tools_by_code
from .node_dsl import NodeGraph, GraphNode, NodeInput

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
    ShaderNodeAttribute,
    ShaderNodeMath,
    ShaderNodeMix,
    ShaderNodeOutputMaterial,
    ShaderNodeSeparateXYZ,
    ShaderNodeGroup,
    ShaderNodeVectorTransform,
    ShaderNodeVectorMath,
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

    # Set the color in the viewport.
    # This can use the default LDraw color for familiarity.
    material.diffuse_color = (r, g, b, a)

    # Partially complete alternatives to LDraw colors for better realism.
    if code in rgb_ldr_tools_by_code:
        r, g, b = rgb_ldr_tools_by_code[code]
    elif code in rgb_peeron_by_code:
        r, g, b = rgb_peeron_by_code[code]

    # For speckle materials, this will be reassigned to a node reference later.
    base_color: tuple[float, float, float, float] | GraphNode[ShaderNodeMix]
    # Alpha is specified using transmission instead.
    base_color = (r, g, b, 1.0)

    # Normal opaque materials.
    metallicity = 0.0
    roughness = (0.075, 0.2)
    transmission = 0.0
    refraction = 1.5

    finish_name = "" if ldraw_color is None else ldraw_color.finish_name
    match finish_name:
        case "MatteMetallic":
            metallicity = 1.0
        case "Chrome":
            # Glossy metal coating.
            metallicity = 1.0
            roughness = (0.075, 0.1)
        case "Metal":
            # Rougher metals.
            metallicity = 1.0
            roughness = (0.15, 0.3)
        case "Pearlescent":
            metallicity = 0.35
            roughness = (0.3, 0.5)
        case "Speckle":
            # TODO: Are all speckled colors metals?
            metallicity = 1.0

            speckle_node = graph.node(
                ShaderNodeGroup,
                location=(-620, 700),
                node_tree=speckle_node_group(),
                # Adjust the thresholds to control speckle size and density.
                inputs={"Min": 0.5, "Max": 0.6},
            )

            speckle_r, speckle_g, speckle_b, _ = ldraw_color.speckle_rgba_linear

            # Blend between the two speckle colors.
            base_color = graph.node(
                ShaderNodeMix,
                data_type="RGBA",
                location=(-430, 750),
                inputs={
                    "Factor": speckle_node,
                    "A": base_color,
                    "B": (speckle_r, speckle_g, speckle_b, 1.0),
                },
            )

    # Transparent colors specify an alpha of 128 / 255.
    if a <= 0.6:
        transmission = 1.0
        refraction = 1.55
        if finish_name == "Rubber":
            # Make transparent rubber appear cloudy.
            roughness = (0.1, 0.35)
        else:
            roughness = (0.01, 0.15)

    # Procedural roughness.
    roughness_node = graph.node(
        ShaderNodeGroup,
        location=(-430, 500),
        node_tree=roughness_node_group(),
        inputs={"Min": roughness[0], "Max": roughness[1]},
    )

    # Procedural normals.
    main_normals = graph.node(
        ShaderNodeGroup, location=(-630, 200), node_tree=normals_node_group()
    )

    normals: GraphNode[ShaderNodeGroup | ShaderNodeMix] = main_normals

    if is_slope:
        is_slope_node = graph.node(
            ShaderNodeGroup, location=(-630, 300), node_tree=is_slope_node_group()
        )

        slope_normals = graph.node(
            ShaderNodeGroup, location=(-630, 100), node_tree=slope_normals_node_group()
        )

        # Choose between grainy and smooth normals depending on the face.
        normals = graph.node(
            ShaderNodeMix,
            location=(-430, 330),
            data_type="VECTOR",
            inputs={
                "Factor": is_slope_node,
                "A": main_normals,
                "B": slope_normals,
            },
        )

    scale = graph.node(
        ShaderNodeGroup, location=(-630, 0), node_tree=object_scale_node_group()
    )

    subsurface_scale = graph.node(
        ShaderNodeMath, location=(-430, 105), operation="MULTIPLY", inputs=[scale, 2.5]
    )

    bsdf = graph.node(
        ShaderNodeBsdfPrincipled,
        location=(-240, 460),
        # RANDOM_WALK is more accurate but has discoloration around thin corners.
        subsurface_method="BURLEY",
        inputs={
            "Base Color": base_color,
            "Normal": normals,
            # Use a less accurate SSS method instead.
            "Subsurface Radius": (r, g, b),
            "Subsurface Weight": 1.0,
            "Subsurface Scale": subsurface_scale,
            "Roughness": roughness_node,
            "Metallic": metallicity,
            "Transmission Weight": transmission,
            "IOR": refraction,
        },
    )

    graph.node(ShaderNodeOutputMaterial, location=(60, 460), inputs={"Surface": bsdf})

    return material


def _shader_node_group(name: str) -> tuple[ShaderNodeTree, bool]:
    tree = bpy.data.node_groups.get(name)
    existing = tree is not None
    tree = tree or bpy.data.node_groups.new(name, "ShaderNodeTree")  # type: ignore[arg-type]
    assert isinstance(tree, ShaderNodeTree)
    return tree, existing


def roughness_node_group() -> ShaderNodeTree:
    tree, existing = _shader_node_group("Roughness (ldr_tools)")
    if existing:
        return tree

    graph = NodeGraph(tree)

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


def speckle_node_group() -> ShaderNodeTree:
    tree, existing = _shader_node_group("Speckle (ldr_tools)")
    if existing:
        return tree

    graph = NodeGraph(tree)

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


def normals_node_group() -> ShaderNodeTree:
    tree, existing = _shader_node_group("Normals (ldr_tools)")
    if existing:
        return tree

    graph = NodeGraph(tree)

    graph.output(NodeSocketVector, "Normal")

    scale = graph.node(
        ShaderNodeGroup, location=(-720, 100), node_tree=object_scale_node_group()
    )

    tex_coord = graph.node(ShaderNodeTexCoord, location=(-720, 0))

    bevel = graph.node(ShaderNodeBevel, location=(-480, -300), inputs={"Radius": scale})

    # Faces of bricks are never perfectly flat.
    # Create a very low frequency noise to break up highlights
    noise = graph.node(
        ShaderNodeTexNoise,
        location=(-480, 0),
        inputs={
            "Scale": 0.01,
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
            "Distance": scale,
            "Height": noise["Fac"],
            "Normal": bevel,
        },
    )

    graph.node(NodeGroupOutput, location=(0, 0), inputs=[bump])
    return graph.tree


def slope_normals_node_group() -> ShaderNodeTree:
    tree, existing = _shader_node_group("Slope Normals (ldr_tools)")
    if existing:
        return tree

    graph = NodeGraph(tree)

    graph.output(NodeSocketVector, "Normal")

    scale = graph.node(
        ShaderNodeGroup, location=(-720, 100), node_tree=object_scale_node_group()
    )
    tex_coord = graph.node(ShaderNodeTexCoord, location=(-720, 0))

    bevel = graph.node(ShaderNodeBevel, location=(-480, -300), inputs={"Radius": scale})

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

    bump_distance = graph.node(
        ShaderNodeMath, location=(-480, 165), operation="MULTIPLY", inputs=[scale, 0.5]
    )

    bump = graph.node(
        ShaderNodeBump,
        location=(-240, 0),
        inputs={
            "Strength": 0.5,
            "Distance": bump_distance,
            "Height": noise["Fac"],
            "Normal": bevel,
        },
    )

    graph.node(NodeGroupOutput, location=(0, 0), inputs=[bump])
    return graph.tree


def is_slope_node_group() -> ShaderNodeTree:
    tree, existing = _shader_node_group("Is Slope (ldr_tools)")
    if existing:
        return tree

    graph = NodeGraph(tree)

    graph.output(NodeSocketFloat, "Factor")

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

    slope_normals = graph.node(
        ShaderNodeGroup, location=(-630, 100), node_tree=slope_normals_node_group()
    )

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

    graph.node(NodeGroupOutput, location=(-600, 400), inputs=[subtract_studs])
    return graph.tree


def object_scale_node_group() -> NodeTree:
    tree, existing = _shader_node_group("Object Scale (ldr_tools)")
    if existing:
        return tree

    graph = NodeGraph(tree)

    # Extract the magnitude of the object space scale.
    graph.output(NodeSocketFloat, "Value")

    transform = graph.node(
        ShaderNodeVectorTransform,
        vector_type="VECTOR",
        convert_from="OBJECT",
        convert_to="WORLD",
        inputs=[(1.0, 0.0, 0.0)],
    )

    length = graph.node(
        ShaderNodeVectorMath, operation="LENGTH", location=(200, 0), inputs=[transform]
    )

    graph.node(NodeGroupOutput, location=(400, 0), inputs=[length])
    return graph.tree
