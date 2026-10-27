# AGENTS.md

The repository root is a **packaged Windows release** of BongoCat v2.4.0. `bongocat_final/` additionally contains a locally built, patched runnable copy and its full source.

## Critical constraints

- Root `bongocat-app.exe` (x86_64 Windows GUI, Rust/GPUI) and `uninstall.exe` (NSIS) are the original release artifacts. Do not rebuild, replace, or delete them.
- `bongocat_final/bongocat-app.exe` is the local Xbox/XInput patched build. Its source is `bongocat_final/source/`; build and test it with Cargo 1.97.1 on Windows. The root release artifacts remain untouched.
- The host is Linux; the Windows app can only be launched through Windows interop (for example WSL), not natively. Static verification remains valid for JSON, paths, and images.
- Root `resources/build-provenance.json` is generated metadata for source commit `b82b4750b776df1408db07d5858d37db1594c3f3` / `x86_64-pc-windows-msvc`. Never hand-edit or reformat it; a line-ending change rewrites the whole file.
- No git remote is configured. Source for the patched build lives under `bongocat_final/source/`; do not reverse-engineer the root release binary when a source change is needed.
- Large binaries are committed intentionally (no Git LFS). Do not "clean up" or regenerate them without an explicit request.

## Model assets

- `resources/models/{standard,keyboard,gamepad}/` and `bongocat_final/resources/models/` are Live2D Cubism 3 presets. Each package must have a `*.model3.json` at its root; the app's preset root is `resources/models` next to the executable.
- The patched build reads `bongocat_final/resources/models/`. Fixing key images there is what changes the running `bongocat_final/bongocat-app.exe`; `bongocat_final/source/resources/models/` is the source/test copy used when rebuilding.
- `cat.model3.json` declares every `Moc`, texture, expression, motion, and sound. When adding or removing assets, update the manifest and ensure every relative path resolves from the preset root.
- Key overlays are `resources/left-keys/*.png` and `resources/right-keys/*.png`, named by input code (`KeyA`, `ControlLeft`, `DpadUp`, `East`). Keep filenames exact; they act as runtime lookup keys.
- `.moc3`, `.cdi3.json`, textures, and Live2D motion/expression files are exported Cubism artifacts; no Cubism tooling exists in this repo. Limit edits to files you can verify (images, JSON, audio) and never hand-edit binaries.

## Verification

- Run `python3 -m json.tool <file>` on changed JSON, check that all paths in `cat.model3.json` exist, and use `file` to validate edited images. There is no project test command.
