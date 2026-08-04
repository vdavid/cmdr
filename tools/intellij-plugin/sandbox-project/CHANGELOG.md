# Changelog

Tier 2 fixture, shaped like the real `CHANGELOG.md`. With the plugin loaded, and only because this directory carries
`tools/intellij-plugin/cmdr-plugin.json`, every eight-character hash below renders link-colored and ⌘-click opens the
commit on GitHub.

## [0.37.0] - 2026-08-03

### Added

- Add a "Chat memory size" setting: Automatic, or 16,000 up to 200,000 tokens (75121419, 14aacf89)
- Add an Acknowledgements dialog crediting all 775 open-source packages Cmdr ships (b626d7a4, 2d41cc14, 18add0b0,
  42f76971, ede1a7d6, 84e5f3a5)
  - A nested bullet is its own entry, so it links on its own (e301c1e4)

### Changed

- Return a broad search in under half a second instead of twelve (~40x speed-up!)
- Nothing here links: the decade of beaded facade parsing is over, and a (deadbeef) mid-sentence stays prose
- A seven-character ref is a mistake, and staying unlinked is how it becomes visible (fd6fc29)
