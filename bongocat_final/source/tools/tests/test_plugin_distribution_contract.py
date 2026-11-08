"""Check the local SDK dependency and plugin distribution boundaries."""

import base64
import re
import tomllib
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[2]
WORKSPACE = ROOT / "Cargo.toml"
RELEASE_IDENTITY = (
    ROOT / "crates" / "bongocat-update" / "src" / "runtime" / "release_identity.rs"
)
RUNTIME_MOD = ROOT / "crates" / "bongocat-update" / "src" / "runtime" / "mod.rs"
APP_SOURCE = ROOT / "crates" / "bongocat-app" / "src" / "dev_plugins.rs"
DEV_PLUGINS = ROOT / "tools" / "dev_plugins.py"
WORKFLOWS = ROOT / ".github" / "workflows"
JUSTFILE = ROOT / "justfile"


def read(path):
    return path.read_text(encoding="utf-8")


def rust_constant(name):
    source = read(RELEASE_IDENTITY)
    match = re.search(
        rf'pub const {re.escape(name)}: &str =\s*"(.*?)";',
        source,
        re.DOTALL,
    )
    if match is None:
        raise AssertionError(f"{name} is not declared as a string constant")
    return "".join(match.group(1).split())


def application_signing_key():
    source = read(RUNTIME_MOD)
    match = re.search(
        r'pub const RELEASE_SIGNING_KEY: Option<&str> = Some\(\s*"(.*?)"\s*\);',
        source,
        re.DOTALL,
    )
    if match is None:
        raise AssertionError("RELEASE_SIGNING_KEY is not declared")
    return "".join(match.group(1).split())


class PluginDistributionContractTests(unittest.TestCase):
    def test_the_sdk_resolves_to_the_local_plugin_workspace(self):
        manifest = tomllib.loads(read(WORKSPACE))
        sdk = manifest["workspace"]["dependencies"]["bongocat-plugin-sdk"]
        self.assertEqual(sdk, {"path": "../../bongocat_plugins/crates/bongocat-plugin-sdk"})
        self.assertTrue((ROOT / sdk["path"] / "Cargo.toml").is_file())

    def test_no_official_plugin_source_or_release_pipeline_is_copied_back(self):
        self.assertFalse((ROOT / "plugins").exists())
        self.assertFalse((ROOT / "crates" / "bongocat-plugin-sdk").exists())
        for workflow in WORKFLOWS.glob("*.y*ml"):
            with self.subTest(workflow=workflow.name):
                self.assertNotIn("PLUGIN_SIGNING_PRIVATE_KEY", read(workflow))
                self.assertNotIn("PLUGIN_SIGNING_KEY_PASSWORD", read(workflow))
        justfile = read(JUSTFILE)
        for removed in (
            "dev-plugins",
            "release-plugins",
            "plugin-catalog",
            "release-plugin-catalog",
        ):
            self.assertNotIn(removed, justfile)

    def test_plugin_repository_identity_is_separate_from_the_application_release(self):
        owner = rust_constant("PLUGIN_REPOSITORY_OWNER")
        repository = rust_constant("PLUGIN_REPOSITORY_NAME")
        self.assertEqual((owner, repository), ("ayangweb", "BongoCat-Plugins"))
        self.assertNotEqual(
            (owner, repository),
            (
                rust_constant("RELEASE_REPOSITORY_OWNER"),
                rust_constant("RELEASE_REPOSITORY_NAME"),
            ),
        )

        public_key = base64.b64decode(rust_constant("PLUGIN_RELEASE_SIGNING_KEY"))
        self.assertTrue(
            public_key.startswith(b"untrusted comment: minisign public key: "),
            "the embedded value must be the base64-encoded Minisign public-key box",
        )
        self.assertNotEqual(
            rust_constant("PLUGIN_RELEASE_SIGNING_KEY"),
            application_signing_key(),
        )

    def test_production_and_development_sources_are_environment_isolated(self):
        source = read(APP_SOURCE)
        self.assertIn("PLUGIN_REPOSITORY_OWNER", source)
        self.assertIn("PLUGIN_REPOSITORY_NAME", source)
        self.assertIn("PLUGIN_RELEASE_SIGNING_KEY", source)
        self.assertIn("BONGOCAT_PLUGIN_DEV_DIRECTORY", source)
        self.assertLess(
            source.index("BuildEnvironment::Development"),
            source.index("BuildEnvironment::Production"),
        )

    def test_development_entry_points_prepare_a_local_catalog(self):
        justfile = read(JUSTFILE)
        self.assertIn(
            "{{python}}tools/dev_plugins.py--cargorun--locked-pbongocat-app--release",
            "".join(justfile.split()),
        )
        script = read(DEV_PLUGINS)
        self.assertIn("BONGOCAT_PLUGINS_ROOT", script)
        self.assertIn("BONGOCAT_PLUGIN_DEV_DIRECTORY", script)
        self.assertIn("BONGOCAT_PLUGIN_SMOKE_REQUIRE_CARDS", script)
        self.assertIn("catalog", script)
        self.assertIn("dev", script)
        self.assertIn("clone", script)
        self.assertIn("sdk_revision", script)
        self.assertIn("checkout_pinned_revision", script)
        self.assertIn("BONGOCAT_PLUGINS_ROOT", script)
        self.assertNotIn("/Users/", script)


if __name__ == "__main__":
    unittest.main()
