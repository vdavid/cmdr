# Tell me about this – your review

One page for David's QA and decisions on #1104. Every string with its key: `tell-me-about-this-copy.md` (its open copy
questions are at its top). The eval's full evidence: `docs/notes/about-verdict-eval.md`.

## How to QA

1. **Build the fixture folder** (one item per case the card must read well: a Chrome-style `.dmg` download, a program
   named `.jpg`, a name hiding a direction override, a photo with a camera and a location, a plain document, a
   hard-linked pair, an empty folder, a git repo with clean, changed, untracked, and ignored items, and a fake
   `home/Library` with TextEdit's data, a left-over app's, `Slack`, `Google`, and Safari's cache):

   ```sh
   cd apps/desktop && pnpm about:fixtures     # builds /tmp/cmdr-about-this-qa
   ```

2. **Start the dev app** from the repo root with the line the script prints, plus the fake AI and its pace:

   ```sh
   CMDR_ABOUT_FIXTURE_HOME=/tmp/cmdr-about-this-qa/home \
   CMDR_E2E_ASK_CMDR_FAKE=1 CMDR_E2E_ABOUT_VERDICT_PACE=600,20000 pnpm dev
   ```

   The fake writes a fixed take with no network and no key. The pace holds each piece 600 ms and the last one 20 s, so
   "waiting", "streaming", and Stop stay still long enough to look at. Pick Cloud AI in Settings > AI to see the take;
   pick Off to see the AI-off hint. Without the two `CMDR_E2E_*` variables the card uses your real provider.

3. **Open each fixture item** with `⌘⇧I`, from the palette, and from the right-click menu (once each), and look at:
   - The first frame: the headline on top, no row moving or appearing later, a routine absence as one quiet line.
   - The take: its block right above What removing it means, labeled "AI wrote this", its "From the facts above and …"
     line, Stop (the text so far stays, marked "Stopped"), a reopen of the same item ("From earlier"), and Esc mid-take.
   - The unavailable lines: in the dialog gallery (Debug > Soft dialogs > Tell me about this), every state from recorded
     events, including cloud AI needing allowing again, no key, the three model trust tiers, and AI off.
   - Both themes, and reduced motion on (nothing on the card animates anyway).
   - The footer: three actions plus More and Close, Move to trash never first, and the delete dialog asking first.
4. **What a picture shows**: in the gallery, the four `Picture:` states (tags and text, not indexed, indexing off,
   nothing recognized). On the real dev app, turn on Settings > AI > Image search, let it index a folder with a
   screenshot of a receipt and a photo of a sign, and open a card on each: "What it shows" sits right under "Kind",
   names the tags and the first three lines of text, and says nothing about a scene the tags don't. With Cloud AI on,
   the take may say what the picture is "tagged as", never describe it. A picture outside the indexed folders reads "Not
   in Cmdr’s image index"; with image search off, "Image indexing is off". On Naspolya (with SMB image indexing on), the
   row answers instantly even with the NAS asleep: it reads Cmdr's database, never the share.
5. **The re-consent hint**: on a dev profile that allowed cloud AI before this branch, Settings > AI shows the switch
   off and the new "Tell me about this" items in the disclosure. Cards say "Cloud AI needs your OK…" until you allow it.

## Open decisions (each runs on its default today)

- **The take starts by itself** (default) or on a button. Eval: on Sonnet 5.5 a take is done in 2.0 s p50 and costs
  about $0.003, and it adds something true in 96% of 831 takes (Opus 99%); by the eval agent's stricter reading, about
  three in four ordinary takes are useful and every hostile one is. Its recommendation: automatic.
- **No model words removal** (decided by the eval, yours to confirm). With the removal sentence, every Claude class
  handed out green lights (Sonnet 19 then 9, Haiku 5 then 22, Opus 1 then 1); without it, Sonnet and Opus gave none in
  1,662 takes. Removal stays Cmdr's own row.
- **The look inside on Cloud** (default: every automatic take looks inside, and the block says so). Facts only already
  adds something true in 94% (Sonnet) and 98% (Opus) against 100% with the look. The eval agent leans the other way:
  facts only by default, the look on a second press, since the first page of a tax return would leave on every open.
- **The removal row can fall below the fold** when a take shows on a long card: the take sits right above it by design,
  so the model's sentence always has Cmdr's own removal line next to it. A taller default, or accept it?
- **Folder suggestions stay silent after the consent bump**: every cloud user allows cloud AI again once (version 2 →
  3), and until then folder suggestions, Ask Cmdr, and AI search are off with no word why. A one-line "what changed"
  above the switch, a release-notes line, or both? (#1191)
- **A `cmdr-git` per-path status** (#1188), so a clean file reads "Committed, no changes" and the removal row can say
  git brings it back. Default until you say yes: without it.
- **The `cmdr-operation-log` surface raise** (95 → 97) for "which operation put this file here". Default: in, veto-able.
- **An inspector that follows the cursor** beside the dialog (#1181).

The other follow-ups filed from this effort: #1175–#1180, #1182–#1187, and #1189 (the eval's two cleanups).

## Decided since

- **Which models write a take**: every model, in three trust tiers, so you can see how it feels (you may restrict it
  again). Trusted, with no warning: Anthropic's Opus, Fable, and Sonnet on Anthropic's API, OpenAI's GPT-6 Astra, and
  Gemini 4 Carbon. Often wrong, with a red warning and a link to change the model: the Haiku family and the bundled
  local model (the 3B model gave 29 green lights and followed 24 injections in 139 takes). Untested, with a yellow
  warning: everything else. Only Sonnet 5.5 and Opus 5.5 were measured; the rest of the trusted list is your call. In
  the gallery: "AI take: done", "… by a model Cmdr hasn’t tested (yellow)", "… by a model often wrong (red)".

## Review items by surface

- **The card**: the title ("About “name”"), default size, headline templates, section names and order, each row's
  wording per state (every unknown reason, which count as routine), "Nothing on record", each volume note, the removal
  sentences, the "Its data in your Library" row, self-reported facts ("macOS recorded…", "carries a signature from…"),
  and the context-menu and menu-bar label.
- **The AI take**: its place, its look in each state, "AI wrote this", the given-line, Stop, the unavailable lines, the
  AI-off hint, and whether to show the cost.
- **The footer**: action labels, their order, three plus More, and single-key accelerators (not built).
- **Settings and consent**: the take's switch, both consent items' text, and what the other cloud features show after
  the bump.
- **Outside the app**: the `feature-status.json` note (now `alpha`), the website's roadmap and features page (untouched,
  yours to flip), and the analytics event's props (`apps/desktop/src-tauri/src/analytics/DETAILS.md`), which now also
  carry the take’s trust tier (`verdict_trust`).
