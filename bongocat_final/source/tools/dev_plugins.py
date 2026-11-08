#!/usr/bin/env python3
"""Prepare a local plugin catalog, then execute the requested development command."""

import argparse
import hashlib
import json
import os
import re
import shutil
import subprocess
import sys
import time
import tomllib
import zipfile
from pathlib import Path


PLUGIN_REPOSITORY = "https://github.com/ayangweb/BongoCat-Plugins.git"
ROOT = Path(__file__).resolve().parents[1]
WORKSPACE_MANIFEST = ROOT / "Cargo.toml"
DEV_ROOT = ROOT / "target" / "plugin-dev"
DEFAULT_CHECKOUT = DEV_ROOT / "BongoCat-Plugins"
CATALOG_DIRECTORY = DEV_ROOT / "plugins"
HOST_TRIPLES = {
    ("darwin", "arm64"): "aarch64-apple-darwin",
    ("darwin", "x86_64"): "x86_64-apple-darwin",
    ("win32", "x86_64"): "x86_64-pc-windows-msvc",
}
PLATFORM_KEYS = {
    "aarch64-apple-darwin": "macos-aarch64",
    "x86_64-apple-darwin": "macos-x86_64",
    "x86_64-pc-windows-msvc": "windows-x86_64",
}


class DevPluginError(RuntimeError):
    pass


def sdk_revision():
    match = re.search(
        r'(?m)^bongocat-plugin-sdk\s*=\s*\{[^}]*\brev\s*=\s*"([0-9a-f]{40})"',
        WORKSPACE_MANIFEST.read_text(encoding="utf-8"),
    )
    if match is None:
        raise DevPluginError(
            "Cargo.toml must pin bongocat-plugin-sdk to a full Git revision"
        )
    return match.group(1)


def host_triple():
    platform = sys.platform
    machine = os.uname().machine if hasattr(os, "uname") else os.environ.get("PROCESSOR_ARCHITECTURE")
    return host_triple_for(platform, machine)


def host_triple_for(platform, machine):
    normalized = (machine or "").lower()
    if normalized == "amd64":
        normalized = "x86_64"
    triple = HOST_TRIPLES.get((platform, normalized))
    if triple is None:
        raise DevPluginError(
            f"unsupported host {platform}-{normalized or 'unknown'}; "
            "BongoCat plugin development supports Windows x64 and macOS arm64/x64"
        )
    return triple


def git(checkout, *arguments):
    try:
        return subprocess.run(
            ["git", "-C", str(checkout), *arguments],
            check=True,
            capture_output=True,
            text=True,
        ).stdout.strip()
    except FileNotFoundError as error:
        raise DevPluginError("Git is required to prepare the default plugin checkout") from error
    except subprocess.CalledProcessError as error:
        detail = error.stderr.strip() or error.stdout.strip()
        raise DevPluginError(
            f"cannot prepare the default plugin checkout: {detail}"
        ) from error


def checkout_pinned_revision(revision, clone=False):
    if clone:
        DEV_ROOT.mkdir(parents=True, exist_ok=True)
        print(f"Cloning BongoCat-Plugins into {DEFAULT_CHECKOUT}", file=sys.stderr)
        try:
            subprocess.run(
                ["git", "clone", PLUGIN_REPOSITORY, str(DEFAULT_CHECKOUT)],
                cwd=ROOT,
                check=True,
            )
        except FileNotFoundError as error:
            raise DevPluginError("Git is required to prepare the default plugin checkout") from error
        except subprocess.CalledProcessError as error:
            if DEFAULT_CHECKOUT.exists():
                shutil.rmtree(DEFAULT_CHECKOUT)
            raise DevPluginError(
                f"cannot clone {PLUGIN_REPOSITORY}; set BONGOCAT_PLUGINS_ROOT "
                "to an existing checkout"
            ) from error

    if git(DEFAULT_CHECKOUT, "status", "--porcelain"):
        raise DevPluginError(
            f"{DEFAULT_CHECKOUT} has local changes; set BONGOCAT_PLUGINS_ROOT "
            "to use that checkout for plugin development"
        )
    try:
        if git(DEFAULT_CHECKOUT, "rev-parse", "HEAD") != revision:
            git(DEFAULT_CHECKOUT, "fetch", "--force", "origin", revision)
            git(DEFAULT_CHECKOUT, "checkout", "--detach", revision)
    except DevPluginError as error:
        raise DevPluginError(
            f"cannot check out the SDK revision {revision}; set "
            "BONGOCAT_PLUGINS_ROOT to an existing checkout"
        ) from error
    return DEFAULT_CHECKOUT


