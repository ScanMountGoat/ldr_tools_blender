from __future__ import annotations

from typing import (
    TypeVar,
    Generic,
    TypeAlias,
    Iterable,
    overload,
)

from bpy.types import (
    NodeTree,
    Node,
    NodeSocket,
)

X = TypeVar("X")
T = TypeVar("T", bound=NodeTree)
N = TypeVar("N", bound=Node)
S = TypeVar("S", bound=NodeSocket)


class NodeGraph(Generic[T]):
    def __init__(self, tree: T) -> None:
        self.tree = tree

    def input(self, socket_type: type[S], name: str) -> None:
        stype = socket_type.__name__
        self.tree.interface.new_socket(name, in_out="INPUT", socket_type=stype)

    def output(self, socket_type: type[S], name: str) -> None:
        stype = socket_type.__name__
        self.tree.interface.new_socket(name, in_out="OUTPUT", socket_type=stype)

    def node(
        self,
        /,
        node_type: type[N],
        *,
        inputs: dict[str | int, NodeInput] | list[NodeInput] | None = None,
        **kwargs: object,
    ) -> GraphNode[N]:
        inner_node = self.tree.nodes.new(node_type.__name__)
        assert isinstance(inner_node, node_type)

        for prop_name, prop_val in kwargs.items():
            setattr(inner_node, prop_name, prop_val)

        node = GraphNode(self, inner_node)

        if inputs is not None:
            for socket_name, socket_val in _iter_items(inputs):
                node[socket_name] = socket_val

        return node


class GraphNode(Generic[N]):
    def __init__(self, graph: NodeGraph, node: N) -> None:
        self.graph = graph
        self.node = node

    @overload
    def __getitem__(self, key: str | int) -> NodeSocket: ...

    @overload
    def __getitem__(self, key: str | int, expected_type: type[S]) -> S: ...

    def __getitem__(
        self,
        key: str | int,
        expected_type: type[S] | None = None,
    ) -> NodeSocket:
        socket = self.node.outputs[key]
        if expected_type is not None:
            assert isinstance(socket, expected_type)
        return socket

    def __setitem__(self, key: str | int, val: NodeInput) -> None:
        # narrow down the quality-of-life overloads until we have either a socket or a value
        if isinstance(val, GraphNode):
            return self.__setitem__(key, val.node)
        elif isinstance(val, Node):
            return self.__setitem__(key, _get_default_output(val))

        dst_socket = self.node.inputs[key]

        if isinstance(val, NodeSocket):
            self.graph.tree.links.new(val, dst_socket)
        else:
            dst_socket.default_value = val  # type: ignore


def _get_default_output(node: Node) -> NodeSocket:
    return next(s for s in node.outputs if s.enabled)


def _iter_items(
    collection: list[X] | dict[str | int, X]
) -> Iterable[tuple[str | int, X]]:
    if isinstance(collection, list):
        return enumerate(collection)
    else:
        return collection.items()


Vec2: TypeAlias = tuple[float, float]
Vec3: TypeAlias = tuple[float, float, float]
Vec4: TypeAlias = tuple[float, float, float, float]
Value: TypeAlias = int | float | bool | str | Vec2 | Vec3 | Vec4
NodeInput: TypeAlias = GraphNode | Node | NodeSocket | Value