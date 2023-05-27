# ldr_tools_blender Changelog
All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/).

### unreleased
### Added
* Added `models/colors.ldr` for testing all current LDraw colors.
* Added an option to add additional parts paths when importing.

### Changed
* Adjusted generated materials to improve subsurface scattering and procedural bump mapping.
* Reworked instancing to select either linked duplicates or geometry nodes for instancing.

### Removed
* Removed "Instance on faces" due to compatibility issues with Blender 3.5.1.

## 0.1.0 - 2023-03-15
First public release!