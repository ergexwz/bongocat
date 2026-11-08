"""Regression tests for the local plugin checkout orchestration."""

import importlib.util
import json
import tempfile
import unittest
import zipfile
from pathlib import Path
from unittest import mock


ROOT = Path(__file__).resolve().parents[2]
SCRIPT = ROOT / "tools" / "dev_plugins.py"
SPEC = importlib.util.spec_from_file_location("dev_plugins", SCRIPT)
DEV = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(DEV)


class DevPluginsTests(unittest.TestCase):
    def test_windows_entry_point_isolates_the_companion_browser_profile(self):
        with mock.patch.object(DEV.sys, "argv", ["dev_plugins.py", "--", "echo", "ready"]), mock.patch.object(
            DEV, "prepare_catalog", return_value=(ROOT, "x86_64-pc-windows-msvc", ROOT / "target/catalog")
        ), mock.patch.dict(DEV.os.environ, {"WEBVIEW2_USER_DATA_FOLDER": "production-profile"}), mock.patch.object(
            DEV.subprocess, "run", return_value=__import__("subprocess").CompletedProcess([], 0)
        ) as run:
            self.assertEqual(DEV.main(), 0)
            self.assertEqual(run.call_args.kwargs["env"]["WEBVIEW2_USER_DATA_FOLDER"], ".bongocat-development-webview2")

    def test_local_sdk_path_selects_its_workspace_without_git(self):
        with tempfile.TemporaryDirectory() as directory:
            base = Path(directory)
            source = base / "source"
            source.mkdir()
            workspace = base / "plugins-workspace"
            (workspace / "crates" / "bongocat-plugin-sdk").mkdir(parents=True)
            (workspace / "plugins").mkdir()
            (workspace / "Cargo.toml").write_text("[workspace]\n")
            manifest = source / "Cargo.toml"
            manifest.write_text('[workspace.dependencies]\nbongocat-plugin-sdk = { path = "../plugins-workspace/crates/bongocat-plugin-sdk" }\n')
            with mock.patch.object(DEV, "WORKSPACE_MANIFEST", manifest), mock.patch.dict(DEV.os.environ, {}, clear=True):
                self.assertEqual(DEV.resolve_plugins_root(), workspace.resolve())

    def test_local_catalog_packages_all_built_plugins(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            output = root / "output"
            output.mkdir()
            triple = "x86_64-pc-windows-msvc"
            target = root / "target" / triple / "release"
            target.mkdir(parents=True)
            for name in ("todo", "mail", "cyberwork"):
                plugin = root / "plugins" / name
                vite = plugin / "ui" / "node_modules" / "vite" / "bin" / "vite.js"
                vite.parent.mkdir(parents=True)
                vite.write_text("")
                (plugin / "resources").mkdir()
                (plugin / "resources" / "icon.png").write_bytes(b"icon")
                metadata = {"id": f"com.bongocat.{name}", "name": name, "description": "", "version": "0.1.0", "app_versions": ">=2.5.0", "protocol_version": 1, "entry": f"bongocat-{name}", "icon": {"type": "local_image", "value": "resources/icon.png"}, "localizations": {}}
                (plugin / "plugin.json").write_text(json.dumps({"metadata": metadata}))
                (target / f"bongocat-{name}.exe").write_bytes(b"binary")
            for name in ("LICENSE-MIT.txt", "OFL.txt"):
                (root / name).write_text("license")
            with mock.patch.object(DEV.subprocess, "run") as run:
                DEV.build_local_catalog(root, triple, output)
            self.assertEqual(run.call_count, 5)
            entries = json.loads((output / "catalog.json").read_text())["plugins"]
            self.assertEqual(len(entries), 3)
            for entry in entries:
                with zipfile.ZipFile(output / entry["downloads"]["windows-x86_64"]) as archive:
                    self.assertIn("plugin.json", archive.namelist())
                    self.assertIn("resources/icon.png", archive.namelist())

    def test_host_triples_are_derived_from_os_and_arch(self):
        self.assertEqual(DEV.host_triple_for("darwin", "arm64"), "aarch64-apple-darwin")
        self.assertEqual(DEV.host_triple_for("darwin", "x86_64"), "x86_64-apple-darwin")
        self.assertEqual(
            DEV.host_triple_for("win32", "AMD64"), "x86_64-pc-windows-msvc"
        )
        with self.assertRaises(DEV.DevPluginError):
            DEV.host_triple_for("linux", "x86_64")

    def test_pruning_removes_only_unreferenced_development_artifacts(self):
        with tempfile.TemporaryDirectory() as directory:
            output = Path(directory)
            (output / "kept.zip").write_bytes(b"zip")
            (output / "kept.png").write_bytes(b"png")
            (output / "stale.zip").write_bytes(b"old")
            (output / "stale.png").write_bytes(b"old")
            (output / "unrelated.txt").write_text("keep")
            catalog = output / "catalog.json"
            catalog.write_text(
                json.dumps(
                    {
                        "plugins": [
                            {
                                "downloads": {"macos-aarch64": "kept.zip"},
                                "icon_url": str(output / "kept.png"),
                            }
                        ]
                    }
                ),
                encoding="utf-8",
            )
            DEV.prune_catalog_output(output, catalog)
            self.assertTrue((output / "kept.zip").is_file())
            self.assertTrue((output / "kept.png").is_file())
            self.assertTrue((output / "unrelated.txt").is_file())
            self.assertFalse((output / "stale.zip").exists())
            self.assertFalse((output / "stale.png").exists())

    def test_failed_clone_does_not_leave_a_half_checkout(self):
        with tempfile.TemporaryDirectory() as directory:
            checkout = Path(directory) / "BongoCat-Plugins"
            checkout.mkdir()
            (checkout / "partial").write_text("partial")
            with mock.patch.object(DEV, "DEV_ROOT", Path(directory)), mock.patch.object(
                DEV, "DEFAULT_CHECKOUT", checkout
            ), mock.patch.object(DEV.subprocess, "run", side_effect=__import__("subprocess").CalledProcessError(1, "git")):
                with self.assertRaises(DEV.DevPluginError):
                    DEV.checkout_pinned_revision("0" * 40, clone=True)
            self.assertFalse(checkout.exists())


if __name__ == "__main__":
    unittest.main()
