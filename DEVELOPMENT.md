# Development
This document outlines the basic process for working on this addon. 
This project utilizes Rust as well as Python code, so the process is slightly more complicated than working with pure Python addons.

## IDE and Code Completion
Blender has its own modules. Install [fake-bpy-module](https://github.com/nutti/fake-bpy-module) using Pip to get autocompletion and type hints in your editor of choice. This module doesn't actually contain Blender's Python code. It just serves to make the development process easier.

## Code Formatting
Python code should be formatted according to PEP 8 style. This can be done easily in VS Code by installing the Python extension, setting the formatter to autopep8, and running the format document command (Alt+Shift+F).

Rust code should be formatted by running the `cargo fmt` command. This can also be done in VS Code using the Rust Analyzer extension and using the format document command (Alt+Shift+F). Running code lints with `cargo clippy` is also recommended.

## [Blender Python API Docs](https://docs.blender.org/api/current/index.html)
Blender's docs describe the Python API for the current version with all the types and functions available to use. Sadly, the docs don't do a great job at explaining how the code works or why you should use one method compared to another. If you have any questions, please reach out via posting a comment on an issue or Pull request you plan on working on.

## Building
### Prerequisites
This project uses both Python and Rust code.The latest version of the Rust toolchain can be installed from https://www.rust-lang.org/. The Python version must match Blender's Python version for the ldr_tools_py module to import properly. This is currently Python 3.10 for Blender 3.3 or later. It's recommended to create and activate virtual environment with the appropriate Python 
version to avoid any issues when building.

### Building the Libraries
Building the library code is as simple as running `cargo build --release` from terminal or command line. Don't forget the `--release` since debug builds in Rust will not perform well. Note that the Python extension module in the `target/` directory will only work for the version of Python used when building. 

When building for Blender, the Python interpreter used when building must match the version used by Blender. The easiest way to do this is to activate a virtual environment with the appropriate verson or use the Python bundled with Blender itself. See the [PyO3 guide](https://pyo3.rs) for details. The Python path should be set appropriately depending on the version and location of Blender. For MacOS and Blender 3.6, the build command would be `PYO3_PYTHON="/Applications/Blender.app/Contents/Resources/3.6/python/bin/python3.10" cargo build --release`. 

### Building the Addon
The Blender addon uses the Rust code to simplify the addon code and take advantage of the performance and reliability of Rust. A precompiled binary is not provided for ldr_tools_py, so it will need to be built before installing the addon in Blender. Follow the instructions to build the libaries. This will generate a file like `target/release/ldr_tools_py.dll` or `target/release/libldr_tools_py.dylib`. Change the extension from `.dll` to `.pyd` or `.dylib` to `.so` depending on the platform. The `lib` prefix should also be removed from the filename. This compiled file can be imported like any other Python module. If the import fails, check that the file is in the correct folder, has the right extension, and was compiled using the correct Python version.

Blender loads addons with multiple files from zip files, so place the contents of the `ldr_tools_blender` folder and the native Python module from earlier in a zip file. This zip file can than be installed from the addons menu in Blender and enabled as the `ldr_tools_blender` addon. This addon will only work on the current operating system and target like 64-bit Windows with an x86 processor. The Rust code can easily be compiled for other targets and operating systems like Apple Silicon Macs as needed.

## Running Blender from Terminal
Blender can be run scripts in headless mode without ever loading the UI. This can be a quick way to test that importing works without any errors. See the [Blender tips and tricks](https://docs.blender.org/api/current/info_tips_and_tricks.html#use-blender-without-it-s-user-interface) for information. For example, running `blender --background --python script.py` with the following simple script will call the main import function.

```python
# script.py
import bpy
bpy.ops.import_scene.importldr(filepath="model.ldr")
```

## Reloading Changes
The process of uninstalling and reinstalling the addon when making a new change can be time consuming. Thankfully, this can be almost entirely automated using a script. Simply close Blender, run a script to overwrite the files in the installed addon directory, and reopen Blender. 

Sample scripts for different operating systems are provided below. Note that these scripts will also install the addon if it hasn't been installed already. Addon "installation" in Blender is just the process of moving the folder into the addons directory. Make sure to set the appropriate username and version of Blender!

### Windows
```bat
@REM reload.bat
set OUTPUT=%appdata%\Blender Foundation\Blender\4.0\scripts\addons\ldr_tools_blender
xcopy /E/I/Y "ldr_tools_blender" "%OUTPUT%" 
copy /y "target\release\ldr_tools_py.dll" "%OUTPUT%\ldr_tools_py.pyd"
```

### MacOS
```sh
# reload.sh
OUTPUT="$HOME/library/Application Support/Blender/4.0/scripts/addons/ldr_tools_blender/"
cp -a ldr_tools_blender/. "$OUTPUT"
cp target/release/libldr_tools_py.dylib "$OUTPUT/ldr_tools_py.so"
```

## Troubleshooting Loading Errors
The addon will not be enabled if the code has errors. Check the addon preferences to check if any error messages come up when trying to manually enable the addon. After fixing the error, close Blender and reload the addon using the script. You will need to manually enable the addon again from the preferences menu after opening Blender.
