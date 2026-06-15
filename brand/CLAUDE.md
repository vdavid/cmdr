# Brand assets

The canonical, tracked home for Cmdr's reusable exported artifacts: the press-kit / media-kit set that the README,
website, AlternativeTo, newsletter, and any new surface all pull from.

- `logos/`: clean exported logos (`cmdr-{512,128,32}.png`), copied from the desktop app icons. The grab-here set.
- `screenshots/`: pristine full-window product shots. `app-main-{dark,light}.png` is the master pair that feeds the
  README, the website hero, and AlternativeTo, so they never drift.
- `copy/`: marketing text blobs (taglines, descriptions, feature lists).

This dir holds **exported deliverables**, not working files. The multi-MB source raws (logo `.af`, icon masters,
animation sources) stay in `_ignored/designs/`, which is gitignored.

## Pointers

- **Reshoot screenshots and refresh every consumer**: [`docs/guides/screenshots.md`](../docs/guides/screenshots.md).
- **Visual identity reference** (colors, type, voice, logo description):
  [`docs/guides/branding.md`](../docs/guides/branding.md).
- **Regenerate the logo from source**:
  [`docs/guides/regenerating-app-icon.md`](../docs/guides/regenerating-app-icon.md).

Full details: [DETAILS.md](DETAILS.md).
