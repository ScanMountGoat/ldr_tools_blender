import typing
from typing import Callable, TypeVar

if typing.TYPE_CHECKING:
    from ldr_tools_py import LDrawColor
else:
    from .ldr_tools_py import LDrawColor

from .colors import rgb_peeron_by_code, rgb_ldr_tools_by_code
from .node_dsl import NodeGraph, GraphNode, NodeInput, ShaderGraph, group

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
    ShaderNodeTexImage,
    ShaderNodeVectorTransform,
    ShaderNodeVectorMath,
)

# Materials are based on the techniques described in the following blog posts.
# This covers how to create lego shaders with realistic surface detailing.
# https://stefanmuller.com/exploring-lego-material-part-1/
# https://stefanmuller.com/exploring-lego-material-part-2/
# https://stefanmuller.com/exploring-lego-material-part-3/


def get_material(
    color_by_code: dict[int, LDrawColor],
    code: int,
    is_slope: bool,
    image: bpy.types.Image | None = None,
) -> Material:
    # Cache materials by name.
    # This loads materials lazily to avoid creating unused colors.
    ldraw_color = color_by_code.get(code)

    name = str(code)
    if ldraw_color is not None:
        name = f"{code} {ldraw_color.name}"
        if is_slope:
            name += " slope"

    if image is not None:
        name += f" {image.name}"

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

            # Adjust the thresholds to control speckle size and density.
            speckle_node = graph.group_node(
                speckle_node_group(), {"Min": 0.5, "Max": 0.6}
            )
            speckle_node.node.location = (-620, 700)

            speckle_r, speckle_g, speckle_b, _ = ldraw_color.speckle_rgba_linear

            # Blend between the two speckle colors.
            base_color = graph.node(
                ShaderNodeMix,
                data_type="RGBA",
                inputs={
                    "Factor": speckle_node,
                    "A": base_color,
                    "B": (speckle_r, speckle_g, speckle_b, 1.0),
                },
            )
            base_color.node.location = (-430, 750)

    # Transparent colors specify an alpha of 128 / 255.
    if a <= 0.6:
        transmission = 1.0
        refraction = 1.55
        if finish_name == "Rubber":
            # Make transparent rubber appear cloudy.
            roughness = (0.1, 0.35)
        else:
            roughness = (0.01, 0.15)

    if image is not None:
        texture = graph.node(ShaderNodeTexImage, image=image)
        texture.node.location = (-730, 800)

        base_color = graph.node(
            ShaderNodeMix,
            data_type="RGBA",
            inputs={"Factor": texture["Alpha"], "A": base_color, "B": texture["Color"]},
        )
        base_color.node.location = (-430, 750)

    # Procedural roughness.
    roughness_node = graph.group_node(
        roughness_node_group(),
        {"Min": roughness[0], "Max": roughness[1]},
    )
    roughness_node.node.location = (-430, 500)

    # Procedural normals.
    main_normals = graph.group_node(normals_node_group())
    main_normals.node.location = (-630, 200)

    normals: GraphNode[ShaderNodeGroup | ShaderNodeMix] = main_normals

    if is_slope:
        is_slope_node = graph.group_node(is_slope_node_group())
        is_slope_node.node.location = (-630, 300)

        slope_normals = graph.group_node(slope_normals_node_group())
        slope_normals.node.location = (-630, 100)

        # Choose between grainy and smooth normals depending on the face.
        normals = graph.node(
            ShaderNodeMix,
            data_type="VECTOR",
            inputs={
                "Factor": is_slope_node,
                "A": main_normals,
                "B": slope_normals,
            },
        )
        normals.node.location = (-430, 330)

    scale = graph.group_node(object_scale_node_group())
    scale.node.location = (-630, 0)

    subsurface_scale = graph.math_node("MULTIPLY", [scale, 2.5])
    subsurface_scale.node.location = (-430, 105)

    bsdf = graph.node(
        ShaderNodeBsdfPrincipled,
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
    bsdf.node.location = (-240, 460)

    output = graph.node(ShaderNodeOutputMaterial, {"Surface": bsdf})
    output.node.location = (60, 460)

    return material


@group("Roughness (ldr_tools)")
def roughness_node_group(graph: ShaderGraph) -> None:
    graph.input(NodeSocketFloat, "Min")
    graph.input(NodeSocketFloat, "Max")
    graph.output(NodeSocketFloat, "Roughness")

    input = graph.node(NodeGroupInput)
    input.node.location = (-480, -300)

    # TODO: Create frame called "smudges" or at least name the nodes.
    noise = graph.node(
        ShaderNodeTexNoise,
        {
            "Scale": 4.0,
            "Detail": 2.0,
            "Roughness": 0.5,
            "Distortion": 0.0,
        },
    )
    noise.node.location = (-480, 0)

    # Easier to configure than a color ramp since the input is 1D.
    map_range = graph.node(
        ShaderNodeMapRange,
        {
            "Value": noise["Fac"],
            "To Min": input["Min"],
            "To Max": input["Max"],
        },
    )
    map_range.node.location = (-240, 0)

    output = graph.node(NodeGroupOutput, [map_range])
    output.node.location = (0, 0)


@group("Speckle (ldr_tools)")
def speckle_node_group(graph: ShaderGraph) -> None:
    graph.input(NodeSocketFloat, "Min")
    graph.input(NodeSocketFloat, "Max")
    graph.output(NodeSocketFloat, "Fac")

    input = graph.node(NodeGroupInput)
    input.node.location = (-480, -300)

    noise = graph.node(
        ShaderNodeTexNoise,
        {
            "Scale": 15.0,
            "Detail": 6.0,
            "Roughness": 1.0,
            "Distortion": 0.0,
        },
    )
    noise.node.location = (-480, 0)

    # Easier to configure than a color ramp since the input is 1D.
    map_range = graph.node(
        ShaderNodeMapRange,
        {
            "Value": noise["Fac"],
            "From Min": input["Min"],
            "From Max": input["Max"],
        },
    )
    map_range.node.location = (-240, 0)

    output = graph.node(NodeGroupOutput, [map_range])
    output.node.location = (0, 0)


@group("Normals (ldr_tools)")
def normals_node_group(graph: ShaderGraph) -> None:
    graph.output(NodeSocketVector, "Normal")

    scale = graph.group_node(object_scale_node_group())
    scale.node.location = (-720, 100)

    tex_coord = graph.node(ShaderNodeTexCoord)
    tex_coord.node.location = (-720, 0)

    bevel = graph.node(ShaderNodeBevel, {"Radius": scale})
    bevel.node.location = (-480, -300)

    # Faces of bricks are never perfectly flat.
    # Create a very low frequency noise to break up highlights
    noise = graph.node(
        ShaderNodeTexNoise,
        {
            "Scale": 0.01,
            "Detail": 1.0,
            "Roughness": 1.0,
            # "Distortion": 0.0, # already the default
            "Vector": tex_coord["Object"],
        },
    )
    noise.node.location = (-480, 0)

    bump = graph.node(
        ShaderNodeBump,
        {
            "Strength": 1.0,
            "Distance": scale,
            "Height": noise["Fac"],
            "Normal": bevel,
        },
    )
    bump.node.location = (-240, 0)

    output = graph.node(NodeGroupOutput, [bump])
    output.node.location = (0, 0)


@group("Slope Normals (ldr_tools)")
def slope_normals_node_group(graph: ShaderGraph) -> None:
    graph.output(NodeSocketVector, "Normal")

    scale = graph.group_node(object_scale_node_group())
    scale.node.location = (-720, 100)

    tex_coord = graph.node(ShaderNodeTexCoord)
    tex_coord.node.location = (-720, 0)

    bevel = graph.node(ShaderNodeBevel, {"Radius": scale})
    bevel.node.location = (-480, -300)

    noise = graph.node(
        ShaderNodeTexNoise,
        {
            "Scale": 2.5,
            "Detail": 3.0,
            "Roughness": 0.5,
            # "Lacunarity": 2.0, # already the default
            "Vector": tex_coord["Object"],
        },
    )
    noise.node.location = (-480, 0)

    bump_distance = graph.math_node("MULTIPLY", [scale, 0.5])
    bump_distance.node.location = (-480, 165)

    bump = graph.node(
        ShaderNodeBump,
        {
            "Strength": 0.5,
            "Distance": bump_distance,
            "Height": noise["Fac"],
            "Normal": bevel,
        },
    )
    bump.node.location = (-240, 0)

    output = graph.node(NodeGroupOutput, [bump])
    output.node.location = (0, 0)


@group("Is Slope (ldr_tools)")
def is_slope_node_group(graph: ShaderGraph) -> None:
    graph.output(NodeSocketFloat, "Factor")

    # Apply grainy normals to faces that aren't vertical or horizontal.
    # Use non transformed normals to not consider object rotation.
    ldr_normals = graph.node(ShaderNodeAttribute, attribute_name="ldr_normals")
    ldr_normals.node.location = (-1600, 400)

    separate = graph.node(ShaderNodeSeparateXYZ, [ldr_normals["Vector"]])
    separate.node.location = (-1400, 400)

    # Use normal.y to check if the face is horizontal (-1.0 or 1.0) or vertical (0.0).
    # Any values in between are considered "slopes" and use grainy normals.
    absolute = graph.math_node("ABSOLUTE", [separate["Y"]])
    absolute.node.location = (-1200, 400)
    compare = graph.math_node("COMPARE", [absolute, 0.5, 0.45])
    compare.node.location = (-1000, 400)

    slope_normals = graph.group_node(slope_normals_node_group())
    slope_normals.node.location = (-630, 100)

    is_stud = graph.node(ShaderNodeAttribute, attribute_name="ldr_is_stud")
    is_stud.node.location = (-1000, 200)

    # Don't apply the grainy slopes to any faces marked as studs.
    # We use an attribute here to avoid per face material assignment.
    subtract_studs = graph.math_node("SUBTRACT", [compare, is_stud["Fac"]])
    subtract_studs.node.location = (-800, 400)

    output = graph.node(NodeGroupOutput, [subtract_studs])
    output.node.location = (-600, 400)


@group("Object Scale (ldr_tools)")
def object_scale_node_group(graph: ShaderGraph) -> None:
    # Extract the magnitude of the object space scale.
    graph.output(NodeSocketFloat, "Value")

    transform = graph.node(
        ShaderNodeVectorTransform,
        vector_type="VECTOR",
        convert_from="OBJECT",
        convert_to="WORLD",
        inputs=[(1.0, 0.0, 0.0)],
    )
    transform.node.location = (0, 0)

    length = graph.node(ShaderNodeVectorMath, operation="LENGTH", inputs=[transform])
    length.node.location = (200, 0)

    output = graph.node(NodeGroupOutput, [length])
    output.node.location = (400, 0)
