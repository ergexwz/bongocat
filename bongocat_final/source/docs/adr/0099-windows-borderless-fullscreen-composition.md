# ADR-0099: Windows Borderless Fullscreen Composition

Status: Accepted; composed fullscreen validated, D3D-fullscreen games documented as a game-side limitation.
Date: 2026-10-31

## Cause and Reference

A Windows user reported that the model window disappears while a game is
running. Two distinct fullscreen cases appeared during the investigation.

**Composed fullscreen.** The model HWND was `WS_EX_TOPMOST`, visible, not
cloaked, and enumerated above the game window in the desktop Z order. Hover hide
and idle hide were not active: the window had neither `WS_EX_LAYERED` nor
`WS_EX_TRANSPARENT`, which the presentation path sets whenever it fades the pet
for either hide state. A 2560x1440 physical screen capture showed the pet, while
the user saw it only "for an instant while taking a screenshot". The pet was a
`WS_EX_NOREDIRECTIONBITMAP` DirectComposition window; a composed fullscreen game
keeps DWM composition lazy, so the composition-only pet was not kept on screen.
Giving the pet a layered top-level window makes DWM keep composing it. The user
confirmed after the change that The Last of Us Part II in fullscreen shows the
pet.

**D3D fullscreen presentation.** Stardew Valley 1.6.15 with its in-game
"Windowed Borderless" option selected still hides the pet. While the game is
foreground, `SHQueryUserNotificationState` returns
`QUNS_RUNNING_D3D_FULL_SCREEN` (3), so the shell itself treats the game as a D3D
fullscreen application that owns the display. A normal non-activating topmost
test window was only visible after the game lost foreground and the state became
`QUNS_ACCEPTS_NOTIFICATIONS` (5); at that moment the pet became visible too.
This is the same class as exclusive fullscreen from the composition point of
view: no ordinary window, layered or not, can be composed over it. It is a
property of that game's presentation, not a remaining topmost or layering bug.

ADR-0096 covers the topmost-band ordering problem; this ADR covers the
composition layer that remains after a correct Z order.

## Decision

Create every pet-mode window as a layered top-level window by keeping
`WS_EX_LAYERED` set at all times, not only while click-through is enabled. Keep
the existing DirectComposition swap chain and visual, `WS_EX_NOREDIRECTIONBITMAP`
and every rendering, opacity and hit-testing rule unchanged.

Click-through now toggles only `WS_EX_TRANSPARENT`; the layered style is part of
the pet's presentation contract and is never removed when pointer transparency
is turned off. Window mode keeps its ordinary non-layered window and is not
affected.

DWM always composites a visible layered top-level window. While the pet overlaps
a composed borderless/windowed fullscreen game, the game cannot keep the lazy
composition path without also composing the pet. This uses only documented
window styles, introduces no dependency, does not modify the game process, and
uses no injection or undocumented API.

Games that present as D3D fullscreen (including some that label the option
"Windowed Borderless") are not fixed by this decision. Overlaying them would
require in-process rendering hooks, which this project does not use.

## Consequences

- The pet is visible above composed borderless/windowed fullscreen games,
  without depending on a screenshot or on activating the overlay.
- The fullscreen game falls back to DWM composition while the pet is visibly
  overlapped above it, so DirectFlip and G-Sync/FreeSync may be temporarily
  inactive for that game. This is inherent to compositing any overlay above a
  composed fullscreen application. The user can hide the pet or turn "always on
  top" off to let the game take the independent-flip path again.
- Games that take a D3D fullscreen presentation (`SHQueryUserNotificationState`
  returns `QUNS_RUNNING_D3D_FULL_SCREEN`) still hide the pet. Stardew Valley
  1.6.15 is one such game even with "Windowed Borderless" selected. The safe
  workaround is to run the game in windowed mode and maximize it, or use a
  borderless-window tool that keeps a normal composed window; the project does
  not inspect or patch the game process.
- Exclusive fullscreen remains a Windows limitation; no injection is added.
- Layering does not make the window opaque or add a black background: the
  DirectComposition visual still supplies the pixels and the per-pixel alpha.
  The click-through pass-through pair (`WS_EX_TRANSPARENT` + `WS_EX_LAYERED`)
  remains the input rule.

## Validation

- Windows tests: 111 overlay tests pass, including the creation style (a default
  pet window is layered, not pointer-transparent, and keeps `WS_EX_TOPMOST`,
  `WS_EX_NOACTIVATE`, `WS_EX_TOOLWINDOW` and `WS_EX_NOREDIRECTIONBITMAP`) and the
  click-through contract (disabling it removes only `WS_EX_TRANSPARENT` and
  keeps `WS_EX_LAYERED`). `cargo fmt --all -- --check` and
  `cargo clippy -p bongocat-overlay --all-targets -- -D warnings` pass.
- Composed fullscreen acceptance: The Last of Us Part II on Windows, fullscreen,
  displays the pet after the change.
- Desktop: a 2560x1440 physical capture shows the pet with a transparent
  background and no black box.
- D3D fullscreen limitation: Stardew Valley 1.6.15, Windowed Borderless,
  2560x1440, returns `QUNS_RUNNING_D3D_FULL_SCREEN` while foreground and hides
  the pet; the pet and a normal test window appear only after the game loses
  foreground. Documented as a game-side presentation limit.
- Not run: full workspace test suite, in-game click-through/hover/idle/manual
  resize acceptance, window-mode acceptance on this build, and a second composed
  fullscreen game besides The Last of Us Part II.

