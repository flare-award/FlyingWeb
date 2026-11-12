#!/usr/bin/env python3
"""Validate the FlyingWeb Horizon 26.3 native mod bundle and package `.hnmod`.

Enforces the exact structural, manifest, source-hygiene, dependency, and archive
invariants required by `horizon-mixins`, `horizon-mods/native/tools/native.py`,
and `cargo-horizon-mod check`.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import re
import sys
import tomllib
import zipfile
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]

# Crates locked in Horizon 26.3 Nightly `fdc87ae` Cargo.lock that FlyingWeb may depend on.
ALLOWED_CRATE_DEPS = {
    "bitflags",
    "bytemuck",
    "glam",
    "log",
    "png",
    "rustc-hash",
    "serde",
    "serde_json",
}

FORBIDDEN_INJECTED_PATTERNS = [
    (re.compile(r"^\s*#!\[", re.M), "inner attribute `#![...]` is forbidden in injected files"),
    (re.compile(r"^\s*//!"), "inner doc comment `//!` is forbidden in injected files"),
    (re.compile(r"^\s*(?:pub\s+)?mod\s+[A-Za-z0-9_]+\s*;", re.M), "external `mod foo;` is forbidden in injected files"),
    (re.compile(r"\binclude!\s*\("), "`include!(...)` is forbidden in injected files"),
    (re.compile(r"\binclude_bytes!\s*\("), "`include_bytes!(...)` is forbidden in injected files"),
    (re.compile(r"\binclude_str!\s*\("), "`include_str!(...)` is forbidden in injected files"),
]


def fail(msg: str) -> None:
    print(f"ERROR: {msg}", file=sys.stderr)
    sys.exit(1)


def validate_bundle(horizon_source: Path | None = None) -> tuple[dict, dict, Path]:
    compat_path = ROOT / "compatibility.json"
    if not compat_path.is_file():
        fail("Missing compatibility.json at repository root")
    compat = json.loads(compat_path.read_text(encoding="utf-8"))
    for key in (
        "game",
        "hooks",
        "horizon_repository",
        "horizon_revision",
        "horizon_mixins_repository",
        "horizon_mixins_revision",
    ):
        if key not in compat:
            fail(f"compatibility.json missing required key `{key}`")

    if compat["game"] != "26.3":
        fail(f"Expected game == '26.3', got {compat['game']!r}")
    if not re.fullmatch(r"[0-9a-f]{40}", compat["horizon_revision"]):
        fail("horizon_revision must be a 40-char lowercase hex SHA")
    if not re.fullmatch(r"[0-9a-f]{40}", compat["horizon_mixins_revision"]):
        fail("horizon_mixins_revision must be a 40-char lowercase hex SHA")

    bundle_dir = ROOT / "mixins" / "flyingweb"
    mixin_toml_path = bundle_dir / "horizon-mixin.toml"
    if not mixin_toml_path.is_file():
        fail("Missing mixins/flyingweb/horizon-mixin.toml")
    if not (bundle_dir / "LICENSE").is_file():
        fail("Missing mixins/flyingweb/LICENSE")

    manifest = tomllib.loads(mixin_toml_path.read_text(encoding="utf-8"))
    mod_meta = manifest.get("mod", {})
    if mod_meta.get("id") != "flyingweb":
        fail(f"Expected [mod].id == 'flyingweb', got {mod_meta.get('id')!r}")
    if not mod_meta.get("version"):
        fail("Missing [mod].version in horizon-mixin.toml")

    pinned_rev = manifest.get("compatibility", {}).get("horizon_revision")
    if pinned_rev != compat["horizon_revision"]:
        fail(
            f"horizon-mixin.toml horizon_revision ({pinned_rev}) != "
            f"compatibility.json ({compat['horizon_revision']})"
        )

    # Validate [[crate]] entries
    for crate_entry in manifest.get("crate", []):
        crate_dir = bundle_dir / crate_entry["path"]
        cargo_toml = crate_dir / "Cargo.toml"
        if not cargo_toml.is_file():
            fail(f"Missing crate Cargo.toml at {cargo_toml}")
        if (crate_dir / "Cargo.lock").exists():
            fail(f"Bundled crate {crate_dir} must not contain its own Cargo.lock")
        crate_manifest = tomllib.loads(cargo_toml.read_text(encoding="utf-8"))
        for section in ("dependencies", "dev-dependencies"):
            for dep in crate_manifest.get(section, {}):
                if dep not in ALLOWED_CRATE_DEPS:
                    fail(
                        f"Crate dependency `{dep}` in {cargo_toml} is not in "
                        f"Horizon's locked dependency allowlist: {sorted(ALLOWED_CRATE_DEPS)}"
                    )

    # Validate [[module]] and [[injection]] files against horizon-mixins AST constraints
    injected_files: list[Path] = []
    for mod_entry in manifest.get("module", []):
        src = bundle_dir / mod_entry["source"]
        if not src.is_file():
            fail(f"Missing [[module]] source file: {src}")
        injected_files.append(src)

    hooks_text = None
    if horizon_source is not None:
        nh = horizon_source / "crates" / "horizon-client" / "src" / "game" / "native_hooks.rs"
        if nh.is_file():
            hooks_text = nh.read_text(encoding="utf-8")

    seen_ids: set[str] = set()
    for inj in manifest.get("injection", []):
        inj_id = inj["id"]
        if inj_id in seen_ids:
            fail(f"Duplicate [[injection]] id: {inj_id}")
        seen_ids.add(inj_id)
        helper = bundle_dir / inj["helper"]
        if not helper.is_file():
            fail(f"Missing [[injection]] helper file: {helper}")
        injected_files.append(helper)

        target = inj["target"]
        prefix = "horizon_client::game::native_hooks::Game<'_>::"
        if not target.startswith(prefix):
            fail(f"Injection `{inj_id}` target `{target}` does not target native_hooks::Game")
        fn_name = target[len(prefix) :]
        if hooks_text is not None and f"fn {fn_name}(" not in hooks_text:
            fail(f"Injection `{inj_id}` target `{fn_name}` not found in Horizon native_hooks.rs")

    for src_file in injected_files:
        content = src_file.read_text(encoding="utf-8")
        for line_idx, line in enumerate(content.splitlines(), start=1):
            if line.lstrip().startswith("//!"):
                fail(f"{src_file}:{line_idx}: inner doc comment `//!` is forbidden in injected files")
        for regex, reason in FORBIDDEN_INJECTED_PATTERNS:
            if regex.search(content):
                fail(f"{src_file}: {reason}")

    return compat, manifest, bundle_dir


def package_hnmod(compat: dict, manifest: dict, bundle_dir: Path, out_dir: Path) -> Path:
    mod_id = manifest["mod"]["id"]
    mod_version = manifest["mod"]["version"]
    hn_manifest = {
        "schema": 1,
        "id": mod_id,
        "version": mod_version,
        "game": compat["game"],
        "horizon": compat["horizon_revision"],
        "mixins": compat["horizon_mixins_revision"],
        "hooks": compat["hooks"],
    }
    files: dict[str, bytes] = {
        "horizon-native.json": (
            json.dumps(hn_manifest, indent=2, sort_keys=True) + "\n"
        ).encode("utf-8")
    }
    data_dirs = [Path("data")] + [
        Path(c["path"]) / "data" for c in manifest.get("crate", [])
    ]
    for path in sorted(bundle_dir.rglob("*")):
        if path.is_symlink():
            fail(f"Symlink forbidden inside bundle: {path}")
        if not path.is_file():
            continue
        rel = path.relative_to(bundle_dir)
        if (
            path.suffix in (".rs", ".toml")
            or rel.name in ("LICENSE", "README.md", "NOTICE")
            or any(rel.is_relative_to(d) for d in data_dirs)
        ):
            files[f"mixins/{mod_id}/{rel.as_posix()}"] = path.read_bytes()

    if len(files) > 512:
        fail(f".hnmod entry count {len(files)} exceeds Horizon's 512-entry limit")

    total_bytes = sum(len(b) for b in files.values())
    if total_bytes > 64 * 1024 * 1024:
        fail(f".hnmod uncompressed size {total_bytes} exceeds 64 MiB limit")

    out_dir.mkdir(parents=True, exist_ok=True)
    out_path = out_dir / f"{mod_id}-{mod_version}.hnmod"
    with zipfile.ZipFile(out_path, "w", compression=zipfile.ZIP_DEFLATED) as zf:
        for name, content in sorted(files.items()):
            info = zipfile.ZipInfo(name, (2026, 1, 1, 0, 0, 0))
            info.compress_type = zipfile.ZIP_DEFLATED
            info.external_attr = 0o100644 << 16
            zf.writestr(info, content)

    digest = hashlib.sha256(out_path.read_bytes()).hexdigest()
    print(
        json.dumps(
            {
                "status": "ok",
                "file": str(out_path),
                "entries": len(files),
                "size": out_path.stat().st_size,
                "sha256": digest,
            },
            indent=2,
        )
    )
    return out_path


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "action",
        choices=["check", "package"],
        help="Validate bundle structure or package deterministic .hnmod archive",
    )
    parser.add_argument(
        "--horizon-source",
        type=Path,
        default=None,
        help="Optional unpacked Horizon source tree to verify hook targets against",
    )
    parser.add_argument(
        "--out-dir",
        type=Path,
        default=ROOT / "dist",
        help="Output directory for packaged .hnmod",
    )
    args = parser.parse_args()
    compat, manifest, bundle_dir = validate_bundle(args.horizon_source)
    if args.action == "package":
        package_hnmod(compat, manifest, bundle_dir, args.out_dir)
    else:
        print(
            json.dumps(
                {
                    "status": "ok",
                    "id": manifest["mod"]["id"],
                    "version": manifest["mod"]["version"],
                    "horizon": compat["horizon_revision"],
                    "hooks": compat["hooks"],
                },
                indent=2,
            )
        )


if __name__ == "__main__":
    main()
