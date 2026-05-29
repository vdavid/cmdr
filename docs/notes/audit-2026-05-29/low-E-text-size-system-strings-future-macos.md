# text_size + system_strings depend on undocumented Apple APIs

**Severity:** low
**Lens:** E — macOS pitfalls
**Confidence:** high

## Location

- `apps/desktop/src-tauri/src/text_size.rs:42–115`
- `apps/desktop/src-tauri/src/system_strings.rs:150–179`

## What

Both modules ride on undocumented Apple internals:

- `text_size.rs` reads `NSGlobalDomain.UIPreferredContentSizeCategoryName` (UIKit Dynamic Type key bleeding into macOS) and subscribes to the `com.apple.accessibility.api` distributed notification.
- `system_strings.rs` parses `.loctable` binary plists from hardcoded paths inside `/System/Applications/System Settings.app` and `/System/Library/ExtensionKit/Extensions/SecurityPrivacyExtension.appex` / `Appearance.appex`.

Both modules document the risks accepted (module-doc comments are exemplary — they call out exactly what could break and how it degrades). Failure modes:

- `text_size.rs::read_system_multiplier` returns `1.0` if the key is missing or unrecognized. Cannot crash.
- `system_strings.rs::build_snapshot` falls back to `LocalizedSystemStrings::english_defaults()` per field. Cannot crash.
- Both load eagerly at startup (`SNAPSHOT: LazyLock` and `observe_system_text_size_changes` called from `setup()`). Neither walks TCC paths (System Settings.app's `Resources/` is world-readable; `NSUserDefaults` reads don't touch TCC).

So the actual risk surface for macOS 16+ is: features quietly stop working (font size doesn't track system pref, friendly-error copy stays in English on localized macOS). No crash. The graceful-degradation paths are wired up correctly.

## Why it matters

Filing as low severity because the failure modes are documented and graceful. The only audit point worth raising:

`system_strings.rs::parse_loctable` calls `plist::Value::from_file(path)` against `/System/Applications/System Settings.app/...`. On macOS 26 (Tahoe), Apple has reportedly tightened TCC around `/System/Applications` reads from third-party apps (per the FDA-auto-add regression noted in `onboarding/CLAUDE.md`). It's plausible (untested) that future macOS could deny `read()` on these `.loctable` files for unprivileged apps. Today: they're world-readable. The fallback path handles `parse_loctable` returning `None`, so even a future denial collapses cleanly to English defaults.

This is genuinely fine. Flagging it for thoroughness, not because there's anything to fix.

## Evidence

- Both modules document risks in module-doc comments (text_size.rs:13–23, system_strings.rs:25–36).
- `read_system_multiplier` (text_size.rs:71–81) and `load_for` (system_strings.rs:187–195) both use the `unwrap_or` fallback pattern correctly.
- Snapshot is cached at first access (`LazyLock`), so even if `.loctable` parsing fails once, the English defaults are stable for the session.

## Suggested fix

No code change. Optionally: add `docs/maintenance.md` entry to verify `.loctable` paths + `UIPreferredContentSizeCategoryName` key still resolve when a new macOS major releases. The path strings (`/System/Library/ExtensionKit/Extensions/SecurityPrivacyExtension.appex/...`) are the most likely to move between macOS versions; a 30-second `ls` per major release would catch it.

## Notes

Quick Look's `define_class!` (`quick_look/controller.rs:234`) is in the same risk class (objc2 class registration is process-global and ABI-touching) but is even safer: `define_class!` is a build-time macro, the class is registered exactly once per process (objc2 enforces), and the registration happens lazily on first use. No audit concern.
