# Korean (ko) translation style guide

Working notes for translating Cmdr into Korean. Read `../README.md` for how this fits the translation process, and the
app-wide `docs/style-guide.md` for the English voice these notes carry into Korean.

Korean has no second-person T/V pronoun split, but it has a speech-level (존댓말) system that sets UI tone, plus two
mechanics that bite localizers hard: **word spacing (띄어쓰기)** and **particles that change shape after the preceding
word** (을/를, 은/는, 이/가, 으로/로). Both interact badly with placeholders. Read the Decision points before
translating.

## Voice and tone

Friendly, concise, active, calm. Cmdr's Korean should read like a modern macOS app: polite, clear, and approachable.
macOS Korean is the model. The Microsoft Korean style guide pushes toward plain, easy-to-understand native Korean over
hard Sino-Korean words, and toward a less-formal everyday tone (verified in `ko/microsoft-style-guides/StyleGuide.pdf`,
pdftotext + grep, 2026-06-19).

Error messages stay calm and actionable and never label themselves "오류" (error) or "실패" (failed) as a bare heading:
state what happened and the next step. macOS does this, "폴더를 생성할 수 없습니다." ("could not create the folder",
verified in `ko/macOS/Finder/Localizable.json`, 2026-06-19) is the calm "can't …" framing Cmdr wants, not "오류".

## Formality

**Use polite speech level, leaning to 해요체 (haeyo-che) for friendliness, with 합쇼체/합니다체 (hapsyo-che) acceptable
for system statements.** This needs a real decision because the two majors land differently:

- **macOS Korean uses the deferential 합니다체 / 하십시오체** (hapsyo-che, the most formal polite level): "휴지통으로
  항목을 이동합니다." ("moves items to the Trash", -ㅂ니다 deferential), "다른 위치로 항목을 복사합니다.", and the
  formal imperative "항목의 새로운 이름을 제공하십시오." ("please provide a new name", -하십시오) (all verified in
  `ko/macOS/Finder/Localizable.json`, 2026-06-19).
- **The modern consumer trend is the friendlier 해요체** (haeyo-che, semi-formal). The Microsoft Korean style guide
  prefers 입력하세요 / 참조하세요 (haseyo, the 해요체 imperative) OVER the macOS-style 제공하십시오 / 참조를 하십시오
  (verified in `ko/microsoft-style-guides/StyleGuide.pdf`, 2026-06-19). Google moved away from formal hapsyo-che to the
  middle (haeyo-che) speech level across consumer products (web localization sources; unverified beyond that).
- Recommendation: **하세요/해요체 (haeyo-che) for anything addressed to the user** (prompts, requests, confirmations),
  it matches Cmdr's friendly voice and the current consumer convention; **합니다체 (hapsyo-che) is fine for neutral
  system statements/descriptions** ("…합니다") where macOS uses it. Avoid the stiffest -하십시오 imperative
  ("제공하십시오"); prefer "…하세요" / "입력하세요". Confidence: high (both sources verified; this is a deliberate
  Apple-vs-Microsoft/Google split worth recording). Flag the overall register for David since it's a voice call.

Mechanics:

- **Button and menu labels: bare noun (체언), not a sentence.** macOS labels are nouns: "취소" (Cancel), "복사" (Copy),
  "이동" (Move), "삭제" (Delete), "이름 변경" (Rename). Don't make a button a -합니다/-하세요 sentence.
- **Sentences, prompts, status: the chosen polite level** (haeyo-che for user-facing, hapsyo-che ok for system).

## Decision points

Register is recommended above (haeyo-che-leaning). These are the Korean-specific calls.

- **Script: Hangul only, no variant choice, but the live decision is Hangul vs Hanja vs loanword.** Korean is written in
  Hangul; Hanja (Chinese characters) are essentially never used in modern UI. There is no script variant to pick. The
  recurring choice is whether a term is native Hangul, a Sino-Korean word, or a katakana-style English loanword written
  in Hangul. macOS picks per term: "Folder" → "폴더" (loanword), "Trash" → "휴지통" (Sino-Korean), "Network" →
  "네트워크" (loanword), "Document" → "문서" (Sino-Korean) (verified in `ko/macOS/Finder/Localizable.json`, 2026-06-19).
  The Microsoft style guide leans toward easier native/everyday Korean over hard Sino-Korean.
  - Recommendation: **follow macOS term by term** (glossary below), favoring the more common/approachable form where
    macOS and Microsoft agree. Confidence: high.

