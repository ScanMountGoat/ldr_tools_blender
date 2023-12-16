import os
import json
import bpy
from bpy.props import StringProperty, EnumProperty, BoolProperty
from bpy_extras.io_utils import ImportHelper
from typing import Any
import platform

from .importldr import import_ldraw


def find_ldraw_library() -> str:
    # Get list of possible ldraw installation directories for the platform
    if platform.system() == 'Windows':
        # Windows
        directories = [
            "C:\\LDraw",
            "C:\\Program Files\\LDraw",
            "C:\\Program Files (x86)\\LDraw",
            "C:\\Program Files\\Studio 2.0\\ldraw",
            "~\\Documents\\LDraw",
            "~\\Documents\\ldraw",
            "C:\\Users\\Public\\Documents\\LDraw",
            "C:\\Users\\Public\\Documents\\ldraw"
        ]
    elif platform.system() == 'Darwin':
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

    return ''


class Preferences():
    preferences_path = os.path.join(
        os.path.dirname(__file__), 'preferences.json')

    def __init__(self):
        self.ldraw_path = find_ldraw_library()
        self.instance_type = 'LinkedDuplicates'
        self.additional_paths = []
        self.add_gap_between_parts = True

    def from_dict(self, dict: dict[str, Any]):
        # Fill in defaults for any missing values.
        defaults = Preferences()
        self.ldraw_path = dict.get('ldraw_path', defaults.ldraw_path)
        self.instance_type = dict.get(
            'instance_type', defaults.instance_type)
        self.additional_paths = dict.get(
            'additional_paths', defaults.additional_paths)
        self.add_gap_between_parts = dict.get(
            'add_gap_between_parts', defaults.add_gap_between_parts)

    def save(self):
        with open(Preferences.preferences_path, 'w+') as file:
            json.dump(self, file, default=lambda o: o.__dict__)

    @staticmethod
    def load():
        preferences = Preferences()
        try:
            with open(Preferences.preferences_path, 'r') as file:
                preferences.from_dict(json.load(file))
        except Exception:
            # Set defaults if the loading fails.
            preferences = Preferences()

        return preferences


class LIST_OT_NewItem(bpy.types.Operator):
    """Add a new item to the list."""

    bl_idname = "additional_paths.new_item"
    bl_label = "Add a new item"

    def execute(self, context):
        # TODO: Don't store the preferences in the operator itself?
        # TODO: singleton pattern?
        p = context.scene.ldr_path_to_add
        ImportOperator.preferences.additional_paths.append(p)
        return {'FINISHED'}


class LIST_OT_DeleteItem(bpy.types.Operator):
    """Delete the selected item from the list."""

    bl_idname = "additional_paths.delete_item"
    bl_label = "Deletes an item"

    @classmethod
    def poll(cls, context):
        return ImportOperator.preferences.additional_paths

    def execute(self, context):
        ImportOperator.preferences.additional_paths.pop()
        return {'FINISHED'}


class ImportOperator(bpy.types.Operator, ImportHelper):
    bl_idname = "import_scene.importldr"
    bl_description = "Import LDR (.mpd/.ldr/.dat)"
    bl_label = "Import LDR"
    bl_space_type = "PROPERTIES"
    bl_region_type = "WINDOW"
    bl_options = {'REGISTER', 'UNDO', 'PRESET'}

    preferences = Preferences.load()

    # TODO: Consistent usage of "" vs ''
    # File type filter in file browser
    filename_ext = ".ldr"
    filter_glob: StringProperty(
        default="*.mpd;*.ldr;*.dat",
        options={'HIDDEN'}
    )

    ldraw_path: StringProperty(
        name="LDraw Library",
        default=preferences.ldraw_path
    )

    instance_type: EnumProperty(
        name="Instance Type",
        items=[
            ('LinkedDuplicates', "Linked Duplicates",
             "Objects with linked mesh data blocks (Alt+D). Easy to edit."),
            ('GeometryNodes', "Geometry Nodes",
             "Geometry node instances on an instancer mesh. Faster imports for large scenes but harder to edit.")
        ],
        description="The method to use for instancing part meshes",
        # TODO: this doesn't set properly?
        default=preferences.instance_type
    )

    add_gap_between_parts: BoolProperty(
        name="Gap Between Parts",
        description="Scale to add a small gap horizontally between parts",
        default=preferences.add_gap_between_parts
    )

    def draw(self, context):
        layout = self.layout
        layout.use_property_split = True
        layout.prop(self, "ldraw_path")
        layout.prop(self, "instance_type")
        layout.prop(self, "add_gap_between_parts")

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

    def execute(self, context):
        # Update from the UI values to support saving them to disk later.
        ImportOperator.preferences.ldraw_path = self.ldraw_path
        ImportOperator.preferences.instance_type = self.instance_type
        ImportOperator.preferences.add_gap_between_parts = self.add_gap_between_parts

        import time
        start = time.time()
        import_ldraw(self, self.filepath, self.ldraw_path, ImportOperator.preferences.additional_paths,
                     self.instance_type, self.add_gap_between_parts)
        end = time.time()
        print(f'Import: {end - start}')

        # Save preferences to disk for loading next time.
        ImportOperator.preferences.save()
        return {'FINISHED'}
