# ADR-0102: Companion distribution and render quality

Status: implemented; local installer, native model matrix and ten-minute observation verified; controlled 2.5.1 memory comparison remains open
Date: 2026-11-02

Pinned local Windows releases discover exclusively through the build-pinned bundled catalog. The upstream signed online source remains available to other builds. The companion release retires exactly Pomodoro, Input Source and Key Sounds before discovery/resume; files and sessions are retained. Local installation cannot reactivate retired IDs. First bundled installation remains disabled and intentional removal is respected.

RenderQuality is a shared platform-free configuration enum (low/medium/high), re-exported by the runtime and settings protocol to avoid divergent mappings. The settings protocol and runtime now depend on the configuration crate for this value. Existing whole-overlay commands carry it. Windows recreates presentation resources transactionally when quality changes. Source model assets are untouched. Texture dimensions use 50/75/100 percent after premultiplication; masks use the same fraction of window dimensions with normalized sampling and the viewport restored before composition. Identical mask source sequences share storage and rendering. Capture readback is temporary. HUD font initialization is deferred until content and released after thirty empty seconds, with parse failures reported once.

A missing quality property identifies pre-2.5.2 documents. Under the existing writer lock the store backs up original bytes and commits low quality / 30 FPS. Subsequent loads preserve the explicit property and FPS. Existing v1 validation and unsupported-version rejection remain in effect. UI exposes 30/60 FPS; older valid FPS values remain readable.

Windows-only delivery. macOS quality-resource behavior is not implemented or validated by this change. No global RAM reduction or long-duration stability claim follows from static texture sizes.
