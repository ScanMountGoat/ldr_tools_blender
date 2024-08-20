# ldr_tools_blender Changelog
All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/).

## unreleased
### Fixed
* Fixed an issue where paths relative to the current file would not resolve properly.

## 0.4.1 - 2024-08-05
### Changed
* Adjusted node positions of generated materials.

### Fixed
* Fixed an issue where importing would fail if the UI language was not set to English.

## 0.4.0 - 2024-03-26
### Added
* Added an option to adjust the part primitive resolution.
* Added an option to select the stud type.

### Changed
* Changed supported Blender version to 4.1 or later.

## 0.3.0 - 2023-12-03
### Added
* Added an option to enable or disable the gap between parts when importing.

### Changed
* Changed supported Blender version to 4.0.

## 0.2.0 - 2023-08-10
### Added
* Added `models/colors.ldr` for testing all current LDraw colors.
* Added `models/slopes.ldr` for testing grainy slope materials.
* Added an option to add additional parts paths when importing.

### Changed
* Adjusted generated materials to improve subsurface scattering and procedural bump mapping.
* Reworked instancing to select either linked duplicates or geometry nodes for instancing.
* Adjusted procedural normals to reflect the grainy texture on certain slope faces.
* Moved processing of sharp edges from Blender to ldr_tools to improve import times.
* Increased the autosmooth angle threshold to reduce unwanted sharp seams.

### Fixed
* Fixed an issue where some parts would import with the wrong orientations when instancing.

### Removed
* Removed "Instance on faces" in favor of geometry nodes due to compatibility issues with Blender 3.5.1.

## 0.1.0 - 2023-03-15
First public release!