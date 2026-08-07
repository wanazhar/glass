#!/usr/bin/env python3
"""Certify the 0.2.x knowledge-store migration boundary."""

import argparse
import hashlib
import json
import pathlib
import subprocess
import tempfile


def fail(message: str) -> None:
    raise SystemExit(f"knowledge migration check failed: {message}")


def canonical(value: object) -> str:
    return json.dumps(value, sort_keys=True, separators=(",", ":"))


def run(binary: pathlib.Path, store: pathlib.Path, *args: str, expect_success: bool = True):
    result = subprocess.run(
        [str(binary), "--knowledge-store", str(store), *args],
        capture_output=True,
        text=True,
    )
    if expect_success and result.returncode != 0:
        fail(f"{args}: {result.stderr.strip() or result.stdout.strip()}")
    if not expect_success and result.returncode == 0:
        fail(f"{args}: incompatible input was accepted")
    return result


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--binary", required=True, type=pathlib.Path)
    parser.add_argument("--output", required=True, type=pathlib.Path)
    parser.add_argument(
        "--binding-kind",
        choices=("source_build", "packaged_artifact"),
        default="source_build",
    )
    parser.add_argument("--target")
    parser.add_argument(
        "--corpus",
        type=pathlib.Path,
        default=pathlib.Path("crates/glass-browser/benchmarks/scenarios/knowledge-v1.json"),
    )
    args = parser.parse_args()
    if args.binding_kind == "packaged_artifact" and not args.target:
        fail("--target is required for packaged artifact evidence")

    corpus = json.loads(args.corpus.read_text(encoding="utf-8"))
    records = [fixture["record"] for fixture in corpus["fixtures"]]
    snapshot = {"schemaVersion": 1, "records": records}
    binary_version = subprocess.check_output(
        [str(args.binary), "--version"], text=True
    ).strip()

    with tempfile.TemporaryDirectory(prefix="glass-knowledge-migration-") as directory:
        root = pathlib.Path(directory)
        store = root / "knowledge.json"
        source = root / "source.json"
        exported = root / "exported.json"
        incompatible = root / "incompatible.json"
        after_rejection = root / "after-rejection.json"
        source.write_text(json.dumps(snapshot, indent=2) + "\n", encoding="utf-8")
        run(args.binary, store, "knowledge", "import", str(source))
        run(args.binary, store, "knowledge", "export", str(exported))
        round_trip = json.loads(exported.read_text(encoding="utf-8"))
        if canonical(round_trip) != canonical(snapshot):
            fail("v1 import/export changed the persisted snapshot")

        incompatible_snapshot = {**snapshot, "schemaVersion": 2}
        incompatible.write_text(
            json.dumps(incompatible_snapshot, indent=2) + "\n", encoding="utf-8"
        )
        run(
            args.binary,
            store,
            "knowledge",
            "import",
            str(incompatible),
            expect_success=False,
        )
        run(args.binary, store, "knowledge", "export", str(after_rejection))
        if canonical(json.loads(after_rejection.read_text(encoding="utf-8"))) != canonical(
            snapshot
        ):
            fail("rejected schema version altered the existing store")

    report = {
        "schema_version": 1,
        "type": "knowledge_migration_evidence",
        "source_revision": subprocess.check_output(
            ["git", "rev-parse", "HEAD"], text=True
        ).strip(),
        "binary_version": binary_version,
        "artifact_binding": {
            "kind": args.binding_kind,
            "name": args.binary.name,
            **({"target": args.target} if args.target else {}),
            "sha256": hashlib.sha256(args.binary.read_bytes()).hexdigest(),
            "size_bytes": args.binary.stat().st_size,
        },
        "source_contract": str(args.corpus),
        "from_schema_version": 1,
        "to_schema_version": 1,
        "migration": "no_op_v1",
        "record_count": len(records),
        "round_trip_preserved": True,
        "incompatible_schema_rejected": True,
        "rejected_input_left_store_unchanged": True,
        "runtime_certification": "certified",
    }
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")
    print(
        f"knowledge migration certified: v1 round-trip and v2 rejection "
        f"({len(records)} records)"
    )


if __name__ == "__main__":
    main()
