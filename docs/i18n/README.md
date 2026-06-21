# Per-language translation style guides

Each shippable locale gets a folder at `docs/i18n/<tag>/` (its home: `style.md` plus `glossary.md`), where `<tag>` is
the locale's BCP-47 tag (the same tag as its `apps/desktop/src/lib/intl/messages/<tag>/` catalog dir, e.g. `de`,
`pt-BR`, `en-GB`).

A style guide is the per-language half of the translation context. The other half is per-string and lives in the catalog
(each key's `@key.description`, `placeholders`, and screenshot). The split matters:

- **Per-string context** (in the catalog `@key` metadata): what THIS string means, where it appears, its constraints.
  Never repeated per language.
- **Per-language style** (here): tone, formality, terminology, brand handling, plural notes. Written once per language,
  applied to every string. Never repeated per string.

A translator (human or agent) reads the per-string `@key` context AND this language's style guide together. The full
translation process and the agent prompt that consumes both live in
[`../guides/i18n-translation.md`](../guides/i18n-translation.md).

## Selection roster

Which languages Cmdr plans to localize vs set aside for now:
[language-selection-decisions.md](language-selection-decisions.md).

Formal vs informal address per language, with OS and retail evidence:
[formal-informal-decisions.md](formal-informal-decisions.md).

Which script each digraphic language ships in: [script-decisions.md](script-decisions.md).

Cross-language process learnings discovered while translating (the pipeline, catalog mechanics, pile traps), shared
across batches: [translation-learnings.md](translation-learnings.md).

## Current language guides

One per language (BCP-47 base tag). Each may carry a "Decisions to confirm with David" section, a "Decision points"
section, and a sourced glossary.

- [`ab`](ab/style.md)
- [`af`](af/style.md)
- [`am`](am/style.md)
- [`an`](an/style.md)
- [`ar`](ar/style.md)
- [`as`](as/style.md)
- [`ast`](ast/style.md)
- [`az`](az/style.md)
- [`be`](be/style.md)
- [`bg`](bg/style.md)
- [`bn`](bn/style.md)
- [`bo`](bo/style.md)
- [`br`](br/style.md)
- [`brx`](brx/style.md)
- [`bs`](bs/style.md)
- [`ca`](ca/style.md)
- [`chr`](chr/style.md)
- [`ckb`](ckb/style.md)
- [`crh`](crh/style.md)
- [`cs`](cs/style.md)
- [`cy`](cy/style.md)
- [`da`](da/style.md)
- [`de`](de/style.md)
- [`doi`](doi/style.md)
- [`dz`](dz/style.md)
- [`el`](el/style.md)
- [`en`](en/style.md)
- [`eo`](eo/style.md)
- [`es`](es/style.md)
- [`et`](et/style.md)
- [`eu`](eu/style.md)
- [`fa`](fa/style.md)
- [`ff`](ff/style.md)
- [`fi`](fi/style.md)
- [`fil`](fil/style.md)
- [`fo`](fo/style.md)
- [`fr`](fr/style.md)
- [`fur`](fur/style.md)
- [`fy`](fy/style.md)
- [`ga`](ga/style.md)
- [`gd`](gd/style.md)
- [`gl`](gl/style.md)
- [`gu`](gu/style.md)
- [`guc`](guc/style.md)
- [`gv`](gv/style.md)
- [`ha`](ha/style.md)
- [`he`](he/style.md)
- [`hi`](hi/style.md)
- [`hr`](hr/style.md)
- [`hu`](hu/style.md)
- [`hy`](hy/style.md)
- [`ia`](ia/style.md)
- [`id`](id/style.md)
- [`ie`](ie/style.md)
- [`ig`](ig/style.md)
- [`io`](io/style.md)
- [`is`](is/style.md)
- [`it`](it/style.md)
- [`iu`](iu/style.md)
- [`ja`](ja/style.md)
- [`ka`](ka/style.md)
- [`kab`](kab/style.md)
- [`kk`](kk/style.md)
- [`km`](km/style.md)
- [`kn`](kn/style.md)
- [`ko`](ko/style.md)
- [`kok`](kok/style.md)
- [`ks`](ks/style.md)
- [`ku`](ku/style.md)
- [`ky`](ky/style.md)
- [`lb`](lb/style.md)
- [`li`](li/style.md)
- [`ln`](ln/style.md)
- [`lo`](lo/style.md)
- [`lt`](lt/style.md)
- [`lv`](lv/style.md)
- [`mai`](mai/style.md)
- [`mg`](mg/style.md)
- [`mi`](mi/style.md)
- [`mjw`](mjw/style.md)
- [`mk`](mk/style.md)
- [`ml`](ml/style.md)
- [`mn`](mn/style.md)
- [`mni`](mni/style.md)
- [`mr`](mr/style.md)
- [`ms`](ms/style.md)
- [`mt`](mt/style.md)
- [`my`](my/style.md)
- [`nb`](nb/style.md)
- [`nds`](nds/style.md)
- [`ne`](ne/style.md)
- [`nl`](nl/style.md)
- [`nn`](nn/style.md)
- [`nso`](nso/style.md)
- [`oc`](oc/style.md)
- [`or`](or/style.md)
- [`pa`](pa/style.md)
- [`pl`](pl/style.md)
- [`prs`](prs/style.md)
- [`ps`](ps/style.md)
- [`pt`](pt/style.md)
- [`qut`](qut/style.md)
- [`quz`](quz/style.md)
- [`ro`](ro/style.md)
- [`ru`](ru/style.md)
- [`rw`](rw/style.md)
- [`sa`](sa/style.md)
- [`sat`](sat/style.md)
- [`sd`](sd/style.md)
- [`si`](si/style.md)
- [`sk`](sk/style.md)
- [`sl`](sl/style.md)
- [`so`](so/style.md)
- [`sq`](sq/style.md)
- [`sr`](sr/style.md)
- [`sv`](sv/style.md)
- [`sw`](sw/style.md)
- [`ta`](ta/style.md)
- [`te`](te/style.md)
- [`tg`](tg/style.md)
- [`th`](th/style.md)
- [`ti`](ti/style.md)
- [`tk`](tk/style.md)
- [`tn`](tn/style.md)
- [`tr`](tr/style.md)
- [`tt`](tt/style.md)
- [`ug`](ug/style.md)
- [`uk`](uk/style.md)
- [`ur`](ur/style.md)
- [`uz`](uz/style.md)
- [`vec`](vec/style.md)
- [`vi`](vi/style.md)
- [`wa`](wa/style.md)
- [`wo`](wo/style.md)
- [`xh`](xh/style.md)
- [`yi`](yi/style.md)
- [`yo`](yo/style.md)
- [`zh`](zh/style.md)
- [`zu`](zu/style.md)

## Starting a new language

Copy [`_template/style.md`](_template/style.md) to `<tag>/style.md` and fill it in before the first translation pass.
These files are working notes, not catalog data: they are never loaded by the app and never affect the build.
