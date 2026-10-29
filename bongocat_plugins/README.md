# BongoCat companion plugins

Two independent Tauri 2 companion plugins for the patched BongoCat desktop pet:

- `com.bongocat.todo` — hand-drawn todo panel, quick add and due bubbles. The panel and quick-add window support title-bar zoom and corner drag-resize, with the scale persisted.
- `com.bongocat.mail` — hand-drawn IMAP notification center, archive and settings. All panels drag by their title bar and the settings form scrolls inside its window.

The plugins are separate native processes. They speak the versioned
`bongocat-plugin-sdk` JSON protocol on stdin/stdout and own their Tauri windows.
The BongoCat host only supplies plugin lifecycle, a companion menu, pet
coordinates, local ZIP installation and storage paths.

## Build on Windows

Requirements: Rust 1.97.1, Node 20+/pnpm, Visual Studio Build Tools with the
Windows SDK, and the WebView2 runtime (present on normal Windows 10/11 installs).

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
cargo build -p bongocat-todo -p bongocat-mail
```

The host passes `BONGOCAT_PLUGIN_DATA_DIR` when it starts a plugin. JSON state and
the two UI windows live there; mail passwords are stored in the OS keyring.

## Licensing

The hand-drawn assets, CSS and copy are ported from
[ChHsiching/bongocat-todo](https://github.com/ChHsiching/bongocat-todo) under the
MIT license. The handwriting font is distributed under the SIL Open Font
License 1.1. See `LICENSE-MIT.txt` and `OFL.txt`.
