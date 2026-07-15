from __future__ import annotations

import argparse
import hashlib
import json
from pathlib import Path


def scan_null_terminated_ascii(data: bytes, min_length: int) -> list[dict[str, object]]:
    if min_length < 1:
        raise ValueError("min_length must be positive")

    observations: list[dict[str, object]] = []
    run_start: int | None = None
    for offset, value in enumerate(data):
        if 0x20 <= value <= 0x7E:
            if run_start is None:
                run_start = offset
            continue

        if value == 0 and run_start is not None and offset - run_start >= min_length:
            raw = data[run_start:offset]
            observations.append(
                {
                    "offset": run_start,
                    "length": len(raw),
                    "raw_hex": raw.hex(" ").upper(),
                    "text": raw.decode("ascii"),
                }
            )
        run_start = None

    return observations


def build_report(data: bytes, min_length: int) -> dict[str, object]:
    return {
        "schema_version": 1,
        "artifact_kind": "research_output",
        "question": "Which byte ranges match a NUL-terminated printable ASCII candidate?",
        "source": {
            "len": len(data),
            "sha256": hashlib.sha256(data).hexdigest(),
        },
        "parameters": {"min_length": min_length},
        "observations": scan_null_terminated_ascii(data, min_length),
        "product_input": False,
    }


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument("--source", type=Path, required=True)
    parser.add_argument("--evidence", type=Path, required=True)
    parser.add_argument("--min-length", type=int, default=4)
    return parser.parse_args()


def main() -> None:
    args = parse_args()
    report = build_report(args.source.read_bytes(), args.min_length)
    args.evidence.parent.mkdir(parents=True, exist_ok=True)
    args.evidence.write_text(
        json.dumps(report, ensure_ascii=False, indent=2, sort_keys=True) + "\n",
        encoding="utf-8",
    )


if __name__ == "__main__":
    main()