def resolve_plugins_root():
    configured = os.environ.get("BONGOCAT_PLUGINS_ROOT")
    if configured:
        root = Path(configured).expanduser().resolve()
        if not (root / "Cargo.toml").is_file() or not (root / "plugins").is_dir():
            raise DevPluginError(
                f"BONGOCAT_PLUGINS_ROOT does not name a BongoCat-Plugins checkout: {root}"
            )
        return root

    workspace = tomllib.loads(WORKSPACE_MANIFEST.read_text(encoding="utf-8"))
    sdk = workspace["workspace"]["dependencies"]["bongocat-plugin-sdk"]
    if "path" in sdk:
        sdk_path = (WORKSPACE_MANIFEST.parent / sdk["path"]).resolve()
        root = sdk_path.parent.parent
        if not (root / "Cargo.toml").is_file() or not (root / "plugins").is_dir():
            raise DevPluginError(f"local plugin SDK has no plugin workspace: {root}")
        return root

    revision = sdk_revision()
    if (DEFAULT_CHECKOUT / "Cargo.toml").is_file():
        return checkout_pinned_revision(revision)
    if DEFAULT_CHECKOUT.exists():
        raise DevPluginError(
            f"{DEFAULT_CHECKOUT} exists but is not a BongoCat-Plugins checkout; "
            "remove it or set BONGOCAT_PLUGINS_ROOT"
        )

    return checkout_pinned_revision(revision, clone=True)


def prepare_catalog():
    plugins_root = resolve_plugins_root()
    triple = host_triple()
    CATALOG_DIRECTORY.mkdir(parents=True, exist_ok=True)
    if not (plugins_root / "crates" / "bongocat-plugin-packaging" / "Cargo.toml").is_file():
        build_local_catalog(plugins_root, triple, CATALOG_DIRECTORY)
        catalog = CATALOG_DIRECTORY / "catalog.json"
        prune_catalog_output(CATALOG_DIRECTORY, catalog)
        return plugins_root, triple, CATALOG_DIRECTORY
    packaging = ["cargo", "run", "--locked", "-q", "-p", "bongocat-plugin-packaging", "--"]
    catalog_source = plugins_root / "crates" / "bongocat-plugin-packaging" / "src" / "catalog.rs"
    supports_catalog_dev = (
        catalog_source.is_file()
        and '"dev" => dev_command' in catalog_source.read_text(encoding="utf-8")
    )
    command = packaging + (
        [
            "catalog",
            "dev",
            "--root",
            ".",
            "--target",
            triple,
            "--output",
            str(CATALOG_DIRECTORY),
        ]
        if supports_catalog_dev
        else [
            "build",
            "--root",
            ".",
            "--target",
            triple,
            "--output",
            str(CATALOG_DIRECTORY),
        ]
    )
    try:
        subprocess.run(command, cwd=plugins_root, check=True)
    except FileNotFoundError as error:
        raise DevPluginError("Cargo is required to build the official plugins") from error
    except subprocess.CalledProcessError as error:
        raise DevPluginError(
            f"cannot build official plugins for {triple} from {plugins_root}"
        ) from error

    if not supports_catalog_dev:
        write_development_catalog(plugins_root, triple, CATALOG_DIRECTORY)
    catalog = CATALOG_DIRECTORY / "catalog.json"
    if not catalog.is_file():
        raise DevPluginError(f"plugin build did not produce {catalog}")
    prune_catalog_output(CATALOG_DIRECTORY, catalog)
    return plugins_root, triple, CATALOG_DIRECTORY


