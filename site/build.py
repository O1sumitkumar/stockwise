#!/usr/bin/env python3
"""Build the documentation site into target/site with mdBook.

Copies docs/*.md, the site's own pages (SUMMARY.md, index.md) and the
top-level CHANGELOG, ROADMAP, CONTRIBUTING and DISCLAIMER into
target/site-src, rewrites links that point outside docs/ (images are copied
next to the pages, project pages become site pages, source files link to
GitHub), then runs `mdbook build site`.

Usage: python3 site/build.py [--serve]   (needs mdBook on PATH)
"""

import re
import shutil
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
SRC = ROOT / "target" / "site-src"
THEME = ROOT / "target" / "site-theme"
REPO = "https://github.com/iamsaquib8/stockwise"

# Top-level project files published as site pages.
PROJECT_PAGES = {
    "CHANGELOG.md": "changelog.md",
    "ROADMAP.md": "roadmap.md",
    "CONTRIBUTING.md": "contributing.md",
    "DISCLAIMER.md": "disclaimer.md",
}

LINK = re.compile(r"(!?\[[^\]]*\]\()([^)\s]+)((?:\s+\"[^\"]*\")?\))")


def site_target(path_from_root: str) -> str:
    """Where a link to `path_from_root` (relative to the repo root) goes on the site."""
    path, _, frag = path_from_root.partition("#")
    frag = f"#{frag}" if frag else ""
    if path in PROJECT_PAGES:
        return PROJECT_PAGES[path] + frag
    if path.startswith("docs/"):
        return path[len("docs/"):] + frag
    if path.startswith("assets/"):
        return path + frag
    kind = "tree" if path.endswith("/") or "." not in Path(path).name else "blob"
    return f"{REPO}/{kind}/master/{path}{frag}"


def rewrite(text: str, base: str) -> str:
    """Rewrite relative links in a page that lives at `base` in the repo
    ("docs" for docs pages, "" for top-level files)."""

    def fix(m: re.Match) -> str:
        url = m.group(2)
        if re.match(r"^[a-z]+:|^#|^/", url):
            return m.group(0)
        if base == "docs":
            if not url.startswith("../"):
                return m.group(0)  # sibling page, already correct
            url = url[len("../"):]
        return m.group(1) + site_target(url) + m.group(3)

    return LINK.sub(fix, text)


def main() -> int:
    if SRC.exists():
        shutil.rmtree(SRC)
    (SRC / "assets").mkdir(parents=True)

    for page in sorted((ROOT / "docs").glob("*.md")):
        (SRC / page.name).write_text(rewrite(page.read_text(), "docs"))
    for name in ("SUMMARY.md", "index.md"):
        shutil.copy(ROOT / "site" / name, SRC / name)
    for source, dest in PROJECT_PAGES.items():
        (SRC / dest).write_text(rewrite((ROOT / source).read_text(), ""))
    for image in (ROOT / "assets").glob("*.svg"):
        shutil.copy(image, SRC / "assets" / image.name)

    THEME.mkdir(parents=True, exist_ok=True)
    shutil.copy(ROOT / "assets" / "logo.svg", THEME / "favicon.svg")

    cmd = ["mdbook", "serve" if "--serve" in sys.argv else "build", str(ROOT / "site")]
    return subprocess.call(cmd)


if __name__ == "__main__":
    sys.exit(main())
