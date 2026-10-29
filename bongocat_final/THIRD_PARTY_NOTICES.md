# Third-party notices

This folder is a local build of BongoCat 2.5.0, based on upstream 2.4.0.

## BongoCat

- Upstream: https://github.com/ayangweb/BongoCat
- Local version: 2.5.0
- Upstream base: 2.4.0, commit `b82b4750b776df1408db07d5858d37db1594c3f3`
- License: Apache License 2.0; see `source/LICENSE`
- Local changes: Xbox/XInput fix, zero-scan-code input reset, and the companion plugin system

## Live2D Cubism Core

- Vendored under `source/vendor/cubism/5-r.5/`
- See `source/vendor/cubism/5-r.5/LICENSE.md` and `NOTICE.md`
- The Windows build links `Live2DCubismCore_MD.lib`; distribution is subject to the
  Live2D Cubism Core license terms.

## Microsoft Visual C++ runtime

The following files are copied from the Visual Studio Build Tools
`VC\Redist\MSVC\14.51.36231\x64\Microsoft.VC145.CRT` directory:

- `vcruntime140.dll`
- `vcruntime140_1.dll`

They are redistributed under the Microsoft Visual Studio license terms for
application-local deployment.

## Other Rust dependencies

The full dependency graph and versions are in `source/Cargo.lock`. License
metadata for the workspace is checked by `source/deny.toml` and the tools under
`source/tools/`.
