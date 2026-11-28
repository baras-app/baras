#!/usr/bin/env python3
"""Bake the map pages each boss is fought on into the bundled encounter definitions.

Replays every log under --logs with `baras-validate --map-pages`, unions the
pages each boss's fights used and writes them as `map_pages` in the boss's
definition, in the map index's order. A boss no log covers keeps what it has.

Run through `just bake-boss-maps` after adding logs or re-extracting the maps.
"""

from __future__ import annotations

import argparse
import re
import subprocess
import tomllib
from pathlib import Path

MAP_PAGES = re.compile(r"^map_pages = \[.*?\]\n", re.DOTALL | re.MULTILINE)


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--validate", type=Path, required=True, help="baras-validate binary")
    parser.add_argument("--index", type=Path, required=True, help="pages.toml of the maps repository")
    parser.add_argument("--encounters", type=Path, required=True, help="Folder of encounter TOMLs to write")
    parser.add_argument("--logs", type=Path, required=True, help="Folder of combat logs to replay")
    args = parser.parse_args()

    index = tomllib.loads(args.index.read_text(encoding="utf-8"))
    order = {m["image"]: i for area in index["area"] for i, m in enumerate(area["maps"])}

    used: dict[str, set[str]] = {}
    logs = sorted(args.logs.glob("*.txt"))
    for log in logs:
        out = subprocess.run(
            [args.validate, "--log", log, "--map-pages", args.index, "--definitions", args.encounters.parent.parent],
            capture_output=True, text=True, check=True,
        ).stdout
        for line in out.splitlines():
            boss, image = line.split("\t")
            used.setdefault(boss, set()).add(image)
    print(f"Replayed {len(logs)} logs: {len(used)} bosses seen")

    baked = set()
    for path in sorted(args.encounters.glob("*.toml")):
        if path.stem.endswith("_custom"):
            continue
        # Split before each boss so an edit stays inside its own block
        blocks = re.split(r"(?m)^(?=\[\[boss\]\]\n)", path.read_text(encoding="utf-8"))
        for i, block in enumerate(blocks):
            boss = re.search(r'(?m)^id = "([^"]+)"$', block)
            if not block.startswith("[[boss]]") or not boss or boss[1] not in used:
                continue
            if boss[1] in baked:
                print(f"  ! {boss[1]}: id used by more than one boss, {path.name} left alone")
                continue
            baked.add(boss[1])
            pages = sorted(used[boss[1]], key=order.__getitem__)
            entry = "map_pages = [\n" + "".join(f'    "{p}",\n' for p in pages) + "]\n"
            # The boss's own keys end at the first blank line
            head, sep, rest = MAP_PAGES.sub("", block, count=1).partition("\n\n")
            blocks[i] = f"{head}\n{entry}" + (f"\n{rest}" if sep else "")
        path.write_text("".join(blocks), encoding="utf-8")

    missing = sorted(used.keys() - baked)
    if missing:
        print(f"  ! no definition for: {', '.join(missing)}")
    print(f"✓ map_pages written for {len(baked)} bosses")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
