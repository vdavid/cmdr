# Search dialog fix-up: round 3 brief

Rounds 1+2 landed. David ran the app again and has a tight round 3 list. TDD discipline mandatory.

## David's verbatim feedback

> Better! It looks like this: [screenshot in r3 conversation]
>
> Follow-up:
>
> - Fix P6 properly (whatever P6 is, I never gave that name to any of my requests)
> - The search button text says "⏎Search⏎". Wtf? Needs to be "Search ⏎". Also there needs to be more space between
>   "Search" and "⏎". It looks crowded. Match the rest of the labels on this screen
> - The "Recent searches" list should also have some dynamic love: "Recent searches: " on the left side and "All
>   searches..." on the right side must always be visible, and in the rest of the available space, only as many recent
>   searches must be displayed as many fit.
> - In Recent searches, some expressions don't fit, and they are abbreviated by ellipses. Whereever this happens, the
>   tooltip must contain the full string.
> - Bug: when I enter and a filename or regex search manually, then an AI search, then the pattern from the AI search
>   doesn't overwrite my matching previous search. It must overwrite it. E.g. if the LLM searches for `*test*` then my
>   last filename search text should be overwritten with `*test*`.
> - From the filters, when I click Size, then it looks great, but the Custom field should be inside the option, not
>   under it, so the user can click into the text field right away, and type away. Saves the user one click.
> - In the "Modified" filter, there are several things that are wrong:
>   - Custom is capitalized, the rest are not. I say neither should be capitalized.
>   - "this week" / "last week" etc. sound and work weird. Like, when I select "after last week", it filters for files
>     since last Monday, which is the beginning of the last week, definitely not after it. The items should be "today
>     0:00, yesterday 0:00, this Monday 0:00, last Monday 0:00 (if we can make the first-day-of-week dynamic based on
>     system settings, even nicer!), 1st of May 0:00 (dynamic for current month), 1st of April, 2026, 0:00 (for last
>     month, always write the year too), and instead of "this year", "1st of January, 2026, 0:00". Remove this option if
>     this month or last month is Jan.
>   - "Custom..." is always selected, and I think it's because when I click any other item, Custom's value updates. The
>     update itself is a nice touch, just let's make the logic for selection: "Only one item must be selected. If any
>     other item than Custom is selected, Custom must not be selected"
>   - When it's set to "any" and I click any of the disabled items, I want the selector to move to "after" and the
>     disabled items (and the filter) to enable. Same for the Size filter, btw; if it's set to `any`, and I click a
>     number or any unit, set the operator to `>=`.
> - In filters > search in, rename "Hide system folders" to "Hide boring folders". In the tooltip, list ALL of them,
>   don't just say "30 more". How would the user know what that 30 are? Render them as a nice list.
> - Nit: inside the Side filter editor, we correctly write "kB" if SI decimal is selected, but for the filter chip, we
>   incorrectly write "KB".
> - Result list
>   - The paths are still abbreviated too aggressively: I'm seeing `~/{..}/apps/desktop`. There is NO WAY that some of
>     the `projects-git/vdavid/cmdr/.claude/worktrees/search-fixup` didn't fit. Actually, with the current font, ALL of
>     it would fit. I think it's because the agent optimized for the minimum possible width of the window. But that's
>     not correct; this should be dynamically rendered based on current width. UPDATE: I see that first, the full path
>     renders, then it gradually wraps back to ellipses. I think the intention is right, then, just we have some bug in
>     place. If we have an E2E test on this, it probably passes now because the initial state is probably correct. A
>     100ms wait (don't use more) would probably solve this and the test would fail. But in any case, this just needs to
>     be fixed.
>   - That said, the font is too small for the path. Use the same font size as for the rest of the line. If that would
>     stretch the rows too much vertically then decrease the paddings and just vertically center all the text. Keep the
>     row height the same as it is now, while increasing that font size.
> - Results in pane
>   - When I press ⌥⏎ in the search window, the hits open nicely, but the pane is not active so can't navigate without
>     an extra click on the pane. Not nice.
>   - The volume selector says the search term, and the path is empty. The volume selector should say "Search results",
>     and the path should contain the search term (or the AI-gen title). I asked this before.
>
> Run an agent again, and again, include my prompt verbatim. Use TDD wherever reasonable, keep docs updated, make sure
> to pass the style guide and design guidelines and principles to the agent as I asked before, and make sure great UX is
> kept in mind.

## Resolved clarifications

1. **Path column font**: use `--font-size-sm` (same as the filename / Name column). Row height stays at its current
   value; decrease vertical padding; vertically center all cells in the row.
2. **First day of week**: use `new Intl.Locale(navigator.language).weekInfo?.firstDay` (a number 1–7 where 1 = Monday)
   with a Monday fallback. Available in WebKit since 2024. No IPC needed.
3. **Year-start option redundancy rule**: the "1st of January, {year} 0:00" option is OMITTED whenever it would coincide
   with another option already in the list. Specifically: drop it when (a) current month is January (then "1st of this
   month" already covers it), OR (b) last month is January (then "1st of last month" already covers it).
4. **"P6"**: David never used this label. It was my internal mapping to track the 17 items in round 2. The relevant item
   is the ⌘A off-by-one in the snapshot pane. Round 2's "fix" gated `hasParent: !isSearchResultsView` with no test ("no
   test infra in repo for FilePane unit tests"). "Fix properly" means: add a real regression test. Pick the cheaper
   option of (a) a Vitest mount of the snapshot pane with a fixture, or (b) a Playwright spec that exercises ⌘A on a
   real snapshot pane in the running binary. Don't ship without a test.

## Process constraints

- **TDD where reasonable.** Failing test first, then fix, then green.
- **Do NOT commit.** Leave changes uncommitted in the worktree; David reviews.
- **No em-dashes anywhere.** Sentence case. Tokens for radius/font/z-index. No hardcoded colors.
- Update colocated CLAUDE.mds as part of each fix.
- Test via MCP. Dev app is running on `localhost:19225` (cmdr) and `localhost:9223` (tauri). Frontend hot-reloads.
- Same high bar as the rest of the redesign.

## Issue catalog (use this as a test checklist)

Each item needs (a) a failing test that reproduces it, (b) a fix, (c) the test passing. The previous agent in part B
successfully delivered all 17 items of round 2 this way — match that bar.

### Bugs / regressions

- B1. **`⏎Search⏎` double-hint on the bar's Search button.** Today the run button renders the keyboard hint twice (once
  via the tooltip-shortcut slot, once via the label suffix, plausibly). Render it exactly once, with a proper
  `--spacing-xs` (or larger) gap between `Search` and `⏎`. Match the spacing/styling of the other shortcut-suffix labels
  on the dialog (e.g. `Go to file ⏎`, `All searches… ⌘H`).
- B2. **AI search doesn't overwrite the matching hand-typed buffer.** Round 2 introduced per-mode buffers
  (`handTyped.ai|filename|regex`). They were too protective: when AI runs and produces a glob, it should OVERWRITE
  `handTyped.filename` (because the user just told the AI to take over). Same for regex. Update `recordAiTranslation()`
  (or wherever the buffer write lives) to overwrite the matching-kind buffer with the AI-produced pattern.
- B3. **Filter chip writes `KB` instead of `kB` when SI is selected.** The popover renders `kB` correctly (round 2 D10),
  but the chip text rendered on the filter strip uses uppercase regardless. Read `appearance.fileSizeFormat` and pick
  the right label everywhere.
- B4. **Path collapse runs even when there's room.** David: "first, the full path renders, then it gradually wraps back
  to ellipses." Likely the `ResizeObserver` measurement runs before fonts/layout settle, so the first measurement
  under-reports the container width. Re-measure after `requestAnimationFrame` or use a `resize`/`fontsloaded` event. Add
  a Vitest that simulates the racy initial measurement (the brief explicitly mentions 100ms — write a test with a fake
  timer that flushes RAF after 100ms and asserts the layout has settled to "no collapse needed").
- B5. **Custom option in Modified popover is always co-selected.** Today selecting any preset writes the resolved date
  into the Custom input, which then renders Custom as selected. Fix the selection model: exactly one preset is selected
  at a time; Custom is only selected if the user explicitly clicked it (or has typed into the Custom input without
  clicking a preset).
- B6. **Volume selector / breadcrumb are inverted in the snapshot pane.** David: "The volume selector says the search
  term, and the path is empty. The volume selector should say 'Search results', and the path should contain the search
  term (or the AI-gen title)." Find the responsible component (likely `VolumeBreadcrumb.svelte` or its data prop in
  `FilePane.svelte`) and swap. Reference: round 1 brief §6 already specified this; the implementation must have drifted.

