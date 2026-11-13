#!/usr/bin/env python3
"""End-to-end Horizon 26.3 native mod builder & verifier for CI and local Rust hosts.

Downloads the pinned Horizon 26.3 commit from `compatibility.json`, builds
`horizon-mixins-cli` at the pinned revision, applies FlyingWeb's mixin bundle
onto Horizon's workspace, compiles `horizon-client` with `RUSTFLAGS="-D warnings"`,
packages `dist/flyingweb-<version>.hnmod`, and validates the `.hnmod` archive
using Horizon's own `cargo-horizon-mod check`.
"""

from __future__ import annotations

import argparse
import hashlib
import io
import json
import os
import shutil
import subprocess
import sys
import tarfile
import urllib.request
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
WORK = ROOT / "target" / "horizon-ci"


def run(cmd: list[str], cwd: Path | None = None, env: dict[str, str] | None = None) -> None:
    print("+", " ".join(cmd), flush=True)
    subprocess.run(cmd, cwd=cwd, env=env, check=True)


def fetch_bytes(url: str) -> bytes:
    req = urllib.request.Request(url, headers={"User-Agent": "flyingweb-horizon-ci"})
    token = os.environ.get("GITHUB_TOKEN")
    if token and url.startswith("https://api.github.com/"):
        req.add_header("Authorization", f"Bearer {token}")
    with urllib.request.urlopen(req, timeout=300) as resp:
        return resp.read()


def ensure_horizon_source(repo_url: str, rev: str) -> Path:
    slug = repo_url.rstrip("/").removeprefix("https://github.com/")
    dest = WORK / "horizon" / rev
    marker = dest / ".horizon-revision"
    if marker.is_file():
        return dest

    print(f"Downloading Horizon {slug} @ {rev[:10]}...", flush=True)
    archive_bytes = fetch_bytes(f"https://codeload.github.com/{slug}/tar.gz/{rev}")
    partial = WORK / "horizon" / f".{rev}.partial"
    shutil.rmtree(partial, ignore_errors=True)
    partial.mkdir(parents=True, exist_ok=True)
    with tarfile.open(fileobj=io.BytesIO(archive_bytes), mode="r:gz") as tar:
        tar.extractall(partial)
    extracted = next(partial.iterdir())
    # horizon-mixins rejects symlinks inside the target workspace
    for p in extracted.rglob("*"):
        if p.is_symlink():
            p.unlink()
    (extracted / ".horizon-revision").write_text(rev, encoding="utf-8")
    shutil.rmtree(dest, ignore_errors=True)
    dest.parent.mkdir(parents=True, exist_ok=True)
    extracted.rename(dest)
    shutil.rmtree(partial, ignore_errors=True)
    return dest


def ensure_mixins_cli(repo_url: str, rev: str) -> Path:
    install_root = WORK / "horizon-mixins" / rev
    binary = install_root / "bin" / "horizon-mixins"
    if binary.is_file():
        return binary
    print(f"Installing horizon-mixins-cli @ {rev[:10]}...", flush=True)
    run(
        [
            "cargo",
            "install",
            "--locked",
            "--git",
            repo_url,
            "--rev",
            rev,
            "--root",
            str(install_root),
            "horizon-mixins-cli",
        ]
    )
    return binary


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--out-dir",
        type=Path,
        default=ROOT / "dist",
        help="Output directory for verified .hnmod and .sha256",
    )
    args = parser.parse_args()

    compat = json.loads((ROOT / "compatibility.json").read_text(encoding="utf-8"))
    horizon_tree = ensure_horizon_source(
        compat["horizon_repository"], compat["horizon_revision"]
    )
    mixins_bin = ensure_mixins_cli(
        compat["horizon_mixins_repository"], compat["horizon_mixins_revision"]
    )

    # 1. Structural & hook-target validation + deterministic .hnmod packaging
    run(
        [
            sys.executable,
            str(ROOT / "tools" / "verify_bundle.py"),
            "package",
            "--horizon-source",
            str(horizon_tree),
            "--out-dir",
            str(args.out_dir),
        ]
    )

    # 2. Stage mixin bundle and apply onto Horizon 26.3 workspace
    mods_dir = WORK / "mods"
    stage_dir = WORK / "stage"
    shutil.rmtree(mods_dir, ignore_errors=True)
    shutil.rmtree(stage_dir, ignore_errors=True)
    shutil.copytree(ROOT / "mixins" / "flyingweb", mods_dir / "flyingweb")

    run(
        [
            str(mixins_bin),
            "apply",
            "--workspace",
            str(horizon_tree),
            "--mods",
            str(mods_dir),
            "--out",
            str(stage_dir),
        ]
    )

    # 3. Compile Horizon 26.3 client with FlyingWeb injected under -D warnings
    env = dict(os.environ, RUSTFLAGS="-D warnings")
    run(
        [
            "cargo",
            "check",
            "--locked",
            "-p",
            "horizon-client",
            "--bin",
            "horizon-client",
        ],
        cwd=stage_dir,
        env=env,
    )

    # 4. Run Horizon's official `cargo-horizon-mod check` on the packaged .hnmod
    hnmod_files = sorted(args.out_dir.glob("*.hnmod"))
    if len(hnmod_files) != 1:
        raise SystemExit(f"Expected 1 .hnmod in {args.out_dir}, found {hnmod_files}")
    hnmod = hnmod_files[0]

    run(
        [
            "cargo",
            "run",
            "--locked",
            "--manifest-path",
            str(horizon_tree / "crates" / "cargo-horizon-mod" / "Cargo.toml"),
            "--",
            "check",
            str(hnmod),
        ]
    )

    sha256 = hashlib.sha256(hnmod.read_bytes()).hexdigest()
    sha_file = args.out_dir / f"{hnmod.name}.sha256"
    sha_file.write_text(f"{sha256}  {hnmod.name}\n", encoding="utf-8")
    print(f"Verified {hnmod.name} (SHA256: {sha256})", flush=True)


if __name__ == "__main__":
    main()
