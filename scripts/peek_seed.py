#!/usr/bin/env python3
"""Print the rows the contract seed contains, read-only.

Handy when a capture looks wrong: it answers "is the server reading the database
I think it is?" without opening the file read-write (which would create WAL
sidecars next to a baseline artefact).

Usage:
    python3 scripts/peek_seed.py /tmp/ica-contract/seed.db
"""

from __future__ import annotations

import sqlite3
import sys
from pathlib import Path


def main(argv: list[str]) -> int:
    path = Path(argv[0] if argv else "/tmp/ica-contract/seed.db")
    connection = sqlite3.connect(f"file:{path}?mode=ro", uri=True)
    try:
        print(f"{path}")
        print("  id                                    created_at                  archived  status")
        for row in connection.execute(
            "SELECT id, created_at, archived, status FROM positions ORDER BY created_at DESC"
        ):
            print(f"  {row[0]:36s}  {row[1]:26s}  {row[2]}         {row[3]}")
        count = connection.execute("SELECT COUNT(*) FROM candidates").fetchone()[0]
        print(f"  candidates: {count}")
    finally:
        connection.close()
    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
