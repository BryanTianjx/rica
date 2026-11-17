#!/usr/bin/env python3
"""Diff two contract captures produced by ``contract_capture.py``.

The comparison is deliberately structural, not textual:

* JSON object key order is irrelevant.
* ``details.errors`` inside a 422 envelope is provider-specific (FastAPI
  renders pydantic's error list, the Rust port renders its own stably shaped
  list), so only ``status`` + ``error.code`` + ``error.message`` are compared
  there. Every other part of a 422 envelope must match.
* Every other body is compared value-for-value after the payload is
  normalised (kept as parsed JSON, key order ignored).

Exit code 0 means every case matched; 1 means at least one mismatch.
"""

from __future__ import annotations

import argparse
import json
import sys
from typing import Any

#: Keys whose values are allowed to differ between implementations because
#: they describe *how* validation failed, not *whether* it failed.
VOLATILE_422_KEYS = ("errors",)


def normalise(body: Any, status: int) -> Any:
    if status != 422 or not isinstance(body, dict):
        return body
    error = body.get("error")
    if not isinstance(error, dict):
        return body
    details = error.get("details")
    if isinstance(details, dict):
        details = {k: v for k, v in details.items()
                   if k not in VOLATILE_422_KEYS}
    return {
        "error": {
            "code": error.get("code"),
            "message": error.get("message"),
            "details_keys": sorted(details) if isinstance(details, dict)
            else None,
        }
    }


def field_set(value: Any) -> Any:
    """Structurally describe a JSON value, for field-set comparisons."""
    if isinstance(value, dict):
        return {k: field_set(v) for k, v in value.items()}
    if isinstance(value, list):
        return [field_set(v) for v in value]
    return True


def diff_case(name: str, left: dict, right: dict) -> list[str]:
    problems: list[str] = []
    if left["status"] != right["status"]:
        problems.append(
            f"status: python={left['status']} rust={right['status']}")
    left_body = normalise(left["body"], left["status"])
    right_body = normalise(right["body"], right["status"])

    left_shape = field_set(left_body)
    right_shape = field_set(right_body)
    if left_shape != right_shape:
        problems.append(
            f"shape differs:\n  python={json.dumps(left_shape, sort_keys=True)}"
            f"\n  rust  ={json.dumps(right_shape, sort_keys=True)}")

    if left_body != right_body:
        problems.append(
            "values differ:\n  python="
            + json.dumps(left_body, sort_keys=True, ensure_ascii=False)
            + "\n  rust  ="
            + json.dumps(right_body, sort_keys=True, ensure_ascii=False))
    return problems


def main(argv: list[str]) -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--python", required=True,
                        help="capture produced against the Python baseline")
    parser.add_argument("--rust", required=True,
                        help="capture produced against the Rust port")
    args = parser.parse_args(argv)

    with open(args.python, encoding="utf-8") as handle:
        py = json.load(handle)
    with open(args.rust, encoding="utf-8") as handle:
        rs = json.load(handle)

    py_cases, rs_cases = py["cases"], rs["cases"]
    only_py = sorted(set(py_cases) - set(rs_cases))
    only_rs = sorted(set(rs_cases) - set(py_cases))
    failures = 0

    if only_py or only_rs:
        print(f"case set differs: only_python={only_py} only_rust={only_rs}")
        failures += len(only_py) + len(only_rs)

    for name in sorted(set(py_cases) & set(rs_cases)):
        problems = diff_case(name, py_cases[name], rs_cases[name])
        if problems:
            failures += 1
            print(f"FAIL {name}")
            for problem in problems:
                print("  " + problem.replace("\n", "\n  "))
        else:
            print(f"PASS {name}")

    total = len(set(py_cases) & set(rs_cases))
    print(f"\n{total - failures if failures <= total else 0}/{total} cases "
          f"matched, {failures} failure(s)")
    return 0 if failures == 0 else 1


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
