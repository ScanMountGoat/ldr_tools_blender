# ldr_tools_blender
A Blender addon for importing LDraw files into Blender 3.3 or later. 

Report bugs or request new features in [issues](https://github.com/ScanMountGoat/ldr_tools_blender/issues). Download the latest version from [releases](https://github.com/ScanMountGoat/ldr_tools_blender/releases).

## Features
The goal of this project is to provided a reliable and performant way to import digital LEGO models into modern versions of Blender for rendering. This includes models built with [LDraw editing programs](https://www.ldraw.org/downloads-2/third-party-software.html) and LDR files exported from [Bricklink Studio](https://www.bricklink.com/v3/studio/download.page). 

* Compatible with LDR and MPD files. If you have a file that doesn't open correctly or an extension you'd like supported, please report it in [issues](https://github.com/ScanMountGoat/ldr_tools_blender/issues).
*  Easily load LEGO models with tens of thousands of parts. For extremely large scenes, see [instancing](#instancing).
* Create photorealistc renders taking full advantage of Blender Cycles with automatically created PBR materials with accurate colors and procedurally generated surface detail. 

## Getting Started
1. Install the [LDraw parts library](https://www.ldraw.org/help/getting-started.html) if you haven't already. Bricklink Studio bundles its own LDraw library and should be detected automatically by the addon.
2. Download the appropriate version of the addon for your system from [releases](https://github.com/ScanMountGoat/ldr_tools_blender/releases).
3. In Blender, navigate to Edit > Preferences > Addon and click Install. Select the zip downloaded in step 2. Do not extract the zip file!
4. The addons menu should now allow you to check the ldr_tools_blender addon to enable it.
5. Import an LDraw model into Blender by clicking File > Import > LDraw and selecting a .mpd or .ldr file.

## Instancing
This project is built from the ground up with performance in mind. The ldr_tools_blender addon can easily handle very large models with hundreds of thousands of parts. BThe addon will always instance geometry by part name and color to reduce memory usage and improve import times. Memory usage will be similar for both methods.

Blender itself does not scale well with the number of objects created in the scene. For large scenes with more than 50000 parts, it's recommended to check "Instance on faces" before importing. This instances each part on the faces of a hidden mesh, which makes the individual objects harder to edit but avoids most of the Blender overhead for scenes with high object counts.

### PBR Renders
The addon also creates Blender materials suitable for photorealistic rendering. This includes more accurate colors, edge bevels, and procedural surface details. Import the provided color test LDR file to see 

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
The easiest way to see the steps used to create a new release is to check the [release.yaml](https://github.com/ScanMountGoat/ldr_tools_blender/blob/main/.github/workflows/release.yml) script that runs using Github actions. See [development](https://github.com/ScanMountGoat/ldr_tools_blender/blob/main/DEVELOPMENT.md) for working on your personal machine.  See [contributing](https://github.com/ScanMountGoat/ldr_tools_blender/blob/main/CONTRIBUTING.md) for contributing to this project.

## Copyrights
LDraw™ is a trademark owned and licensed by the Jessiman Estate, which does not sponsor, endorse, or authorize this project.  

LEGO® is a registered trademark of the LEGO Group, which does not sponsor, endorse, or authorize this project.
