import bpy
from . import operator

bl_info = {
    "name": "ldr_tools_blender",
    "description": "Import LDraw models in .mpd .ldr .l3b and .dat formats",
    "author": "ScanMountGoat (SMG)",
    "version": (0, 1, 0),
    "blender": (3, 3, 0),
    "location": "File > Import",
    "warning": "",
    "wiki_url": "https://github.com/ScanMountGoat/ldr_tools_blender",
    "tracker_url": "https://github.com/ScanMountGoat/ldr_tools_blender/issues",
    "category": "Import-Export"
}


def menuImport(self, context):
    self.layout.operator(operator.ImportOperator.bl_idname,
                         text="LDraw (.mpd/.ldr/.dat)")


def register():
    bpy.utils.register_class(operator.ImportOperator)
    bpy.types.TOPBAR_MT_file_import.append(menuImport)


def unregister():
    bpy.utils.unregister_class(operator.ImportOperator)
    bpy.types.TOPBAR_MT_file_import.remove(menuImport)


if __name__ == "__main__":
    register()
