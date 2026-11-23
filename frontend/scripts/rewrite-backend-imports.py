#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""F0-2 one-shot mechanical rewrite of backend imports (SPE-BAS-004 §5.4/§8.1).

Pass 1: `services/endpoints` imports become `import { backend } from ...`.
Pass 2: `ApiError` imports move from `services/api` to `services/backend`.
Pass 3: domain call sites gain the `backend.` prefix.

`backend/index.ts` is excluded (it owns the endpoints import). This is an L4
script: the default is a read-only preview; --force authorises writes, and every
file is backed up to `<name>.bak` before being overwritten. Any leftover
endpoints/api import outside backend makes the script exit 1.

Usage:
    python3 frontend/scripts/rewrite-backend-imports.py --help
    python3 frontend/scripts/rewrite-backend-imports.py --dry-run
    python3 frontend/scripts/rewrite-backend-imports.py --force
"""

from __future__ import annotations

import argparse
import logging
import re
import shutil
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent / "src"

DOMAINS = [
    "candidatesApi", "positionsApi", "conversationsApi", "messagesApi",
    "importsApi", "followUpsApi", "draftsApi", "draftGroupsApi", "aiApi",
    "dashboardApi", "authApi", "settingsApi", "browserApi",
]
# domain key on the backend client (strip the Api suffix, lower-case first)
KEY = {name: name[:-3][0].lower() + name[:-3][1:] for name in DOMAINS}

IMPORT_RE = re.compile(
    r"import\s*\{[^}]*\}\s*from\s*(['\"])((?:[^'\"]*?services[/\\\\])?endpoints)\1;?",
    re.DOTALL,
)
APIERROR_RE = re.compile(
    r"import\s*\{\s*ApiError\s*\}\s*from\s*(['\"])((?:[^'\"]*?services[/\\\\])?api)\1;?"
)
ENDPOINTS_LEFT = re.compile(r"from\s*['\"][^'\"]*services/endpoints['\"]")
API_LEFT = re.compile(r"from\s*['\"][^'\"]*services/api['\"]")

LOG = logging.getLogger("rewrite-backend-imports")


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--dry-run", action="store_true",
        help="preview the files that would be rewritten; never write (default mode)",
    )
    parser.add_argument(
        "--force", action="store_true",
        help="authorise writing (L4); a .bak copy is made before each overwrite",
    )
    parser.add_argument(
        "--root", type=Path, default=ROOT,
        help="frontend source root to scan (default: <repo>/frontend/src)",
    )
    return parser.parse_args()


def source_files(root: Path) -> list[Path]:
    """All TypeScript sources except the backend layer itself."""
    result: list[Path] = []
    if not root.is_dir():
        LOG.error("source root does not exist: %s", root)
        return result
    for path in sorted(root.rglob("*")):
        if path.suffix not in (".ts", ".tsx"):
            continue
        if not path.is_file():
            continue
        if "services/backend" in path.as_posix():
            continue
        result.append(path)
    return result


def rewrite_text(text: str) -> str:
    """Apply the three mechanical passes to one file's contents."""
    if ENDPOINTS_LEFT.search(text) or re.search(r"services[/\\]endpoints", text):
        def import_sub(match: re.Match[str]) -> str:
            prefix = match.group(2)
            depth = prefix.split("services")[0]
            return f"import {{ backend }} from '{depth}services/backend';"

        text = IMPORT_RE.sub(import_sub, text)
        for api_name, key in KEY.items():
            text = re.sub(rf"\b{api_name}\b", f"backend.{key}", text)

    def apierr_sub(match: re.Match[str]) -> str:
        prefix = match.group(2)
        depth = prefix.split("services")[0]
        return f"import {{ ApiError }} from '{depth}services/backend';"

    return APIERROR_RE.sub(apierr_sub, text)


def backup(path: Path) -> Path:
    """Create a uniquely numbered .bak beside the target before overwriting."""
    candidate = path.with_name(path.name + ".bak")
    index = 1
    while candidate.exists():
        candidate = path.with_name(f"{path.name}.bak.{index}")
        index += 1
    shutil.copy2(path, candidate)
    return candidate


def main() -> int:
    logging.basicConfig(level=logging.INFO, format="%(levelname)s %(message)s")
    args = parse_args()
    root = args.root.expanduser().resolve()
    if args.dry_run and args.force:
        LOG.error("--dry-run and --force are mutually exclusive")
        return 2

    changed: list[Path] = []
    for path in source_files(root):
        original = path.read_text(encoding="utf-8")
        new_text = rewrite_text(original)
        if new_text == original:
            continue
        LOG.info("REWRITE TARGET: %s", path)
        changed.append(path)
        if args.dry_run:
            continue
        if not args.force:
            LOG.error("refusing to rewrite %s without --force", path)
            continue
        saved = backup(path)
        LOG.info("backup: %s", saved)
        path.write_text(new_text, encoding="utf-8", newline="\n")

    remaining_endpoints: list[str] = []
    remaining_api: list[str] = []
    for path in source_files(root):
        text = path.read_text(encoding="utf-8")
        rel = path.relative_to(root).as_posix()
        if ENDPOINTS_LEFT.search(text):
            remaining_endpoints.append(rel)
        if API_LEFT.search(text):
            remaining_api.append(rel)

    LOG.info("rewrote %s file(s): %s", len(changed),
             [p.relative_to(root).as_posix() for p in changed])
    LOG.info("endpoints imports outside backend: %s", remaining_endpoints or "NONE")
    LOG.info("api imports outside backend: %s", remaining_api or "NONE")
    if args.dry_run:
        LOG.info("[DRY-RUN] no files were modified")
    if remaining_endpoints or remaining_api:
        LOG.error("residual unrewritten imports found; F0-2 is NOT complete")
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
