from typing import TypeAlias, Sequence, Literal, Final
from abc import ABCMeta, ABC

import numpy as np

Vec4: TypeAlias = tuple[float, float, float, float]
Mat4: TypeAlias = tuple[Vec4, Vec4, Vec4, Vec4]

# T = TypeVar("T")
# Array1: TypeAlias = np.ndarray[tuple[int], np.dtype[T]]
# UIntArray: TypeAlias = Array1[np.uint32]
# FloatArray: TypeAlias = Array1[np.float32]
# UVec2Array: TypeAlias = np.ndarray[tuple[int, Literal[2]], np.dtype[np.uint32]]
# Vec3Array: TypeAlias = np.ndarray[tuple[int, Literal[3]], np.dtype[np.float32]]
# Mat4Array: TypeAlias = np.ndarray[tuple[int, Literal[4], Literal[4]], np.dtype[np.float32]]

# the true types are exactly as described above,
# but bpy stubs claim that foreach_set does not accept ndarray

class UIntArray(Sequence[int], metaclass=ABCMeta):
    size: Final[int]

class FloatArray(Sequence[float], metaclass=ABCMeta):
    size: Final[int]

class Vec3Array(ABC):
    shape: Final[tuple[int, Literal[3]]]
    def reshape(self, _: Literal[-1]) -> FloatArray: ...

class UVec2Array(ABC):
    shape: Final[tuple[int, Literal[2]]]
    def reshape(self, _: Literal[-1]) -> UIntArray: ...

class Mat4Array(ABC):
    shape: Final[tuple[int, Literal[4], Literal[4]]]
    def reshape(self, _: Literal[-1]) -> FloatArray: ...
