#!/usr/bin/env python3
"""Report map, campaign, and mod dependencies embedded in SC2 save archives.

Install the only dependency once:
    python3 -m pip install mpyq

Usage:
    python3 inspect_sc2_saves.py /path/to/StarCraft\ II/Accounts/<account-id>
"""

from __future__ import annotations

import argparse
from collections import defaultdict
from pathlib import Path
import re
import sys

try:
    import mpyq
except ModuleNotFoundError:
    sys.exit("Missing dependency: run `python3 -m pip install mpyq` first.")


def printable_strings(data: bytes) -> list[str]:
    """Return printable strings from Blizzard's binary save-details blob."""
    return [match.decode("utf-8", "replace") for match in re.findall(rb"[\x20-\x7e]{3,}", data)]


def dependency_path(value: str, marker: str) -> str | None:
    """Remove the binary protocol tag immediately preceding an archive path."""
    start = value.find(marker)
    return value[start:] if start >= 0 else None


def inspect_save(path: Path) -> tuple[str, list[str], list[str], list[str]]:
    """Extract the display title and dependency paths from one .SC2Save."""
    archive = mpyq.MPQArchive(str(path))
    details = archive.read_file("save.details") or b""
    values = printable_strings(details)

    maps = [dependency_path(value, "Campaign/") for value in values if value.endswith(".SC2Map")]
    mods = [dependency_path(value, "Mods/") for value in values if value.endswith(".SC2Mod")]
    campaigns = [dependency_path(value, "Campaigns/") for value in values if value.endswith(".SC2Campaign")]

    dependency_values = {*maps, *mods, *campaigns}
    title = next(
        (
            value
            for value in values
            if value not in dependency_values and "/" not in value and "\\" not in value
        ),
        "?",
    )
    return title, maps, mods, campaigns


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("save_root", type=Path, help="Directory containing .SC2Save files")
    args = parser.parse_args()

    root = args.save_root.expanduser().resolve()
    if not root.is_dir():
        parser.error(f"not a directory: {root}")

    grouped: dict[tuple[tuple[str, ...], tuple[str, ...], tuple[str, ...]], list[tuple[Path, str]]] = defaultdict(list)
    errors: list[tuple[Path, str]] = []

    for path in sorted(root.rglob("*.SC2Save")):
        try:
            title, maps, mods, campaigns = inspect_save(path)
            grouped[(tuple(maps), tuple(mods), tuple(campaigns))].append((path, title))
        except Exception as exc:  # Keep reporting even if one archive is malformed.
            errors.append((path, str(exc)))

    for index, ((maps, mods, campaigns), saves) in enumerate(grouped.items(), 1):
        print(f"GROUP {index}: {len(saves)} save(s)")
        print("  Mods:", "; ".join(mods) if mods else "—")
        print("  Map:", "; ".join(maps) if maps else "—")
        print("  Campaign:", "; ".join(campaigns) if campaigns else "—")
        for path, title in saves:
            print(f"    {path.relative_to(root)}  [{title}]")
        print()

    if errors:
        print("ERRORS")
        for path, error in errors:
            print(f"  {path}: {error}")
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
