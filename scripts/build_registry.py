#!/usr/bin/env python3
"""Build the module registry published on GitHub Pages.

The IDE's extension picker reads `registry.json` to list installable modules and
download the zip for the running platform (see the Coding Unicorns repo,
src/extension/). Each module entry comes from `<crate>/manifest.toml`; its assets
are the per-platform zips produced by the release job (`<crate>-<platform>.zip`).

Usage:
    python scripts/build_registry.py --zips release-zips --tag v0.4.0 \
        --repo Ajustor/coding-unicorns-modules --out site
"""
import argparse
import datetime as dt
import hashlib
import html
import json
import re
import tomllib
from pathlib import Path

SCHEMA = 1
# Matrix platform -> registry key (`{std::env::consts::OS}-{std::env::consts::ARCH}`).
PLATFORMS = {
    "linux": "linux-x86_64",
    "windows": "windows-x86_64",
    "macos": "macos-aarch64",
}


def workspace_members(root: Path) -> list[str]:
    with open(root / "Cargo.toml", "rb") as f:
        return tomllib.load(f)["workspace"]["members"]


def sha256(path: Path) -> str:
    h = hashlib.sha256()
    with open(path, "rb") as f:
        for chunk in iter(lambda: f.read(1 << 16), b""):
            h.update(chunk)
    return h.hexdigest()


def module_entry(root: Path, crate: str, zips: Path, tag: str, repo: str) -> dict:
    with open(root / crate / "manifest.toml", "rb") as f:
        manifest = tomllib.load(f)
    ext = manifest["extension"]
    caps = manifest.get("capabilities", {})
    entry = {
        "id": ext["id"],
        "dir": crate,
        "name": ext["name"],
        "version": ext["version"],
        "description": ext.get("description", ""),
        "author": ext.get("author", ""),
        "languages": caps.get("languages", []),
        "lsp_server": caps.get("lsp_server") or None,
        "dependencies": {k: v for k, v in manifest.get("dependencies", {}).items() if v},
        "assets": {},
    }
    for key in PLATFORMS.values():
        name = f"{crate}-{key}.zip"
        z = zips / name
        if z.is_file():
            entry["assets"][key] = {
                "url": f"https://github.com/{repo}/releases/download/{tag}/{name}",
                "sha256": sha256(z),
                "size": z.stat().st_size,
            }
    return entry


def render_html(registry: dict) -> str:
    rows = []
    for m in registry["modules"]:
        langs = ", ".join(f".{l}" for l in m["languages"])
        platforms = ", ".join(sorted(m["assets"])) or "—"
        rows.append(
            "<tr>"
            f"<td><strong>{html.escape(m['name'])}</strong><br><code>{html.escape(m['id'])}</code></td>"
            f"<td>{html.escape(m['version'])}</td>"
            f"<td>{html.escape(m['description'])}</td>"
            f"<td>{html.escape(langs)}</td>"
            f"<td>{html.escape(m['lsp_server'] or '—')}</td>"
            f"<td>{html.escape(platforms)}</td>"
            "</tr>"
        )
    return f"""<!doctype html>
<html lang="en"><head><meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>Coding Unicorns modules</title>
<style>
  body {{ font: 15px/1.5 system-ui, sans-serif; margin: 2rem auto; max-width: 1100px; padding: 0 16px; }}
  table {{ border-collapse: collapse; width: 100%; }}
  th, td {{ text-align: left; padding: .5rem; border-bottom: 1px solid #ddd; vertical-align: top; }}
  code {{ font-size: 12px; color: #666; }}
  @media (prefers-color-scheme: dark) {{ body {{ background: #16131f; color: #e8e4f4; }} td, th {{ border-color: #333; }} code {{ color: #aaa; }} }}
</style></head><body>
<h1>🦄 Coding Unicorns modules</h1>
<p>Release <strong>{html.escape(registry['release'])}</strong>. Install them from the IDE's extension picker
(Registry tab), or point it at <a href="registry.json">registry.json</a>.</p>
<table><thead><tr><th>Module</th><th>Version</th><th>Description</th><th>Files</th><th>LSP</th><th>Platforms</th></tr></thead>
<tbody>{''.join(rows)}</tbody></table>
</body></html>
"""


def main() -> None:
    ap = argparse.ArgumentParser()
    ap.add_argument("--root", type=Path, default=Path("."), help="workspace root")
    ap.add_argument("--zips", type=Path, required=True, help="dir with <crate>-<platform>.zip")
    ap.add_argument("--tag", required=True)
    ap.add_argument("--repo", required=True, help="owner/name for release download URLs")
    ap.add_argument("--out", type=Path, required=True)
    args = ap.parse_args()

    if not re.fullmatch(r"v\d+\.\d+\.\d+(-[\w.]+)?", args.tag):
        raise SystemExit(f"unexpected tag: {args.tag}")
    modules = [
        module_entry(args.root, crate, args.zips, args.tag, args.repo)
        for crate in workspace_members(args.root)
    ]
    registry = {
        "schema": SCHEMA,
        "release": args.tag,
        "generated_at": dt.datetime.now(dt.timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ"),
        "modules": sorted(modules, key=lambda m: m["name"].lower()),
    }
    args.out.mkdir(parents=True, exist_ok=True)
    (args.out / "registry.json").write_text(json.dumps(registry, indent=2, ensure_ascii=False), encoding="utf-8")
    (args.out / "index.html").write_text(render_html(registry), encoding="utf-8")
    missing = [m["id"] for m in modules if len(m["assets"]) < len(PLATFORMS)]
    print(f"registry: {len(modules)} modules, {args.tag}" + (f"; incomplete assets: {missing}" if missing else ""))


if __name__ == "__main__":
    main()
