import configparser
import os
import bpy
from bpy.props import (StringProperty,
                       FloatProperty,
                       EnumProperty,
                       BoolProperty
                       )
from bpy_extras.io_utils import ImportHelper

from .importldr import importldraw

class ImportOperator(bpy.types.Operator, ImportHelper):
    bl_idname       = "import_scene.importldr"
    bl_description  = "Import LDR (.mpd/.ldr/.dat)"
    bl_label        = "Import LDR"
    bl_space_type   = "PROPERTIES"
    bl_region_type  = "WINDOW"
    bl_options      = {'REGISTER', 'UNDO', 'PRESET'}

    # File type filter in file browser
    filename_ext = ".ldr"
    filter_glob: StringProperty(
        default="*.mpd;*.ldr;*.dat",
        options={'HIDDEN'}
    )

    # TODO: make this an enum for instance_method (linked_duplicates, instance_on_faces, etc)
    instance_on_faces: BoolProperty(
        name="Instance on faces",
        description="Instance parts on the faces of a mesh instead of linking object meshes. Faster imports but harder to edit",
        default=False
    )

    def draw(self, context):
        layout = self.layout
        layout.prop(self, "instance_on_faces")

    def execute(self, context):
        import time
        # TODO: Import the file.
        start = time.time()
        importldraw(self, self.filepath, use_instancing=self.instance_on_faces)
        end = time.time()
        print(f'Import: {end - start}')
        return {'FINISHED'}
