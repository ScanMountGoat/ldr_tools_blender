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
    "doc_url": "https://github.com/ScanMountGoat/ldr_tools_blender/wiki",
    "tracker_url": "https://github.com/ScanMountGoat/ldr_tools_blender/issues",
    "category": "Import-Export"
}


def menuImport(self, context):
    self.layout.operator(operator.ImportOperator.bl_idname,
                         text="LDraw (.mpd/.ldr/.dat)")
    
classes = [operator.ImportOperator, operator.LIST_OT_NewItem, operator.LIST_OT_DeleteItem]

def register():
    for cls in classes:
        bpy.utils.register_class(cls)

    bpy.types.Scene.ldr_path_to_add = bpy.props.StringProperty(name="", description="Additional LDraw parts path")

    bpy.types.TOPBAR_MT_file_import.append(menuImport)


def unregister():
    for cls in classes:
        bpy.utils.unregister_class(cls)

    del bpy.types.Scene.ldr_path_to_add

    bpy.types.TOPBAR_MT_file_import.remove(menuImport)


if __name__ == "__main__":
    register()