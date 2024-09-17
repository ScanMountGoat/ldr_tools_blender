from typing import TypeVar, TypeAlias, Literal
from abc import ABCMeta, ABC

import numpy as np

Vec2: TypeAlias = tuple[float, float]
Vec4: TypeAlias = tuple[float, float, float, float]
Mat4: TypeAlias = tuple[Vec4, Vec4, Vec4, Vec4]

T = TypeVar("T")
Array1: TypeAlias = np.ndarray[tuple[int], np.dtype[T]]
UByteArray: TypeAlias = Array1[np.uint8]
UIntArray: TypeAlias = Array1[np.uint32]
FloatArray: TypeAlias = Array1[np.float32]
UVec2Array: TypeAlias = np.ndarray[tuple[int, Literal[2]], np.dtype[np.uint32]]
Vec2Array: TypeAlias = np.ndarray[tuple[int, Literal[2]], np.dtype[np.float32]]
Vec3Array: TypeAlias = np.ndarray[tuple[int, Literal[3]], np.dtype[np.float32]]
Mat4Array: TypeAlias = np.ndarray[
    tuple[int, Literal[4], Literal[4]], np.dtype[np.float32]
]
