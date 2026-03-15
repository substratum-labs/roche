#!/usr/bin/env python3
"""Async Roche usage with context manager for auto-cleanup."""

import asyncio
from roche_sandbox import AsyncRoche


async def main():
    roche = AsyncRoche()

    # Context manager auto-destroys on exit
    async with await roche.create(image="python:3.12-slim") as sandbox:
        print(f"Sandbox {sandbox.id} created")

        # Run multiple commands
        for cmd in ["echo hello", "python3 -c 'print(2+2)'", "uname -a"]:
            output = await sandbox.exec(["sh", "-c", cmd])
            print(f"$ {cmd}")
            print(f"  {output.stdout.strip()}")

    print("Sandbox auto-destroyed.")


if __name__ == "__main__":
    asyncio.run(main())
