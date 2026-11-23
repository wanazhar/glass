#!/usr/bin/env python3
"""Validate cargo-binstall metadata against the official release asset names."""

from __future__ import annotations

import sys
from pathlib import Path

try:
    import tomllib
except ImportError:  # pragma: no cover - Python < 3.11
    import tomli as tomllib  # type: ignore


REPO = "https://github.com/wanazhar/glass"
PACKAGES = (
    "crates/glass-dev/Cargo.toml",
    "crates/glass-browser/Cargo.toml",
)
TARGETS = (
    "x86_64-unknown-linux-gnu",
    "aarch64-unknown-linux-gnu",
    "x86_64-apple-darwin",
    "aarch64-apple-darwin",
    "x86_64-pc-windows-msvc",
)
REQUIRED_KEYS = ("pkg-url", "bin-dir", "pkg-fmt")
UNIX_URL = "{ repo }/releases/download/v{ version }/glass-{ target }.tar.gz"
WINDOWS_URL = "{ repo }/releases/download/v{ version }/glass-{ target }.zip"


def fail(message: str) -> None:
    raise SystemExit(message)


def main() -> None:
    root = Path(__file__).resolve().parents[1]
    errors: list[str] = []

    for relpath in PACKAGES:
        manifest = tomllib.loads((root / relpath).read_text(encoding="utf-8"))
        package = manifest["package"]["name"]
        meta = (
            manifest.get("package", {})
            .get("metadata", {})
            .get("binstall")
        )
        if not isinstance(meta, dict):
            errors.append(f"{relpath}: missing [package.metadata.binstall]")
            continue
        for key in REQUIRED_KEYS:
            if key not in meta:
                errors.append(f"{relpath}: missing {key}")
        if meta.get("pkg-url") != UNIX_URL:
            errors.append(
                f"{relpath}: pkg-url must be {UNIX_URL!r}, got {meta.get('pkg-url')!r}"
            )
        if meta.get("bin-dir") != "{ bin }{ binary-ext }":
            errors.append(f"{relpath}: bin-dir must place binaries at the archive root")
        if meta.get("pkg-fmt") not in {"tgz", "tar.gz"}:
            errors.append(f"{relpath}: pkg-fmt must be tgz or tar.gz for Unix archives")
        overrides = meta.get("overrides") or {}
        windows = overrides.get("x86_64-pc-windows-msvc") or {}
        if windows.get("pkg-fmt") != "zip":
            errors.append(f"{relpath}: Windows override pkg-fmt must be zip")
        if windows.get("pkg-url") != WINDOWS_URL:
            errors.append(
                f"{relpath}: Windows pkg-url must be {WINDOWS_URL!r}, got {windows.get('pkg-url')!r}"
            )

        version = "0.0.0"
        for target in TARGETS:
            if target.endswith("-windows-msvc"):
                url = (
                    WINDOWS_URL.replace("{ repo }", REPO)
                    .replace("{ version }", version)
                    .replace("{ target }", target)
                )
                expected = f"{REPO}/releases/download/v{version}/glass-{target}.zip"
            else:
                url = (
                    UNIX_URL.replace("{ repo }", REPO)
                    .replace("{ version }", version)
                    .replace("{ target }", target)
                )
                expected = f"{REPO}/releases/download/v{version}/glass-{target}.tar.gz"
            if url != expected:
                errors.append(f"{package}: expanded URL for {target} was {url}")

    if errors:
        fail("cargo-binstall metadata check failed:\n- " + "\n- ".join(errors))
    print(
        "cargo-binstall metadata matches official asset names for "
        f"{len(PACKAGES)} packages and {len(TARGETS)} targets"
    )


if __name__ == "__main__":
    main()
