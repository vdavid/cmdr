# Language selection decisions

Which of the 139 researched languages Cmdr plans to localize, and which are set aside for now. The per-language
localization knowledge lives in `<tag>-style.md`; this is the ship/skip roster.

- **UNDECIDED**: a real candidate, not yet assigned to an implementation wave (waves TBD).
- **exclude RTL**: set aside until Cmdr supports right-to-left layout (mirroring + bidi). Decision 2026-06-20: no RTL for now.
- **exclude long-tail**: no major-product localization ecosystem (low-resource / native-review-only). Decision 2026-06-20: skip the long tail for now.

Order: UNDECIDED first, then future implementation waves, then exclusions by type.

| id | language | decision | comment |
| -- | -------- | -------- | ------- |
| af | Afrikaans | UNDECIDED | MS style guide + terminology + GNOME; solid precedent |
| am | Amharic | UNDECIDED | MS style guide + terminology + GNOME; Google ships Amharic |
| as | Assamese | UNDECIDED | MS style guide + terminology + GNOME; honorific norm documented |
| az | Azerbaijani | UNDECIDED | MS style guide + terminology (az-Latn) + GNOME |
| be | Belarusian | UNDECIDED | MS style guide + terminology + GNOME |
| bg | Bulgarian | UNDECIDED | Microsoft + Google ship Bulgarian; mainstream Cyrillic |
| bn | Bengali | UNDECIDED | MS terminology + GNOME; Google ships Bengali; no macOS |
| bs | Bosnian | UNDECIDED | MS ships bs-Latn UI + style guide; Latin dominant |
| ca | Catalan | UNDECIDED | Apple macOS + MS + GNOME ship Catalan |
| chr | Cherokee | UNDECIDED | MS shipped Windows LIP/Office, Google Gmail; syllabary UI corpus |
| cs | Czech | UNDECIDED | Apple macOS + MS ship Czech; mainstream |
| da | Danish | UNDECIDED | Apple macOS + MS + Google ship Danish |
| de | German | UNDECIDED | Apple macOS + MS + Google ship German |
| el | Greek | UNDECIDED | Apple macOS + MS + Google ship Greek |
| en | English | UNDECIDED | Reference English, region-neutral; the source locale |
| es | Spanish | UNDECIDED | Apple macOS + MS + Google + Netflix ship Spanish |
| et | Estonian | UNDECIDED | MS style guide + terminology + GNOME; no macOS |
| eu | Basque | UNDECIDED | MS style guide + GNOME ship Basque |
| fi | Finnish | UNDECIDED | Apple macOS + MS + Google ship Finnish |
| fil | Filipino | UNDECIDED | MS style guide + terminology; Google ships Filipino |
| fr | French | UNDECIDED | Apple macOS + MS + Google ship French; fr + fr-CA |
| ga | Irish | UNDECIDED | MS style guide + terminology + GNOME; no macOS |
| gd | Scottish Gaelic | UNDECIDED | MS style guide + terminology + GNOME; no macOS |
| gl | Galician | UNDECIDED | MS style guide + terminology + GNOME/Xfce; no macOS |
| gu | Gujarati | UNDECIDED | MS style guide + terminology + GNOME; Google ships Gujarati |
| hi | Hindi | UNDECIDED | Tier-1: Apple, MS, Google, Spotify, Netflix all ship Hindi |
| hr | Croatian | UNDECIDED | macOS Finder + MS + GNOME + Xfce, well-sourced |
| hu | Hungarian | UNDECIDED | macOS + MS + GNOME + Xfce all ship Hungarian |
| id | Indonesian | UNDECIDED | Apple + MS ship one Indonesian; macOS Finder reference |
| is | Icelandic | UNDECIDED | No Apple Icelandic; MS terminology + Thunar + GNOME |
| it | Italian | UNDECIDED | macOS Finder + MS + GNOME/Xfce; strong sources |
| ja | Japanese | UNDECIDED | Tier-1: Apple, MS, Google all ship Japanese |
| ka | Georgian | UNDECIDED | MS + GNOME + Xfce; no macOS; conversational MS voice |
| kk | Kazakh | UNDECIDED | MS + GNOME + Xfce; Cyrillic now, Latin transition coming |
| km | Khmer | UNDECIDED | MS terminology + style guide + sparse GNOME; ZWSP burden |
| kn | Kannada | UNDECIDED | MS + GNOME; no macOS; MS ships Kannada UI |
| ko | Korean | UNDECIDED | Tier-1: Apple + MS; macOS Finder reference |
| kok | Konkani | UNDECIDED | MS only (terminology + style guide); multi-script, Devanagari |
| lo | Lao | UNDECIDED | MS terminology + near-complete Xfce Thunar; no macOS |
| lt | Lithuanian | UNDECIDED | No macOS; MS terminology + style guide + GNOME/Xfce, well-sourced |
| lv | Latvian | UNDECIDED | No macOS; MS terminology + 97pg style guide + GNOME/Xfce |
| mk | Macedonian | UNDECIDED | No Apple; MS full localization + style guide + GNOME |
| ml | Malayalam | UNDECIDED | MS ships UI + TBX + style guide; no Apple; Google ships too |
| mn | Mongolian | UNDECIDED | Cyrillic; MS terminology + style guide; avoid-pronoun rule |
| mr | Marathi | UNDECIDED | MS ships UI + TBX + style guide; no Apple; Google ships |
| ms | Malay | UNDECIDED | Apple macOS Finder + MS + GNOME ship Malay; well-sourced |
| mt | Maltese | UNDECIDED | EU official; thin outside MS + EU; no Apple; Latin script |
| my | Burmese | UNDECIDED | Moderate-thin; no Apple UI; MS + Nautilus; Zawgyi/Unicode pitfall |
| nb | Norwegian Bokmål | UNDECIDED | Apple, MS, Google, Spotify, Netflix all ship Bokmål |
| ne | Nepali | UNDECIDED | No macOS; MS terminology + style guide + GNOME; MS modern voice |
| nl | Dutch | UNDECIDED | macOS Tier 1 fully informal; mainstream |
| nn | Norwegian Nynorsk | UNDECIDED | Minority written standard; Apple/MS/GNOME ship du |
| or | Odia | UNDECIDED | Google/MS ship Odia UI; ~38M speakers |
| pa | Punjabi | UNDECIDED | Google ships Gurmukhi Android UI; ~125M speakers |
| pl | Polish | UNDECIDED | Mainstream; macOS/MS ship Polish |
| pt | Portuguese | UNDECIDED | Mainstream; Apple/MS ship pt-BR/pt-PT |
| quz | Quechua (Cusco) | UNDECIDED | MS localized Windows/Office into Quechua |
| ro | Romanian | UNDECIDED | Mainstream; macOS/MS ship Romanian |
| ru | Russian | UNDECIDED | Mainstream; Apple/MS ship Russian |
| si | Sinhala | UNDECIDED | MS full Windows/Office + Google web UI |
| sk | Slovak | UNDECIDED | Mainstream; macOS ships Slovak |
| sl | Slovenian | UNDECIDED | Mainstream; macOS ships Slovenian |
| sq | Albanian | UNDECIDED | MS localized Windows into Albanian |
| sr | Serbian | UNDECIDED | Mainstream; MS+GNOME ship Serbian |
| sv | Swedish | UNDECIDED | Mainstream; macOS/Windows ship Swedish |
| sw | Swahili | UNDECIDED | Google Android/Search + MS Office ship Swahili |
| ta | Tamil | UNDECIDED | Google Android/Search ships Tamil; ~80M |
| te | Telugu | UNDECIDED | Google Android/Search ships Telugu; ~95M |
| th | Thai | UNDECIDED | Mainstream; macOS/MS ship Thai |
| tr | Turkish | UNDECIDED | Mainstream; macOS+MS both formal |
| uk | Ukrainian | UNDECIDED | Mainstream; macOS ships Ukrainian |
| uz | Uzbek | UNDECIDED | MS localized Windows + GNOME ship Uzbek Latin |
| vi | Vietnamese | UNDECIDED | Mainstream; macOS/MS ship Vietnamese |
| zh | Chinese | UNDECIDED | Mainstream; macOS/MS ship zh; Simplified/Traditional split |
| ar | Arabic | exclude RTL | Apple/MS/Google ship full mirrored RTL UIs; MSA standard |
| ckb | Central Kurdish (Sorani) | exclude RTL | MS + GNOME localize Sorani Perso-Arabic RTL |
| fa | Persian | exclude RTL | MS style guide + terminology + GNOME; Perso-Arabic RTL |
| he | Hebrew | exclude RTL | macOS Finder + MS + GNOME + Xfce; RTL gates shipping |
| ks | Kashmiri | exclude RTL | Perso-Arabic (RTL) official default; no term source; MS style guides only |
| prs | Dari | exclude RTL | Only MS terminology; no GNOME/macOS |
| ps | Pashto | exclude RTL | Only GNOME+MS terminology; no macOS |
| sd | Sindhi | exclude RTL | Low-resource; MS terminology only, no macOS, script split |
| ug | Uyghur | exclude RTL | Only MS terminology glossary; no shipped UI, no macOS |
| ur | Urdu | exclude RTL | Google ships Urdu UI (Noto Nastaliq, Android) |
| yi | Yiddish | exclude RTL | No macOS/MS UI; only GNOME (Hebrew script) |
| ab | Abkhaz | exclude long-tail | No Apple/MS/Google UI; only GNOME, no computing lexicon, native-review only |
| an | Aragonese | exclude long-tail | No major UI; only GNOME; users read Spanish; native-only |
| ast | Asturian | exclude long-tail | No major UI; only GNOME; speakers fluent in Spanish; native-only |
| bo | Tibetan | exclude long-tail | No major ships Tibetan UI; only GNOME; native-review only |
| br | Breton | exclude long-tail | No major UI; only GNOME; revitalization community |
| brx | Bodo | exclude long-tail | Only MS style guide PDF; no UI, no GNOME; native-only |
| crh | Crimean Tatar | exclude long-tail | Only GNOME + Google Translate; no major UI; Latin now official |
| cy | Welsh | exclude long-tail | No macOS/MS UI; only GNOME; OS chrome is English |
| doi | Dogri | exclude long-tail | Only MS style guide PDF; no UI, no terminology, no GNOME |
| dz | Dzongkha | exclude long-tail | No major UI; only GNOME + DDA effort; native-review only |
| eo | Esperanto | exclude long-tail | No commercial major; only GNOME/Xfce volunteer; conlang |
| ff | Fula | exclude long-tail | Only MS terminology; dialect-fragmented macrolanguage; native-only |
| fo | Faroese | exclude long-tail | No macOS/MS; only GNOME ~85%; OS chrome Danish/English |
| fur | Friulian | exclude long-tail | No major UI; only GNOME ~85%; OS chrome Italian/English |
| fy | Western Frisian | exclude long-tail | No major UI; only GNOME ~33%; leans on Dutch; native-only |
| guc | Wayuunaiki | exclude long-tail | Only MS terminology (~2800 terms); no UI, no style guide; native-gated |
| gv | Manx | exclude long-tail | Revived minority lang; only GNOME Nautilus (~82%), no macOS/MS |
| ha | Hausa | exclude long-tail | GNOME + MS terminology only; no macOS, no MS style guide |
| hy | Armenian | exclude long-tail | Sparse: GNOME only, no native macOS, partial MS; Eastern Armenian |
| ia | Interlingua | exclude long-tail | Constructed auxlang, no native speakers, GNOME only (~245) |
| ie | Interlingue | exclude long-tail | Constructed auxlang (Occidental); GNOME + Xfce only, no commercial |
| ig | Igbo | exclude long-tail | Sparse; GNOME Nautilus only, no macOS/MS; tonal w/ dotted letters |
| io | Ido | exclude long-tail | Constructed auxlang (reformed Esperanto); GNOME only (~422) |
| iu | Inuktitut | exclude long-tail | MS terminology (iu-Latn) only; tiny base, polysynthetic |
| kab | Kabyle | exclude long-tail | Berber/Amazigh; GNOME only (~700); active FOSS community |
| ku | Kurdish (Kurmanji) | exclude long-tail | Kurmanji-Latin; GNOME only (~1180); no macOS/MS |
| ky | Kyrgyz | exclude long-tail | Near-zero major-vendor UI; MS terminology + old GNOME (~42%) |
| lb | Luxembourgish | exclude long-tail | No macOS/GNOME/Xfce; MS only; trilingual country, low priority |
| li | Limburgish | exclude long-tail | Bottom-of-backlog; one aged fuzzy GNOME (2003); skip→Dutch |
| ln | Lingala | exclude long-tail | Bantu lingua franca; one aged GNOME (2016); no major vendor |
| mai | Maithili | exclude long-tail | No macOS, no MS TBX; MS style guide + old GNOME (~73%, 2008) |
| mg | Malagasy | exclude long-tail | No macOS; MS style guide + old GNOME (~95%, 2006-07); native-review |
| mi | Māori | exclude long-tail | Strong for low-resource: MS LIP + style guide + TBX; no macOS |
| mjw | Karbi | exclude long-tail | Very low priority; no vendor ships; one Nautilus; no inherited vocab |
| mni | Manipuri/Meitei | exclude long-tail | Tibeto-Burman; only input/keyboard support; MS 2025 style guide; coin vocab |
| nds | Low German | exclude long-tail | Separate W-Germanic lang; only GNOME (Tier 3); no Apple/MS |
| nso | Northern Sotho (Sepedi) | exclude long-tail | Only MS terminology + GNOME; no shipped consumer UI |
| oc | Occitan | exclude long-tail | Regional minority; only GNOME/Xfce community catalogs |
| qut | K'iche' | exclude long-tail | VERY LOW; only MS Guatemala terminology |
| rw | Kinyarwanda | exclude long-tail | LOW; GNOME+MS terminology, no macOS |
| sa | Sanskrit | exclude long-tail | VERY LOW; only MS style guide, symbolic |
| sat | Santali | exclude long-tail | LOW; only MS sat-Olck style guide, no Apple |
| so | Somali | exclude long-tail | Low-priority low-resource; no Apple, MS only |
| tg | Tajik | exclude long-tail | GNOME+MS terminology only; no macOS |
| ti | Tigrinya | exclude long-tail | VERY LOW; MS terminology only, native-review-only |
| tk | Turkmen | exclude long-tail | Very sparse; MS terminology + old GNOME, no Apple |
| tn | Tswana | exclude long-tail | Very sparse; only MS terminology, no Apple/GNOME |
| tt | Tatar | exclude long-tail | Sparse later-tier; MS only, Cyrillic-only, no Apple |
| vec | Venetian | exclude long-tail | Only Xfce catalog; regional/colloquial |
| wa | Walloon | exclude long-tail | Only ~52% GNOME catalog; no macOS/MS |
| wo | Wolof | exclude long-tail | Only MS terminology (wo-SN); thin low-resource |
| xh | Xhosa | exclude long-tail | GNOME + MS terminology; native-review-only |
| yo | Yoruba | exclude long-tail | MS terminology + GNOME; native-review-only |
| zu | Zulu | exclude long-tail | GNOME + MS terminology; native-review-only |
