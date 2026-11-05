# cmdr-recents details

## Why its own crate

The list rules have five users: four desktop features (search, selection, Go to path, and multi-rename's field history
and last settings) and the browser demo, which keeps recent searches for the session. Inside any one of them, the others
would depend on that feature (Go to path on search, say), and no shared crate is about lists (`cmdr-fs` is filesystem
vocabulary). So the rules are a crate of their own, Tauri-free and dependency-free, and one dedupe and cap rule holds on
every host: a recent search dedupes in the demo exactly as on a Mac. ❌ Never inside `cmdr-search`: Go to path and
multi-rename would then depend on search.

## The edge

- **`RecentItem` asks for three facts**: the id a caller removes by, a way to replace it, and the dedupe key. What an
  entry MEANS (a mode, filters, a path) stays in the consumer, and so does its wire and on-disk shape.
- **The desktop's half** (`apps/desktop/src-tauri/src/recents/`): `RecentEntry`, which requires `RecentItem` and adds
  the file's name, log identity, schema version, and recovery key, plus `RecentsFile`, the cached list, its two locks,
  the durable write, and the quarantine. Rationale there: `apps/desktop/src-tauri/src/recents/DETAILS.md`.
- **Ids come from the host**: the desktop mints a uuid v4. Minting reads an RNG, and a browser host answers that its own
  way, so `add_to` takes `fresh_id` and calls it only when the added entry's id is already in the list.

## The rules

- `add_to`: drops EVERY earlier entry with the new one's key (a list that somehow grew two rows for one key heals), puts
  it on top, then trims. A cap of zero empties the list and drops the entry: the person asked for no list.
- `trim_to` and `remove_from` answer whether anything changed, so the desktop skips a disk write when nothing did.
- `dedupe_keeping_newest`: after a rewrite respelled entries in place (Go to path's paths after a saved server moved),
  two rows can be one thing; the newer, nearer the top, stays.
