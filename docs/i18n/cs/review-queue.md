# cs review queue

Open questions only a native reviewer can settle: a `tentative` ruling the sources couldn't decide, a phrasing that
reads correctly but maybe not naturally, a register call. One bullet per item, citing the keys in backticks. Not
translator input: translators work from the brief and `terms.json`. Remove an item once a reviewer settles it (and
record the outcome in `terms.json` or `decisions.md`).

The native reviewer settled every earlier question (outcomes in `terms.json` and `decisions.md`). What is left needs a
real device or console to verify:

- Full Disk Access (`onboarding.stepFda.*`): confirm the macOS 27 Privacy & Security row is still „Plný přístup k
  disku“.
- `settings.fileOperations.adbEnabled.description`: confirm Android Developer options say „Ladění přes USB“.
- `settings.fileOperations.mtpEnabled.description`: confirm „Předvolby USB“ and „Přenos souborů“ on a Czech phone.
- `menu.context.makeAvailableOffline`, `commands.cloudMakeOffline.label`: confirm current Finder still offers „Stáhnout
  nyní“ for an iCloud-only file.
- `settings.fileExplorer.typeToJump.mode.description`: check what Czech VoiceOver calls the ⌫ key (written Delete).
- `servers.sheet.s3GcsKeyHelp`: check the Czech Google Cloud console's name for the Interoperability tab (written „karta
  Interoperabilita“).
- `ai.secretError.body`: confirm GNOME's Czech keyring app is „Hesla a klíče“.
- `shortcuts.system.characterViewer`: check whether Czech macOS titles the Character Viewer window „Prohlížeč znaků“;
  switch from „Emotikony a symboly“ if so.
- `queue.chip.ariaLabel`: check VoiceOver reads „42 %“ as „42 procent“.
- `ui.toast.age`: check the duration formatter gives Czech units („před 2 min“, not „před 2m“).

Awaiting a native read, from the unified AI tools:

- `ai.cloudConsent.askCmdr.protectedContents`: „Některý obsah zůstává doma, ať si o něj říká kdokoli“ keeps the English
  image in a consent text; confirm it reads as "never leaves your Mac" to a native reader.
- `askCmdr.tool.navToPath.done`, `askCmdr.tool.navBack.done`, `askCmdr.tool.await.done`: Přešlo se… and Čekalo se… avoid
  a gendered subject; check they read as finished status lines, not as narration.

Awaiting a native read, from the #739 batch:

- `commands.paneFocusControls.*`, `commands.appFocusNotifications.*`: catalog says „fokus“, macOS says „zaměření
  klávesnice“; switch the labels and descriptions together if macOS wins.
- `commands.helpAccessibility.*`: „průvodce“ vs macOS „příručka“ (Uživatelská příručka); switch label and description
  together.
- `askCmdr.limit.*`: confirm „vyhledávání“ for a lookup reads distinct from ⌘F „hledání“.
