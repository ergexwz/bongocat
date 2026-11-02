# ADR-0101: Windows single-channel clipping masks

Status: Accepted, 2026-11-02.

Each masked drawable previously owned a window-sized BGRA8 target while only
its alpha coverage was sampled. Use R8_UNORM for these targets and the empty
mask. Keep the resolution, eight-bit coverage precision, filtering, source
geometry and inverted-mask behavior unchanged. This reduces unaligned mask
texel storage from four bytes to one; it is not a claim about total process
memory or driver allocation alignment.

The mask shader writes coverage to red and also supplies the same source alpha
to the existing ONE / INV_SRC_ALPHA blend state. Consequently overlap remains
`source + destination * (1 - source)`. The model shader reads red instead of
alpha. Model color, premultiplication and composition formats are unchanged.

R8_UNORM render-target blending is required at the renderer's Direct3D 11
feature level; see Microsoft's [required DXGI formats](https://learn.microsoft.com/en-us/windows-hardware/drivers/display/required-dxgi-formats).
No new dependency or persistent data format is introduced. macOS is unchanged.

Verification: the WARP regression executes the actual mask shader and blend
state, reads back faint, partial and opaque overlapping coverage, and compares
R8 bytes against the prior BGRA alpha bytes. Native overlay tests and packaged
model startup remain required before delivery. Performance evidence belongs in
the 2.5.1 benchmark report, with process, plugin and WebView2 attribution separate.
