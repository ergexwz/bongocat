# Windows game input comparison, 2026-11-03

Status: partial investigation; this is not release acceptance.

An isolated Development build was tested on the user's Windows desktop with
Identity V in the foreground. The user confirmed that the pet's idle animation
continued but keyboard and mouse reactions stopped, then returned immediately
after Alt+Tab. A first sandboxed run recorded no desktop input and was discarded
as an invalid control.

The same diagnostic build was then started outside the sandbox with isolated
data. In the ordinary-permission comparison, desktop input was observed before
and after the game, but one approximately one-minute interval showed no increase
in Raw Input keyboard/mouse counters; frame and animation counters continued at
roughly 30 frames per second. An anonymous count of currently pressed virtual
keys stayed at zero during that interval. The user confirmed holding a direction
key and moving the mouse during the game test.

The user then approved a UAC prompt for a separate isolated elevated test. They
reported that the pet reacted in Identity V with the elevated host; its input
counters advanced. This supports a permission-dependent input restriction on
this machine and game configuration, but does not establish a universal cause
or compatibility across game versions. No key identities, text, game memory,
injection or game rendering hooks were collected or used.

The elevated input-only run skipped plugin initialization. A subsequent separate
interactive UAC acceptance asserted an elevated host and started, ticked and
stopped all three extracted companions through the new broker; both broker and
actual plugin tokens were verified as same-account medium integrity. This did
not exercise plugins while Identity V was running. Identity V display mode and
Windows/anti-cheat versions were not recorded. Clean installation and upgrade
remain untested.

In Stardew Valley the user reported the pet was not visible in the regular
fullscreen mode. During that test Windows reported `QUNS_RUNNING_D3D_FULL_SCREEN`
for 31 one-second samples. The result is consistent with exclusive D3D fullscreen;
the mode cannot be covered reliably by a normal desktop overlay. The user also
confirmed that the pet was visible in the small windowed mode but not in the
game's borderless-window mode.

The user also tested The Last of Us in their usual fullscreen mode with the
isolated ordinary-permission development build and reported that the pet was
visible and its input reactions were normal. Game edition and exact presentation
mode were not recorded.

The local 2.5.3 installer was subsequently integrity-tested and extracted.
With isolated plugin data, its three companions started, ticked and stopped
under an ordinary host and under an asserted elevated host (30 seconds in the
elevated run); the packaged broker and child token checks enforced same-account
medium integrity. Direct unauthenticated broker invocation was rejected. These
package tests did not repeat the game tests with the final installer payload.
