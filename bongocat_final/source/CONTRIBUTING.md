# Contributing to BongoCat

English | [简体中文](CONTRIBUTING.zh-CN.md)

Thank you for helping improve BongoCat. Please keep contributions focused, understandable, and
supported by repeatable verification.

## Before you start

- Search existing issues and pull requests before opening a new one.
- For a substantial behavior change, discuss the user need and intended result before implementation.
- Keep unrelated cleanup and formatting out of the same change.
- When changing a user- or contributor-facing document, update its English and Simplified Chinese
  counterpart in the same change.
- Keep public documentation focused on product behavior and workflows. Internal project labels,
  migration narratives, implementation-stack emphasis, and source filenames do not belong in public
  documentation unless they are essential to complete the task.

## Development setup

BongoCat is developed on Windows and macOS. Linux is not a release target and is not required for
routine validation.

Install `rustup` (the repository pins the required toolchain), `just`, and Python 3.11 or newer. Run all commands
from the repository root:

```text
just dev
just check
just build
just schema
```

`just dev`, `just dev-smoke`, and `just plugin-tests` prepare the official plugins. They use `BONGOCAT_PLUGINS_ROOT` when set, otherwise the local plugin workspace containing the SDK referenced by `Cargo.toml`. When the SDK is pinned to Git instead, the development tool can prepare the pinned checkout under `target/plugin-dev`.

For the local workspace, install Node.js and each plugin's frontend dependencies before building. The tool rebuilds the frontends and plugin binaries for the current host and creates a Development catalog. It does not install packages into the user's Production data. On Windows, development and test launches also isolate each companion's WebView2 cache beside its installed executable. An explicit plugin workspace can contain uncommitted changes:

```text
BONGOCAT_PLUGINS_ROOT=/path/to/plugin-workspace just plugin-tests
```

The local lifecycle test requires Todo, Mail, and Cyber Work and fails if any package is missing. Older Pomodoro-specific tests require a separate checkout containing that plugin; they are not covered by the local workspace test. SDK and protocol changes must be verified against both the host and the local plugin workspace. This workflow does not sign or publish plugins.

The SDK's [plugin author guide](../../bongocat_plugins/crates/bongocat-plugin-sdk/README.md)
documents negotiated capabilities, event acknowledgements, checkpoints and requested wakeups.
`just plugin-tests` also builds and runs the standalone SDK examples. Resource comparison and
long-run validation use `tools/prepare_performance_baseline.py` and
`tools/measure-plugin-performance.ps1`; record their scope and results under `docs/benchmark`.

When building on Windows, ensure you use the MSVC toolchain (of the form
`stable-x86_64-pc-windows-**msvc**`, not `stable-x86_64-pc-windows-**gnu**`). This project depends
on Live2D Cubism Core, whose Windows distribution is only available in MSVC format (`.lib`), so MSVC
is mandatory on Windows. Use `rustup show` to check the active toolchain. If it is not MSVC, switch
to it with `rustup override set`.

Building on macOS also needs an Xcode Swift toolchain, because the guided Input Monitoring flow
links a Swift static library (ADR-0078). SwiftPM refuses to load a package whose manifest declares a
newer tools version than the installed toolchain, and that package asks for 6.1. The Rust toolchain
file does not cover it, so keep Xcode up to date — the failure otherwise appears as a `swift build`
error inside `permission-flow`'s build script.

Use `just dev-smoke` to exercise the settings-window lifecycle. Run `just schema` after changing the
configuration or window-state types; it regenerates the checked-in JSON Schema contracts.
Platform-specific changes must also be verified on every affected operating system; a build or test
on another host is not a substitute for platform smoke coverage.

## Validation

Before opening a pull request:

- Run `just check` for code changes.
- Run focused tests while iterating, then use the full check before submission.
- Add or update automated tests for changed behavior whenever practical.
- Include screenshots for visible UI changes and note the window size, scale, theme, and platform
  used to capture them.
- Update both language versions when user-facing text changes.

Platform features must not be declared complete from compilation alone. Record any unrun platform
checks and the remaining risk in the pull request.

## Build and packaging

`just build` is the only supported build and packaging entry point. Do not add a parallel packaging
path to another tool, workflow, or script. Do not commit build artifacts, user data, local
environment files, or signing keys.

## Commits and pull requests

Use [Conventional Commits](https://www.conventionalcommits.org/) for commit messages, for example
`feat: add model import validation`.

A pull request should explain:

- the user-visible problem and intended behavior;
- the scope of the change;
- the validation commands and results;
- affected platforms, permissions, and any checks that could not be run;
- screenshots or recordings when behavior is visual.

If a change affects user-visible behavior, defaults, compatibility, supported platforms, or upgrade
guidance, update both `CHANGELOG.md` and `CHANGELOG.zh-CN.md` unless maintainers decide otherwise.

## Privacy and security

Never include real user paths, key sequences, clipboard contents, private model data, credentials, or
signing keys in issues, pull requests, logs, screenshots, or test fixtures.

## Earlier versions

The previous implementation remains available on the protected
[`pre-refactor-tauri`](https://github.com/ayangweb/BongoCat/tree/pre-refactor-tauri) branch. Use it
only as a historical reference; do not modify or reintroduce that implementation.
