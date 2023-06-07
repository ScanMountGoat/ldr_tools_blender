# Architecture
This document describes the high level architecture of ldr_tools_blender. The goal is to become familiar with the codebase and its various projects.

## Introduction
This project utilizes Rust code to enable an importer for Blender that is both faster and more robust than what would be possible with pure Python. This makes the project structure slightly more complex, but the resulting Blender addon requires very minimal Python code since most processing is done by Rust. Blender has special optimizations for numpy arrays, so the mesh creation has very low overhead.

The basic processing pipeline from an LDraw file on disk to a final imported Blender scene is outlined below. Note how each project is used for a specific part of the process.

1. Parse model.ldr. (weldr)
2. Create instanced scene geometry and create attribute arrays for positions, normals, and colors (ldr_tools).
3. Convert attribute arrays to numpy arrays and scene data to Python classes (ldr_tools_py).
4. Convert numpy arrays into Blender meshes and create material node groups (ldr_tools_blender).

## Code Map
### ldr_tools
The Rust project that does the loading and processing of LDraw files. Multiple functions and scene representations are supported in `lib.rs` depending on how the user wants to instance the parts in the scene. The other files like `geometry.rs` or `slope.rs` handle dedicated processing functions like triangulation or detecting grainy slopes. For the actual parsing of the LDraw format itself, see [weldr](https://github.com/djeedai/weldr).

### ldr_tools_blender
This is the actual Blender addon and is what is deployed to releases. The operator and import settings are defined in `operator.py`. Creation of Cycles materials is performed in `material.py`. Conversions from LDraw colors to more Cycles friendly colors are defined in `colors.py`. The main importing code is contained in `importldr.py`. There is very little code in `importldr.py` since most of the processing is done by the `ldr_tools_py` Python package.

### ldr_tools_py
Python bindings to ldr_tools using pyo3. This project just defines associated python classes and functions for the top level types and functions in ldr_tools. 