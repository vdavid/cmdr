# Albanian (sq) translation style guide

Working notes for translating Cmdr into Albanian. Read [`README.md`](README.md) for how this fits the translation
process, and the app-wide [`/docs/style-guide.md`](../style-guide.md) for the English voice these notes carry into
Albanian.

## Priority and coverage reality (read first)

Albanian is a low-priority target for a macOS app, but it's better-resourced than Somali.

- **Apple does not localize macOS into Albanian.** Albanian isn't among macOS's ~47 system languages (Apple's own
  language list, checked 2026-06-20), so there's no Finder/AppKit reference to mirror. The "prefer the macOS term" rule
  that anchors the Swedish guide has no anchor here.
- **The reference pile is decent for terms even without macOS:** Microsoft terminology (`ALBANIAN.tbx`), the Microsoft
  style guide, GNOME/Nautilus, and Xfce/Thunar are all present (verified against the reference pile, 2026-06-20). So
  Tier-2 (Microsoft) plus Tier-3 (GNOME/Xfce) triangulation is possible. Many terms can reach `high`, unlike Somali.
- **One caveat on the open-source catalogs:** the Albanian Nautilus/Thunar `.po` files are partial community
  translations with mixed quality and some Kosovo-influenced wording (for example Nautilus renders Trash as `Koshi`,
  Thunar renders Side Pane as `Menu Anësore`). Use them as a cross-check, not a primary source; weight Microsoft
  terminology higher (verified against the reference pile, 2026-06-20).

## Voice and tone

Friendly, concise, active, calm. Microsoft's Albanian voice guidance lines up with Cmdr's English voice: "warm and
relaxed", "crisp and clear", "we make it simple above all", short everyday words over formal technical register,
written for scanning first (verified against the reference pile, 2026-06-20). Carry the English voice over directly.

Error messages stay calm and actionable: name the problem and the next step. The MS guide's own sample does exactly
this ("Ky çelës produkti nuk funksionoi. Kontrolloje dhe provoje përsëri." = "This product key didn't work. Check it
and try again."), phrasing the problem and the fix rather than leading with "error" or "failed". Mirror that pattern.

## Formality

- **Address the user as `ti` (informal second person singular), not `ju`.** Microsoft's Albanian guidance addresses the
  user directly with the familiar second person and explicitly avoids the formal/impersonal third person ("user"); its
  samples use `ti` ("Ti zgjedh…") (verified against the reference pile, 2026-06-20). Albanian has a real `ti`/`ju`
  T-V distinction and `ju` is the polite/formal form, so this is a genuine choice, but the MS-backed default for warm
  consumer software is informal `ti`. `high`.
  - Flag for David: if Cmdr's Albanian voice should feel more deferential or business-formal, `ju` is the lever.
    The recommended default is informal `ti`, matching the app's friendly English voice and Microsoft's Albanian
    convention.
- **Buttons and menu items: imperative verb, informal singular.** The MS terminology gives the imperative forms
  directly: "Fshi" (delete), "Anulo" (cancel), "rendit" (sort), "hap" (open), "mbyll" (close). Use the singular
  imperative, consistent with `ti` address.
- **Gendered agreement is unavoidable; keep it consistent.** Albanian nouns and adjectives carry gender, and the chosen
  term's gender drives article and adjective agreement (for example `dosje` folder is feminine, `skedar` file is
  masculine). Keep agreement correct within each string rather than pattern-matching off English. There's no
  gender-neutral dodge as clean as Somali's article trick, so pick natural phrasings that don't force the user's own
  gender.

## Decision points

These are the calls that actually move the needle for Albanian. Each: how the majors handle it, a recommended default,
a confidence, and whether only David can settle it.

- **Formality (`ti` vs `ju`).** Covered under Formality above. Microsoft uses informal `ti`; recommended default is
  `ti`; David's call only if a more formal register is wanted. `high`.

- **Regional variant: Standard (Tosk-based) Albanian, not Gheg.**
  - Standard literary Albanian (`Shqipja standarde`) is Tosk-based and is the written norm across both Albania and
    Kosovo in education, media, and government. Gheg (northern Albania, Kosovo, North Macedonia) is widely spoken but
    not the standard written form. The Microsoft terminology tags every Albanian entry `ALB` (Albania), with no Kosovo
    (`KOS`) split present, so MS targets Albania-standard (verified against the reference pile, 2026-06-20).
  - Recommendation: write **Standard (Tosk-based) Albanian** under the base tag `sq`, serving both Albania and Kosovo.
    Don't translate into Gheg. Watch for Kosovo-flavored wording leaking in from the community GNOME/Xfce catalogs and
    normalize it to the standard term. `high`.
  - Flag for David only if Cmdr ever wants a dedicated Kosovo build (`sq-XK`); not worth it at this priority.

- **Translate vs borrow tech terms.**
  - Albanian is more mature than Somali here: Microsoft terminology supplies native terms for most core nouns and
    verbs, so borrowing is less necessary. But Albanian IT usage does borrow some English/international roots
    (`server`, `volum`, `direktori`, `Tab`), and the MS terminology itself keeps these.
  - Recommended default: **prefer the established Albanian term where Microsoft terminology and the file-manager
    catalogs agree; keep the borrowed/international term where that's what the sources use** (notably `server`, `volum`,
    `Tab`). Don't coin new natives over a settled loan. `high` where sources agree, `tentative` where only the partial
    community catalogs had it.

