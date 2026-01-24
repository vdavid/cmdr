# ADR 018: Settings architecture

## Status

Accepted

## Summary

The settings system uses a hybrid declarative registry with custom UI components. A central registry defines all
settings metadata (for search, persistence, and defaults), while individual section components render custom UI. Search
uses the same uFuzzy engine as the command palette. A CI check enforces bidirectional completeness between the registry
and UI components. Settings apply immediately without an explicit "Apply" button.

## Context, problem, solution

### Context

Cmdr has configurable values scattered across multiple locations: Rust compile-time constants (`config.rs`), environment
variables, TypeScript stores (`settings-store.ts`, `app-status-store.ts`), CSS custom properties, and hardcoded magic
values. The app needs a unified settings dialog that's easy to search (like IntelliJ's) and easy to maintain as features
are added.

### Problem

1. Users have no UI to discover or change settings — everything requires code knowledge or env vars.
2. As the app grows, configurable values multiply. We need a system that scales without becoming a maintenance burden.
3. Settings must be instantly searchable across all section titles, labels, descriptions, and keywords.
4. We need a way to ensure new features get settings entries and that UI stays in sync with the registry.

Non-goals:
- Generated/schema-driven UI (too rigid, loses per-section UX polish).
- A parser that scrapes component source for searchable text (fragile, drifts on refactors).
- Lint-based detection of "naked constants" (heuristic, false positives — periodic agent audits serve this purpose).

### Possible solutions considered

1. **JSON schema → generated UI**: Consistent and inherently searchable, but loses custom UX per section (color pickers,
   inline previews, conditional visibility). Generated settings UIs always feel generic.
2. **Manual pages + parser script**: Full UX control, but the parser is a second source of truth that drifts. Breaks on
   dynamic labels and refactors.
3. **Full architectural enforcement** (registry as the only runtime API for config values): Strongest guarantee, but
   adds ceremony. Without a lint to catch raw constants, it's just a convention. Overkill for a solo dev + agents
   workflow.

### Solution

**Hybrid declarative registry with custom UI components:**

- A central `settings-registry.ts` defines every setting's metadata: section path, label, description, keywords, type,
  default value, and whether it requires a restart.
- Individual settings section components import their settings from the registry and render custom UI. Labels and
  descriptions come from the registry, so there's no drift.
- Search builds a `searchableText` string per setting (section path + label + description + keywords). The user's query
  runs through uFuzzy (same engine and config as the command palette). The settings tree narrows to show only sections
  with matches, and matched items get character-level highlighting.
- Settings apply immediately on change — no "Apply" button. The rare setting that requires a restart is marked in the
  registry and shows a restart prompt in the UI.
- The settings dialog is a separate Tauri window (not an HTML dialog). ESC closes it.

**Completeness enforcement (registry ↔ UI check):**

- A check in the CI pipeline verifies:
  1. Every setting ID in the registry is referenced by at least one settings UI component.
  2. Every settings UI component only renders settings that exist in the registry.
- Additionally, periodic agent audits sweep the codebase for constants, env vars, and hardcoded values that should be
  exposed as settings but aren't yet registered.

## Consequences

### Positive

- Search works perfectly because the registry IS the search index — no parsing, no scraping, no drift.
- Full UX freedom per section (custom components, conditional visibility, inline previews).
- Single source of truth for: what settings exist, their defaults, their searchable metadata, and their persistence
  keys.
- CI catches missing UI or orphaned registry entries automatically.
- uFuzzy reuse means consistent search behavior across command palette and settings.

### Negative

- Every new setting requires touching two places: the registry entry and the UI component. (Mitigated by the CI check
  catching omissions.)
- The registry file grows as settings accumulate. (Acceptable — it's just data, easy to navigate with sections.)

### Notes

- UI components use Ark UI (see ADR 017).
- Keyboard shortcuts and theme customization are handled as dedicated subsystems with their own UI, not as individual
  registry entries.
- Session state (pane paths, widths, sort orders) remains in `app-status-store.ts` — these aren't user-configured
  "settings" in the traditional sense.
