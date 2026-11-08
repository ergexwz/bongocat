"""Package the three SDK examples without a host checkout or third-party Python modules."""
import argparse
import hashlib
import json
from pathlib import Path
import zipfile

EXAMPLES = ("event_counter", "scheduled_timer", "companion_hud")


def package(binary_directory, output, target):
    suffix = ".exe" if target == "x86_64-pc-windows-msvc" else ""
    inputs = []
    for name in EXAMPLES:
        manifest = (Path(__file__).parent / "manifests" / f"{name}.json").read_bytes()
        metadata = json.loads(manifest)["metadata"]
        entry = metadata["entry"] + suffix
        archive = output / f"{metadata['id']}-{metadata['version']}-{target}.zip"
        if archive.exists():
            raise FileExistsError(f"Keep the existing package; choose a fresh output directory: {archive}")
        inputs.append((manifest, entry, (binary_directory / entry).read_bytes(), archive))
    output.mkdir(parents=True, exist_ok=True)
    receipts = []
    for manifest, entry, binary, archive in inputs:
        with zipfile.ZipFile(archive, "x", compression=zipfile.ZIP_DEFLATED) as bundle:
            bundle.writestr("plugin.json", manifest)
            executable = zipfile.ZipInfo(entry)
            executable.create_system = 3
            executable.external_attr = 0o100755 << 16
            executable.compress_type = zipfile.ZIP_DEFLATED
            bundle.writestr(executable, binary)
        receipts.append({"package": archive.name, "sha256": hashlib.sha256(archive.read_bytes()).hexdigest()})
    print(json.dumps(receipts, indent=2))


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--bin-dir", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--target", choices=("x86_64-pc-windows-msvc", "aarch64-apple-darwin", "x86_64-apple-darwin"), required=True)
    args = parser.parse_args()
    package(args.bin_dir, args.output, args.target)
