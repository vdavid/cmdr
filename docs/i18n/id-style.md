# Indonesian (id) translation style guide

Working notes for translating Cmdr into Indonesian (Bahasa Indonesia). Read [`README.md`](README.md) for how this fits
the translation process, and the app-wide [`/docs/style-guide.md`](../style-guide.md) for the English voice.

## Voice and tone

Friendly, concise, active, calm. Indonesian UI copy is naturally polite-neutral; no special warmth markers needed.
Error messages stay calm and actionable and avoid a bare "Galat"/"Gagal" label: state the problem and a next step
("Tidak dapat mengganti nama file. Coba lagi?"). macOS Indonesian uses "Tidak dapat…" ("cannot…") for this and it reads
calm, so prefer that pattern over "Gagal".

## Formality

**Indonesian UI has no T-V pronoun split to resolve, and the macOS convention is to avoid the second-person pronoun
entirely.** macOS Indonesian phrases actions impersonally with verb prefixes, not "Anda" (formal you) or "kamu"
(informal): "Memindahkan item ke Tong Sampah" (the verb does the work). Use this register.

- Buttons and menu items: bare imperative verb stem, often without the meN- prefix: "Salin" (copy), "Pindahkan" (move),
  "Hapus" (delete), "Ubah nama" (rename), "Buka" (open), "Cari" (search), "Batalkan" (cancel). This matches macOS Finder.
- Where direct address is unavoidable, use "Anda" (capital A, polite-neutral), never "kamu". But avoid needing it.
- Affixation is the real subtlety, not formality: "pindah" (move, intransitive) vs "pindahkan" (move something). macOS
  uses the transitive "Pindahkan" for the action on files; follow that. Get the prefix/suffix right per verb.

## Decision points

### Single national standard, no regional split
- Indonesian is one standardized language (Bahasa Indonesia); do NOT confuse with Malay (ms/Bahasa Melayu), which is a
  separate locale with different vocabulary ("folder" vs "folder", but "search"=cari vs cari, "file"=fail in Malay).
- Majors: Apple and Microsoft ship one Indonesian (id). No regional variants.
- Recommendation: target plain `id`. Keep Malay out of scope. Confidence: high.

### Loanword vs native term (the recurring choice)
- Many tech terms have both an English loan and a native coinage; the majors split on a few.
- "folder": macOS Indonesian keeps the English loan "Folder" (verbatim, capitalized as a term). GNOME/Microsoft also use
  "folder". So "folder" stays "folder", NOT "map".
- "file": stays "file" (loan) across macOS and MS, NOT "berkas" (the native term GNOME/older gov style prefers).
- "tab": stays "Tab" (loan). "volume": stays "Volume" (loan) for a disk volume.
- Recommendation: prefer the loanword where macOS uses it (file, folder, tab, volume, server), since Cmdr is a macOS
  app; use native verbs for actions (salin, pindahkan, hapus). Confidence: high.

### Reduplication for plurals is NOT needed
- Indonesian has no grammatical plural; nouns are number-neutral ("3 file", not "3 file-file"). Reduplication
  ("file-file") marks plurality only for emphasis and is wrong in counted UI strings. See Plurals. Confidence: high.

### No grammatical gender
- Indonesian has no gendered nouns, articles, or adjectives, and a single neutral third-person pronoun ("dia"). Inclusive
  language is a non-issue here; no decision needed. Confidence: high.

## Terminology and glossary

Format: `English → chosen · sources · confidence`. Tier: macOS (1) → MS (2) → GNOME/Xfce (3). All from mined
`id/macOS/` Finder + AppKit unless noted.

- file → file (loan, invariable) · macOS, MS · high
- folder → folder (loan) · macOS ("Folder" verbatim) · high
- directory → direktori · MS · high
- drive → drive / disk · MS · high
- item (generic) → item · macOS ("Salin dan Pindahkan Item", "Memindahkan item ke Tong Sampah") · high
- trash → Tong Sampah · macOS Finder · high
- delete → hapus ("Hapus") · macOS · high
- copy → salin ("Salin") · macOS · high
- move → pindahkan ("Pindahkan", transitive) · macOS ("Pindahkan Arsip Ke") · high
- move to trash → pindahkan ke Tong Sampah · macOS · high
- rename → ubah nama ("Ubah nama ${target} menjadi …") · macOS · high
- open → buka ("Buka") · macOS · high
- search → cari ("Cari file dan folder di Finder") · macOS · high
- cancel → batalkan ("Batalkan") · macOS · high
- replace / overwrite → ganti ("Ganti") · macOS · high
- settings → Pengaturan · macOS ("Pengaturan") · high
- preferences → Preferensi · macOS · high
- volume → Volume (loan, disk sense) · macOS · high
- disconnect → putuskan hubungan ("Putuskan Hubungan") · macOS · high
- eject → keluarkan · MS/macOS common (verify exact macOS Finder label) · tentative
- server → server · macOS · high
- tab → Tab · macOS · high
- bookmark → markah / penanda · MS ("markah"); confirm preferred form · tentative
- sidebar → bilah samping · macOS/MS · high
- sort → urutkan · macOS sort UI · high
- pane → panel · MS · tentative
- listing → daftar (file) · no single source · tentative
- transfer → transfer / pemindahan · MS · high

## Brand and do-not-translate

Keep verbatim: Cmdr, macOS, GitHub, SMB, MTP, Tauri, Rust, Svelte, Quick Look, plus `{system_settings}`-style tokens.
Enforced by `desktop-i18n-dont-translate`. Note "file", "folder", "tab", "volume", "server" are also kept (as loanwords)
but for the glossary reason above, not the brand check.

## Plurals

CLDR category: `other` only (verified `new Intl.PluralRules('id')`, 2026-06-20). Indonesian marks no grammatical
number, so every plural message needs ONLY the `other` branch: "{count} item" works for 1 and for 1,000. Do NOT
reduplicate the noun in counted strings. The `desktop-i18n-plural` check requires just `other` for `id`.

## Notes and decisions

- **Capitalization:** macOS Indonesian title-cases menu items ("Salin dan Pindahkan Item", "Buka di Jendela Baru") but
  Cmdr's rule is sentence case. Follow Cmdr (sentence case): only first word and proper nouns/brand terms capitalized.
  So "Pindahkan ke tong sampah" as a sentence, but keep "Tong Sampah", "Finder" capitalized as names.
- **Quotation marks:** standard `"…"` (curly) is used; no special national quote convention.
- **Length:** Indonesian runs ~10–20% longer than English (multi-word verbs like "Putuskan Hubungan"). Overflow-check
  against the pseudolocale (`en-XA`).
- **Affixation per verb** is the main correctness risk: pick the right meN-/-kan/-i form for transitivity. macOS strings
  are the reference.

## Decisions to confirm with David

- **bookmark → markah vs penanda**, **eject → keluarkan** (exact macOS Finder label), **pane → panel**, **listing →
  daftar** (all tentative): triangulate against `id/macOS/` for the exact Finder strings before finalizing.
