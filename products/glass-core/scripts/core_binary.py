"""Resolve the Core executable from the root Cargo target directory."""
from pathlib import Path
import os

REPOSITORY_ROOT = Path(__file__).resolve().parents[3]


def core_binary_path(profile: str) -> Path:
    override = os.environ.get("GLASS_CORE_TEST_BINARY")
    if override:
        return Path(override)
    target_profile = "core-release" if profile == "release" else profile
    target_root = Path(os.environ.get("CARGO_TARGET_DIR", REPOSITORY_ROOT / "target"))
    return target_root / target_profile / "glass-core"