### Visible UX fixes

- U1. **Recent searches strip has dynamic layout.** Today it scrolls. New layout:
  - Left edge: `Recent searches:` label, always rendered.
  - Right edge: `All searches… ⌘H` button, always rendered.
  - Middle: pack as many recent-search chips as fit in the remaining width. Drop overflowing chips silently (no scroll,
    no ellipsis on the strip itself). Re-measure on resize.
  - Use `ResizeObserver` + a simple greedy fit (each chip's width measurable by pretext OR via DOM after first render).
    Tests: mock the strip width and assert N chips fit, the rest are dropped.
- U2. **Truncated recent-search chips get a tooltip with the full string.** Today chips truncate via CSS
  `text-overflow: ellipsis` (or similar) without a tooltip. Wire the existing tooltip primitive: when the chip's
  rendered text width < its full text width, attach a tooltip containing the full text. Use `overflowOnly: true` on the
  tooltip helper if supported, otherwise compute it.
- U3. **Size > Custom**: the text input is currently rendered BELOW the option. Move it INSIDE the Custom row so
  clicking it focuses the input and the user can type immediately. One click saved.
- U4. **Modified preset labels rewrite.** Current strings (`today`, `yesterday`, `this week`, `last week`, `this month`,
  `last month`, `this year`, `Custom...`) become:
  - `today 0:00`
  - `yesterday 0:00`
  - `this Monday 0:00` (or whatever the first-day-of-week resolves to, e.g. `this Sunday 0:00` for US locales)
  - `last Monday 0:00` (likewise)
  - `1st of May 0:00` (dynamic to current month; localize the month name)
  - `1st of April, 2026, 0:00` (dynamic to last month; always include the year)
  - `1st of January, 2026, 0:00` (year start; ONLY rendered when not redundant per clarification 3)
  - `custom…` (lowercase, ellipsis)

  All-lowercase rule: weekdays and months stay capitalized (proper nouns), but `custom…` is lowercase. The leading word
  ("today", "yesterday", "this", "last", "1st") is lowercase too.

  The Modified selection encodes the absolute date: "after this Monday" resolves to a specific timestamp at click time
  (not a rolling reference). Then the search runs against that timestamp. Document this in the test.

- U5. **`any` + clicked-on-disabled-cell auto-promotes the operator.** Size: if col 1 = `any` and the user clicks a
  number in col 2 or a unit in col 3, switch col 1 to `>=` and enable col 2/3 with the clicked value selected. Modified:
  same, switch to `after`. (Don't fire the search; the user can still adjust before hitting Enter.)
- U6. **Search in: "Hide system folders" -> "Hide boring folders".** Both the toggle label and the tooltip prose. The
  tooltip currently shows a small subset of the system-dir list plus "+30 more". Render the FULL list (read via the
  existing `get_system_dir_excludes` IPC) in a clean list (use the existing tooltip-rich-html slot; one folder per line;
  `--font-mono` for the folder names). No truncation.
- U7. **Path column font + row layout.** Use `--font-size-sm` for the path (match the filename). Decrease vertical row
  padding to compensate; vertically center all cells. Keep the row height pixel value the same as today.
- U8. **Pane focus on `⌥⏎`.** When `Show all in main window` lands in a pane, the pane should receive focus (so the user
  can immediately navigate, select, etc.) without an extra click. Find the right place in `+page.svelte` or
  `DualPaneExplorer.svelte` (probably `handleOpenSearchInPane`) and call the focus method on the target pane after
  navigation.

### "P6" follow-up

- T1. **⌘A in the snapshot pane: real regression test.** Round 2's fix worked but had no test (`hasParent` gating). Add
  a regression test. Either:
  - Vitest: mount `FilePane` (or a small focused harness) with a snapshot fixture, dispatch `⌘A` keydown, assert that
    index 0 IS in the selected set.
  - Playwright: a new spec or an extension of `search-open-in-pane.spec.ts` that opens a search, opens-in-pane, presses
    `⌘A`, and asserts via MCP `cmdr://state` that the first row is selected.

  Vitest preferred if a mount harness can be built cheaply; otherwise Playwright. Don't ship without one of these.

## Playwright builds are allowed

The `playwright-e2e`-flagged Rust build takes ~1 minute (David confirmed). Running it is fine if T1's regression test
ends up being a Playwright spec, or if any other E2E coverage is needed. Build command from `apps/desktop/`:

```
node scripts/tauri-wrapper.js build --no-bundle --target $(rustc -vV | grep host | cut -d' ' -f2) -- --features playwright-e2e,virtual-mtp,smb-e2e
```

Binary lands at `<repo>/target/<triple>/release/Cmdr`.

## TDD playbook (mandatory)

For each item:

1. Write the failing test first. Comment the line that explains WHY it should fail today.
2. Run it scoped: `cd apps/desktop && pnpm vitest run path/to/test --reporter=verbose 2>&1 | tail -40`. Confirm red.
3. Implement the fix.
4. Run the same test, confirm green.
5. Run the surrounding file's whole suite to catch regressions.

For the "no test infra" excuse on T1: try the Vitest mount harness once seriously before bailing to Playwright. Look at
`FilePane.test.ts` (if it exists) or any sibling test for a starting point. If it really can't be mounted, write a
Playwright spec; the search-spec suite already has one (`search-open-in-pane.spec.ts`) that mounts a snapshot pane and
presses keys. Extend it.

## Hard rules (re-stated)

- ❌ NEVER `git stash`, `git push`, `git checkout`, `git commit`, `--no-verify`.
- ❌ No em-dashes anywhere. Use colons, parens, sentences.
- ❌ No raw `px` on font-size, border-radius, font-family, z-index ≥ 10. Use tokens.
- ❌ No hardcoded colors. Use `--color-*` vars.
- ❌ No `head` / `tail` / `2>&1 |` truncation on `./scripts/check.sh`.
- ❌ No bailing early. If genuinely blocked, document the EXACT blocker.
- ❌ No "this is too big for one session" framing. Take all the time and context you need.
- ✅ TDD for each item.
- ✅ Use existing utilities (tooltip primitive, `useShortenMiddle`, `createPretextMeasure`, etc.).
- ✅ Update colocated CLAUDE.mds AS PART of each fix.
- ✅ Verify via MCP screenshots to `/tmp/r3-*.png` per visible change.
- ✅ At the end: `./scripts/check.sh` (default lane), full output read.

## Final report

Match the format of round 2's final report. Per-item table with red->green status, TDD evidence, MCP screenshots,
check.sh result, self-review, `git diff HEAD --stat`.
