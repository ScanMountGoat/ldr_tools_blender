# ldr_tools_blender Changelog
All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/).

## 0.4.9 - 2025-05-05
### Fixed
* Fixed an issue where Studio models with multiple assigned textures would only load the first texture.

## 0.4.8 - 2025-04-14
### Fixed
* Fixed an issue where some Bricklink Studio files would fail to import due to incorrectly specified optional line commands.

### Changed
* Improved error messages by including the line that failed to parse.

## 0.4.7 - 2025-02-07
### Fixed
* Fixed an issue where importing some files would not correctly process all LDraw commands.

## 0.4.6 - 2025-01-28
### Changed
* Adjusted procedural normals for grainy slope materials.

### Fixed
* Fixed an issue where splitting edge lines would produce loose vertices in some cases.
* Fixed an issue where some parts would have incorrectly smoothed edges in some cases.

## 0.4.5 - 2024-11-11
### Changed
* Improved compatibility with older Linux distributions for compiled releases.

## 0.4.4 - 2024-10-07
### Fixed
* Fixed an issue where some files would not correctly import data from all subfiles.

## 0.4.3 - 2024-09-17
### Added
* Added support for importing .io files saved by recent versions of Bricklink Studio.
* Added an option for adjusting the scale when importing.
* Added support for Studio texture parts using the PE_TEX_INFO extension.

### Fixed
* Fixed an issue where some slope pieces did not use slope materials.

## 0.4.2 - 2024-08-20
### Fixed
* Fixed an issue where paths relative to the current file would not resolve properly.
* Fixed an issue where file loading did not correctly ignore case of subfiles.

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