def build_local_catalog(plugins_root, triple, output):
    """Build the local companion workspace and stage development-only packages."""
    plugins = ("todo", "mail", "cyberwork")
    try:
        for plugin in plugins:
            frontend = plugins_root / "plugins" / plugin / "ui"
            vite = frontend / "node_modules" / "vite" / "bin" / "vite.js"
            if not vite.is_file():
                raise DevPluginError(f"frontend dependencies are missing for {plugin}: {vite}")
            subprocess.run(
                ["node", str(vite), "build"], cwd=frontend, check=True,
            )
        subprocess.run(
            ["cargo", "build", "--locked", "--release", "--target", triple,
             "-p", "bongocat-todo", "-p", "bongocat-mail", "-p", "bongocat-cyberwork",
             "--features", "custom-protocol"],
            cwd=plugins_root, check=True,
        )
        subprocess.run(
            ["cargo", "build", "--locked", "--release", "--target", triple,
             "-p", "bongocat-plugin-sdk", "--examples"],
            cwd=plugins_root, check=True,
        )
    except FileNotFoundError as error:
        raise DevPluginError("Cargo and Node.js are required to build local plugins") from error
    except subprocess.CalledProcessError as error:
        raise DevPluginError(f"cannot build local plugins for {triple}") from error

    platform = PLATFORM_KEYS[triple]
    for plugin in plugins:
        directory = plugins_root / "plugins" / plugin
        manifest_path = directory / "plugin.json"
        manifest = json.loads(manifest_path.read_text(encoding="utf-8"))["metadata"]
        binary_name = manifest["entry"] + (".exe" if triple.startswith("x86_64-pc-windows") else "")
        binary = plugins_root / "target" / triple / "release" / binary_name
        destination = output / f"{manifest['id']}-{manifest['version']}-{platform}.zip"
        temporary = destination.with_suffix(".zip.tmp")
        with zipfile.ZipFile(temporary, "w", compression=zipfile.ZIP_DEFLATED) as archive:
            archive.write(manifest_path, "plugin.json")
            archive.write(binary, binary_name)
            archive.write(directory / manifest["icon"]["value"], manifest["icon"]["value"])
            for license_name in ("LICENSE-MIT.txt", "OFL.txt"):
                archive.write(plugins_root / license_name, license_name)
        temporary.replace(destination)
    write_development_catalog(plugins_root, triple, output)


def prune_catalog_output(output, catalog_path):
    """Remove only artifacts no longer named by the newly written catalog."""
    catalog = json.loads(catalog_path.read_text(encoding="utf-8"))
    referenced = {"catalog.json"}
    for entry in catalog["plugins"]:
        referenced.update(entry["downloads"].values())
        icon = entry.get("icon_url")
        if icon:
            referenced.add(Path(icon).name)
    for stale in output.iterdir():
        if (
            stale.is_file()
            and stale.suffix in {".zip", ".png"}
            and stale.name not in referenced
        ):
            stale.unlink()


