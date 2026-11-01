"""Package local companion builds for profiling or an explicitly hash-pinned installer."""
import argparse
import hashlib
import json
from pathlib import Path
import zipfile


def prepare(checkout, output, bundled=False):
    output.mkdir(parents=True, exist_ok=True)
    entries = []
    for name in ("todo", "mail", "cyberwork"):
        root = checkout / "plugins" / name
        metadata = json.loads((root / "plugin.json").read_text(encoding="utf-8"))["metadata"]
        binary_name = metadata["entry"] + ".exe"
        binary = checkout / "target" / "release" / binary_name
        if not binary.is_file():
            raise ValueError(f"Build the Windows release plugin first: {binary}")
        icon = root / "resources" / "icon.png"
        if not icon.is_file():
            icon = root / "icons" / "icon.png"
        archive = output / f"{metadata['id']}-{metadata['version']}.zip"
        with zipfile.ZipFile(archive, "w", compression=zipfile.ZIP_DEFLATED) as package:
            package.write(root / "plugin.json", "plugin.json")
            package.write(binary, binary_name)
            package.write(icon, "resources/icon.png")
            for license_file in ("LICENSE-MIT.txt", "OFL.txt"):
                package.write(checkout / license_file, license_file)
        entries.append({key: metadata[key] for key in
                        ("id", "name", "description", "localizations", "version", "app_versions", "protocol_version", "icon")})
        entries[-1].update(icon_url="" if bundled else str(icon.resolve()), downloads={"windows-x86_64": archive.name},
                           sha256=hashlib.sha256(archive.read_bytes()).hexdigest(), homepage=None)
    (output / ("plugins.json" if bundled else "catalog.json")).write_text(json.dumps({"plugins": entries}, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--checkout", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--bundled", action="store_true", help="prepare for --bundle-local-plugins")
    args = parser.parse_args()
    prepare(args.checkout.resolve(), args.output.resolve(), args.bundled)
