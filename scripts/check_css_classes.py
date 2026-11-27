#!/usr/bin/env python3
"""Fail when a CSS class name is claimed by more than one place.

The app's stylesheets share one global namespace, so a class named in two
features styles both. A class counts as claimed where a selector starts with
it (`.card`, `.card.open`, `.card .title`); `.panel .card` only restyles
someone else's class inside `.panel` and is not a claim.

Reported:
  - a class claimed in more than one stylesheet
  - a class with plain `.name { ... }` rules in distant parts of one stylesheet

Collisions that already exist are listed in css_class_allowlist.txt and are
not reported; `--update` rewrites that list from the current stylesheets.

Usage: check_css_classes.py [--update] [CSS_DIR]
"""

import re
import sys
from collections import defaultdict
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
ALLOWLIST = Path(__file__).with_name("css_class_allowlist.txt")
# Plain rules for one class further apart than this are taken as two features
SECTION_LINES = 400

FIRST_CLASS = re.compile(r"^\.([A-Za-z_][\w-]*)")
PLAIN = re.compile(r"^\.([A-Za-z_][\w-]*)(?:::?[\w-]+(?:\([^)]*\))?)*$")


def selectors(css: str):
    """(selector, line) for every style rule, at any nesting of at-rules"""
    # Blank the comments, keeping the line count
    css = re.sub(r"/\*.*?\*/", lambda m: re.sub(r"[^\n]", " ", m.group()), css, flags=re.S)
    start = 0
    for brace in re.finditer(r"[{};]", css):
        if brace.group() == "{":
            prelude = css[start : brace.start()]
            if not prelude.lstrip().startswith("@"):
                line = css.count("\n", 0, brace.start() - len(prelude.lstrip())) + 1
                # Commas inside :is(...) / :not(...) do not separate selectors
                for sel in re.split(r",(?![^()]*\))", prelude):
                    yield sel.strip(), line
        start = brace.end()


def collisions(css_dir: Path) -> dict[str, str]:
    """class -> why it collides"""
    claimed = defaultdict(lambda: defaultdict(list))  # class -> file -> lines
    plain = defaultdict(lambda: defaultdict(list))
    for path in sorted(css_dir.glob("*.css")):
        for sel, line in selectors(path.read_text()):
            if m := FIRST_CLASS.match(sel):
                claimed[m.group(1)][path.name].append(line)
            if m := PLAIN.match(sel):
                plain[m.group(1)][path.name].append(line)

    found = {}
    for name, files in claimed.items():
        if len(files) > 1:
            where = ", ".join(f"{f}:{lines[0]}" for f, lines in files.items())
            found[name] = f"claimed in several stylesheets ({where})"
    for name, files in plain.items():
        for f, lines in files.items():
            if name not in found and max(lines) - min(lines) > SECTION_LINES:
                found[name] = f"defined in distant parts of {f} (lines {min(lines)} and {max(lines)})"
    return found


def main() -> int:
    args = [a for a in sys.argv[1:] if a != "--update"]
    css_dir = Path(args[0]) if args else ROOT / "app" / "assets"
    found = collisions(css_dir)

    if "--update" in sys.argv:
        header = "# Class names that already collide; see check_css_classes.py. Do not add new ones.\n"
        ALLOWLIST.write_text(header + "".join(f"{name}\n" for name in sorted(found)))
        print(f"{ALLOWLIST.name}: {len(found)} names")
        return 0

    allowed = set()
    if ALLOWLIST.exists():
        allowed = {l.strip() for l in ALLOWLIST.read_text().splitlines() if l.strip() and not l.startswith("#")}
    new = {name: why for name, why in found.items() if name not in allowed}
    for name, why in sorted(new.items()):
        print(f"css class collision: .{name} is {why}")
    if new:
        print("Rename the new class, or scope its rules under the feature's own parent class.")
    return 1 if new else 0


if __name__ == "__main__":
    sys.exit(main())
