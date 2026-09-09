# Translator glossaries as data, with the markdown generated

**Status:** proposed, not started. Needs David's go-ahead before any migration work.

## The problem

`docs/i18n/<locale>/glossary.md` and `style.md` are 28,263 lines of hand-maintained prose across 13 locales
(`nl` 3,065, `fr` 3,047, `hu` 2,947, `es` 2,810). Only agents read them. Every load-bearing fact in them is really a
typed field wearing prose: a term, its translation, the catalog keys it governs, the shipped English and translated
values, the evidence, a confidence, and whether a later decision superseded it.

Because those fields are prose, nothing can check them, and three classes of rot have already happened:

1. **Dead citations.** A citation names a catalog key that no longer exists, or never existed. 41 found: 8 invented by
   an agent, 33 orphaned by the renames in `c33a47512` and `181ebf6ee`. `desktop-i18n-doc-citations` now catches these.
2. **Value drift.** The key exists, but the value quoted beside it is not what the app ships. 20 locations across 14
   term families. No check can catch this today, because the quote is free prose and legitimate elision is common.
3. **Self-contradiction.** The guides are append-only: a new decision gets a new dated section, and the old entry gets a
   prose "SUPERSEDED" note. Both copies survive, and they disagree.

Class 3 is the one that shows the structure is wrong, so it is worth seeing in full.

## The current shape

From `docs/i18n/hu/glossary.md:366`, one bullet, reflowed here but otherwise verbatim:

```
- queue (the transfer queue) → `sor` (`átviteli sor` = transfer queue) · double-commander (operations viewer `Queue` =
  `Sor`, `New queue` = `Új sor`), ms (`várólista`/`várakozási sor`) · high. DC's file-manager-native `Sor` beats MS's
  generic `várólista`. **SUPERSEDED for the window's NAME as of 2026-08-08** (English renamed it "Operation queue"; it
  is now `Műveleti sor` — see the dated block at the end of this file). The head `queue → sor` below still stands; only
  the `átviteli` modifier is retired. Window title `queue.windowTitle` = `Átviteli sor`; the command
  `commands.queueShow.label` = `Átviteli sor megjelenítése`; empty state "Nothing in the queue" = `A sor üres`.
```

Read the last two lines against the middle two. The bullet announces that `Átviteli sor` was retired on 2026-08-08, and
then, four lines later in the same bullet, prescribes `queue.windowTitle` = `Átviteli sor`. The app ships
`Műveleti sor`. A translator who reads to the end of the bullet copies the value the same bullet already told them was
dead, and `desktop-i18n-term-consistency` raises nothing, because both forms are plausible Hungarian.

The correction sits three lines from the error. No amount of care fixes this, because the format lets one entry hold two
states at once, and it invites the "SUPERSEDED, see the dated block at the end" pointer that duplicates a fact instead
of moving it.

