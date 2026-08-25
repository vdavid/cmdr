# Crash reporter (frontend) details

Depth behind `CLAUDE.md`. The backend half (capture, redaction, symbolication, the crash-file lifecycle, the exact
payload catalog) is `apps/desktop/src-tauri/src/crash_reporter/DETAILS.md`; nothing here restates it.

## The next-launch flow

Everything starts in `routes/(main)/+layout.svelte`, in `checkForPendingCrashReport`, called after settings load (the
auto-send branch reads `updates.crashReports`, so running earlier would read the registry default):

1. `checkPendingCrashReport()` over IPC. It returns `null` on the normal path, so a clean launch does nothing further.
2. A report came back. With `updates.crashReports` on AND `possibleCrashLoop` false, `sendCrashReport(report)` fires and
   `CrashReportToastContent` goes up as a persistent info toast. The user is told, not asked.
3. Otherwise `CrashReportDialog` mounts with the report. That covers both the not-opted-in case and the crash-loop case,
   which is why the condition is an AND rather than the setting alone: an app crashing on launch would otherwise mail a
   report every single time, and the user would have no way to intervene.

Send failures are logged and swallowed at every step. A crash report is best-effort; a failed upload must never produce
a second error surface on top of the crash the user already lived through.

## The three report consents

`updates.crashReports` defaults to ON. `updates.errorReports` and `updates.attachEmailToReports` default to OFF, and
that split is a privacy position rather than a preference: a crash report is narrow and stack-shaped (app version, macOS
version, where the code stopped) and everything in it passes `sanitize_panic_message`, while the error report uploads an
unbounded log bundle and the email attaches a person's identity. `settings-registry.test.ts` pins all three, so the next
"let's be consistent" pass has to argue with a test.

Where each population hears about the ON default:

- **A new install**: the open-beta onboarding step, in the caption under the usage-stats disclosure
  (`$lib/onboarding/StepBeta.svelte`). That's the only place a first launch is told anything ships by default, so the
  caption is load-bearing rather than decorative; `$lib/onboarding/DETAILS.md` § Step 3.
- **An existing install**: the CHANGELOG entry for the release that flips it. The settings store persists only what
  someone explicitly set, so an install that never touched the switch carries no key and picks the new default up on
  update. There is deliberately no in-app notice for that: one was built and dropped as too heavy for a beta population
  the changelog already reaches.

Someone who explicitly turned crash reports off keeps them off, since their stored `false` outranks the default.

### Why the flow isn't in this directory

The layout owns it because it's launch sequencing (ordered after settings load, alongside the other post-load checks),
not crash-reporter behavior. This directory stays presentational: two components that render a report and report a
choice. If the sequencing grows past a handful of lines, it moves to a `crash-report-flow.svelte.ts` here, mirroring
`error-reporter/error-report-flow.svelte.ts`.

## Dialog states and choices

`CrashReportDialog` renders one report and returns nothing; it calls the IPC itself and then `onClose()`.

- **Opening sentence**: `crashDialogBodyKey(report)` picks one of `crashReporter.dialog.body.ended` / `.keptRunning` /
  `.unknown`. `ended` is the old fixed string; `keptRunning` says the app carried on and deliberately says "a report"
  rather than "a crash report"; `unknown` names no outcome, so it reads true whichever way an older report went. The
  backend settles the fate before the frontend ever sees it (`src-tauri/src/crash_reporter/DETAILS.md` § App fate), so
  the `default:` branch here is a safety net, not a case the flow produces.
- **Title**: `crashDialogTitleKey(report)`, split TWO ways rather than three. `ended` keeps the specific
  `crashReporter.dialog.title.crash` ("Send crash report?"); the other two share `.title.report` ("Send report?"),
  because they want identical wording and a third key would be duplication. Flattening `ended` to the neutral title
  would trade accurate specificity for symmetry, which is the opposite of the point.
- **Sent toast**: `crashSentToastKey(report)`, split the same two ways for the same reason. The toast takes the report
  as a prop (`addToast(..., { props: { report } })`) purely to make that choice.
- **Privacy note**: ONE string for all three cases. It says which part of the code "ran into the problem" rather than
  "crashed", which is true of a genuine crash too, so three near-identical strings would buy nothing and give a future
  edit two more places to miss.
- **`alwaysSend` stays "Always send crash reports"**, deliberately un-split: the checkbox writes `updates.crashReports`,
  whose canonical label in Settings > Updates is "Send crash reports". A neutral "Always send reports" would desync the
  two surfaces and imply it also covers the separate error-reports setting. Renaming the setting itself is the coherent
  fix, and it isn't this component's call.
- **Report ID line**: only when `report.shortId` is set. Reports written by older app versions have none.
- **Details block**: collapsed by default, expands to the pretty-printed report JSON with a Copy button. The JSON is
  already redacted and capped backend-side, which is what makes it safe to show verbatim; `user-select: text` is set so
  a user can grab part of it.
- **Always send**: on send, writes `updates.crashReports = true`. Only ever flips the setting ON, and only from an
  explicit tick; there's no path here that turns it off (that's Settings > Updates).
- **Attach my email**: `$lib/attach-email`. Hidden when no `analytics.email` is on file, never pre-ticked on first use,
  sticky across the error-report and feedback dialogs.
- **Enter** sends (the dialog has no text input to swallow it). **Dismiss**, Escape, and the × all route through
  `handleDismiss` → `dismissCrashReport()`, which deletes the crash file backend-side. Without that call the same report
  is pending again on the next launch, so any new close affordance has to go through the same function.

## Testing

- `crash-copy.test.ts` covers the fate → key mapping, including the two unsettled inputs (`unconfirmed`, absent) that
  must land on the `unknown` sentence.
- `crash-reporter-i18n-parity.test.ts` freezes every en string the dialog and toast render. An intended copy edit lands
  in `intl/messages/en/crashReporter.json` and here together. The three body goldens carry a second assertion: the
  `keptRunning` and `unknown` strings must not contain "quit", so a copy edit can't reintroduce the claim they exist to
  avoid. The shared attach-email label is frozen once in `$lib/attach-email/attach-email-i18n-parity.test.ts`, not per
  dialog.
- `CrashReportDialog.a11y.test.ts` / `CrashReportToastContent.a11y.test.ts` run axe over the default renders. The
  toast's report only picks which sentence it renders, so one state covers its markup. The dialog test mocks
  `analytics.email` to empty, so it exercises the no-email shape only; the attach-email checkbox's own behavior is
  covered by `$lib/attach-email/attach-email.test.ts` and by the error reporter's a11y coverage, which renders it with
  an email on file and asserts the sticky write. An a11y test doesn't have to sit beside its component: a
  DIRECTORY-LEVEL `<area>.a11y.test.ts` importing several of them satisfies `a11y-coverage` just as well, and much of
  the frontend is consolidated that way. Don't assume a colocated file when looking for a component's coverage.
- All three dialog states are in the dialog gallery (`dialog-gallery/fixtures/crash-report.ts`), one per fate the
  frontend can see: `panic` (a modern `ended` report with a short id and a long backtrace, to keep the scrollable
  details block honest at 440px), `survived-panic` (the same panic with `appFate: 'keptRunning'`), and
  `signal-no-report-id` (an older signal crash with no short id and no fate, flagged as a possible crash loop). Add a
  fixture state whenever a new branch appears in the markup.