def write_development_catalog(plugins_root, triple, output):
    """Project an older packaging checkout's ZIPs into the Development catalog."""
    platform = PLATFORM_KEYS[triple]
    entries = []
    for manifest_path in sorted((plugins_root / "plugins").glob("*/plugin.json")):
        manifest = json.loads(manifest_path.read_text(encoding="utf-8"))["metadata"]
        package = output / f"{manifest['id']}-{manifest['version']}-{platform}.zip"
        if not package.is_file():
            raise DevPluginError(
                f"plugin build is missing {package.name} for {triple}"
            )
        icon_relative = manifest["icon"]["value"]
        icon_bytes = (manifest_path.parent / icon_relative).read_bytes()
        icon_name = f"{manifest['id']}-{manifest['version']}.png"
        icon_path = output / icon_name
        icon_path.write_bytes(icon_bytes)
        entries.append(
            {
                "id": manifest["id"],
                "name": manifest["name"],
                "description": manifest["description"],
                "localizations": manifest["localizations"],
                "version": manifest["version"],
                "app_versions": manifest["app_versions"],
                "protocol_version": manifest["protocol_version"],
                "icon": manifest["icon"],
                "icon_url": str(icon_path.resolve()),
                "downloads": {platform: package.name},
                "sha256": hashlib.sha256(package.read_bytes()).hexdigest(),
                "homepage": None,
            }
        )
    if not entries:
        raise DevPluginError(f"no plugin manifests found under {plugins_root / 'plugins'}")
    catalog_path = output / "catalog.json"
    temporary = catalog_path.with_suffix(".json.tmp")
    temporary.write_text(
        json.dumps({"plugins": entries}, separators=(",", ":")),
        encoding="utf-8",
    )
    temporary.replace(catalog_path)


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("command", nargs=argparse.REMAINDER)
    arguments = parser.parse_args()
    command = arguments.command
    if command[:1] == ["--"]:
        command = command[1:]
    if not command:
        parser.error("a command to run after preparing plugins is required")

    try:
        plugins_root, triple, catalog_directory = prepare_catalog()
        environment = os.environ.copy()
        environment["BONGOCAT_PLUGIN_DEV_DIRECTORY"] = str(catalog_directory)
        environment["BONGOCAT_REQUIRE_LOCAL_PLUGIN_TESTS"] = "1"
        environment["BONGOCAT_V2_EXAMPLES"] = str(plugins_root / "target" / triple / "release" / "examples")
        if triple == "x86_64-pc-windows-msvc":
            # Each installed Development/test executable has its own directory.
            # Keep its browser cache separate from the production companion.
            environment["WEBVIEW2_USER_DATA_FOLDER"] = ".bongocat-development-webview2"
        environment.setdefault("BONGOCAT_PLUGIN_SMOKE_REQUIRE_CARDS", "1")
        print(
            f"Using BongoCat-Plugins at {plugins_root} ({triple})",
            file=sys.stderr,
        )
        started = time.monotonic()
        if command[:2] == ["cargo", "test"] and "--workspace" in command:
            fixture = subprocess.run(["cargo", "build", "--locked", "-p", "bongocat-plugin-manager", "--bin", "plugin-test-fixture"], cwd=ROOT, env=environment)
            if fixture.returncode:
                return fixture.returncode
            for package, test, selected in [
                ("bongocat-plugin-manager", "protocol_v2", None),
                ("bongocat-plugin-manager", "bundled_distribution", "development_catalog_runs_all_local_companions"),
                ("bongocat-plugin-service", "local_companions", "current_companions_run_through_the_async_service_and_resume"),
            ]:
                contract = ["cargo", "test", "--locked", "-p", package, "--test", test]
                if selected:
                    contract.append(selected)
                result = subprocess.run(contract + ["--", "--ignored"], cwd=ROOT, env=environment)
                if result.returncode:
                    return result.returncode
            result = subprocess.run(
                ["cargo", "build", "--locked", "--release", "-p", "bongocat-plugin-manager",
                 "--bin", "plugin-test-fixture"], cwd=ROOT, env=environment,
            )
            if result.returncode:
                return result.returncode
            result = subprocess.run(
                ["cargo", "test", "--locked", "--release", "-p", "bongocat-plugin-service",
                 "--test", "service", "nine_healthy_plugins_keep_replying_when_one_plugin_times_out",
                 "--", "--ignored", "--test-threads=1"], cwd=ROOT, env=environment,
            )
            if result.returncode:
                return result.returncode
        result = subprocess.run(command, cwd=ROOT, env=environment)
        if result.returncode == 0 and "--settings-window-smoke" in command:
            if time.monotonic() - started < 1:
                print(
                    "plugin Development smoke exited before the settings window could render",
                    file=sys.stderr,
                )
                return 1
    except DevPluginError as error:
        print(f"plugin development setup failed: {error}", file=sys.stderr)
        return 1
    except FileNotFoundError as error:
        print(f"cannot execute plugin development command: {error}", file=sys.stderr)
        return 1
    return result.returncode


if __name__ == "__main__":
    raise SystemExit(main())
