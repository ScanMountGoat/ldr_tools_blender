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
### Prerequisites
This project uses both Python and Rust code. The Python version should match the current version of Python used by Blender. 
This is currently Python 3.10 for Blender 3.3 or later. It's recommended to create a virtual environment with the appropriate Python 
version to avoid any issues when building. The latest version of the Rust toolchain can be installed from https://www.rust-lang.org/.

### Building the Libraries
Building the library code is as simple as running `cargo build --release`. Don't forget the `--release` since debug builds in Rust will not perform well. When building the libraries for use in the Blender addon, it's recommended to enable the virtual environment with the appropriate Python version. This ensures that the Python bindings will be built for the version of Python used by Blender. If the versions do not match, Blender will not be able to import the compiled ldr_tools_py native Python module.

### Building the Addon
The Blender addon uses the Rust code to simplify the addon code and take advantage of the performance and reliability of Rust. A precompiled binary is not provided for ldr_tools_py, so it will need to be built before installing the addon in Blender. Follow the instructions to build the libaries. This will generate a file like `target/release/ldr_tools_py.dll` or `target/release/ldr_tools_py.dylib`. Change the extension from `.dll` to `.pyd` or `.dylib` to `.so` depending on the platform. This compiled file can be imported like any other Python module. If the import fails, check that the file is in the correct folder, has the right extension, and was compiled using the correct Python version.

Blender loads addons with multiple files from zip files, so place the contents of the `ldr_tools_blender` folder and the native Python module from earlier in a zip file. This zip file can than be installed from the addons menu in Blender and enabled as the `ldr_tools_blender` addon. This addon will only work on the current operating system and target like 64-bit Windows with an x86 processor. The Rust code can easily be compiled for other targets and operating systems like Apple Silicon Macs as needed.

## Copyrights
LDraw™ is a trademark owned and licensed by the Jessiman Estate, which does not sponsor, endorse, or authorize this project.  
LEGO® is a registered trademark of the LEGO Group, which does not sponsor, endorse, or authorize this project.
