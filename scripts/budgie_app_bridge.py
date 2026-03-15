#!/usr/bin/env python3
"""Budgie Browser Linux app bridge for sidebar-integrated web apps."""

from __future__ import annotations

import argparse
import shutil
import subprocess
from dataclasses import dataclass


@dataclass(frozen=True)
class Integration:
    app_id: str
    command: list[str]


INTEGRATIONS: dict[str, Integration] = {
    "spotify": Integration("com.spotify.Client", ["flatpak", "run", "com.spotify.Client"]),
    "discord": Integration("com.discordapp.Discord", ["flatpak", "run", "com.discordapp.Discord"]),
    "telegram": Integration("org.telegram.desktop", ["flatpak", "run", "org.telegram.desktop"]),
}


def launch_integration(name: str) -> int:
    integration = INTEGRATIONS.get(name)
    if integration is None:
        raise ValueError(f"Unknown integration: {name}")

    executable = integration.command[0]
    if shutil.which(executable) is None:
        raise RuntimeError(f"Required executable '{executable}' not found")

    result = subprocess.run(integration.command, check=False)
    return result.returncode


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(description="Launch Budgie sidebar Linux integrations")
    parser.add_argument("integration", choices=sorted(INTEGRATIONS.keys()))
    return parser


if __name__ == "__main__":
    args = build_parser().parse_args()
    raise SystemExit(launch_integration(args.integration))
