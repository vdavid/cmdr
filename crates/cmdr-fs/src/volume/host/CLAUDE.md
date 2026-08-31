# The volume host seams

Everything a storage backend needs from the app around it, as named traits instead of `crate::`-qualified reaches
upward. A backend implements `Volume` and knows one protocol; what it can't answer for itself (what the panes show,
where credentials live, which runtime to spawn on, who to tell when a watch breaks) arrives through here.

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
- **The backend says WHAT, the host says what the user sees.** Event payload types, their `tauri_specta` derives, and
  every English word stay app-side. A seam carries a typed value (`VolumeConnection::NeedsCredentials`), never prose.
- **❌ Nothing identifying in an analytics property**: no host, path, share, bucket, filename, or username, not hashed,
  not truncated. The `&[(&str, &str)]` shape exists so a struct can't slip in.
- **"Is the user busy on this volume?" has TWO halves and one composer.** `UserActivity` reports in-flight foreground
  leases AND how long it's been quiet; ❌ never read either alone — `activity::volume_busy_for_user` is the rule, and a
  timestamp-only read stops counting an operation the user is still waiting on. ❌ Moving either signal without bumping
  its `watch_volume` leaves a parked transfer asleep.
- **Connection parameters are constructor arguments, not settings.** `settings` is only for what the user changes while
  a volume is mounted, which is why it's read per dispatch.
- **A trust seam degrades to trusting NOTHING.** `HostKeys` under `VolumeHost::detached()` answers every key unknown; ❌
  never "trust everything", which is how a man-in-the-middle regression ships green. Its `testing` double REMEMBERS, so
  an approve-then-reconnect harness terminates. `DETAILS.md` § "Seam by seam".
- **Background work that outlives a call reaches its own state through a `SelfHandle`, never a volume id it looks up.**
  An id answers with the SUCCESSOR after a replace, and "still here" forever after a removal an in-flight holder keeps
  allocated. Publish a `Retirement` from `Volume::retirement` so the registry can write "you left". `DETAILS.md` § "The
  two registry reach-backs".

## Module map

- `mod.rs`: `VolumeHost` (the bundle), its builder, and `detached()`.
- `listings.rs`: `ListingHost`, the busiest seam. Report a change, ask the fresh-listing oracle, refresh archive panes.
- One seam per file: `runtime.rs` (the injected `tokio::runtime::Handle`, not a trait), `events.rs` (`VolumeEventSink` +
  `VolumeConnection`), `credentials.rs` (`CredentialStore`), `host_keys.rs` (`HostKeys`), `indexing.rs`
  (`IndexNotifier` + `WatchGap`), `settings.rs` (`BackendSettings`), `activity.rs` (`UserActivity`), `analytics.rs`
  (`AnalyticsSink`). What each one replaces: `DETAILS.md` § "Seam by seam".
- Each carries a recording or scripted fake under the `testing` feature, for tests that assert on what a backend told
  its host.

The other end is `apps/desktop/src-tauri/src/volume_host.rs`, where Cmdr builds its host, each answer an adapter next to
the subsystem that gives it.

Rationale, where the app answers each seam, and what deliberately isn't a seam: `DETAILS.md`. Read it before any
non-trivial work here: editing, planning, reorganizing, or advising.
