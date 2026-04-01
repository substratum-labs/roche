# SPDX-License-Identifier: Apache-2.0
# Copyright 2025 Substratum Labs

"""Multi-agent workspace: shared sandbox across Castor agent hierarchy.

A workspace is a long-lived sandbox + session that multiple agents can share.
Parent creates the workspace, children join it and execute in the same sandbox.

Usage::

    bridge = RocheCastorBridge()

    async def coordinator(proxy):
        # Create workspace — sandbox stays alive across agent calls
        ws = await bridge.workspace(image="python:3.12-slim", writable=True)
        async with ws:
            # Parent writes a file
            await proxy.syscall("execute_in_workspace",
                code="open('/tmp/data.csv','w').write('a,b\\n1,2')",
                workspace_id=ws.id)

            # Child can read the same file
            result = await proxy.syscall("execute_in_workspace",
                code="print(open('/tmp/data.csv').read())",
                workspace_id=ws.id)
            # stdout: "a,b\\n1,2"
"""

from __future__ import annotations

from dataclasses import dataclass, field
from typing import Any

from roche_sandbox.client import AsyncRoche
from roche_sandbox.types import Budget, DynamicPermissions


@dataclass
class Workspace:
    """A shared sandbox + session for multi-agent collaboration.

    Attributes:
        id: Workspace identifier (= session_id).
        sandbox_id: Underlying Roche sandbox ID.
        provider: Provider used for the sandbox.
    """

    id: str
    sandbox_id: str
    provider: str
    _client: Any = field(repr=False)
    _agent_pids: list[str] = field(default_factory=list, repr=False)

    async def exec(
        self,
        command: list[str],
        timeout_secs: int | None = None,
        trace_level: str | None = "standard",
    ) -> Any:
        """Execute a command in this workspace's sandbox."""
        return await self._client.exec(
            self.sandbox_id, command, timeout_secs, trace_level=trace_level
        )

    async def destroy(self) -> None:
        """Destroy the workspace: destroy session then sandbox."""
        try:
            await self._client.destroy_session(self.id)
        except Exception:
            pass
        try:
            await self._client.destroy(self.sandbox_id)
        except Exception:
            pass

    async def __aenter__(self) -> Workspace:
        return self

    async def __aexit__(self, *exc: object) -> None:
        await self.destroy()


class WorkspaceManager:
    """Creates and tracks workspaces for multi-agent coordination."""

    def __init__(self, provider: str | None = None):
        self._provider = provider or "docker"
        self._workspaces: dict[str, Workspace] = {}

    async def create(
        self,
        *,
        image: str = "python:3.12-slim",
        timeout_secs: int = 600,
        network: bool = False,
        writable: bool = True,
        memory: str | None = None,
        network_allowlist: list[str] | None = None,
        fs_paths: list[str] | None = None,
        budget: Budget | None = None,
    ) -> Workspace:
        """Create a new workspace: sandbox + session.

        The sandbox stays alive until explicitly destroyed or timed out.
        Multiple agents can exec into it via the workspace ID.
        """
        client = AsyncRoche(provider=self._provider)

        # Create a long-lived sandbox
        sandbox = await client.create(
            image=image,
            timeout_secs=timeout_secs,
            network=network,
            writable=writable,
            memory=memory,
            network_allowlist=network_allowlist or [],
            fs_paths=fs_paths or [],
        )

        # Create a session bound to this sandbox
        permissions = DynamicPermissions(
            network=network,
            network_allowlist=network_allowlist or [],
            writable=writable,
            fs_paths=fs_paths or [],
        )
        session_id = await client.create_session(
            sandbox.id,
            permissions=permissions,
            budget=budget,
        )

        ws = Workspace(
            id=session_id,
            sandbox_id=sandbox.id,
            provider=self._provider,
            _client=client,
        )
        self._workspaces[session_id] = ws
        return ws

    def get(self, workspace_id: str) -> Workspace | None:
        """Get a workspace by ID."""
        return self._workspaces.get(workspace_id)

    async def destroy(self, workspace_id: str) -> None:
        """Destroy a workspace."""
        ws = self._workspaces.pop(workspace_id, None)
        if ws:
            await ws.destroy()

    async def destroy_all(self) -> None:
        """Destroy all workspaces."""
        for ws_id in list(self._workspaces.keys()):
            await self.destroy(ws_id)
