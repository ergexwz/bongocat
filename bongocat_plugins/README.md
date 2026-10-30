# BongoCat companion plugins

Three independent Tauri 2 companion plugins for the patched BongoCat desktop pet:

- `com.bongocat.todo` — hand-drawn todo panel, quick add and due bubbles. The panel and quick-add window support title-bar zoom and corner drag-resize, with the scale persisted.
- `com.bongocat.mail` — hand-drawn IMAP notification center, archive and settings. All panels drag by their title bar; the mail list, archive and settings windows support title-bar zoom and corner drag-resize with a fixed aspect ratio, and the per-window scale is persisted.
- `com.bongocat.cyberwork` — “赛博打工”: a hover-revealed tap counter and gamepad time tracker above the cat. The context menu under the host's plugin submenu offers the eight counter scales and the usage-history panel. Daily history is kept for 400 days; the counter and history are persisted in the plugin data directory.

The plugins are separate native processes. They speak the versioned
`bongocat-plugin-sdk` JSON protocol on stdin/stdout and own their Tauri windows.
The BongoCat host only supplies plugin lifecycle, a companion menu, pet
coordinates, local ZIP installation and storage paths.

Cyber Work needs the host build in this repository: it declares
`pet_state_v1` and `plugin_menu_tree_v1`, which the earlier 2.5.0 executable did
not expose. It shows the counter only for the built-in `standard` and `gamepad`
presets; the built-in `keyboard` preset and imported models hide it. The plugin's
data is `<plugin data dir>/com.bongocat.cyberwork/cyberwork.json`; the historical
daily buckets are local calendar dates and the all-time totals are the values
shown on the counter.

## Build on Windows

Requirements: Rust 1.97.1, Node 20+/pnpm, Visual Studio Build Tools with the
Windows SDK, and the WebView2 runtime (present on normal Windows 10/11 installs).
The frontend lockfile and `pnpm.onlyBuiltDependencies` build allowlist are
verified with pnpm 10.x; newer pnpm major versions may require migrating that
allowlist to `pnpm-workspace.yaml`.

```powershell
cd bongocat_plugins
.\scripts\package.ps1
```

The ZIPs are written to `dist/`; prebuilt Windows packages are committed there for
direct installation. Install them from BongoCat's Plugin Center with **Install from file**.

For a fixed version:

```powershell
.\scripts\package.ps1 -Version 1.0.0
```

Release builds must enable `--features custom-protocol` so Tauri embeds
`ui/dist` instead of expecting a dev server. The packaging script does this.

## Development

Build the frontends first; Tauri embeds `ui/dist` at compile time.

```powershell
pnpm --dir plugins/todo/ui install
pnpm --dir plugins/todo/ui build
pnpm --dir plugins/mail/ui install
pnpm --dir plugins/mail/ui build
pnpm --dir plugins/cyberwork/ui install
pnpm --dir plugins/cyberwork/ui build
cargo build -p bongocat-todo -p bongocat-mail -p bongocat-cyberwork
```

The host passes `BONGOCAT_PLUGIN_DATA_DIR` when it starts a plugin. JSON state and
per-plugin data live there; the plugin's Tauri windows are created by its process.
Mail passwords are stored in the OS keyring. Cyber Work stores its counter
scale, all-time totals and daily history in `cyberwork.json`; it never stores
key identities, text or pointer positions.

## Licensing

The hand-drawn assets, CSS and copy are ported from
[ChHsiching/bongocat-todo](https://github.com/ChHsiching/bongocat-todo) under the
MIT license. The handwriting font is distributed under the SIL Open Font
License 1.1. See `LICENSE-MIT.txt` and `OFL.txt`. The counter background in
`plugins/cyberwork/ui/public/counter/counter_background.png` was supplied by the
user from the local BongoCat 2.4.1 installation; it is not covered by the font
license.