- **Word spacing (띄어쓰기): get it right; it changes meaning and is a top localizer error.** Korean separates words
  with spaces, but particles attach with NO space to the preceding word, and compound nouns may or may not take a space.
  The Microsoft Korean style guide devotes a section to spacing: no space inside a true compound (구름다리(O), not 구름
  다리(X)) (verified in `ko/microsoft-style-guides/StyleGuide.pdf`, 2026-06-19). The classic pitfall is putting a space
  before a particle (책 을 ✗, 책을 ✓).
  - Recommendation: attach particles directly (no space), follow standard 한글 맞춤법 spacing, and match macOS's spacing
    in the glossary terms ("이름 변경" has a space; "휴지통" doesn't). Confidence: high.

- **Particles + placeholders: the biggest ICU trap in Korean.** Object/subject/topic particles change form by whether
  the preceding word ends in a consonant or vowel: 을/를 (object), 은/는 (topic), 이/가 (subject), 으로/로 (direction).
  When the preceding word is a `{placeholder}` (a filename, a count), the catalog can't know which form to use. macOS
  handles it with the explicit dual form "(으)로", "${target}의 이름을 ${newName}(으)로 변경" (verified in
  `ko/macOS/Finder/Localizable.json`, 2026-06-19), showing both options because `{newName}` is unknown.
  - Recommendation: when a particle follows a `{placeholder}`, use the macOS-style **dual-particle form** "(으)로",
    "을(를)", "이(가)", it's the standard Korean way to write a particle whose host word is unknown, and it's exactly
    what macOS does. Don't guess one form. ICU `select` on the placeholder's final-jamo isn't available, so the dual
    form is the correct tool. Confidence: high (macOS pattern verified; flag any string where dual form reads
    awkwardly).

- **Regional variant: one base `ko`, no country split.** Standard Korean (South Korea, ko-KR) is the target; North
  Korean is not relevant for software. Apple and Microsoft ship one Korean. Recommendation: one `ko`. Confidence: high.

- **Gender: not grammatically marked; nothing to handle.** Korean verbs and adjectives don't inflect for gender, and UI
  omits subject pronouns, so there's no gendered-default trap. Recommendation: no special handling. Confidence: high.

## Terminology and glossary

Format per term: `English → chosen · sources · confidence`. Tier order: macOS (Tier 1) → Microsoft (Tier 2) → GNOME/Xfce
(Tier 3). All terms below are from macOS Korean Finder strings unless noted (verified in
`ko/macOS/Finder/Localizable.json`, 2026-06-19).

- file → 파일 · macOS Finder · high
- folder → 폴더 · macOS ("Folder" → "폴더", "Could not create the folder." → "폴더를 생성할 수 없습니다.") · high
- trash → 휴지통 · macOS ("Trash" → "휴지통") · high
- copy → 복사 · macOS ("Copies items …" → "다른 위치로 항목을 복사합니다.") · high
- move → 이동 · macOS ("Move ${entities} to ${destinationFolder}" → "${destinationFolder} 폴더로 ${entities} 이동") ·
  high
- move to trash → 휴지통으로 이동 · macOS ("Moves items to the Trash" → "휴지통으로 항목을 이동합니다.") · high
- rename → 이름 변경 (with space) · macOS ("Rename ${target} to ${newName}" → "${target}의 이름을 ${newName}(으)로
  변경") · high
- delete → 삭제 · macOS (Finder 삭제 usage) · high
- cancel → 취소 · macOS ("Cancel" → "취소") · high
- item → 항목 · macOS ("Items" → "항목") · high
- document → 문서 · macOS ("Document" → "문서") · high
- computer → 컴퓨터 · macOS ("Computer" → "컴퓨터") · high
- network → 네트워크 · macOS ("Network" → "네트워크") · high
- search → 검색 · macOS ("Search This Mac" → "이 Mac 검색") · high
- create → 생성 · macOS ("Could not create the folder." → "폴더를 생성할 수 없습니다.") · high
- server → 서버 · macOS server UI · high (verify against macOS strings)
- viewer / preview → 미리보기 · macOS preview UI · high
- pane → 영역 · no clean Finder source · tentative
  - macOS uses "사이드바" for the sidebar; a generic two-pane file-list pane has no canonical term. "영역" (area) or
    "창" candidates. Flag for David.
- listing → 목록 · no direct source · tentative
  - "listing" (the file list) → "목록" (list) reads naturally; confirm with David.
- volume → 볼륨 · macOS uses 볼륨 for a mounted disk volume · tentative (verify against macOS volume strings)
- tab → 탭 · macOS tab UI · high

Add rows as terms come up, each with sources and a confidence.

## Brand and do-not-translate

Keep verbatim, in Latin script: Cmdr, macOS, GitHub, SMB, MTP, Tauri, Rust, Svelte, Quick Look, plus the
`{system_settings}`-style tokens. Enforced by `desktop-i18n-dont-translate` (list in
`apps/desktop/scripts/i18n-catalog-lib.ts`). macOS keeps these Latin in Korean text ("이 Mac 검색" keeps "Mac" Latin).
**Particle-after-brand watch**: when a particle follows a Latin brand token (e.g. "SMB로" vs "SMB으로"), Korean reads
the token by how it's pronounced; use the dual-particle form "(으)로" if unsure, or spell the particle for the token's
actual Korean reading. See the particle decision point.

## Plurals

CLDR categories: **`other` only**, Korean has no grammatical plural that the count selects (verified with
`new Intl.PluralRules('ko')` and corroborated by the GNOME Korean catalog header `nplurals=1` in
`ko/gnome-nautilus/nautilus.po`, 2026-06-19). The optional plural suffix "-들" is not count-driven and is usually
omitted in UI counting.

- A single `other` branch satisfies `desktop-i18n-plural`. The count is carried by the number plus a counter (개 for
  generic items, 명 for people), e.g. "파일 1개" / "파일 5개", same noun form for any count.
- Write the `other` branch so it reads naturally with the counter for any number.

## Notes and decisions

- **Spacing and particles are the dominant Korean concern**, re-read those two decision points. They cause the most
  localizer errors and interact with placeholders.
- **Punctuation: standard Korean uses the same period "." and comma "," as Latin** (Korean doesn't use full-width
  Japanese marks). macOS Korean uses ASCII-style punctuation. Quotation marks follow macOS usage.
- **Counter words** carry counting (no plural inflection), see Plurals.
- **Numbers and dates come from the formatter layer**, never hardcode separators. Korean uses Western digits in modern
  UI (macOS does), so digits aren't a decision here.
- **ICU mechanics** (catalog-level, easy to miss): double every apostrophe in a value (`'` becomes `''`; ICU treats a
  lone `'` as an escape and silently swallows text), and keep every `{placeholder}` and `<tag>` verbatim. Full rules:
  the agent-handoff block in `docs/guides/i18n-translation.md` and `apps/desktop/src/lib/intl/messages/CLAUDE.md`.

## Decisions to confirm with David

- **Speech level / register** (recommended haeyo-che-leaning, but a voice call): macOS uses formal hapsyo-che;
  Microsoft/Google lean friendlier haeyo-che. Confirm the overall tone.
- **pane → 영역 / 창** (tentative): no canonical Finder term for a file-list pane.
- **listing → 목록** (tentative): no canonical source.
- **volume → 볼륨** (tentative): verify against macOS volume strings.

## Glossary

The living term glossary for this language is in `glossary.md`. Read it before translating and add to it as you settle
terms, each sourced from the reference pile (`_ignored/i18n/ko/`; recipes in `docs/i18n/reference-pile/how-to-mine.md`).
Never guess a term.
