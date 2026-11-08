"""The bundled manifest must use exactly the requested executable build."""
import hashlib
import importlib.util
import json
from pathlib import Path
import tempfile
import unittest
import zipfile

ROOT = Path(__file__).resolve().parents[2]
SPEC = importlib.util.spec_from_file_location("prepare_local_companions", ROOT / "tools" / "prepare-local-companions.py")
PREPARE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(PREPARE)


class BundlePreparationTests(unittest.TestCase):
    def test_explicit_binary_directory_and_catalog_hashes(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            selected = root / "target" / "x86_64-pc-windows-msvc" / "release"
            stale = root / "target" / "release"
            selected.mkdir(parents=True)
            stale.mkdir(parents=True)
            for license_file in ("LICENSE-MIT.txt", "OFL.txt"):
                (root / license_file).write_text("license")
            for name in ("todo", "mail", "cyberwork"):
                plugin = root / "plugins" / name
                (plugin / "resources").mkdir(parents=True)
                (plugin / "resources" / "icon.png").write_bytes(b"icon")
                metadata = {"id": f"com.example.{name}", "name": name, "description": "", "version": "0.2.0", "app_versions": ">=2.5.3", "protocol_version": 2, "entry": f"plugin-{name}", "icon": {"type": "local_image", "value": "resources/icon.png"}, "localizations": {}}
                (plugin / "plugin.json").write_text(json.dumps({"metadata": metadata}), encoding="utf-8")
                (selected / f"plugin-{name}.exe").write_bytes(b"selected-v2")
                (stale / f"plugin-{name}.exe").write_bytes(b"stale-v1")
            output = root / "candidate"
            PREPARE.prepare(root, output, selected, bundled=True)
            catalog = json.loads((output / "plugins.json").read_text(encoding="utf-8"))
            self.assertEqual(len(catalog["plugins"]), 3)
            for entry in catalog["plugins"]:
                archive = output / entry["downloads"]["windows-x86_64"]
                self.assertEqual(entry["sha256"], hashlib.sha256(archive.read_bytes()).hexdigest())
                with zipfile.ZipFile(archive) as package:
                    exe = next(name for name in package.namelist() if name.endswith(".exe"))
                    self.assertEqual(package.read(exe), b"selected-v2")
            previous = {path.name: path.read_bytes() for path in output.iterdir() if path.is_file()}
            (selected / "plugin-todo.exe").write_bytes(b"different-same-version")
            with self.assertRaisesRegex(ValueError, "immutable"):
                PREPARE.prepare(root, output, selected, bundled=True)
            self.assertEqual(previous, {path.name: path.read_bytes() for path in output.iterdir() if path.is_file()})

    def test_missing_binary_preserves_existing_catalog(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            plugin = root / "plugins" / "todo"
            plugin.mkdir(parents=True)
            (plugin / "plugin.json").write_text(json.dumps({"metadata": {"entry": "missing"}}))
            output = root / "output"
            output.mkdir()
            (output / "plugins.json").write_bytes(b"existing catalog")
            with self.assertRaises(ValueError):
                PREPARE.prepare(root, output, root / "missing-build", bundled=True)
            self.assertEqual((output / "plugins.json").read_bytes(), b"existing catalog")

    def test_entry_path_cannot_escape_the_selected_binary_directory(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            plugin = root / "plugins" / "todo"
            plugin.mkdir(parents=True)
            (plugin / "plugin.json").write_text(json.dumps({"metadata": {"entry": "../unrelated"}}))
            with self.assertRaisesRegex(ValueError, "safe executable name"):
                PREPARE.prepare(root, root / "output", root / "build", bundled=True)
            self.assertFalse((root / "output").exists())


if __name__ == "__main__":
    unittest.main()
