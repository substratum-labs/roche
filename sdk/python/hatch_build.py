import os

from hatchling.builders.hooks.plugin.interface import BuildHookInterface


class CustomBuildHook(BuildHookInterface):
    def initialize(self, version, build_data):
        platform_tag = os.environ.get("ROCHE_PLATFORM_TAG")
        if platform_tag:
            build_data["pure_python"] = False
            build_data["tag"] = f"py3-none-{platform_tag}"
