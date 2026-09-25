# Viewer route

`+page.svelte` owns the viewer session, mode, keyboard shortcuts, selection, search, and menus. `ViewerToolbar.svelte`,
`ViewModePicker.svelte`, and `ViewerStatusBar.svelte` present controls and status. `RawByteView.svelte` displays
original bytes in Binary and Hex modes; `raw-byte-view.ts` formats its rows. Text rendering and scrolling live in the
other components and helpers beside this file. Architecture and decision detail: `DETAILS.md`.

## Must-knows

- The text view works in **rows, not physical lines**. Long lines split into rows; text coordinates use row indexes and
  UTF-16 columns. Binary and Hex work in byte offsets. Keep their state and selection paths separate.
- Mode keys **1, 2, 3** select Text, Binary, and Hex; **0** selects Media only for a supported image or PDF. Ignore them
  in editable controls. Preserve the backend media kind when switching away from and back to it.
- Binary and Hex read bounded chunks through `viewerGetBytes`; a routed or phone file uses the session's materialized
  path. Do not decode these reads through the text backends. Their DOM text selection covers rendered rows only.
- Text-only effects, search, word wrap, encoding, and tail mode must not run in a raw byte or media view. A binary
  warning concerns decoded text and appears only in Text mode.
- Text selection is geometric, with its own caret and anchor logic. Native selection is disabled there. A pointer
  gesture must focus the viewer first; preserve gesture granularity and UTF-16 columns when changing selection code.
- The virtual text scroll uses pixel heights and chunk walks. `scrollToMatch` has separate indexed and approximate
  paths; `needsFetch()` samples three points. The height map owns wrapped row geometry.
- Text Copy and Save use row metrics and `EOF_ROW`; do not treat a row index as a physical line or byte offset.
- `closeSelfWindow()` waits two animation frames before closing because of a macOS WebKit crash. The `canClose` gate
  queues Escape pressed during mount; it must become ready after mount, even while a remote pull remains active.
- Escape closes the context menu or search bar before the window. Media URLs use the `cmdr-media://` token scheme and
  its CSP rules; do not pass arbitrary local paths to the webview.
- Tail mode has no persistent setting. Keep menu events and their local echo from forming a toggle loop.

Read `DETAILS.md` before changing viewer behavior, especially selection, virtual scrolling, or media rendering.
