# Changelog

Tier 2 fixture, shaped like the real `CHANGELOG.md`. With the plugin loaded, and only because this directory carries
`tools/intellij-plugin/cmdr-plugin.json`, every nine-character hash below renders link-colored and ⌘-click opens the
commit on GitHub.

## [0.37.0] - 2026-08-03

### Added

- Add a "Chat memory size" setting: Automatic, or 16,000 up to 200,000 tokens (751214190, 14aacf891)
- Add an Acknowledgements dialog crediting all 775 open-source packages Cmdr ships (b626d7a4b, 2d41cc147, 18add0b0c,
  42f76971d, ede1a7d6e, 84e5f3a5f)
  - A nested bullet is its own entry, so it links on its own (e301c1e42)

### Changed

- Return a broad search in under half a second instead of twelve (~40x speed-up!)
- Nothing here links: the decade of beaded facade parsing is over, and a (deadbeef) mid-sentence stays prose
- An eight-character ref is a mistake, and staying unlinked is how it becomes visible (fd6fc293)
