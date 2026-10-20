# State recovery: details

Issue #461. Must-knows in `CLAUDE.md`.

## Flow

1. A store finds its file unreadable: it doesn't parse, or it carries a schema version this build doesn't know.
2. It calls `set_aside(path, StateFile::…)`. The file moves to `<name>.broken-YYYY-MM-DDTHHMMSS` (local time; a second
   one in the same second gets `-2`), and all but the newest three copies of that file go.
3. `report` records `StateFileRecovered { file }` in the security log (`../security_log/DETAILS.md` § Event catalog)
   and queues a `StateRecovery { file, kept_copy }`. Once `init` has run it also emits `state-recovery-queued`.
4. The main window calls `take_state_recoveries` at start and on every nudge, and shows one toast per recovery
   (`apps/desktop/src/lib/state-recovery/`). Taking drains, so a recovery shows once, and a later launch with a healthy
   file finds nothing.

## Per store

- **`settings.json`, `shortcuts.json`, `app-status.json`, `license.json`** (`plugin_stores.rs`, from `setup()`): first
  sweeps the `.<name>.*.tmp` files a killed save left (the patched plugin's temp naming,
  `vendor/tauri-plugin-store/CMDR-PATCH.md`), then checks that the file reads and parses as the JSON object the plugin
  reads. One that doesn't, for either reason, is set aside, so the plugin starts from no file. A recovered
  `settings.json` is replaced right away with `{"analytics.enabled": false, "updates.crashReports": false}`, written
  temp+rename. A rename that doesn't happen leaves the file, and the patched plugin then refuses every save of that
  store for the session, the exit save included (#506). Paths match their readers: the frontend's stores sit in
  `CMDR_DATA_DIR` when that's set (`resolveStorePath`), `license.json` always in the plugin's app data dir.
- **`operation-log.db`, `main.db`** (`OperationLogStore::open`, which `operation_log_host::start` hands `set_aside`
  through `OperationLog::open`; `AgentStore::open`): only on the typed
  `NotADatabase` / `DatabaseCorrupt` code. The DB is set aside, its `-wal` and `-shm` removed, and a fresh one built. A
  rename that doesn't happen falls back to deleting, the only way to a working DB.
- **Favorites, the recents lists, the saved-server lists** (`state_file.rs`, through `read_state_file`): a file that
  can't be read (mode 000, `EIO`, bytes that aren't UTF-8), doesn't parse, or carries an unknown schema version is set
  aside, and the store starts empty. Favorites reads as `Some(empty)`, never re-seeding (`../favorites/DETAILS.md`). A
  rename that doesn't happen leaves the file and HOLDS the path: `write_state_file` refuses every write to it
  (`StateWriteError::NotLoaded`) until a later read succeeds, and a store that rereads its file on every call
  (`manual-servers.json`) hears about it once.
- **Lists of independent entries** (saved S3 places and accounts, SFTP and WebDAV servers, SMB share rows, opt-outs and
  pending moves, manual SMB servers, trusted SSH host keys): each list field is `#[serde(deserialize_with = "lenient_entries")]`, so an entry
  that doesn't read (a provider `kind` from a newer build, a typo) is dropped alone. `read_state_file` then COPIES the
  whole file aside (`keep_copy`: same naming, same three kept) and the person hears it once. The file itself stays
  until the next save writes the entries that read; a launch before that finds a copy of the same bytes already there
  and neither copies nor tells again, so repeat launches can't push an older, different copy out of the three.
  The shared loader is `../network/server_list_file.rs`.
- **`usage.json`, `ai/ai-state.json`**: through `read_state_file` like the lists above (set aside, or held when the
  rename fails), each starting from its defaults.
- **`credentials.enc`** (`../secrets/encrypted_file.rs`, Linux without a keyring): on the first `set` or `delete` that
  finds it undecryptable under this machine's id (a restored home folder, a cloned VM) or unparseable. The store starts
  empty; a rename that doesn't happen lets the save replace the file, which no key on this machine opens anyway. `get`
  and `keys` keep answering the error and touch nothing: a read decides nothing.
- **Left on their own fallback, on purpose**: `update-check.json` (when the next update check is due; losing it costs
  one early check), `analytics-heartbeat.json` and the event spool (unsent telemetry and uptime, never the person's),
  `.window-state.json` (window geometry), and `install-ids.json` (two random ids; a lost one is minted again, which only
  resets analytics continuity, and holding it would mint a new in-memory id every launch instead). The agent's
  `outcomes.md` isn't JSON: `MemoryStore::record_outcome` skips a decision when the log is there but unreadable.

## Decisions

- **Consent settings fail toward privacy.** A damaged `settings.json` may have held a "no" to usage stats or crash
  reports, both default on, and starting at the defaults would undo it without a word. So the fresh file says "no" to
  both, and the notice says how to turn them back on. Security-relevant settings need nothing here: their defaults are
  the safe ones (updates, cloud AI, MCP).
- **The "no" doesn't depend on the fresh file landing.** `recover_settings` opens the settings store before the frontend
  does, with both consents off as its DEFAULTS, so the frontend gets that instance. A fresh file overrides nothing (it
  says "no" too), a fresh file that didn't land leaves the defaults, and a file that stayed unloaded (rename refused)
  never reaches the cache, so all three read "no". Rust's analytics gate reads through `settings_for_consent`, where a
  present-but-unloadable file is also "no", and so is no file at all once this session recovered `settings.json`
  (`SETTINGS_RECOVERED`): a fresh file that didn't land isn't a first launch. A file the person saves later that
  session is their answer again.
- **`operationLog.enabled` stays on** (David, 2026-10-16): the operation log never leaves the Mac and undo depends on
  it, so it isn't a consent-type setting.
- **A queue, not an event payload.** Most recoveries happen in `setup()`, before any window listens, so an event alone
  would be lost. The nudge carries nothing; the drain is the one source.
- **Timestamped copies, three kept.** A single rotating `.broken` copy would lose the first copy to the second
  corruption, and the first is often the more telling one. Three bound the disk a file that keeps breaking can use.
- **Its own channel, not `security_notices`.** That module's notices never carry a path; this one has to say where the
  copy went.
- **Set aside even when the file is only unreadable.** An unreadable file may be perfectly healthy (a root-owned
  copy after a `sudo` run), but leaving it in place was what let the next save write the empty store over it (#506,
  #517). The rename needs only the folder's permission, so it works on a mode-000 file, and the copy keeps the bytes.
- **Hold, don't delete, when the rename fails.** Deleting a file nobody could read or move would be the data loss this
  module exists to prevent; a held path costs that store's saves for one session.
- **No `#[serde(other)]` catch-alls on the saved-server enums.** `S3ProviderChoice`, `ConnectionMode`, and `AuthOptions`
  cross IPC as specta types, and a catch-all would both widen every frontend `switch` and write a newer build's entry
  back without its fields. Dropping the entry and keeping the copy loses nothing.
- **A `license.json` recovery loses the "key is in the secret store" marker**, so `recover_plugin_stores` records it
  (`license_file_recovered`) and licensing probes the secret store once at launch to restore it
  (`../licensing/DETAILS.md` § Key storage). When the probe finds nothing (or can't read), the license reads as absent
  until the person enters the key again; the notice's conditional copy covers both outcomes.
