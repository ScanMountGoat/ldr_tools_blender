# ldr_tools_blender [![GitHub release (latest by date including pre-releases)](https://img.shields.io/github/v/release/ScanMountGoat/ldr_tools_blender?include_prereleases)](https://github.com/ScanMountGoat/ldr_tools_blender/releases/latest)

![falcon render](https://github.com/ScanMountGoat/ldr_tools_blender/assets/23301691/6cfd557b-243d-491e-bd05-a7ae93db7eca)

> Cycles render of [10179-1 Millennium Falcon UCS (LDraw OMR)](https://omr.ldraw.org/files/347)

An addon for importing LDraw and Studio files into Blender 4.1 or later. Check out [discussions](https://github.com/ScanMountGoat/ldr_tools_blender/discussions) for reading announcements, asking questions, or discussing new features. Report bugs or request new features in [issues](https://github.com/ScanMountGoat/ldr_tools_blender/issues). Download the latest version from [releases](https://github.com/ScanMountGoat/ldr_tools_blender/releases).

## Features
The goal of this project is to provided a reliable and performant way to import digital LEGO models into modern versions of Blender for rendering. This includes importing and rendering [Bricklink Studio](https://www.bricklink.com/v3/studio/download.page) models or models built with [LDraw editing programs](https://www.ldraw.org/downloads-2/third-party-software.html). 

* Compatible with LDR and MPD files. If you have a file that doesn't open correctly or an LDraw extension you'd like supported, please report it in [issues](https://github.com/ScanMountGoat/ldr_tools_blender/issues).
* Compatible with newer versions of Bricklink Studio .io files.
* Easily load LEGO models with hundreds of thousands of parts. For extremely large scenes, see [performance](#performance).
* Create photorealistic renders taking full advantage of Blender Cycles with generated materials with accurate colors and procedurally generated surface detail. 

## Bricklink Studio Compatibility
Bricklink Studio models from newer versions of the program can be imported directly from .io files. Older Studio models that fail to import as .io files should be resaved with a newer version of Studio. This avoids an issue with password protection on older .io files. Studio can also export files as LDraw under File > Export As before importing into Blender. 

## Getting Started
1. Install the [LDraw parts library](https://www.ldraw.org/help/getting-started.html) if you haven't already. Bricklink Studio bundles its own LDraw library and should be detected automatically by the addon.
2. Download the appropriate version of the addon for your system from [releases](https://github.com/ScanMountGoat/ldr_tools_blender/releases). For older Blender versions, download one of the previous releases.
3. In Blender, navigate to Edit > Preferences > Addon and click Install. Select the zip downloaded in step 2. Do not extract the zip file!
4. The addons menu should now allow you to check the ldr_tools_blender addon to enable it.
5. Import an LDraw model into Blender by clicking File > Import > LDraw and selecting a .mpd, .ldr, or .io file.

## Uninstalling/Upgrading
Upgrading the addon requires uninstalling the addon, downloading the latest version from releases, and then reinstalling the addon. Windows users will need to disable the addon, restart Blender, and then uninstall the addon to properly remove the previous version.

## Performance
This project is built from the ground up with performance in mind. The ldr_tools_blender addon can easily handle very large models with hundreds of thousands of parts. The addon will always instance geometry by part name and color to reduce memory usage and improve import times. Memory usage will be similar for both methods.  

Blender itself does not scale well with the number of objects created in the scene. For large scenes with more than 10000 parts, it's recommended to use "Geometry Nodes" as the instance type before importing. Geometry nodes make the individual objects harder to edit but avoids most of the Blender overhead for scenes with high object counts. For very large scenes that don't need to be rendered up close, setting the resolution to "Normal" and stud type to "Normal" to remove stud logos can greatly reduce memory usage and improve import times.

## Projects
### ldr_tools
A Rust library for working with LDraw files. This performs all the parsing and geometry handling. This project can be used in 
other Rust projects by adding the following line to the `Cargo.toml`. ldr_tools is used for loading models in the [ldr_wgpu](https://github.com/ScanMountGoat/ldr_wgpu) renderer.  

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
