# Language selection decisions

Which of the 139 researched languages Cmdr plans to localize, and in what order. Per-language knowledge lives in
`<tag>/style.md`; this is the ship/skip and sequencing roster.

- **wave 1-4**: implementation order. Waves 1-2 are driven by Cmdr' 30-day install usage (analytics, 2026-06-20); waves
  3-4 are market-size / reach estimates, to re-rank as the user base grows. `en` is the source locale (not translated).
- **deferred**: a regional/script variant added after its base, on demand (es-ES, fr-CA, zh-Hant, zh-HK).
- **exclude RTL**: set aside until Cmdr supports right-to-left layout. Decision 2026-06-20: no RTL for now.
- **exclude long-tail**: no major-product localization ecosystem. Decision 2026-06-20: skip the long tail.

Order: source, waves 1 to 4, deferred variants, then exclusions. Formality and script choices per language:
[formal-informal-decisions.md](formal-informal-decisions.md) and [script-decisions.md](script-decisions.md).

| id      | language                  | decision          | comment                                                                           |
| ------- | ------------------------- | ----------------- | --------------------------------------------------------------------------------- |
| en      | English                   | source            | Reference English, region-neutral; the source locale; en-GB is a wave-2 variant   |
| bn      | Bengali                   | wave 1            | MS terminology + GNOME; Google ships Bengali; no macOS                            |
| de      | German                    | wave 1            | Apple macOS + MS + Google ship German                                             |
| es      | Spanish                   | wave 1            | Apple macOS + MS + Google + Netflix ship Spanish; pan-regional es, es-ES deferred |
| fr      | French                    | wave 1            | Apple macOS + MS + Google ship French; fr + fr-CA                                 |
| hu      | Hungarian                 | wave 1            | macOS + MS + GNOME + Xfce all ship Hungarian                                      |
| nl      | Dutch                     | wave 1            | macOS Tier 1 fully informal; mainstream                                           |
| pt      | Portuguese                | wave 1            | Mainstream; Apple/MS ship pt-BR/pt-PT; ships as pt-BR (pt-PT is a wave-2 variant) |
| sv      | Swedish                   | wave 1            | Mainstream; macOS/Windows ship Swedish                                            |
| vi      | Vietnamese                | wave 1            | Mainstream; macOS/MS ship Vietnamese                                              |
| zh      | Chinese                   | wave 1            | Mainstream; macOS/MS ship zh; Simplified/Traditional split                        |
| ca      | Catalan                   | wave 2            | Apple macOS + MS + GNOME ship Catalan                                             |
| cs      | Czech                     | wave 2            | Apple macOS + MS ship Czech; mainstream                                           |
| da      | Danish                    | wave 2            | Apple macOS + MS + Google ship Danish                                             |
| el      | Greek                     | wave 2            | Apple macOS + MS + Google ship Greek                                              |
| en-GB   | English (UK/AU)           | wave 2            | British/Australian; mainly Trash->Bin and -our/-ise spelling                      |
| fi      | Finnish                   | wave 2            | Apple macOS + MS + Google ship Finnish                                            |
| hi      | Hindi                     | wave 2            | Tier-1: Apple, MS, Google, Spotify, Netflix all ship Hindi                        |
| id      | Indonesian                | wave 2            | Apple + MS ship one Indonesian; macOS Finder reference                            |
| it      | Italian                   | wave 2            | macOS Finder + MS + GNOME/Xfce; strong sources                                    |
| ja      | Japanese                  | wave 2            | Tier-1: Apple, MS, Google all ship Japanese                                       |
| ko      | Korean                    | wave 2            | Tier-1: Apple + MS; macOS Finder reference                                        |
| ms      | Malay                     | wave 2            | Apple macOS Finder + MS + GNOME ship Malay; well-sourced                          |
| nb      | Norwegian Bokmål          | wave 2            | Apple, MS, Google, Spotify, Netflix all ship Bokmål                               |
| pl      | Polish                    | wave 2            | Mainstream; macOS/MS ship Polish                                                  |
| pt-PT   | Portuguese (Portugal)     | wave 2            | European Portuguese; a separate pass from pt-BR                                   |
| ro      | Romanian                  | wave 2            | Mainstream; macOS/MS ship Romanian                                                |
| ru      | Russian                   | wave 2            | Mainstream; Apple/MS ship Russian                                                 |
| sk      | Slovak                    | wave 2            | Mainstream; macOS ships Slovak                                                    |
| sr      | Serbian                   | wave 2            | Mainstream; MS+GNOME ship Serbian                                                 |
| th      | Thai                      | wave 2            | Mainstream; macOS/MS ship Thai                                                    |
| tr      | Turkish                   | wave 2            | Mainstream; macOS+MS both formal                                                  |
| uk      | Ukrainian                 | wave 2            | Mainstream; macOS ships Ukrainian                                                 |
| az      | Azerbaijani               | wave 3            | MS style guide + terminology (az-Latn) + GNOME                                    |
| bg      | Bulgarian                 | wave 3            | Microsoft + Google ship Bulgarian; mainstream Cyrillic                            |
| et      | Estonian                  | wave 3            | MS style guide + terminology + GNOME; no macOS                                    |
| gu      | Gujarati                  | wave 3            | MS style guide + terminology + GNOME; Google ships Gujarati                       |
| hr      | Croatian                  | wave 3            | macOS Finder + MS + GNOME + Xfce, well-sourced                                    |
| kk      | Kazakh                    | wave 3            | MS + GNOME + Xfce; Cyrillic now, Latin transition coming                          |
| kn      | Kannada                   | wave 3            | MS + GNOME; no macOS; MS ships Kannada UI                                         |
| lt      | Lithuanian                | wave 3            | No macOS; MS terminology + style guide + GNOME/Xfce, well-sourced                 |
| lv      | Latvian                   | wave 3            | No macOS; MS terminology + 97pg style guide + GNOME/Xfce                          |
| mk      | Macedonian                | wave 3            | No Apple; MS full localization + style guide + GNOME                              |
| ml      | Malayalam                 | wave 3            | MS ships UI + TBX + style guide; no Apple; Google ships too                       |
| mr      | Marathi                   | wave 3            | MS ships UI + TBX + style guide; no Apple; Google ships                           |
| ne      | Nepali                    | wave 3            | No macOS; MS terminology + style guide + GNOME; MS modern voice                   |
| or      | Odia                      | wave 3            | Google/MS ship Odia UI; ~38M speakers                                             |
| pa      | Punjabi                   | wave 3            | Google ships Gurmukhi Android UI; ~125M speakers                                  |
| si      | Sinhala                   | wave 3            | MS full Windows/Office + Google web UI                                            |
| sl      | Slovenian                 | wave 3            | Mainstream; macOS ships Slovenian                                                 |
| sq      | Albanian                  | wave 3            | MS localized Windows into Albanian                                                |
| ta      | Tamil                     | wave 3            | Google Android/Search ships Tamil; ~80M                                           |
| te      | Telugu                    | wave 3            | Google Android/Search ships Telugu; ~95M                                          |
| af      | Afrikaans                 | wave 4            | MS style guide + terminology + GNOME; solid precedent                             |
| am      | Amharic                   | wave 4            | MS style guide + terminology + GNOME; Google ships Amharic                        |
| as      | Assamese                  | wave 4            | MS style guide + terminology + GNOME; honorific norm documented                   |
| be      | Belarusian                | wave 4            | MS style guide + terminology + GNOME                                              |
| bs      | Bosnian                   | wave 4            | MS ships bs-Latn UI + style guide; Latin dominant                                 |
| chr     | Cherokee                  | wave 4            | MS shipped Windows LIP/Office, Google Gmail; syllabary UI corpus                  |
| eu      | Basque                    | wave 4            | MS style guide + GNOME ship Basque                                                |
| fil     | Filipino                  | wave 4            | MS style guide + terminology; Google ships Filipino                               |
| ga      | Irish                     | wave 4            | MS style guide + terminology + GNOME; no macOS                                    |
| gd      | Scottish Gaelic           | wave 4            | MS style guide + terminology + GNOME; no macOS                                    |
| gl      | Galician                  | wave 4            | MS style guide + terminology + GNOME/Xfce; no macOS                               |
| is      | Icelandic                 | wave 4            | No Apple Icelandic; MS terminology + Thunar + GNOME                               |
| ka      | Georgian                  | wave 4            | MS + GNOME + Xfce; no macOS; conversational MS voice                              |
| km      | Khmer                     | wave 4            | MS terminology + style guide + sparse GNOME; ZWSP burden                          |
| kok     | Konkani                   | wave 4            | MS only (terminology + style guide); multi-script, Devanagari                     |
| lo      | Lao                       | wave 4            | MS terminology + near-complete Xfce Thunar; no macOS                              |
| mn      | Mongolian                 | wave 4            | Cyrillic; MS terminology + style guide; avoid-pronoun rule                        |
| mt      | Maltese                   | wave 4            | EU official; thin outside MS + EU; no Apple; Latin script                         |
| my      | Burmese                   | wave 4            | Moderate-thin; no Apple UI; MS + Nautilus; Zawgyi/Unicode pitfall                 |
| nn      | Norwegian Nynorsk         | wave 4            | Minority written standard; Apple/MS/GNOME ship du                                 |
| quz     | Quechua (Cusco)           | wave 4            | MS localized Windows/Office into Quechua                                          |
| sw      | Swahili                   | wave 4            | Google Android/Search + MS Office ship Swahili                                    |
| uz      | Uzbek                     | wave 4            | MS localized Windows + GNOME ship Uzbek Latin                                     |
| es-ES   | Spanish (Spain)           | deferred          | only if the pan-regional es proves insufficient                                   |
| fr-CA   | French (Canada)           | deferred          | Quebec French; base fr covers most                                                |
| zh-HK   | Chinese (Traditional, HK) | deferred          | later optional override of zh-Hant                                                |
| zh-Hant | Chinese (Traditional)     | deferred          | Taiwan-norm fast-follow after zh-Hans                                             |
| ar      | Arabic                    | exclude RTL       | Apple/MS/Google ship full mirrored RTL UIs; MSA standard                          |
| ckb     | Central Kurdish (Sorani)  | exclude RTL       | MS + GNOME localize Sorani Perso-Arabic RTL                                       |
| fa      | Persian                   | exclude RTL       | MS style guide + terminology + GNOME; Perso-Arabic RTL                            |
| he      | Hebrew                    | exclude RTL       | macOS Finder + MS + GNOME + Xfce; RTL gates shipping                              |
| ks      | Kashmiri                  | exclude RTL       | Perso-Arabic (RTL) official default; no term source; MS style guides only         |
| prs     | Dari                      | exclude RTL       | Only MS terminology; no GNOME/macOS                                               |
| ps      | Pashto                    | exclude RTL       | Only GNOME+MS terminology; no macOS                                               |
| sd      | Sindhi                    | exclude RTL       | Low-resource; MS terminology only, no macOS, script split                         |
| ug      | Uyghur                    | exclude RTL       | Only MS terminology glossary; no shipped UI, no macOS                             |
| ur      | Urdu                      | exclude RTL       | Google ships Urdu UI (Noto Nastaliq, Android)                                     |
| yi      | Yiddish                   | exclude RTL       | No macOS/MS UI; only GNOME (Hebrew script)                                        |
| ab      | Abkhaz                    | exclude long-tail | No Apple/MS/Google UI; only GNOME, no computing lexicon, native-review only       |
| an      | Aragonese                 | exclude long-tail | No major UI; only GNOME; users read Spanish; native-only                          |
| ast     | Asturian                  | exclude long-tail | No major UI; only GNOME; speakers fluent in Spanish; native-only                  |
| bo      | Tibetan                   | exclude long-tail | No major ships Tibetan UI; only GNOME; native-review only                         |
| br      | Breton                    | exclude long-tail | No major UI; only GNOME; revitalization community                                 |
| brx     | Bodo                      | exclude long-tail | Only MS style guide PDF; no UI, no GNOME; native-only                             |
| crh     | Crimean Tatar             | exclude long-tail | Only GNOME + Google Translate; no major UI; Latin now official                    |
| cy      | Welsh                     | exclude long-tail | No macOS/MS UI; only GNOME; OS chrome is English                                  |
| doi     | Dogri                     | exclude long-tail | Only MS style guide PDF; no UI, no terminology, no GNOME                          |
| dz      | Dzongkha                  | exclude long-tail | No major UI; only GNOME + DDA effort; native-review only                          |
| eo      | Esperanto                 | exclude long-tail | No commercial major; only GNOME/Xfce volunteer; conlang                           |
| ff      | Fula                      | exclude long-tail | Only MS terminology; dialect-fragmented macrolanguage; native-only                |
| fo      | Faroese                   | exclude long-tail | No macOS/MS; only GNOME ~85%; OS chrome Danish/English                            |
| fur     | Friulian                  | exclude long-tail | No major UI; only GNOME ~85%; OS chrome Italian/English                           |
| fy      | Western Frisian           | exclude long-tail | No major UI; only GNOME ~33%; leans on Dutch; native-only                         |
| guc     | Wayuunaiki                | exclude long-tail | Only MS terminology (~2800 terms); no UI, no style guide; native-gated            |
| gv      | Manx                      | exclude long-tail | Revived minority lang; only GNOME Nautilus (~82%), no macOS/MS                    |
| ha      | Hausa                     | exclude long-tail | GNOME + MS terminology only; no macOS, no MS style guide                          |
| hy      | Armenian                  | exclude long-tail | Sparse: GNOME only, no native macOS, partial MS; Eastern Armenian                 |
| ia      | Interlingua               | exclude long-tail | Constructed auxlang, no native speakers, GNOME only (~245)                        |
| ie      | Interlingue               | exclude long-tail | Constructed auxlang (Occidental); GNOME + Xfce only, no commercial                |
| ig      | Igbo                      | exclude long-tail | Sparse; GNOME Nautilus only, no macOS/MS; tonal w/ dotted letters                 |
| io      | Ido                       | exclude long-tail | Constructed auxlang (reformed Esperanto); GNOME only (~422)                       |
| iu      | Inuktitut                 | exclude long-tail | MS terminology (iu-Latn) only; tiny base, polysynthetic                           |
| kab     | Kabyle                    | exclude long-tail | Berber/Amazigh; GNOME only (~700); active FOSS community                          |
| ku      | Kurdish (Kurmanji)        | exclude long-tail | Kurmanji-Latin; GNOME only (~1180); no macOS/MS                                   |
| ky      | Kyrgyz                    | exclude long-tail | Near-zero major-vendor UI; MS terminology + old GNOME (~42%)                      |
| lb      | Luxembourgish             | exclude long-tail | No macOS/GNOME/Xfce; MS only; trilingual country, low priority                    |
| li      | Limburgish                | exclude long-tail | Bottom-of-backlog; one aged fuzzy GNOME (2003); skip→Dutch                        |
| ln      | Lingala                   | exclude long-tail | Bantu lingua franca; one aged GNOME (2016); no major vendor                       |
| mai     | Maithili                  | exclude long-tail | No macOS, no MS TBX; MS style guide + old GNOME (~73%, 2008)                      |
| mg      | Malagasy                  | exclude long-tail | No macOS; MS style guide + old GNOME (~95%, 2006-07); native-review               |
| mi      | Māori                     | exclude long-tail | Strong for low-resource: MS LIP + style guide + TBX; no macOS                     |
| mjw     | Karbi                     | exclude long-tail | Very low priority; no vendor ships; one Nautilus; no inherited vocab              |
| mni     | Manipuri/Meitei           | exclude long-tail | Tibeto-Burman; only input/keyboard support; MS 2025 style guide; coin vocab       |
| nds     | Low German                | exclude long-tail | Separate W-Germanic lang; only GNOME (Tier 3); no Apple/MS                        |
| nso     | Northern Sotho (Sepedi)   | exclude long-tail | Only MS terminology + GNOME; no shipped consumer UI                               |
| oc      | Occitan                   | exclude long-tail | Regional minority; only GNOME/Xfce community catalogs                             |
| qut     | K'iche'                   | exclude long-tail | VERY LOW; only MS Guatemala terminology                                           |
| rw      | Kinyarwanda               | exclude long-tail | LOW; GNOME+MS terminology, no macOS                                               |
| sa      | Sanskrit                  | exclude long-tail | VERY LOW; only MS style guide, symbolic                                           |
| sat     | Santali                   | exclude long-tail | LOW; only MS sat-Olck style guide, no Apple                                       |
| so      | Somali                    | exclude long-tail | Low-priority low-resource; no Apple, MS only                                      |
| tg      | Tajik                     | exclude long-tail | GNOME+MS terminology only; no macOS                                               |
| ti      | Tigrinya                  | exclude long-tail | VERY LOW; MS terminology only, native-review-only                                 |
| tk      | Turkmen                   | exclude long-tail | Very sparse; MS terminology + old GNOME, no Apple                                 |
| tn      | Tswana                    | exclude long-tail | Very sparse; only MS terminology, no Apple/GNOME                                  |
| tt      | Tatar                     | exclude long-tail | Sparse later-tier; MS only, Cyrillic-only, no Apple                               |
| vec     | Venetian                  | exclude long-tail | Only Xfce catalog; regional/colloquial                                            |
| wa      | Walloon                   | exclude long-tail | Only ~52% GNOME catalog; no macOS/MS                                              |
| wo      | Wolof                     | exclude long-tail | Only MS terminology (wo-SN); thin low-resource                                    |
| xh      | Xhosa                     | exclude long-tail | GNOME + MS terminology; native-review-only                                        |
| yo      | Yoruba                    | exclude long-tail | MS terminology + GNOME; native-review-only                                        |
| zu      | Zulu                      | exclude long-tail | GNOME + MS terminology; native-review-only                                        |
