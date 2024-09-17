import os
import json
import bpy
from bpy.props import StringProperty, EnumProperty, BoolProperty, FloatProperty
from bpy_extras.io_utils import ImportHelper
import typing
from typing import Any
import platform

from .importldr import import_ldraw

if typing.TYPE_CHECKING:
    import ldr_tools_py
else:
    from . import ldr_tools_py

Status: typing.TypeAlias = set[
    typing.Literal[
        "RUNNING_MODAL", "CANCELLED", "FINISHED", "PASS_THROUGH", "INTERFACE"
    ]
]


def find_ldraw_library() -> str:
    # Get list of possible ldraw installation directories for the platform
    if platform.system() == "Windows":
        # Windows
        directories = [
            "C:\\LDraw",
            "C:\\Program Files\\LDraw",
            "C:\\Program Files (x86)\\LDraw",
            "C:\\Program Files\\Studio 2.0\\ldraw",
            "~\\Documents\\LDraw",
            "~\\Documents\\ldraw",
            "C:\\Users\\Public\\Documents\\LDraw",
            "C:\\Users\\Public\\Documents\\ldraw",
        ]
    elif platform.system() == "Darwin":
        # MacOS
        directories = [
            "~/ldraw/",
            "/Applications/LDraw/",
            "/Applications/ldraw/",
            "/usr/local/share/ldraw",
            "/Applications/Studio 2.0/ldraw",
            "~/Documents/ldraw",
        ]
    else:
        # Linux
        directories = [
            "~/LDraw",
            "~/ldraw",
            "~/.LDraw",
            "~/.ldraw",
            "/usr/local/share/ldraw",
        ]

    # Find a directory that looks like an LDraw library.
    for dir in directories:
        dir = os.path.expanduser(dir)
        if os.path.isfile(os.path.join(dir, "LDConfig.ldr")):
            return dir

    return ""


class Preferences:
    preferences_path = os.path.join(os.path.dirname(__file__), "preferences.json")

    def __init__(self) -> None:
        self.ldraw_path = find_ldraw_library()
        self.instance_type = "LinkedDuplicates"
        self.stud_type = "Logo4"
        self.primitive_resolution = "Normal"
        self.additional_paths: list[str] = []
        self.add_gap_between_parts = True
        # default matches hardcoded behavior of previous versions
        self.scene_scale = 0.01

    def from_dict(self, dict: dict[str, Any]) -> None:
        # Fill in defaults for any missing values.
        defaults = Preferences()
        self.ldraw_path = dict.get("ldraw_path", defaults.ldraw_path)
        self.instance_type = dict.get("instance_type", defaults.instance_type)
        self.stud_type = dict.get("stud_type", defaults.stud_type)
        self.primitive_resolution = dict.get(
            "primitive_resolution", defaults.primitive_resolution
        )
        self.additional_paths = dict.get("additional_paths", defaults.additional_paths)
        self.add_gap_between_parts = dict.get(
            "add_gap_between_parts", defaults.add_gap_between_parts
        )
        self.scene_scale = dict.get("scene_scale", defaults.scene_scale)

    def save(self) -> None:
        with open(Preferences.preferences_path, "w+") as file:
            json.dump(self, file, default=lambda o: o.__dict__)

    @staticmethod
    def load() -> Preferences:
        preferences = Preferences()
        try:
            with open(Preferences.preferences_path, "r") as file:
                preferences.from_dict(json.load(file))
        except Exception:
            # Set defaults if the loading fails.
            preferences = Preferences()

        return preferences


class LIST_OT_NewItem(bpy.types.Operator):
    """Add a new item to the list."""

    bl_idname = "additional_paths.new_item"
    bl_label = "Add a new item"

    def execute(self, context: bpy.types.Context) -> Status:
        # TODO: Don't store the preferences in the operator itself?
        # TODO: singleton pattern?
        p = context.scene.ldr_path_to_add  # type: ignore[attr-defined]
        ImportOperator.preferences.additional_paths.append(p)
        return {"FINISHED"}


class LIST_OT_DeleteItem(bpy.types.Operator):
    """Delete the selected item from the list."""

    bl_idname = "additional_paths.delete_item"
    bl_label = "Deletes an item"

    @classmethod
    def poll(cls, context: bpy.types.Context) -> list[str]:  # type: ignore[override]
        return ImportOperator.preferences.additional_paths

    def execute(self, context: bpy.types.Context) -> Status:
        ImportOperator.preferences.additional_paths.pop()
        return {"FINISHED"}


