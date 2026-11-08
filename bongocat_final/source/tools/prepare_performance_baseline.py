"""Build the unchanged HEAD plugin service with the same performance harness.

Only a generated tree under target/ is modified; Git history and the checkout
remain unchanged. Run from the host source directory with Cargo on PATH.
"""
import argparse
import json
from pathlib import Path
import subprocess
import sys
import tomllib
import uuid
import zipfile

ROOT = Path(__file__).resolve().parents[1]


def build(source, release=False):
    command = ["cargo", "test", "--locked", "-p", "bongocat-plugin-service", "--test", "performance", "--no-run", "--message-format=json", "--target-dir", str(ROOT / "target")]
    if release:
        command.append("--release")
    result = subprocess.run(command, cwd=source, capture_output=True, text=True, encoding="utf-8", errors="replace")
    if result.returncode:
        print(result.stderr, file=sys.stderr)
        raise RuntimeError("performance harness build failed")
    executables = [entry["executable"] for line in result.stdout.splitlines() if (entry := json.loads(line)).get("reason") == "compiler-artifact" and entry.get("executable")]
    if len(executables) != 1:
        raise RuntimeError(f"expected one performance executable, found {executables}")
    return executables[0]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--current", action="store_true")
    parser.add_argument("--release", action="store_true", help="measure the optimized release profile")
    args = parser.parse_args()
    profile = "release" if args.release else "debug"
    if args.current:
        print(json.dumps({"source": str(ROOT), "profile": profile, "executable": build(ROOT, args.release)}))
        return
    destination = ROOT / "target" / "performance-baseline" / uuid.uuid4().hex
    destination.mkdir(parents=True)
    archive = destination / "source.zip"
    subtree = subprocess.check_output(["git", "rev-parse", "--show-prefix"], cwd=ROOT, text=True).strip().rstrip("/")
    git_root = Path(subprocess.check_output(["git", "rev-parse", "--show-toplevel"], cwd=ROOT).decode("utf-8").strip())
    subprocess.run(["git", "archive", f"HEAD:{subtree}", "-o", str(archive)], cwd=git_root, check=True)
    source = destination / "source"
    with zipfile.ZipFile(archive) as package:
        package.extractall(source)
    sdk = destination / "sdk"
    sdk_archive = destination / "sdk.zip"
    subprocess.run(["git", "archive", "HEAD:bongocat_plugins/crates/bongocat-plugin-sdk", "-o", str(sdk_archive)], cwd=git_root, check=True)
    with zipfile.ZipFile(sdk_archive) as package:
        package.extractall(sdk)
    plugin_workspace = tomllib.loads(subprocess.check_output(["git", "show", "HEAD:bongocat_plugins/Cargo.toml"], cwd=git_root).decode("utf-8"))
    serde = plugin_workspace["workspace"]["dependencies"]["serde"]["version"]
    serde_json = plugin_workspace["workspace"]["dependencies"]["serde_json"]
    sdk_manifest = sdk / "Cargo.toml"
    sdk_manifest.write_text(sdk_manifest.read_text(encoding="utf-8") + f'\n[workspace]\n[workspace.dependencies]\nserde = {{ version = "{serde}", features = ["derive"] }}\nserde_json = "{serde_json}"\n', encoding="utf-8")
    manifest = source / "Cargo.toml"
    text = manifest.read_text(encoding="utf-8")
    old = 'bongocat-plugin-sdk = { path = "../../bongocat_plugins/crates/bongocat-plugin-sdk" }'
    if old not in text:
        raise RuntimeError("baseline SDK declaration changed; inspect it before benchmarking")
    manifest.write_text(text.replace(old, f'bongocat-plugin-sdk = {{ path = "{sdk.as_posix()}" }}'), encoding="utf-8")
    test = source / "crates/bongocat-plugin-service/tests/performance.rs"
    test.write_bytes((ROOT / "crates/bongocat-plugin-service/tests/performance.rs").read_bytes())
    print(json.dumps({"baseline_commit": subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=ROOT, text=True).strip(), "source": str(source), "profile": profile, "executable": build(source, args.release)}))


if __name__ == "__main__":
    main()
