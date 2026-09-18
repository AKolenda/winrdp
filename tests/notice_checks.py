#!/usr/bin/env python3
"""Check the Rust dependency notice inventory against the licence texts on disk."""
import hashlib
import json
import re
import subprocess
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
DOCS = ROOT / "docs"
manifest = json.loads((DOCS / "rust-dependency-notices.json").read_text())
packages = manifest["packages"]
failures = []

if manifest["package_count"] != len(packages):
    failures.append(f'package_count {manifest["package_count"]} disagrees with {len(packages)} package records')

referenced = set()
for package in packages:
    label = f'{package["name"]} {package["version"]}'
    for notice in package["notice_files"]:
        path = DOCS / notice["path"]
        referenced.add(path)
        if not path.is_file():
            failures.append(f"{label} references a missing notice file: {notice['path']}")
            continue
        digest = hashlib.sha256(path.read_bytes()).hexdigest()
        if digest != notice["sha256"]:
            failures.append(f"{label} notice {notice['path']} has sha256 {digest}, manifest says {notice['sha256']}")
    if package["status"] != "collected":
        failures.append(f'{label} is recorded as "{package["status"]}"')
    if not package["notice_files"] and not package["inherits_ironrdp_licenses"]:
        failures.append(f"{label} has no notice file and does not inherit the IronRDP licences")
    source = package.get("notice_source")
    if source is not None:
        commit = source.get("commit", "")
        if not re.fullmatch(r"[0-9a-f]{40}", commit):
            failures.append(f"{label} notice_source commit is not a full revision: {commit!r}")
        for entry in source["files"]:
            if not entry["url"].startswith("https://"):
                failures.append(f'{label} notice_source url is not https: {entry["url"]}')
            if "raw.githubusercontent.com" in entry["url"] and commit not in entry["url"]:
                failures.append(f'{label} notice_source url is not pinned to {commit}: {entry["url"]}')

for path in sorted((DOCS / "licenses/rust").rglob("*")):
    if path.is_file() and path not in referenced:
        failures.append(f"{path.relative_to(DOCS)} is on disk but no package references it")

explained = {(entry["name"], entry["version"]) for entry in manifest.get("notice_exceptions", [])}
for entry in manifest["missing_license_text"]:
    if (entry["name"], entry["version"]) not in explained:
        failures.append(f'{entry["name"]} {entry["version"]} has no licence text and no recorded exception')

pointer = subprocess.run(
    ["git", "ls-tree", "HEAD", "third_party/ironrdp"],
    cwd=ROOT,
    capture_output=True,
    text=True,
)
if pointer.returncode == 0 and pointer.stdout.strip():
    revision = pointer.stdout.split()[2]
    if revision != manifest["ironrdp_revision"]:
        failures.append(f'ironrdp_revision {manifest["ironrdp_revision"]} disagrees with the submodule pointer {revision}')

if failures:
    raise SystemExit("Dependency notice inventory problems:\n- " + "\n- ".join(failures))
print(f"Dependency notices agree: {len(packages)} packages, {len(referenced)} licence files")
