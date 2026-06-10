# Feature status

`feature-status.json` at the repo root is the single source of truth for per-feature stability. The website and the
desktop app both read it, so a status change in one place updates every surface.

## Schema

```json
{
  "statusDefinitions": {
    "alpha": "Fresh feature. Should work. Might be broken."
  },
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
- `statusDefinitions`: the canonical user-facing explanation per status. Rendered as the badge tooltip in the app and
  the pill tooltip on the website, so the two surfaces can't drift. Per the website voice rule (style guide § Voice),
  keep them free of "I"/"we".

## Status semantics

The user-facing definitions live in the JSON's `statusDefinitions` (alpha: fresh, might be broken; beta: works, smaller
bugs; stable: well-tested, mature; planned: not built yet). Surface rules:

- `alpha` / `beta`: uppercase badge in the app (dialog titles, command palette rows) and a pill on the website.
- `stable`: **no badge in the app** (`getBadgeStatus` returns nothing). The website DOES show a quiet "Stable" pill on
  feature cards and in the `/features#status` list.
- `planned`: website only; the app never references planned features.

The whole app is in open beta; these statuses are relative to that baseline. "Stable" means stable within the open beta,
not "1.0 frozen".

## Consumers

- **Website** (`apps/website/`):
  - `src/lib/feature-status.ts`: typed loader + pill labels + `getStatusTooltip`.
  - `src/components/StatusPill.astro`: the shared pill (label + definition tooltip), used on the homepage feature cards
    and the features page.
  - `src/pages/features.astro`: rich feature sections with pills, plus the compact full list at `#status`. The old
    `/feature-status` page merged into it; `astro.config.mjs` redirects the old URL.
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
