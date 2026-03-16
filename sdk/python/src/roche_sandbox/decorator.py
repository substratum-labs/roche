# SPDX-License-Identifier: Apache-2.0
# Copyright 2025 Substratum Labs

"""Decorator for automatic sandbox injection."""

from __future__ import annotations

import asyncio
import functools
import inspect
from typing import Any, Callable

from roche_sandbox.client import AsyncRoche, Roche


def roche_sandbox(
    *,
    image: str = "python:3.12-slim",
    provider: str = "docker",
    network: bool = False,
    writable: bool = False,
    timeout_secs: int = 300,
    memory: str | None = None,
    cpus: float | None = None,
    sandbox_param: str = "sandbox",
) -> Callable:
    """Decorator that auto-creates a sandbox and injects it into the function.

    The decorated function must have a parameter (default name: ``sandbox``)
    that will receive a :class:`Sandbox` (sync) or :class:`AsyncSandbox`
    (async) instance.  The sandbox is created before the call and destroyed
    after it returns (or raises).

    Example::

        @roche_sandbox(image="python:3.12-slim")
        def run_code(code: str, sandbox: Sandbox) -> str:
            result = sandbox.exec(["python3", "-c", code])
            return result.stdout

        output = run_code("print('hello')")  # sandbox is auto-managed

    Works with agent framework decorators::

        @function_tool
        @roche_sandbox(image="python:3.12-slim")
        def run_code(code: str, sandbox: Sandbox) -> str:
            result = sandbox.exec(["python3", "-c", code])
            return result.stdout
    """

    create_kwargs: dict[str, Any] = dict(
        image=image,
        provider=provider,
        network=network,
        writable=writable,
        timeout_secs=timeout_secs,
    )
    if memory is not None:
        create_kwargs["memory"] = memory
    if cpus is not None:
        create_kwargs["cpus"] = cpus

    def decorator(fn: Callable) -> Callable:
        if asyncio.iscoroutinefunction(fn):

            @functools.wraps(fn)
            async def async_wrapper(*args: Any, **kwargs: Any) -> Any:
                client = AsyncRoche(provider=provider)
                async with (await client.create(**create_kwargs)) as sb:
                    kwargs[sandbox_param] = sb
                    return await fn(*args, **kwargs)

            # Remove sandbox_param from the signature so framework
            # introspection (e.g. OpenAI function_tool) doesn't see it.
            _strip_param(async_wrapper, sandbox_param)
            return async_wrapper

        else:

            @functools.wraps(fn)
            def sync_wrapper(*args: Any, **kwargs: Any) -> Any:
                client = Roche(provider=provider)
                with client.create(**create_kwargs) as sb:
                    kwargs[sandbox_param] = sb
                    return fn(*args, **kwargs)

            _strip_param(sync_wrapper, sandbox_param)
            return sync_wrapper

    return decorator


def _strip_param(wrapper: Callable, param_name: str) -> None:
    """Remove a parameter from the function's __signature__.

    This prevents agent frameworks from treating the sandbox parameter
    as a user-supplied argument.
    """
    try:
        sig = inspect.signature(wrapper.__wrapped__)  # type: ignore[attr-defined]
        params = [p for name, p in sig.parameters.items() if name != param_name]
        wrapper.__signature__ = sig.replace(parameters=params)  # type: ignore[attr-defined]
    except (ValueError, TypeError):
        pass
