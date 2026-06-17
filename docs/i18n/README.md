# Per-language translation style guides

Each shippable locale gets a style guide at `docs/i18n/<tag>-style.md`, where `<tag>` is the locale's BCP-47 tag (the
same tag as its `apps/desktop/src/lib/intl/messages/<tag>/` catalog dir, e.g. `de`, `pt-BR`, `en-GB`).

A style guide is the per-language half of the translation context. The other half is per-string and lives in the catalog
(each key's `@key.description`, `placeholders`, and screenshot). The split matters:

- **Per-string context** (in the catalog `@key` metadata): what THIS string means, where it appears, its constraints.
  Never repeated per language.
- **Per-language style** (here): tone, formality, terminology, brand handling, plural notes. Written once per language,
  applied to every string. Never repeated per string.

A translator (human or agent) reads the per-string `@key` context AND this language's style guide together. The full
translation process and the agent prompt that consumes both live in
[`../guides/i18n-translation.md`](../guides/i18n-translation.md).

## Starting a new language

Copy [`_template-style.md`](_template-style.md) to `<tag>-style.md` and fill it in before the first translation pass.
These files are working notes, not catalog data: they are never loaded by the app and never affect the build.
