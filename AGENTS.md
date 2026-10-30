# AGENTS.md

The repository contains a locally built, patched BongoCat 2.5.0 development copy in `bongocat_final/` (with full source), a minimal runtime-only release in `version/BongoCat_2.5.0/`, and the archived 2.4.1 release in `version/BongoCat_2.4.1/`. The companion plugins live in `bongocat_plugins/` (todo, mail, cyberwork).

## Critical constraints

- `bongocat_final/bongocat-app.exe` is the local Xbox/XInput patched 2.5.0 development build. Its source is `bongocat_final/source/`; build and test it with Cargo 1.97.1 on Windows.
- `version/BongoCat_2.5.0/` is the current user-facing minimal release: only `BongoCat.exe`, trimmed runtime models, app-local VC++ runtime, and its instructions. Do not add source, build output, docs, licenses, or development files to it. `version/BongoCat_2.4.1/` is archived and must remain unchanged.
- `bongocat_final/source` depends on the plugin SDK at `../bongocat_plugins/crates/bongocat-plugin-sdk`; keep that sibling directory when building the host from source.
- The host is Linux; the Windows app can only be launched through Windows interop (for example WSL), not natively. Static verification remains valid for JSON, paths, and images.
- `bongocat_final/resources/build-provenance.json` is generated metadata for upstream source commit `b82b4750b776df1408db07d5858d37db1594c3f3` / `x86_64-pc-windows-msvc`. Never hand-edit or reformat it; a line-ending change rewrites the whole file.
- No git remote is configured. Source for the patched build lives under `bongocat_final/source/`; do not reverse-engineer a release binary when a source change is needed.
- Large binaries are committed intentionally (no Git LFS). Do not "clean up" or regenerate them without an explicit request.

## Model assets

- `bongocat_final/resources/models/` and `version/BongoCat_2.5.0/resources/models/` are Live2D Cubism 3 presets. Each package must have a `*.model3.json` at its root; the app's preset root is `resources/models` next to the executable.
- The patched development build reads `bongocat_final/resources/models/`; the release reads `version/BongoCat_2.5.0/resources/models/`. Fixing key images there is what changes the running executables. `bongocat_final/source/resources/models/` is the source/test copy used when rebuilding.
- `cat.model3.json` declares every `Moc`, texture, expression, motion, and sound. When adding or removing assets, update the manifest and ensure every relative path resolves from the preset root.
- Key overlays are `resources/left-keys/*.png` and `resources/right-keys/*.png`, named by input code (`KeyA`, `ControlLeft`, `DpadUp`, `East`). Keep filenames exact; they act as runtime lookup keys.
- `.moc3`, `.cdi3.json`, textures, and Live2D motion/expression files are exported Cubism artifacts; no Cubism tooling exists in this repo. Limit edits to files you can verify (images, JSON, audio) and never hand-edit binaries.

## Verification

- Run `python3 -m json.tool <file>` on changed JSON, check that all paths in `cat.model3.json` exist, and use `file` to validate edited images. There is no project test command.

## Workspace hygiene

- Temporary files go to `$env:TEMP` (session-private) or the repo-root `.dsh-scratch/`; delete the latter before the final answer.
- Build caches live in `bongocat_final/source/target` and `bongocat_plugins/target`; report their sizes and never create a second copy.
- `diagnose-windows-sandbox-acl` writes `../dsh-acl-recovery/` outside the repo, so the turn change card cannot show it. After a verified repair, delete it or list it (with its rollback command) in the final answer.
- Finish with `git status --porcelain -uall`: only intended changes; account for every untracked path.
- Keep `version/BongoCat_2.5.0/` free of development or temporary files.
