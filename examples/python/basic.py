#!/usr/bin/env python3
"""Basic Roche usage — create a sandbox, run code, clean up."""

from roche_sandbox import Roche

def main():
    roche = Roche()

    # Create a sandbox with AI-safe defaults (no network, readonly FS)
    sandbox = roche.create(image="python:3.12-slim")
    print(f"Created sandbox: {sandbox.id}")

    # Execute a command
    output = sandbox.exec(["python3", "-c", "print('Hello from Roche!')"])
    print(f"stdout: {output.stdout.strip()}")
    print(f"exit code: {output.exit_code}")

    # List active sandboxes
    sandboxes = roche.list()
    print(f"Active sandboxes: {len(sandboxes)}")

    # Clean up
    sandbox.destroy()
    print("Sandbox destroyed.")


if __name__ == "__main__":
    main()