class ImportOperator(bpy.types.Operator, ImportHelper):
    bl_idname = "import_scene.importldr"
    bl_description = "Import LDR (.mpd/.ldr/.dat/.io)"
    bl_label = "Import LDR"
    bl_space_type = "PROPERTIES"
    bl_region_type = "WINDOW"
    bl_options = {"REGISTER", "UNDO", "PRESET"}

    preferences = Preferences.load()

    # TODO: Consistent usage of "" vs ''
    # File type filter in file browser
    filename_ext = ".ldr"

    if typing.TYPE_CHECKING:
        filter_glob: str
        ldraw_path: str
        instance_type: typing.Literal["LinkedDuplicates", "GeometryNodes"]
        stud_type: typing.Literal["Disabled", "Normal", "Logo4", "HighContrast"]
        primitive_resolution: typing.Literal["Low", "Normal", "High"]
        add_gap_between_parts: bool
        scene_scale: float
    else:
        filter_glob: StringProperty(
            default="*.mpd;*.ldr;*.dat;*.io", options={"HIDDEN"}
        )

        ldraw_path: StringProperty(name="LDraw Library", default=preferences.ldraw_path)

        instance_type: EnumProperty(
            name="Instance Type",
            items=[
                (
                    "LinkedDuplicates",
                    "Linked Duplicates",
                    "Objects with linked mesh data blocks (Alt+D). Easy to edit.",
                ),
                (
                    "GeometryNodes",
                    "Geometry Nodes",
                    "Geometry node instances on an instancer mesh. Faster imports for large scenes but harder to edit.",
                ),
            ],
            description="The method to use for instancing part meshes",
            # TODO: this doesn't set properly?
            default=preferences.instance_type,
        )

        stud_type: EnumProperty(
            name="Stud Type",
            items=[
                ("Disabled", "Disabled", "No studs"),
                ("Normal", "Normal", "Studs without logos"),
                ("Logo4", "Logo4", "Studs with modeled LEGO logos"),
                (
                    "HighContrast",
                    "High Contrast",
                    "Studs with instruction style colors",
                ),
            ],
            description="The type of stud for imported parts",
            # TODO: this doesn't set properly?
            default=preferences.stud_type,
        )

        primitive_resolution: EnumProperty(
            name="Resolution",
            items=[
                ("Low", "Low", "Low resolution 8 segment primitives"),
                ("Normal", "Normal", "Normal resolution 16 segment primitives"),
                ("High", "High", "High resolution 48 segment primitives"),
            ],
            description="The segment quality for part primitives",
            # TODO: this doesn't set properly?
            default=preferences.primitive_resolution,
        )

        add_gap_between_parts: BoolProperty(
            name="Gap Between Parts",
            description="Scale to add a small gap horizontally between parts",
            default=preferences.add_gap_between_parts,
        )

        scene_scale: FloatProperty(
            name="Scale",
            description="Scale factor for the imported model",
            default=preferences.scene_scale,
        )

    def draw(self, context: bpy.types.Context) -> None:
        layout = self.layout
        layout.use_property_split = True
        layout.prop(self, "ldraw_path")
        layout.prop(self, "instance_type")
        layout.prop(self, "stud_type")
        layout.prop(self, "primitive_resolution")
        layout.prop(self, "add_gap_between_parts")
        layout.prop(self, "scene_scale")

        # TODO: File selector?
        # TODO: Come up with better UI for this?
        layout.label(text="Additional Library Paths")
        for path in ImportOperator.preferences.additional_paths:
            layout.label(text=path)

        row = layout.row()
        row.prop(context.scene, "ldr_path_to_add")
        row.operator("additional_paths.new_item", text="Add Path")
        row = layout.row()
        row.operator("additional_paths.delete_item", text="Remove Path")

    def execute(self, context: bpy.types.Context) -> Status:
        # Update from the UI values to support saving them to disk later.
        ImportOperator.preferences.ldraw_path = self.ldraw_path
        ImportOperator.preferences.instance_type = self.instance_type
        ImportOperator.preferences.stud_type = self.stud_type
        ImportOperator.preferences.primitive_resolution = self.primitive_resolution
        ImportOperator.preferences.add_gap_between_parts = self.add_gap_between_parts
        ImportOperator.preferences.scene_scale = self.scene_scale

        settings = self.get_settings()

        import time

        start = time.time()
        import_ldraw(
            self,
            self.filepath,  # type: ignore[attr-defined]
            self.ldraw_path,
            ImportOperator.preferences.additional_paths,
            self.instance_type,
            settings,
        )
        end = time.time()
        print(f"Import: {end - start}")

        # Save preferences to disk for loading next time.
        ImportOperator.preferences.save()
        return {"FINISHED"}

    def get_settings(self):
        settings = ldr_tools_py.GeometrySettings()
        settings.triangulate = False
        settings.add_gap_between_parts = self.add_gap_between_parts

        if self.stud_type == "Disabled":
            settings.stud_type = ldr_tools_py.StudType.Disabled
        elif self.stud_type == "Normal":
            settings.stud_type = ldr_tools_py.StudType.Normal
        elif self.stud_type == "Logo4":
            settings.stud_type = ldr_tools_py.StudType.Logo4
        elif self.stud_type == "HighContrast":
            settings.stud_type = ldr_tools_py.StudType.HighContrast

        if self.primitive_resolution == "Low":
            settings.primitive_resolution = ldr_tools_py.PrimitiveResolution.Low
        elif self.primitive_resolution == "Normal":
            settings.primitive_resolution = ldr_tools_py.PrimitiveResolution.Normal
        elif self.primitive_resolution == "High":
            settings.primitive_resolution = ldr_tools_py.PrimitiveResolution.High

        settings.scene_scale = self.scene_scale
        # Required for calculated normals.
        settings.weld_vertices = True

        return settings
