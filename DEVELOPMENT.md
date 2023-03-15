# Development
This document outlines the basic process for working on this addon. 
This project utilizes Rust as well as Python code, so the process is slightly more complicated than working with pure Python addons.

## IDE and Code Completion
Blender has its own modules. Install [fake-bpy-module](https://github.com/nutti/fake-bpy-module) using Pip to get autocompletion and type hints in your editor of choice. This module doesn't actually contain Blender's Python code. It just serves to make the development process easier.

## [Blender Python API Docs](https://docs.blender.org/api/current/index.html)
Blender's docs describe the Python API for the current version with all the types and functions available to use. Sadly, the docs don't do a great job at explaining how the code works or why you should use one method compared to another. If you have any questions, please reach out via posting a comment on an issue or Pull request you plan on working on.

## Building
### Prerequisites
This project uses both Python and Rust code. The Python version should match the current version of Python used by Blender. 
This is currently Python 3.10 for Blender 3.3 or later. It's recommended to create a virtual environment with the appropriate Python 
version to avoid any issues when building. The latest version of the Rust toolchain can be installed from https://www.rust-lang.org/.

### Building the Libraries
Building the library code is as simple as running `cargo build --release` from terminal or command line. Don't forget the `--release` since debug builds in Rust will not perform well. When building the libraries for use in the Blender addon, it's recommended to enable the virtual environment with the appropriate Python version. This ensures that the Python bindings will be built for the version of Python used by Blender. If the versions do not match, Blender will not be able to import the compiled ldr_tools_py native Python module.

MacOS users may experience errors when trying to build the Python bindings. The easiest way to fix this is to install the PyO3's build tool [maturin](https://github.com/PyO3/maturin) using pip. This is the method used by github actions in the CI script for MacOS. Simply call `maturin build --release` from within the `ldr_tools_py` directory. 

### Building the Addon
The Blender addon uses the Rust code to simplify the addon code and take advantage of the performance and reliability of Rust. A precompiled binary is not provided for ldr_tools_py, so it will need to be built before installing the addon in Blender. Follow the instructions to build the libaries. This will generate a file like `target/release/ldr_tools_py.dll` or `target/release/libldr_tools_py.dylib`. Change the extension from `.dll` to `.pyd` or `.dylib` to `.so` depending on the platform. The `lib` prefix should also be removed from the filename. This compiled file can be imported like any other Python module. If the import fails, check that the file is in the correct folder, has the right extension, and was compiled using the correct Python version.

Blender loads addons with multiple files from zip files, so place the contents of the `ldr_tools_blender` folder and the native Python module from earlier in a zip file. This zip file can than be installed from the addons menu in Blender and enabled as the `ldr_tools_blender` addon. This addon will only work on the current operating system and target like 64-bit Windows with an x86 processor. The Rust code can easily be compiled for other targets and operating systems like Apple Silicon Macs as needed.
