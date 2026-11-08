"""Package local companion builds for profiling or an explicitly hash-pinned installer."""
import argparse
import hashlib
import json
from pathlib import Path
import tempfile
import zipfile


def prepare(checkout, output, binary_dir, bundled=False):
    # Require the build directory explicitly: target/release and
    # target/<triple>/release can both exist and contain different protocols.
    inputs = []
    for name in ("todo", "mail", "cyberwork"):
        root = checkout / "plugins" / name
        metadata = json.loads((root / "plugin.json").read_text(encoding="utf-8"))["metadata"]
        binary_name = metadata["entry"] + ".exe"
        if not binary_name.isascii() or any(not (c.isalnum() or c in "._-") for c in binary_name):
            raise ValueError("Plugin entry is not a safe executable name")
        binary = binary_dir / binary_name
        if not binary.is_file():
            raise ValueError(f"Build the Windows release plugin first: {binary}")
        icon_name = metadata["icon"]["value"]
        icon_path = Path(icon_name)
        icon_file = (root / icon_path).resolve()
        if (icon_path.is_absolute() or ".." in icon_path.parts
                or not icon_file.is_relative_to(root.resolve()) or not icon_file.is_file()):
            raise ValueError(f"Plugin icon is missing or unsafe: {icon_name}")
        archive_name = f"{metadata['id']}-{metadata['version']}.zip"
        if not archive_name.isascii() or any(not (c.isalnum() or c in "._-") for c in archive_name):
            raise ValueError("Plugin package identity is not a safe filename")
        inputs.append((root, metadata, binary, icon_path, archive_name))
    for license_file in ("LICENSE-MIT.txt", "OFL.txt"):
        if not (checkout / license_file).is_file():
            raise ValueError(f"Plugin license is missing: {license_file}")
    output.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix=".companion-stage-", dir=output) as directory:
        stage = Path(directory).resolve()
        if not stage.is_relative_to(output.resolve()) or stage == output.resolve():
            raise ValueError("Unsafe staging directory")
        entries = []
        for root, metadata, binary, icon_path, archive_name in inputs:
            icon = root / icon_path
            archive = stage / archive_name
            with zipfile.ZipFile(archive, "w", compression=zipfile.ZIP_DEFLATED) as package:
                package.write(root / "plugin.json", "plugin.json")
                package.write(binary, binary.name)
                package.write(icon, icon_path.as_posix())
                for license_file in ("LICENSE-MIT.txt", "OFL.txt"):
                    package.write(checkout / license_file, license_file)
            digest = hashlib.sha256(archive.read_bytes()).hexdigest()
            existing = output / archive_name
            if existing.exists() and hashlib.sha256(existing.read_bytes()).hexdigest() != digest:
                raise ValueError(f"Package version is immutable; use a fresh output directory: {archive_name}")
            entries.append({key: metadata[key] for key in
                            ("id", "name", "description", "localizations", "version", "app_versions", "protocol_version", "icon")})
            entries[-1].update(icon_url="" if bundled else str(icon.resolve()), downloads={"windows-x86_64": archive_name},
                               sha256=digest, homepage=None)
        catalog_name = "plugins.json" if bundled else "catalog.json"
        (stage / catalog_name).write_text(json.dumps({"plugins": entries}, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
        for _, _, _, _, archive_name in inputs:
            if not (output / archive_name).exists():
                (stage / archive_name).replace(output / archive_name)
        (stage / catalog_name).replace(output / catalog_name)


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--checkout", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--binary-dir", type=Path, required=True,
                        help="exact directory containing the Windows release executables")
    parser.add_argument("--bundled", action="store_true", help="prepare for --bundle-local-plugins")
    args = parser.parse_args()
    prepare(args.checkout.resolve(), args.output.resolve(), args.binary_dir.resolve(), args.bundled)
