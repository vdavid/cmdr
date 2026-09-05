# What a Logitech mouse's back / forward buttons actually deliver on macOS (2026-09-04)

Why Cmdr's mouse back / forward navigation didn't work on a Logitech MX Master 4, measured rather than reasoned. The
short version: **with Logi Options+ installed, the thumb buttons emit no mouse button at all** — they emit a macOS swipe
gesture. Every reading that assumes a button is looking for an event that is never posted.

Read this before changing `apps/desktop/src-tauri/src/mouse_nav.rs`, before "fixing" the DOM path in
`apps/desktop/src/routes/(main)/mouse-nav.ts`, and before attributing any missing pointer input to WKWebView.

## The wrong answer we shipped first

The first implementation (2026-09-04, commit `594a42dd5`) read the symptom — no `MouseEvent.button === 3 / 4` in the
DOM — and concluded that **WKWebView doesn't deliver extra mouse buttons to the webview**. The fix that followed from
that premise was an AppKit `NSEvent` local monitor for `otherMouseDown` / `otherMouseUp`, reading `buttonNumber`.

It didn't work either, because the premise was wrong. The DOM saw nothing for the same reason AppKit saw nothing: there
was no mouse-button event anywhere in the system. The verification behind the original claim only ever covered the DOM
half; the AppKit half was never confirmed against the device.

**The lesson worth keeping:** "the webview didn't get it" and "the event doesn't exist" look identical from inside the
webview. Confirm at the layer below the one that's failing before naming a cause there.

## Method

An AppKit probe: a 60-line Swift app (`NSApplication`, one window, `.regular` activation policy) installing
`NSEvent.addLocalMonitorForEvents(matching: .any)` and printing every event's type, `buttonNumber`, `deltaX`, `phase`,
and scroll deltas to stderr. Pointer parked over the probe's own window; back, forward, then the middle button as a
control; the whole sequence run twice.

This is the cheapest instrument for any "which event is this really?" question on macOS, and it needs no accessibility
permission (a LOCAL monitor only sees events already destined for its own app). Rebuild it before guessing.

## What came back

Identical across both runs (`phase` raw values: 1 = `began`, 8 = `ended`):

```
back     ->  swipe [type=31]  deltaX= 0.000  phase=1
             swipe [type=31]  deltaX=+1.000  phase=8
forward  ->  swipe [type=31]  deltaX= 0.000  phase=1
             swipe [type=31]  deltaX=-1.000  phase=8
middle   ->  otherMouseDown   buttonNumber=2
             otherMouseUp     buttonNumber=2
```

Four things this settles:

1. **No `otherMouse` event with `buttonNumber` 3 or 4 exists**, on either press or release. A monitor watching for one
   can never fire for these buttons.
2. The buttons post `NSEventType::Swipe` (31), `deltaX` `+1` for back and `-1` for forward. That is AppKit's own
   `swipeWithEvent:` sign convention, not a Logitech invention, so a Magic Mouse two-finger swipe reads the same way.
3. **A press is a PAIR**, and only the second half carries the direction. Anything acting on `deltaX` alone must ignore
   the `began` half, or it fires twice with no direction the first time.
4. **A swipe can't collide with horizontal scrolling.** The thumb wheel shows up as `scrollWheel` (type 22, `deltaX`
   ≈ −4, `hasPreciseScrollingDeltas`) — a different event type entirely, which the swipe mask never sees.

## Why: the Options+ config trail

Logi Options+ owns the buttons, and its stored profile says exactly what it substitutes. Two files, both readable:

`~/Library/Application Support/LogiOptionsPlus/settings.db` — a SQLite file whose `data` table holds one JSON blob.
Under the single global profile key (`profile-…`, one per configured app; there was no Cmdr-specific profile):

```
mx-master-4-2b042_c82  ->  card_global_presets_middle_button
mx-master-4-2b042_c83  ->  card_global_presets_osx_back      (back thumb button)
mx-master-4-2b042_c86  ->  card_global_presets_osx_forward   (forward thumb button)
```

`/Library/Application Support/Logitech.localized/LogiOptionsPlus/card_presets/card_presets_osx.json` — what those cards
do. The contrast is the whole answer:

| Card | `macro.mouse.action` | `hidUsage` |
| --- | --- | --- |
| `card_global_presets_middle_button` | `BUTTON` | **3** |
| `card_global_presets_osx_back` | `OSX_GESTURE_BACK` | **0** |
| `card_global_presets_osx_forward` | `OSX_GESTURE_FORWARD` | **0** |

`hidUsage: 0` means no HID button is emitted. The middle button on the same device keeps a real one, which is why
Cmdr's middle-click gestures (close a tab, open a folder in a background tab) worked throughout while the side buttons
did nothing.

## What shipped

`mouse_nav.rs` now watches `NSEventMask::Swipe` alongside the two `otherMouse` masks, and classifies every event
through one pure `action_for(event_type, button_number, delta_x)` so the decision table is unit-tested without an
`NSEvent`. Three outcomes, because "ours" and "carries a direction" are not the same thing — the `began` half of a
swipe is swallowed and dispatches nothing.

The `otherMouse` path was **kept, not replaced**. It's still the live path for a plain five-button mouse, for a
Logitech user who hasn't installed Options+, and for any vendor that passes the raw button through.

## What would change the answer

- **A different Options+ assignment.** These readings are the shipped default. A user who assigns the thumb buttons to
  a keystroke, or to `card_global_presets_middle_button`, gets a different event — the keystroke case would reach Cmdr
  through the normal shortcut path and needs nothing from this module.
- **Options+ uninstalled**, which should restore raw HID buttons 4 / 5 and exercise the `otherMouse` path. Not measured.
- **Another vendor's driver** (Razer Synapse, SteerMouse, USB Overdrive) may substitute something else again. The probe
  is the way to find out; don't extrapolate from this note.

_All readings: Logitech MX Master 4 (`mx-master-4-2b042`) over Bolt, Logi Options+ with `logioptionsplus_agent` and
`LogiPluginService` running, macOS 27, 2026-09-04._