- **Gender and inclusive language.**
  - Albanian is grammatically gendered (see Formality). There's no neutral pronoun dodge; the practical move is to
    phrase neutrally (imperative verbs, addressing `ti` directly) so a string doesn't have to assume the user's gender.
  - Recommendation: lean on imperatives and direct `ti` address to sidestep gendered third-person references; keep
    noun/adjective agreement internally correct. `high`.

- **Length.**
  - Albanian runs close to English in width, occasionally a bit longer (definite-article suffixes, some longer native
    compounds). Lower overflow risk than German.
  - Recommendation: overflow-check the layout against the pseudolocale (`en-XA`) as for every language; expect roughly
    English-width to mild expansion. `tentative` (no measured Albanian UI corpus).

## Terminology and glossary

Anchors triangulated this round (Microsoft terminology Tier 2 plus GNOME/Xfce Tier 3; verified against the reference
pile, 2026-06-20). Sources read to decide the term, never pasted (MS copyrighted, GNOME/Xfce GPL). Format:
`chosen · sources · confidence`.

- **folder: `dosje`** (feminine) · MS terminology (`dosje`, ALB). `high`.
- **file: `skedar`** (masculine) · MS terminology (`skedar`). `high`.
- **directory: `direktori`** · MS terminology; use only where the technical filesystem sense matters, else `dosje`.
  `high`.
- **pane: `pjesë`** · MS terminology (`pjesë`, the window-region sense). Reads a little generic; confirm against the
  two file lists with a native reviewer. `tentative`.
- **tab: `Tab`** (borrowed) · MS terminology keeps `Tab`. `high`.
- **volume: `volum`** · MS terminology (`volum`). A mounted disk volume. `high`.
- **drive: `njësia e diskut`** · MS terminology. `high`.
- **trash: `shportë` / `shporta e riciklimit`** · MS terminology (`shporta e riciklimit`); Nautilus community uses
  `Koshi`. Prefer the MS `shportë`-based term; flag exact short label with a native reviewer. `tentative` (sources
  differ).
- **delete: `Fshi`** (imperative) · MS terminology. `high`.
- **rename: `riemërto`** · Nautilus community (`Riemërto`); MS terminology had no clean hit. `tentative`.
- **cancel: `Anulo`** · MS terminology (`Anulo`); Nautilus community uses `Anullo` (double-l, nonstandard). Use
  `Anulo`. `high`.
- **sort: `rendit`** · MS terminology (`rendit`). `high`.
- **search: `kërkim` (noun) / `kërko` (verb)** · MS terminology (`kërkim`). `high`.
- **settings: `cilësimet`** · MS terminology (`cilësimet`). `high`.
- **server: `server`** · MS terminology (`server`); Nautilus "Connect to Server" → `Lidhu me server-in`. `high`.
- **download: `shkarkim` (noun) / `shkarko` (verb)** · MS terminology (`shkarkim`). `high`.
- **disconnect: `shkëput`** · MS terminology (`shkëput`). `high`.
- **share (network): `bashkëndaj` (verb)** · MS terminology (`bashkëndaj`); for the SMB share noun a native reviewer
  should confirm the natural "shared folder" phrasing. `tentative`.
- **view: `pamje`** · MS terminology (`pamje`); list view → `Listë`. `high`.
- **preview / viewer: `paraafishim`** · MS terminology (`paraafishim`, "preview"). Cmdr's file viewer is a preview
  surface, so `paraafishim` fits; confirm the exact viewer noun with a native reviewer. `tentative`.
- **bookmark: `shëno si referim` / `referim`** · MS terminology (`shëno si referim`). Reads long for a short label;
  flag a shorter native label with a reviewer. `tentative`.
- **eject / overwrite:** no clean source hit in the pile this round; defer to a native reviewer. `tentative`.

Add terms in the same `chosen · sources · confidence` shape; keep the catalog consistent.

## Brand and do-not-translate

Keep verbatim: Cmdr, macOS, GitHub, SMB, MTP, Tauri, Rust, Svelte, Quick Look, plus the `{system_settings}`-style
tokens. The curated list is enforced by `desktop-i18n-dont-translate`; see `apps/desktop/scripts/i18n-catalog-lib.js`.

## Plurals

CLDR categories for `sq`: `one`, `other` (the Albanian Nautilus catalog declares `nplurals=2; plural=(n != 1)`,
verified against the reference pile, 2026-06-20; confirm with `new Intl.PluralRules('sq').resolvedOptions()`). Write
both branches. Keep noun and adjective gender agreement correct inside each branch. Numbers and dates come from the
formatter layer (`formatNumber()` / `formatBytes()`); never hardcode separators.

## Notes and decisions

- **Sentence case is native-friendly.** Albanian doesn't title-case common nouns; the app's sentence-case rule applies.
  The MS guide notes English over-capitalizes and Albanian should not follow it. Don't title-case.
- **Albanian-specific letters** `ë` and `ç` are everyday letters, not optional diacritics; always write them (for
  example `dosje`, `cilësimet`, `paraafishim`). Dropping them is a spelling error.
- **Punctuation:** the MS guide reserves the em dash for emphasis/distance-and-time ranges and uses the en dash as a
  minus sign in direct speech. Cmdr's house rule already bans em dashes, so this doesn't bite; use en dashes for ranges
  only.
- Record any case-by-case rulings here so they aren't relitigated.
