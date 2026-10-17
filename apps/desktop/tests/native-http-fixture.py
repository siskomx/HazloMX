"""Owned MCP HTTP fixture; requires an already installed Python MCP SDK."""
import sys
from mcp.server.fastmcp import FastMCP
from mcp.types import ToolAnnotations

server = FastMCP("owned-native-http-fixture", host="127.0.0.1", port=int(sys.argv[1]))
state = {"value": 0}


@server.tool(annotations=ToolAnnotations(readOnlyHint=True, destructiveHint=False, openWorldHint=False))
def observe_native_state() -> dict:
    return dict(state)


@server.tool()
def program_owned_parameter_edit(value: int) -> dict:
    state["value"] = value
    return dict(state)


server.run(transport="streamable-http")
