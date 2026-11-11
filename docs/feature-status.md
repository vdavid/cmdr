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
      "issueUrl": "https://github.com/getcmdr/cmdr/issues/123"
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
  feature cards on `/features`.
- `planned`: website only; the app never references planned features.

The whole app is in open beta; these statuses are relative to that baseline. "Stable" means stable within the open beta,
not "1.0 frozen".

## Consumers

- **Website** (`apps/website/`):
  - `src/lib/feature-status.ts`: typed loader + pill labels + `getStatusTooltip`.
  - `src/components/StatusPill.astro`: the shared pill (label + definition tooltip), used on the homepage feature cards
    and the features page.
  - `src/pages/features.astro`: the feature tour, one chapter per area with its product clips, then that area's cards
    (name, note, pill) from `FeatureCards.astro`. Which feature sits in which chapter is
    `apps/website/src/lib/features-page.ts`, whose build-time check fails on a feature that's missing, unknown,
    duplicated, or planned outside the "Coming next" chapter (so a `planned` → `alpha` flip means moving its card). The
    old `/feature-status` page merged into it; `astro.config.mjs` redirects the old URL.
- **Desktop app** (`apps/desktop/`):
  - `src/lib/feature-status.ts`: `getFeatureStatus(id)` + `getBadgeStatus(id)` (alpha/beta only; stable and planned
    return no badge).
  - `src/lib/ui/StatusBadge.svelte`: the uppercase pill that renders the badge.
  - Wired into the Search, Selection, Operation log, and Multi-rename dialog titles, the Ask Cmdr rail and settings
    section, the Image search settings card, and the matching command palette rows.

## Graduation criteria

Internal bar for moving a feature up, checked at each release review (`docs/guides/releasing.md` step 5). Agent-facing;
users only see `statusDefinitions`.

- **Alpha → beta**: all of these.
  - Shipped for at least four weeks.
  - Used by real installs outside David's.
  - No open serious or data-loss bug in its area.
  - No data-loss fix for it in the last two releases.
- **Beta → stable**: all of these.
  - Its main paths work and are tested, with E2E covering the happy paths.
  - No open serious or data-loss bug in its area.
  - The remaining fixes are edge cases.
  - Install counts are evidence when we have them, never a gate.
- **Owner override**: David may graduate a feature on his own long-term testing. Record it as an override (in the commit
  message). SMB (`network-drives`) and MTP (`android-devices`) went stable this way, 2026-11-11.
- **Evidence to read at review**:
  - The "Feature usage" section of the dashboard's `/api/report`: distinct installs per feature over 30 days
    (`docs/tooling/analytics-dashboard.md`).
  - "Fixed" entries per feature in the last ~8 `CHANGELOG.md` releases. A steady stream of non-edge-case fixes means
    it's not ready.
  - Open `bug` issues by area label (`docs/tooling/backlog-board.md`).
- **Refresh the note on every status change.** Notes like "brand new" or "not yet tested against many servers" rot the
  moment the pill moves; rewrite the note to match the new pill in the same edit.

## Updating a status

Every release is a review point: the release procedure (`docs/guides/releasing.md`, step 5) walks this file against what
shipped, flipping `planned` → `alpha`, graduating maturing features, and refreshing stale notes, alongside the roadmap
update. Do it then, or any time a status genuinely changes. How many installs use each feature: the "Feature usage"
section of the dashboard's `/api/report` (`docs/tooling/analytics-dashboard.md`).

Edit `feature-status.json`, then run `pnpm check svelte-tests` (`apps/desktop/src/lib/feature-status.test.ts` pins the
JSON shape and the statuses of every feature that wears an in-app badge) and `pnpm check website-build` (the
`features-page.ts` chapter check). The website picks the change up at build time; the app at compile time. No component
changes are needed for a status flip, since every lookup goes through `getBadgeStatus`, but tests that assert a rendered
badge change with it: `OperationLogDialog.test.ts` and the E2E specs `ask-cmdr.spec.ts` and `operation-log.spec.ts`.
