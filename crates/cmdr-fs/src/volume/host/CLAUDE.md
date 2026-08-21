# The volume host seams

Everything a storage backend needs from the app around it, as named traits instead of `crate::`-qualified reaches
upward. A backend implements `Volume` and knows one protocol; what it can't answer for itself — what the open panes
show, where credentials live, which runtime to spawn on, who to tell when a watch breaks — arrives through here.

Writing a new backend? Read `DETAILS.md` first: it maps every seam to the call sites it replaces, and says what
deliberately isn't one.

## Must-knows

- **❌ A seam may be called per mutation, never per directory entry.** Every seam is a `dyn` trait object: free at human
  cadence, not in a loop over 250 000 entries. Hoist the call, take one answer, carry it.
  `RecordingListings::change_count` catches a regression; `DETAILS.md` § "The dispatch rule".
- **❌ Never `tokio::spawn` in a backend.** It INHERITS an ambient runtime; `host.runtime()` RESOLVES one. Watcher OS
  threads and synchronous startup have no reactor, so `tokio::spawn` panics there.
- **Every seam degrades, none panics.** `VolumeHost::detached()` is a complete host that answers nothing, so no backend
  needs an `Option<VolumeHost>` or a "there was no host" error path.
- **The host is a VALUE, not a static.** The app builds one and hands clones to backends; a test builds one with fakes.
  ❌ Don't stash a seam in a static: that forces install-and-restore guards and serialized tests.
- **❌ Never gate a fake or a test path on `cfg(test)`.** Use `#[cfg(any(test, feature = "testing"))]`, the crate rule:
  `cfg(test)` is set only while this crate builds its own test target, so a backend crate's test build sees it vanish.
- **The backend says WHAT, the host says what the user sees.** Event payload types, their `tauri_specta` derives, and
  every English word stay app-side. A seam carries a typed value (`VolumeConnection::NeedsCredentials`), never prose.
- **❌ Nothing identifying in an analytics property.** No host, path, share, bucket, filename, or username — not hashed,
  not truncated. The `&[(&str, &str)]` shape exists so a struct can't slip in.
- **Connection parameters are constructor arguments, not settings.** `settings` is only for what the user changes while
  a volume is mounted, which is why it's read per dispatch.
- **Background work that outlives a call reaches its own state through a `SelfHandle`, never a volume id it looks up.**
  An id answers with the SUCCESSOR after a replace, and "still here" forever after a removal an in-flight holder is
  keeping allocated. Publish a `Retirement` from `Volume::retirement` so the registry has somewhere to write "you left".
  `DETAILS.md` § "The two registry reach-backs".

## Module map

- `mod.rs` — `VolumeHost` (the bundle), its builder, and `detached()`.
- `listings.rs` — `ListingHost`: report a change, ask the fresh-listing oracle, refresh archive panes. The busiest one.
- `runtime.rs` — the injected `tokio::runtime::Handle` and the shared fallback. Not a trait.
- `events.rs` — `VolumeEventSink` + `VolumeConnection`, the typed connection transitions.
- `credentials.rs` — `CredentialStore` + `StoredCredentials`, over the OS secret store.
- `indexing.rs` — `IndexNotifier` + `WatchGap`, for when live watching lost continuity.
- `settings.rs` — `BackendSettings`, the live user-tunable knobs.
- `activity.rs` — `UserActivity`, so bulk work stands aside for the user.
- `analytics.rs` — `AnalyticsSink`, PII-free product counters.

Each file also carries a recording or scripted fake under the `testing` feature, for the tests that assert on what a
backend told its host.

The other end: Cmdr builds its host in `apps/desktop/src-tauri/src/volume_host.rs`, each answer an adapter next to the
subsystem that gives it (`DETAILS.md` § "Where the app answers each seam").

Rationale, the seam-by-seam map of the sites each replaces, and what deliberately isn't a seam: `DETAILS.md`.