This also puts the guides in breach of two rules the repo already states: `AGENTS.md` § Docs ("a load-bearing claim
lives in exactly ONE canonical doc ... everywhere else points to it by path, never restates it") and the user-level
`describe-current-not-history` rule.

## The new shape

One row per term per locale, as data. Same entry:

```yaml
- term: queue
  gloss: the operation queue
  translation: sor
  modifiers:
    # `átviteli sor` was retired 2026-08-08 when English renamed the window to "Operation queue".
    - form: műveleti sor
      governs: [queue.windowTitle, commands.queueShow.label]
  confidence: high
  rationale: >
    Double Commander's file-manager-native `Sor` beats Microsoft's generic `várólista`.
  evidence:
    - source: double-commander
      cite: "operations viewer `Queue` = `Sor`, `New queue` = `Új sor`"
    - source: microsoft-tbx
      cite: "várólista, várakozási sor"
  governs:
    - queue.windowTitle
    - commands.queueShow.label
    - queue.empty
```

The generated markdown then reads roughly:

```
- queue (the operation queue) → `sor` · high
  Double Commander's file-manager-native `Sor` beats Microsoft's generic `várólista`.
  Evidence: double-commander (operations viewer `Queue` = `Sor`, `New queue` = `Új sor`), ms (`várólista`,
  `várakozási sor`).
  Governs: `queue.windowTitle` = `Műveleti sor` · `commands.queueShow.label` = `Műveleti sor megjelenítése` ·
  `queue.empty` = `A sor üres`
```

## Why this fixes the problem rather than cleaning it up

The single decisive change is in that last generated line: **the shipped values are not stored, they are read from the
catalogs at render time.** Everything else follows.

- **Value drift becomes unrepresentable.** There is no second copy of `Műveleti sor` to fall out of date, because the
  guide never stored one. Class 2 stops being a thing we fix and starts being a thing that cannot occur. This is the
  whole argument; the rest is bonus.
- **Citation checking loses its heuristics.** `governs:` is a typed list of keys, so verifying it is a set lookup.
  `desktop-i18n-doc-citations` currently needs a backtick scanner, a namespace allowlist derived from catalog
  basenames, and exact/prefix/suffix/infix segment matching, all to recover a type that prose threw away. Without the
  gate the naive scan fires 1,924 times to find 10 real problems. Against `governs:` the false-positive rate is zero by
  construction, and the check shrinks to a few lines.
- **An entry cannot contradict itself.** A term has one current translation. A retired form is a comment or a dated
  note on the row that replaced it, and it has no `governs:` list of its own, so it cannot prescribe anything. The
  Hungarian bullet above is not expressible.
- **Supersession stops duplicating.** Today a new decision writes a dated section AND edits the old bullet, so the fact
  lives twice. In the new shape you edit the row. Git holds the history, which is what
  `describe-current-not-history` asks for.
- **Renames become mechanical.** A key rename can rewrite `governs:` lists directly. Today it silently rots 33
  citations and someone reconstructs them from `git log -S` days later.
- **Cross-locale questions become queryable.** "Which locales still translate *dismiss* as an ignore-word?" is a filter
  over rows. Today it is 13 greps and a judgment call per hit.

Two smaller wins worth naming: the reverse index (which terms govern a given key) is free, and per-locale coverage
("terms with no `governs:` entries" and "high-traffic keys no term governs") becomes a report rather than a guess.

## What this does not fix

- The prose rationale stays prose, and it can still go stale. It is genuinely unstructured, and that is fine; the
  rationale is the part a human would want to read.
- Nothing here judges translation quality. `desktop-i18n-term-consistency` still owns that.
- The evidence citations (Microsoft TBX ids, macOS bundle paths) point outside the repo and stay unverifiable. Trap 4
  in `docs/i18n/how-to-mine.md` (the first TBX hit is often the wrong sense) is a human problem, not a schema problem.

## Cost, honestly

This is the expensive option. 28,263 lines migrate, and the migration cannot be fully mechanical, because the current
format is not consistent enough to parse reliably. Realistically it is a per-locale pass, one agent per locale, with the
dated append-only sections needing the most judgment: each has to be folded into the row it supersedes and then deleted,
which is where the self-contradictions get resolved and where mistakes would be easy to make.

The payoff is tooling quality, not product. No user sees it. It is worth doing if the guides keep costing us, and it is
not worth doing on the strength of one bad week.

## Suggested sequencing, if it goes ahead

1. Design the schema against three locales that stress it differently: `hu` (heavy dated-section accretion), `zh-Hant`
   (a live supersession chain), `en-GB` (an overlay, so most terms govern nothing).
2. Build the renderer and the checks, and land them alongside the existing markdown so both shapes coexist.
3. Migrate one locale end to end, and only then judge whether the schema survived contact.
4. Migrate the rest, deleting each `glossary.md` as its data lands.
5. Retire the heuristic half of `desktop-i18n-doc-citations`.

Step 3 is the real go or no-go. If one locale takes appreciably longer than a day, stop: the schema is wrong, or the
prose is less regular than it looks.

## Smaller alternatives, for comparison

- **Do nothing structural.** `desktop-i18n-doc-citations` catches class 1, the drift pass fixes the 20 known class-2
  cases by hand, and class 3 stays. Cheapest, and the rot resumes.
- **Fix the citation format only.** Require the key, the English value, and the translated value in one fixed slot
  rather than free prose, so a check can verify all three. This catches classes 1 and 2 without a migration, and it is
  the sensible middle option. It does not fix class 3, and it still stores a value that has to be maintained, so it
  buys detection where the full migration buys impossibility.
