# ldr_tools_blender
A Blender addon for importing LDraw files into Blender 3.3 or later.

## Projects
### ldr_tools
A Rust library for working with LDraw files. This performs all the parsing and geometry handling. This project can be used in 
other Rust projects by adding the following line to the `Cargo.toml`.  

`ldr_tools = { git = "https://github.com/ScanMountGoat/ldr_tools_blender" }` 

### ldr_tools_py
Python bindings to ldr_tools using PyO3. This enables ldr_tools to be usable in Blender. ldr_tools_py makes heavy use of numpy arrays 
to reduce the overhead for converting data from Rust to Python to Blender.

### ldr_tools_blender
The Blender addon for importing LDraw files making use of ldr_tools_py. This is not a pure Python project. See the building instructions for details on how to build this from source.

## Building
The easiest way to see the steps used to create a new release is to check the [release.yaml](https://github.com/ScanMountGoat/ldr_tools_blender/blob/main/.github/workflows/release.yml) script that runs using Github actions. Note that this may not be the most efficient way to develop locally. See [development](https://github.com/ScanMountGoat/ldr_tools_blender/blob/main/DEVELOPMENT.md) for working on your personal machine. The basic process is to build the Rust libraries and Python bindings, copy the native Python module into the addon folder, and create the addon folder ready for users to load into Blender. This needs to be repeated for each supported operating system and CPU architecture.

## Copyrights
LDraw™ is a trademark owned and licensed by the Jessiman Estate, which does not sponsor, endorse, or authorize this project.  
LEGO® is a registered trademark of the LEGO Group, which does not sponsor, endorse, or authorize this project.
