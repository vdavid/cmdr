# Feature status

`feature-status.json` at the repo root is the single source of truth for per-feature stability. The website and the
desktop app both read it, so a status change in one place updates every surface.

## Schema

```json
{
  "features": [
    {
      "id": "search",
      "name": "Search",
      "status": "alpha",
      "note": "One honest line about where the feature stands.",
      "issueUrl": "https://github.com/vdavid/cmdr/issues/123"
    }
  ]
}
```

- `id`: stable kebab-case identifier. Code references it (`getFeatureStatus('search')`), so renaming an id means
  updating every consumer.
- `name`: human-facing feature name, sentence case.
- `status`: one of the four values below.
- `note`: one honest line, written for users. David reviews every note.
- `issueUrl` (optional): a GitHub issue tracking the feature's rough edges or plan.

## Status semantics

- `alpha`: works, but expect bugs and rough edges. Gets an uppercase ALPHA badge in the app (dialog titles, command
  palette) and on the website.
- `beta`: solid for common cases; unusual setups can still surprise. Badge on the website; in-app badges only where a
  feature surface warrants it.
- `stable`: we stand behind it. **Never** gets a badge, anywhere. Listed on the website's feature status page only.
- `planned`: not built yet. Website only ("Coming soon"); the app never references planned features.

The whole app is in open beta; these statuses are relative to that baseline. "Stable" means stable within the open beta,
not "1.0 frozen".

## Consumers

- **Website** (`apps/website/`):
  - `src/lib/feature-status.ts`: typed loader + the status → human label map.
  - `src/pages/feature-status.astro`: the full per-feature status page, grouped by status.
  - `src/components/Features.astro` and `src/pages/features.astro`: badge labels on feature cards come from the JSON (no
    hardcoded "Coming soon" strings).
- **Desktop app** (`apps/desktop/`):
  - `src/lib/feature-status.ts`: `getFeatureStatus(id)` + `getBadgeStatus(id)` (alpha/beta only; stable and planned
    return no badge).
  - `src/lib/ui/StatusBadge.svelte`: the uppercase pill that renders the badge.
  - Wired into the Search dialog title, the Selection dialog title, and the matching command palette rows.

## Updating a status

Edit `feature-status.json`, then run `pnpm check` (the desktop unit tests pin the JSON shape and the alpha set). The
website picks the change up at build time; the app at compile time. No code changes needed for a status flip unless a
feature graduates to `stable` while a dialog hardcodes a badge lookup for it (it doesn't today; lookups go through
`getBadgeStatus`, which returns nothing for stable).
