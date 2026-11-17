#!/usr/bin/env python3
"""Capture the positions endpoint contract from a running ICA server.

Runs the same request set against any base URL (the Python baseline or the
Rust port) and writes one JSON file per side, so ``contract_diff.py`` can
compare them.

Usage:
    python3 scripts/contract_capture.py --base-url http://127.0.0.1:8010 \
        --out docs/contract-baseline/positions-python.json

Standard library only: no third-party dependency, so it runs under any
python3 on the box.
"""

from __future__ import annotations

import argparse
import json
import sys
import urllib.error
import urllib.request

# Every case is a name -> path pair. Names are the diff keys, so they must
# stay stable across both sides.
CASES: list[tuple[str, str]] = [
    ("health", "/health"),
    ("positions_default", "/api/positions"),
    ("positions_limit2", "/api/positions?limit=2"),
    ("positions_page2", "/api/positions?limit=2&offset=2"),
    ("positions_offset_beyond_total", "/api/positions?offset=100"),
    ("positions_include_archived", "/api/positions?include_archived=true"),
    ("positions_include_archived_bool1", "/api/positions?include_archived=1"),
    ("positions_search_hit", "/api/positions?search=beijing"),
    ("positions_search_hit_with_archived",
     "/api/positions?search=beijing&include_archived=true"),
    ("positions_search_case_insensitive", "/api/positions?search=RUST"),
    ("positions_search_miss", "/api/positions?search=zzz-nothing"),
    ("positions_search_padded", "/api/positions?search=%20beijing%20"),
    ("positions_limit_max", "/api/positions?limit=500"),
    ("positions_limit_zero", "/api/positions?limit=0"),
    ("positions_limit_over_max", "/api/positions?limit=501"),
    ("positions_offset_negative", "/api/positions?offset=-1"),
    ("positions_search_too_long",
     "/api/positions?search=" + "a" * 201),
    (
        "position_detail",
        "/api/positions/11111111-1111-1111-1111-111111111111",
    ),
    (
        "position_detail_archived",
        "/api/positions/44444444-4444-4444-4444-444444444444",
    ),
    ("position_detail_missing", "/api/positions/does-not-exist"),
]

BASE_URL_DEFAULT = "http://127.0.0.1:8010"


def fetch(base_url: str, path: str, timeout: float = 10.0) -> dict[str, object]:
    """GET one path and return status + decoded body (or raw text)."""
    url = base_url.rstrip("/") + path
    request = urllib.request.Request(url, headers={"accept": "application/json"})
    try:
        with urllib.request.urlopen(request, timeout=timeout) as response:
            status = response.status
            raw = response.read().decode("utf-8")
            content_type = response.headers.get("content-type", "")
    except urllib.error.HTTPError as exc:  # 4xx/5xx still carries a body
        status = exc.code
        raw = exc.read().decode("utf-8")
        content_type = exc.headers.get("content-type", "")

    try:
        body: object = json.loads(raw)
    except json.JSONDecodeError:
        body = {"__raw__": raw}
    return {"path": path, "status": status,
            "content_type": content_type.split(";")[0], "body": body}


def main(argv: list[str]) -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--base-url", default=BASE_URL_DEFAULT)
    parser.add_argument("--out", required=True)
    args = parser.parse_args(argv)

    results = {}
    for name, path in CASES:
        results[name] = fetch(args.base_url, path)
        print(f"{name:38s} {results[name]['status']} {path}")

    payload = {"base_url": args.base_url, "cases": results}
    with open(args.out, "w", encoding="utf-8") as handle:
        json.dump(payload, handle, indent=2, sort_keys=True, ensure_ascii=False)
        handle.write("\n")
    print(f"wrote {args.out} ({len(results)} cases)")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
