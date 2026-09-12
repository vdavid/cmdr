package checks

// The public-surface ceiling table behind `RunIndexCrateIsolation`, split out of
// index-crate-isolation.go to keep that file under its length cap. The mechanism
// (`countSurface` and friends) and the dependency-graph half of the check stay there;
// this file is data plus the per-crate rationale for each number.

// surfaceCeilings caps each bucket of the public surface. See each crate's entry in
// surfaceGuardedCrates for what justifies its numbers.
type surfaceCeilings struct {
	// RootPromises is what `lib.rs` exports: a `pub` there says "a host may rely on
	// this forever".
	RootPromises int
	// HandleMethods is the count of `pub fn` on the crate's one handle type, when it
	// has one. Zero (with an empty HandleType) means the crate has no such type.
	HandleMethods int
	// PublicModules is how many modules a host can name a path into, across the
	// whole crate.
	PublicModules int
	// SubsystemItems is every `pub` item inside those modules — the surface the
	// root re-exports don't capture.
	SubsystemItems int
}

// surfaceGuardedCrates are the guarded crates whose public surface is ALSO capped.
// Not every guarded crate is: `cmdr-fs` is deliberately absent, because it's shared
// vocabulary whose whole job is to be named from everywhere, so a count of its `pub`
// items would measure the wrong thing.
//
// HandleType names the one type whose methods get their own bucket, or is empty when
// the crate has no such type. A backend crate doesn't: its API is the `Volume` trait
// it implements, whose methods aren't a promise of its own.
var surfaceGuardedCrates = []struct {
	Name       string
	HandleType string
	Ceilings   surfaceCeilings
}{
	{
		Name: "cmdr-index",
		// Measured 2026-07-31, and each number is where the item-by-item audit
		// landed rather than where the code happened to be. `HandleMethods` is 35
		// rather than the audit's headline 34 because this count includes
		// `Index::builder`, the constructor.
		//
		// Raised once on 2026-08-05, with David's say-so, for ONE new concept:
		// COVERAGE — what the index can't answer for yet, and walking the rest.
		// He asked for the whole surface to be designed together and the ceilings
		// raised to match, rather than a bump per method, so both numbers below
		// carry the full concept:
		//
		//   - `RootPromises` 44 → 47: `CoverageMap` (the answer), `CoverageToken`
		//     (which state of the index it describes), `CoverageDimension` (the
		//     forward-compat axis content search will add itself to).
		//   - `HandleMethods` 35 → 38: `Index::coverage` and
		//     `Index::coverage_token`, both landed, plus ONE reserved slot for
		//     `Index::cover`, the walk half — it takes the frontier a coverage
		//     answer named and fills it in. That slot is spoken for; anything else
		//     arriving in it still has to be argued the same way these were.
		//
		// `Index::cover` landed on 2026-08-05 into its reserved slot, and brought
		// the three types its answer is made of. `RootPromises` 47 → 50:
		//
		//   - `CoverWalk` — the running walk: take batches off it, cancel it,
		//     finish it. A host can't drive a walk without a handle to it.
		//   - `CoveredEntry` — one entry the walk found, in the shape a result row
		//     needs. Decision 3 puts the matching host-side, so this type crossing
		//     the boundary is the whole point: it's what keeps a matcher out of
		//     this crate.
		//   - `CoverOutcome` — what the walk covered, and whether it was cancelled.
		//     The terminal state a search's UI phase reads.
		//
		// Nothing else is owed to the coverage concept. A fourth type here needs
		// the same argument these three did.
		//
		// Raised again on 2026-08-05, with David's say-so, for ONE more concept:
		// WHAT THE INDEX OCCUPIES ON DISK, and dropping all of it. Once a search
		// walks, a machine that indexes nothing still accumulates databases, and
		// the settings screen has to be able to show and reclaim them.
		// `HandleMethods` 38 → 40, and no new root promise (both answer in types
		// the crate already promises):
		//
		//   - `Index::disk_footprint` — the bytes every index database occupies,
		//     read off the FILES rather than the registry, which can't see the
		//     database a walk built and nothing re-registered after a restart.
		//   - `Index::forget_all_volumes` — the whole-index sibling of
		//     `forget_volume`, reaching those same unregistered databases.
		//
		// The concept is closed: measuring and clearing is all of it.
		//
		// Raised again on 2026-08-15, with David's say-so, for ONE item:
		// `CoveragePhase`, which phase of a drive's first index is running.
		// `RootPromises` 50 -> 51. The index owns the order and the path space that
		// classifies a root into it, so a host re-deriving the phase from that root
		// would need its own idea of firmlinks: right on one machine, wrong on the
		// next. It rides one event variant and the status response, which is what lets
		// a window that reloaded mid-index name the running phase instead of waiting
		// out the next boundary. Nothing else is owed to the concept: what each phase
		// is CALLED is the host's, and a second type here needs the same argument this
		// one got.
		//
		// Raised again on 2026-08-23, with David's say-so, `RootPromises` 51 -> 52, for
		// ONE item: `FolderChangeRollup`, carried by `IndexEvent::FolderActivity`. The
		// argument, and why the change-kind enum behind it stays crate-private, is in
		// the audit doc below under "A sixteenth followed".
		//
		// ⚠️ WHICH BUCKET a grant lands in is not a choice, so read the right counter
		// before assuming you have headroom. A value an event carries always spends a
		// ROOT PROMISE, never `SubsystemItems`. Why, in the audit doc below, under
		// "Which of the check's counters a new item spends is not a choice".
		HandleType: "Index",
		Ceilings: surfaceCeilings{
			RootPromises:   52,
			HandleMethods:  40,
			PublicModules:  17,
			SubsystemItems: 156,
		},
	},
	{
		// Measured 2026-08-22, at the extraction, with no headroom. What each item is
		// for: `crates/cmdr-smb/DETAILS.md` § "The public surface is capped".
		//
		// Raised on 2026-09-09, with David's say-so, `SubsystemItems` 18 -> 21, for
		// ONE concept: a mount's ANCHOR, where its mount point sits inside the share.
		// Four items in (`MountAnchor` with its two constructors, and
		// `SmbVolume::exchange_mount_roots_with`), one back (`SmbVolume::new` is
		// `pub(crate)`; outside, a volume is dialed). The app is the caller that got
		// this wrong (ERR-48RZX), so the type that forces the mount path and the
		// anchor to travel together belongs at the boundary the app crosses. The
		// argument item by item: the crate DETAILS section above.
		// Root 15 -> 16 and items 21 -> 22 for `try_open_share` (ERR-SHUSC): why, in
		// that same DETAILS section.
		Name: "cmdr-smb",
		Ceilings: surfaceCeilings{
			RootPromises:   16,
			PublicModules:  4,
			SubsystemItems: 22,
		},
	},
	{
		// Measured with this check's own `countSurface`, and set to exactly what the
		// crate exposes — no headroom, so the first widening is a conversation rather
		// than a silent drift.
		//
		// Three public modules is the whole tree a host can name a path into: `auth`
		// (for `AuthRungUsed` and `UnattendedReconnect`), `transport` (for
		// `HostKeyPrompt` and its kind), and `volume` (for `approve_host_key`,
		// `HostKeyApproval`, and the `testing` fixtures). ❗ `errors`, `extensions`,
		// `known_hosts`, `params`, and `trust` stay `pub(crate)`, and the three types
		// the app needs from them arrive as root re-exports: `trust` and `known_hosts`
		// hold the man-in-the-middle decision, which nothing outside this crate has
		// any business reaching into.
		//
		// 27 items. The two per-server switches cost three
		// (`auth::UnattendedReconnect`, `SftpVolume::set_auto_reconnect`,
		// `SftpVolume::unattended_reconnect`), and narrowing `transport`'s
		// `presented_host_key` to `pub(crate)` (it returned a `pub(crate)` type, so
		// nothing outside could call it) gave one back. Saving an edit to a connected
		// place costs two, `SftpVolume::sharing_connection` and
		// `SftpVolume::set_redial_params`, and neither fits a disposition: the app
		// wiring calls both (no gate, no delete), only this crate can build an
		// instance over its private connection state (no facade), and they can't fold
		// into one, because the instance is built BEFORE the edit is checked while the
		// redial params may only move once it is accepted.
		//
		// Item-by-item, and what each module is for: `crates/cmdr-sftp/DETAILS.md`
		// § "The public surface is capped".
		Name: "cmdr-sftp",
		Ceilings: surfaceCeilings{
			RootPromises:   10,
			PublicModules:  3,
			SubsystemItems: 27,
		},
	},
	{
		// Measured 2026-09-01, at the crate's first landing, and set to exactly
		// what it exposes. A WebDAV volume needs no host-key or auth-rung
		// vocabulary, so the whole promise is the params, the typed connect
		// error, the volume, its one unattended-reconnect answer, and the
		// constructor; `volume` is the only public module, and `volume::testing`
		// exists only behind the `testing` feature. Saving an edit to a connected
		// place adds two, `WebdavVolume::sharing_connection` and
		// `WebdavVolume::set_redial_root`, for the reasons `cmdr-sftp`'s entry gives
		// for its twins. Item-by-item:
		// `crates/cmdr-webdav/DETAILS.md` § "The public surface is capped".
		Name: "cmdr-webdav",
		Ceilings: surfaceCeilings{
			RootPromises:   6,
			PublicModules:  1,
			SubsystemItems: 10,
		},
	},
	{
		// Measured 2026-09-05, at the extraction, and set to exactly what the crate
		// exposes — no headroom, so the first addition has to be argued for.
		//
		// EVERY module is private, so a host can name no path into this crate at all
		// and all 11 promises arrive as root re-exports. That's why both other buckets
		// are zero, and it's the tightest shape any backend crate here has taken:
		// `GitPortal`'s own methods are reachable but unmeasured, so the item-by-item
		// argument in the crate's `DETAILS.md` is what holds them, not this number.
		//
		// The 11 serve four callers, and a new one should name which:
		// browsing (`GitPortal`, `GitPortalVolume`, `portal_route`), the chip
		// (`RepoInfo`, `repo_info`, `RepoHandle`), the status column (`EntryStatus`,
		// `EntryStatusCode`, `list_status`), and reporting a change
		// (`GitStateSink`, `no_git_state_sink`).
		//
		// LOWERED 12 -> 11 on 2026-09-05, no conversation needed because a tightening
		// isn't a loosening: `virtual_category_prefixes` lost its only caller when the
		// post-change listing refresh started matching by CANONICAL worktree root
		// instead of by string prefix, and this crate keeps no promise nothing reaches.
		//
		// Item-by-item: `crates/cmdr-git/DETAILS.md` § "The public surface is capped".
		Name: "cmdr-git",
		Ceilings: surfaceCeilings{
			RootPromises:   11,
			PublicModules:  0,
			SubsystemItems: 0,
		},
	},
	{
		// Measured 2026-09-05, at the extraction, and set to exactly what the crate
		// exposes — no headroom, so the first addition has to be argued for.
		//
		// The session layer is most of this crate and NONE of it is a public module:
		// `connection` is private, so all 12 of its names arrive as root re-exports,
		// which is why the root bucket is large and the subsystem one tiny. Two public
		// modules is the whole tree a host can name a path into: `volume` (for
		// `MtpVolume`) and `virtual_device`.
		//
		// 11 of the 13 subsystem items are `virtual_device`'s, which reads oddly for a
		// fixture and is deliberate: the fake phone sits behind `virtual-device` rather
		// than `testing`, so the E2E build ships it and the counter measures it.
		//
		// Item-by-item, and which of the four audiences each one serves:
		// `crates/cmdr-mtp/DETAILS.md` § "The public surface is capped".
		Name: "cmdr-mtp",
		Ceilings: surfaceCeilings{
			RootPromises:   20,
			PublicModules:  2,
			SubsystemItems: 13,
		},
	},
	{
		// Measured 2026-08-03, at the extraction, and set to exactly what the crate
		// exposes — no headroom, so the first addition has to be argued for.
		//
		// A backend's API is the `Volume` trait it implements, which is `cmdr-fs`'s
		// promise rather than this crate's, so everything counted here exists for one
		// of exactly three callers, and a new item should name which:
		//
		//   - the boundary detector the host routes with (`boundary`, 9 of the root
		//     promises),
		//   - the reading core the host's archive-edit driver and file viewer parse
		//     and stream through (`read`, 23 of them),
		//   - `ArchiveVolume` itself, plus `mutator` and `active_watch_count`.
		//
		// Four public modules is the whole module tree a host can name a path into:
		// `boundary`, `read`, `volume`, `watch`. `mutation` is private and reaches the
		// root only as the `mutator` re-export; `test_fixtures` is `testing`-gated and
		// counts apart.
		Name: "cmdr-archive",
		Ceilings: surfaceCeilings{
			RootPromises:   35,
			PublicModules:  4,
			SubsystemItems: 35,
		},
	},
}
