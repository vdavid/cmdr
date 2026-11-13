# Tell me about this – copy for review

Every string the card adds, for David's copy pass (plan § Review items, #1104). All English drafts, none translated yet.
Source of truth: `apps/desktop/src/lib/intl/messages/en/aboutThis.json` (plus the command and menu labels); when a
string changes there, change it here too, or regenerate this list. Where a value holds `{…}` or `<tag></tag>`, the
placeholder or tag is filled in by the card (a date, a size, a path).

Open questions for the review:

1. **The shortcut off macOS** is `Ctrl+Shift+Enter`, not the plan's `Ctrl+Shift+I` (the web inspector's chord on Linux
   and Windows; plan D2). Fine, or another key?
2. **The title** reads "About “name”", which sits close to "About Cmdr". Keep it, or "Tell me about “name”"?
3. **"Nothing on record"** is the collapsed line for a section whose rows all came back as ordinary absences (a file you
   made: no download record, no copy by Cmdr). Right words?
4. **Signer wording**: "Signed by Tessera Labs AB (team T3SS3RA001)" only when the program matched its signature,
   "Carries a signature from …" otherwise. Never "verified".
5. **Removal lines**: two shown, the rest behind "N more". Right number?
6. **What's inside lists eight** biggest children and up to eight largest files, so a big folder's card scrolls before
   "What removing it means". Fewer (five)?
7. **"Not ranked"** replaced "Low by design" for a folder Cmdr's ranking leaves out (caches, `node_modules`, app data in
   the Library, temp folders), so "low" never reads as "this doesn't matter". Right words?

## Strings

- `commands.fileTellMeAbout.label`: Tell me about this
  - Where: Command name that opens Cmdr's own card about the file or folder under the cursor: where it came from, what
    it really is, what it belongs to, how it's used, and what removing it would mean. A friendly, conversational
    request, as if asking a person. Appears in the command palette, the File menu, and the right-click menu, right below
    Get info.
- `commands.fileTellMeAbout.description`: Shows where the item came from, what it is, and what removing it would mean
  - Where: Longer help text for the "Tell me about this" command, shown as a secondary line in the command palette.
    "Item" is a file or a folder.
- `menu.file.tellMeAbout`: Tell me about this
  - Where: VERB phrase (a friendly request, as if asking a person). File menu and file context menu, right below "Get
    info". Opens Cmdr's own card about the file or folder under the cursor: where it came from, what it is, what it
    belongs to, and what removing it would mean. It must say the same words as the command palette entry
    (commands.fileTellMeAbout.label). A dialog follows, but it asks nothing, hence no ellipsis.
- `aboutThis.dialog.title`: About “{name}”
  - Where: Title of the "Tell me about this" card, a dialog that lists what Cmdr knows about one file or folder: where
    it came from, what it is, what it belongs to, and what removing it would mean. {name} is the item's file or folder
    name, exactly as it appears in the file list.
- `aboutThis.dialog.close`: Close
  - Where: Button at the bottom right of the "Tell me about this" card. Closes it; nothing else happens.
- `aboutThis.dialog.lookingUp`: Looking this up
  - Where: Screen-reader text on a row of the "Tell me about this" card whose answer hasn't arrived yet (it shows a gray
    placeholder bar). Short, no closing period.
- `aboutThis.section.origin`: Where it came from
  - Where: Section heading on the "Tell me about this" card: where the item came from (a download record, or a copy or
    move Cmdr made). Sentence case.
- `aboutThis.section.nature`: What it is
  - Where: Section heading on the "Tell me about this" card: what the item is (its size and dates, what its contents
    really are, its code signature). Sentence case.
- `aboutThis.section.use`: How it’s used
  - Where: Section heading on the "Tell me about this" card: how the item is used (when it was last opened, the app that
    opens it, how important a folder is). Sentence case.
- `aboutThis.section.context`: Where it sits
  - Where: Section heading on the "Tell me about this" card: what the item belongs to (the app that owns it, the git
    repository around it). Sentence case.
- `aboutThis.section.contents`: What’s inside
  - Where: Section heading on the "Tell me about this" card for a folder: what the folder holds. Sentence case.
- `aboutThis.section.removal`: What removing it means
  - Where: Section heading on the "Tell me about this" card, always the last one: what deleting the item would mean (for
    example "Git can't bring it back"). It describes consequences only; it never says deleting is safe. Sentence case.
- `aboutThis.section.nothingOnRecord`: Nothing on record
  - Where: The one muted line a section of the "Tell me about this" card shows when none of its rows had anything
    recorded (for example no download record and no copy by Cmdr). Ordinary, not a warning. No closing period.
- `aboutThis.volumeNote.identityOnly`: On {volume}, Cmdr can see the name, size, and dates.
  - Where: Note at the top of the "Tell me about this" card when the item sits on a drive or server that only reports
    names, sizes, and dates (a phone, a network share). Said once instead of on every row.
- `aboutThis.volumeNote.someFacts`: On {volume}, Cmdr can see only some of this.
  - Where: Note at the top of the "Tell me about this" card when the item's drive or server answers some questions but
    not all of them. Said once instead of on every row.
- `aboutThis.volumeNote.unreachable`: {volume} stopped answering, so Cmdr can’t tell more right now.
  - Where: Note at the top of the "Tell me about this" card when the item's drive or server stopped responding while
    Cmdr was looking. Said once; the rows below it stay short.
- `aboutThis.unavailable.unknownVolume`: Cmdr doesn’t know the drive this item is on anymore.
  - Where: The whole body of the "Tell me about this" card when Cmdr can't look at the item at all because its drive was
    removed from Cmdr's list since the pane showed it.
- `aboutThis.unavailable.pathOutsideVolume`: Cmdr couldn’t place this item on its drive.
  - Where: The whole body of the "Tell me about this" card when the item's location doesn't fit the drive the pane says
    it's on.
- `aboutThis.unavailable.noVolumeAtPath`: The drive this item was on isn’t connected.
  - Where: The whole body of the "Tell me about this" card when the item's drive was disconnected.
- `aboutThis.unavailable.notFound`: This item isn’t there anymore. It may have been moved or removed.
  - Where: The whole body of the "Tell me about this" card when the file or folder no longer exists at its place.
- `aboutThis.unavailable.permissionDenied`: {platform, select, macos {macOS didn’t let Cmdr look at this item.} windows
  {Windows didn’t let Cmdr look at this item.} other {The system didn’t let Cmdr look at this item.}}
  - Where: The whole body of the "Tell me about this" card when the operating system refused to describe the item (no
    permission). The branches name the user's operating system.
- `aboutThis.unavailable.unreachable`: The drive didn’t answer in time. Give it another try in a moment.
  - Where: The whole body of the "Tell me about this" card when the item's drive or server didn't respond within a few
    seconds.
- `aboutThis.unavailable.unreadable`: Cmdr couldn’t read anything about this item.
  - Where: The whole body of the "Tell me about this" card when reading the item stopped for another reason.
- `aboutThis.label.download`: Downloaded
  - Where: Row label on the "Tell me about this" card, left of what the operating system recorded about where the item
    was downloaded from. A past participle, like a form label.
- `aboutThis.label.cmdrOperation`: Cmdr’s history
  - Where: Row label on the "Tell me about this" card, left of the copy, move, or extraction Cmdr itself made that put
    the item where it is.
- `aboutThis.label.size`: Size
  - Where: Row label on the "Tell me about this" card, left of the item's size.
- `aboutThis.label.created`: Created
  - Where: Row label on the "Tell me about this" card, left of the date the item was created.
- `aboutThis.label.modified`: Modified
  - Where: Row label on the "Tell me about this" card, left of the date the item last changed.
- `aboutThis.label.otherNames`: Other names
  - Where: Row label on the "Tell me about this" card, left of how many other names (hard links) point to the same file
    contents.
- `aboutThis.label.pointsTo`: Points to
  - Where: Row label on the "Tell me about this" card for a symbolic link, left of the place the link points to.
- `aboutThis.label.realType`: Kind
  - Where: Row label on the "Tell me about this" card, left of what the file's contents really are (for example "Disk
    image"), as Cmdr read them from its first bytes.
- `aboutThis.label.bundle`: Bundle
  - Where: Row label on the "Tell me about this" card for an app or package, left of the name, version, and identifier
    the app's own description file states.
- `aboutThis.label.signature`: Signature
  - Where: Row label on the "Tell me about this" card, left of the code signature an app or program carries (who signed
    it).
- `aboutThis.label.lastOpened`: Last opened
  - Where: Row label on the "Tell me about this" card, left of the date the system recorded the item as last opened.
- `aboutThis.label.defaultApp`: Opens with
  - Where: Row label on the "Tell me about this" card, left of the app that opens the file by default.
- `aboutThis.label.importance`: Importance
  - Where: Row label on the "Tell me about this" card for a folder, left of how important Cmdr's drive index scores the
    folder (high, medium, low).
- `aboutThis.label.owner`: Belongs to
  - Where: Row label on the "Tell me about this" card, left of the app or installer the item belongs to.
- `aboutThis.label.git`: Git
  - Where: Row label on the "Tell me about this" card, left of the git repository the item sits in and its state there.
    "Git" is the version-control tool's name; keep it.
- `aboutThis.label.contents`: Contents
  - Where: Row label on the "Tell me about this" card for a folder, left of how many items sit directly in it.
- `aboutThis.label.appData`: Its data in your Library
  - Where: Row label on the "Tell me about this" card for an app, left of the folders in the user's Library folder that
    are named after the app and stay behind if the app is removed. "Library" is the macOS folder of that name.
- `aboutThis.label.biggest`: Biggest inside
  - Where: Row label on the "Tell me about this" card for a folder, left of the largest items sitting directly in it
    (files and folders), biggest first.
- `aboutThis.label.largestFiles`: Largest files
  - Where: Row label on the "Tell me about this" card for a folder, left of the largest files anywhere inside it, at any
    depth.
- `aboutThis.label.unchanged`: Untouched
  - Where: Row label on the "Tell me about this" card for a folder, left of how much of it hasn't changed in a year.
    Short: one or two words.
- `aboutThis.unknown.noRecord`: Nothing on record
  - Where: Muted one-line answer on a "Tell me about this" card row when nothing recorded this fact. Ordinary, not a
    warning. It doesn't say "never": no record isn't the same as it never happened. No closing period.
- `aboutThis.unknown.notInStatus`: No changes Cmdr can see
  - Where: Muted one-line answer on the Git row of the "Tell me about this" card when git's list of changes doesn't name
    the item: it may be committed with no changes, or ignored. Ordinary, not a warning. No closing period.
- `aboutThis.unknown.noInstalledAppMatches`: No app Cmdr knows of
  - Where: Muted one-line answer on the "Belongs to" row of the "Tell me about this" card when no installed app matches
    the folder's name, somewhere an owner isn't expected. Ordinary, not a warning. No closing period.
- `aboutThis.unknown.volumeCantTell`: Not on this drive
  - Where: Muted one-line answer on a "Tell me about this" card row when the item's drive or server doesn't report this
    fact. The card's note at the top already explains it once. Short, no closing period.
- `aboutThis.unknown.libraryNameMatchesNoApp`: No installed app has this folder’s name.
  - Where: Answer on the "Belongs to" row of the "Tell me about this" card for a folder in the macOS Library folder that
    is named like an app, when no installed app has that name. It doesn't claim the folder is left over.
- `aboutThis.unknown.notIndexed`: Not indexed yet
  - Where: Muted answer on a "Tell me about this" card row (a folder's size, what's inside, its importance, an app's
    data) when Cmdr's drive index hasn't scanned the place yet, or indexing is off. Also the one muted line of a section
    whose every row has that answer. An ordinary state, not a problem. Short, no closing period.
- `aboutThis.unknown.timedOut`: Cmdr didn’t get an answer in time.
  - Where: Answer on a "Tell me about this" card row when looking up that one fact took too long (about a second) and
    Cmdr stopped waiting.
- `aboutThis.unknown.unreachable`: The drive stopped answering.
  - Where: Answer on a "Tell me about this" card row when the item's drive or server stopped responding.
- `aboutThis.unknown.permissionDenied`: {platform, select, macos {macOS didn’t let Cmdr read this.} windows {Windows
  didn’t let Cmdr read this.} other {The system didn’t let Cmdr read this.}}
  - Where: Answer on a "Tell me about this" card row when the operating system refused Cmdr access to that fact. The
    branches name the user's operating system.
- `aboutThis.unknown.protected`: This is a private place, so Cmdr doesn’t look inside.
  - Where: Answer on a "Tell me about this" card row when the item sits somewhere Cmdr deliberately doesn't read (for
    example a folder of keys or passwords).
- `aboutThis.unknown.nodeChangedSince`: Cmdr has a record for this name, and the file has been replaced since.
  - Where: Answer on the "Cmdr's history" row of the "Tell me about this" card when Cmdr's log has a copy or move to
    this name, but the file there now is a different one (it was saved over or replaced), so the record doesn't apply to
    it.
- `aboutThis.unknown.ambiguous`: Several could fit, so Cmdr doesn’t pick one.
  - Where: Answer on a "Tell me about this" card row when more than one answer fits equally and Cmdr won't guess.
- `aboutThis.unknown.unparseable`: There’s a record, and Cmdr can’t read it.
  - Where: Answer on a "Tell me about this" card row when the operating system or a tool kept a record of this fact in a
    form Cmdr can't read.
- `aboutThis.unknown.refusedInBackground`: Cmdr doesn’t reach this drive on its own.
  - Where: Answer on a "Tell me about this" card row when the item is on a network or device drive and Cmdr was asked
    about it without the user doing anything, so it didn't connect.
- `aboutThis.notApplicable.noCodeSigningOnThisOs`: This system doesn’t sign programs
  - Where: Muted one-line answer on the Signature row of the "Tell me about this" card on an operating system that has
    no code signing to read. No closing period.
- `aboutThis.notApplicable.notInARepo`: Not in a git repository
  - Where: Muted one-line answer on the Git row of the "Tell me about this" card when the item isn't inside a git
    repository. No closing period.
- `aboutThis.notApplicable.notABundle`: Not an app or package
  - Where: Muted one-line answer on the Bundle row of the "Tell me about this" card when the folder isn't an app or
    package and so has no bundle name or version. No closing period.
- `aboutThis.notApplicable.volumeHasNone`: This drive keeps no such record
  - Where: Muted one-line answer on a "Tell me about this" card row when the item's drive keeps no record of this kind
    at all (a phone has no download records). No closing period.
- `aboutThis.download.noRecord`: No download record
  - Where: Muted one-line answer on the "Downloaded" row of the "Tell me about this" card when the system recorded no
    download for the item (most files a person made themselves). Ordinary, not a warning. No closing period.
- `aboutThis.download.fromSite`: {platform, select, macos {macOS recorded this as downloaded from {host}{hasApp, select,
  yes { with {app}} other {}}{hasDate, select, yes { on <date></date>} other {}}.} other {Your browser recorded this as
  downloaded from {host}{hasApp, select, yes { with {app}} other {}}{hasDate, select, yes { on <date></date>} other
  {}}.}}
  - Where: Answer on the "Downloaded" row of the "Tell me about this" card. It says what the system RECORDED, not that
    Cmdr checked it: anyone can write such a record. <date></date> becomes the download date. macOS keeps the record
    itself; on other systems the browser writes it.
- `aboutThis.download.fromAddress`: {platform, select, macos {macOS recorded this as downloaded from the address
  {address}.} other {Your browser recorded this as downloaded from the address {address}.}}
  - Where: Answer on the "Downloaded" row of the "Tell me about this" card when the recorded source is a numeric
    internet address rather than a website name. It says what was recorded, not that Cmdr checked it.
- `aboutThis.download.linkedFrom`: The record also names a page on {host}.
  - Where: Second line on the "Downloaded" row of the "Tell me about this" card when the download record names the page
    the download link was on, at a different website than the file itself (browsers record both).
- `aboutThis.download.copyLink`: Copy the download link
  - Where: Accessible name and hover hint of the copy button beside the download source on the "Tell me about this"
    card. It copies the whole recorded web address, of which the card shows only the website name.
- `aboutThis.download.copyPageLink`: Copy the page’s link
  - Where: Accessible name and hover hint of the copy button beside the line naming the page the download link was on,
    on the "Tell me about this" card. It copies that page's whole recorded web address.
- `aboutThis.download.mail`: macOS recorded this as a Mail attachment{hasSender, select, yes {, from {sender}} other
  {}}.
  - Where: Answer on the "Downloaded" row of the "Tell me about this" card when macOS recorded the file as saved from an
    email in Apple Mail. It says what was recorded, not that Cmdr checked it.
- `aboutThis.download.mailSubject`: Subject: {subject}
  - Where: Second line on the "Downloaded" row of the "Tell me about this" card for a Mail attachment: the subject line
    of the email it came with, as Mail recorded it.
- `aboutThis.download.airDrop`: macOS recorded this as received over AirDrop from {sender}.
  - Where: Answer on the "Downloaded" row of the "Tell me about this" card when macOS recorded the file as received over
    AirDrop. "AirDrop" is Apple's feature name. It says what was recorded, not that Cmdr checked it.
- `aboutThis.download.inlineData`: {platform, select, macos {macOS recorded a source that holds the data itself, which
  Cmdr doesn’t show.} other {The browser recorded a source that holds the data itself, which Cmdr doesn’t show.}}
  - Where: Answer on the "Downloaded" row of the "Tell me about this" card when the recorded source is a data address
    that contains the file's own contents (a "data:" or "blob:" link), so there's no website to name.
- `aboutThis.download.localFile`: {platform, select, macos {macOS recorded this as coming from a file on this computer.}
  other {The browser recorded this as coming from a file on this computer.}}
  - Where: Answer on the "Downloaded" row of the "Tell me about this" card when the recorded source is a file on the
    same computer (a "file:" link).
- `aboutThis.download.unrecognized`: {platform, select, macos {macOS recorded a source Cmdr can’t read as an address:
  {text}} other {The browser recorded a source Cmdr can’t read as an address: {text}}}
  - Where: Answer on the "Downloaded" row of the "Tell me about this" card when the recorded source is text Cmdr can't
    read as a web address. The text follows the colon as it was recorded.
- `aboutThis.download.noSource`: macOS recorded this as downloaded{hasApp, select, yes { with {app}} other {}}{hasDate,
  select, yes { on <date></date>} other {}}, without saying from where.
  - Where: Answer on the "Downloaded" row of the "Tell me about this" card when macOS recorded that the file was
    downloaded but not where from. <date></date> becomes the download date.
- `aboutThis.cmdrOperation.noRecord`: No copy or move by Cmdr on record
  - Where: Muted one-line answer on the "Cmdr's history" row of the "Tell me about this" card when Cmdr's own log has no
    copy, move, or extraction that put this very file here. Ordinary, not a warning. No closing period.
- `aboutThis.cmdrOperation.done`: {who, select, you {You} askCmdr {Ask Cmdr} other {An AI client}} {kind, select, copy
  {copied this here} move {moved this here} rename {gave this its name} create {created this} extract {extracted this
  here} compress {made this archive} other {moved this here when deleting it}} with Cmdr on <date></date>.
  - Where: Answer on the "Cmdr's history" row of the "Tell me about this" card: which Cmdr operation put the item where
    it is, who started it, and when. <date></date> becomes the date. "Ask Cmdr" is the name of Cmdr's built-in
    assistant; keep it. "An AI client" is another AI app that was driving Cmdr.
- `aboutThis.cmdrOperation.from`: From <source></source>{hasVolume, select, yes { on {volume}} other {}}
  - Where: Second line on the "Cmdr's history" row of the "Tell me about this" card: where the item was copied or moved
    from. <source></source> becomes the original path; {volume} names the drive or server it was on when that differs.
    No closing period.
- `aboutThis.cmdrOperation.copySource`: Copy where it came from
  - Where: Accessible name and hover hint of the copy button beside the original path on the "Cmdr's history" row. It
    copies the full path.
- `aboutThis.identity.onDisk`: <size></size> on disk
  - Where: Muted addition after the item's size on the "Tell me about this" card when the space it takes on the drive
    differs from its size. <size></size> becomes that size, for example "4 KB".
- `aboutThis.identity.otherNames`: {count, plural, one {One other name points to the same contents}
  other {{countText} other names point to the same contents}}
  - Where: Answer on the "Other names" row of the "Tell me about this" card: how many other names (hard links) lead to
    this same file contents. No closing period.
- `aboutThis.identity.copyLinkTarget`: Copy where it points
  - Where: Accessible name and hover hint of the copy button beside the "Points to" row of a symbolic link on the "Tell
    me about this" card. It copies the full target path.
- `aboutThis.realType.matches`: {kind}, as its name says
  - Where: Answer on the "Kind" row of the "Tell me about this" card when the file's contents match what its extension
    says. No closing period.
- `aboutThis.realType.mismatch`: The contents say {kind}, while the name ends in .{extension}.
  - Where: Answer on the "Kind" row of the "Tell me about this" card when the file's contents are a different kind than
    its extension says (for example a program named like a photo). Shown with a warning mark.
- `aboutThis.realType.byName`: {kind}, going by its name
  - Where: Answer on the "Kind" row of the "Tell me about this" card when Cmdr can't tell from the contents (plain text,
    for example) and the extension names a kind. No closing period.
- `aboutThis.realType.unrecognized`: Cmdr doesn’t know this kind of contents
  - Where: Answer on the "Kind" row of the "Tell me about this" card when neither the contents nor the extension tell
    what kind of file it is. No closing period.
- `aboutThis.kind.diskImage`: Disk image
  - Where: A kind of file on the "Tell me about this" card: a macOS disk image (.dmg).
- `aboutThis.kind.installerPackage`: Installer package
  - Where: A kind of file on the "Tell me about this" card: a macOS installer package (.pkg).
- `aboutThis.kind.machO`: Mac program
  - Where: A kind of file on the "Tell me about this" card: a program that runs on macOS.
- `aboutThis.kind.elf`: Linux program
  - Where: A kind of file on the "Tell me about this" card: a program that runs on Linux.
- `aboutThis.kind.windowsProgram`: Windows program
  - Where: A kind of file on the "Tell me about this" card: a program or installer for Windows.
- `aboutThis.kind.otherCode`: Compiled code
  - Where: A kind of file on the "Tell me about this" card: other compiled code (Java, WebAssembly, Android).
- `aboutThis.kind.script`: Script
  - Where: A kind of file on the "Tell me about this" card: a script a program runs, such as a shell script.
- `aboutThis.kind.archive`: Archive
  - Where: A kind of file on the "Tell me about this" card: an archive such as a zip file.
- `aboutThis.kind.pdf`: PDF
  - Where: A kind of file on the "Tell me about this" card: a PDF document.
- `aboutThis.kind.image`: Image
  - Where: A kind of file on the "Tell me about this" card: a photo or other image.
- `aboutThis.kind.video`: Video
  - Where: A kind of file on the "Tell me about this" card: a video.
- `aboutThis.kind.audio`: Sound file
  - Where: A kind of file on the "Tell me about this" card: music or other sound.
- `aboutThis.kind.font`: Font
  - Where: A kind of file on the "Tell me about this" card: a font.
- `aboutThis.kind.document`: Document
  - Where: A kind of file on the "Tell me about this" card: an office document or an e-book.
- `aboutThis.kind.database`: Database
  - Where: A kind of file on the "Tell me about this" card: a database file.
- `aboutThis.kind.text`: Text
  - Where: A kind of file on the "Tell me about this" card: plain text, web pages included.
- `aboutThis.kind.other`: A known format
  - Where: A kind of file on the "Tell me about this" card when Cmdr recognizes the format but has no family name for
    it.
- `aboutThis.kind.unrecognized`: Contents Cmdr doesn’t know
  - Where: A kind of file on the "Tell me about this" card when the contents match no format Cmdr knows.
- `aboutThis.what.fileWithExtension`: .{extension} file
  - Where: What an item is, at the start of the one-line summary at the top of the "Tell me about this" card, when Cmdr
    knows only the file's extension. For example ".md file".
- `aboutThis.what.file`: File
  - Where: What an item is, at the start of the one-line summary at the top of the "Tell me about this" card, when
    nothing more specific is known.
- `aboutThis.what.folder`: Folder
  - Where: What an item is, at the start of the one-line summary at the top of the "Tell me about this" card: a folder.
- `aboutThis.what.app`: {hasVersion, select, yes {App, version {version}} other {App}}
  - Where: What an item is, at the start of the one-line summary at the top of the "Tell me about this" card: an app,
    with its version when the app states one.
- `aboutThis.what.package`: Package
  - Where: What an item is, at the start of the one-line summary at the top of the "Tell me about this" card: a folder
    the system shows as one item (a document package, a framework).
- `aboutThis.what.symlink`: Link
  - Where: What an item is, at the start of the one-line summary at the top of the "Tell me about this" card: a symbolic
    link to another place.
- `aboutThis.what.archiveEntry`: Item in an archive
  - Where: What an item is, at the start of the one-line summary at the top of the "Tell me about this" card: a file or
    folder inside a zip or other archive.
- `aboutThis.headline.plain`: {what}{sizeShown, select, exact {, <size></size>} atLeast {, at least <size></size>} other
  {}}
  - Where: The one-line summary at the top of the "Tell me about this" card when nothing beyond the kind and size is
    known. <size></size> becomes the size, for example "2.1 MB". No closing period.
- `aboutThis.headline.downloadedFrom`: {what}{sizeShown, select, exact {, <size></size>} atLeast {, at least
  <size></size>} other {}}, recorded as downloaded{hasApp, select, yes { with {app}} other {}} from {host}{hasDate,
  select, yes { on <date></date>} other {}}
  - Where: The one-line summary at the top of the "Tell me about this" card for a file the system recorded as
    downloaded. "Recorded as": Cmdr didn't check it. <size></size> becomes the size, <date></date> the date. No closing
    period.
- `aboutThis.headline.putHereByCmdr`: {what}{sizeShown, select, exact {, <size></size>} atLeast {, at least
  <size></size>} other {}}, {kind, select, copy {copied here} move {moved here} rename {renamed} create {created}
  extract {extracted here} compress {compressed} other {moved here when deleted}} with Cmdr{hasVolume, select, yes {
  from {volume}} other {}} on <date></date>
  - Where: The one-line summary at the top of the "Tell me about this" card for an item a Cmdr operation put where it
    is. <size></size> becomes the size, <date></date> the date. No closing period.
- `aboutThis.headline.receivedByMail`: {what}{sizeShown, select, exact {, <size></size>} atLeast {, at least
  <size></size>} other {}}, recorded as a Mail attachment{hasDate, select, yes { saved on <date></date>} other {}}
  - Where: The one-line summary at the top of the "Tell me about this" card for a file macOS recorded as saved from an
    email in Apple Mail. It never names the sender. No closing period.
- `aboutThis.headline.receivedByAirDrop`: {what}{sizeShown, select, exact {, <size></size>} atLeast {, at least
  <size></size>} other {}}, recorded as received over AirDrop{hasDate, select, yes { on <date></date>} other {}}
  - Where: The one-line summary at the top of the "Tell me about this" card for a file macOS recorded as received over
    AirDrop. It never names the sender. No closing period.
- `aboutThis.headline.dataOfApp`: {what}{sizeShown, select, exact {, <size></size>} atLeast {, at least <size></size>}
  other {}}, {certainty, select, probable {probably {app}’s data} other {{app}’s data}}
  - Where: The one-line summary at the top of the "Tell me about this" card for a folder that holds an installed app's
    data. "probably" when Cmdr matched it by the folder's name only. No closing period.
- `aboutThis.headline.dataOfMissingApp`: {what}{sizeShown, select, exact {, <size></size>} atLeast {, at least
  <size></size>} other {}}, named after {bundleId}, an app that isn’t installed
  - Where: The one-line summary at the top of the "Tell me about this" card for a Library folder named after an app that
    isn't installed on this Mac. No closing period.
- `aboutThis.headline.downloadedBy`: {what}{sizeShown, select, exact {, <size></size>} atLeast {, at least
  <size></size>} other {}}, recorded as downloaded with {app}{hasDate, select, yes { on <date></date>} other {}}
  - Where: The one-line summary at the top of the "Tell me about this" card for a file the system recorded as downloaded
    by an app, without saying from where. No closing period.
- `aboutThis.headline.lastOpened`: {what}{sizeShown, select, exact {, <size></size>} atLeast {, at least <size></size>}
  other {}}, last opened <date></date>
  - Where: The one-line summary at the top of the "Tell me about this" card when the system recorded when the item was
    last opened. <date></date> becomes that date. No closing period.
- `aboutThis.bundle.named`: {hasVersion, select, yes {{name}, version {version}} other {{name}}}
  - Where: Answer on the "Bundle" row of the "Tell me about this" card: the name and version an app or package states
    about itself. No closing period.
- `aboutThis.bundle.unnamed`: {hasVersion, select, yes {No name stated, version {version}} other {No name or version
  stated}}
  - Where: Answer on the "Bundle" row of the "Tell me about this" card when the app's description file states no name.
    No closing period.
- `aboutThis.signature.unsigned`: Not signed
  - Where: Answer on the Signature row of the "Tell me about this" card when the program carries no code signature. No
    closing period.
- `aboutThis.signature.adHoc`: Signed without a developer’s identity, like a local build
  - Where: Answer on the Signature row of the "Tell me about this" card for an "ad hoc" signature: signed, but naming no
    developer (typical of something built on this computer). No closing period.
- `aboutThis.signature.developerId`: {matches, select, yes {Signed by {org} (team {team})} other {Carries a signature
  from {org} (team {team})}}
  - Where: Answer on the Signature row of the "Tell me about this" card for a program signed with a developer's
    certificate. "Signed by" only when Cmdr compared the program with its signature and they matched; otherwise "Carries
    a signature from", since a signature can be copied onto a changed program. Never "verified" or "trusted". No closing
    period.
- `aboutThis.signature.appStore`: {matches, select, yes {Signed by the Mac App Store} other {Carries the Mac App Store’s
  signature}}
  - Where: Answer on the Signature row of the "Tell me about this" card for an app signed by Apple's Mac App Store.
    "Signed by" only when the program matched its signature. Never "verified" or "trusted". No closing period.
- `aboutThis.signature.apple`: {matches, select, yes {Signed by Apple} other {Carries Apple’s signature}}
  - Where: Answer on the Signature row of the "Tell me about this" card for Apple's own software. "Signed by" only when
    the program matched its signature. Never "verified" or "trusted". No closing period.
- `aboutThis.signature.unrecognized`: Carries a signature from an authority Cmdr doesn’t know
  - Where: Answer on the Signature row of the "Tell me about this" card when the signature names an authority Cmdr
    doesn't know. No closing period.
- `aboutThis.signature.executableDiffers`: Its program doesn’t match the signature it carries.
  - Where: Warning line on the Signature row of the "Tell me about this" card: Cmdr compared the app's program with its
    signature and they differ, so the program was changed after signing.
- `aboutThis.signature.checkTimedOut`: Cmdr didn’t finish comparing its program with the signature.
  - Where: Line on the Signature row of the "Tell me about this" card when comparing the program with its signature took
    too long (a very large app), so nothing was confirmed.
- `aboutThis.signature.checkUnavailable`: Cmdr couldn’t compare its program with the signature.
  - Where: Line on the Signature row of the "Tell me about this" card when comparing the program with its signature
    couldn't run.
- `aboutThis.signature.ticketPresent`: A notarization ticket is stapled to it.
  - Where: Line on the Signature row of the "Tell me about this" card: the app carries Apple's notarization ticket
    (Apple scanned it before release). "Notarization" is Apple's term.
- `aboutThis.signature.ticketAbsent`: No notarization ticket is stapled to it. macOS can also check with Apple online.
  - Where: Line on the Signature row of the "Tell me about this" card when the app carries no notarization ticket. The
    second sentence says that this alone doesn't mean it was never notarized.
- `aboutThis.lastOpened.noRecord`: No last-opened record
  - Where: Muted one-line answer on the "Last opened" row of the "Tell me about this" card when the system has no record
    of the item being opened. It doesn't mean it was never opened. No closing period.
- `aboutThis.lastOpened.known`: <date></date>{hasCount, select, yes {, {count, plural, one {opened once} other {opened
  {countText} times}}} other {}}
  - Where: Answer on the "Last opened" row of the "Tell me about this" card: the date the system recorded, and how many
    times it was opened when that's recorded. <date></date> becomes the date. No closing period.
- `aboutThis.importance.band`: {band, select, high {High} medium {Medium} low {Low} other {Not ranked}}
  - Where: Answer on the "Importance" row of the "Tell me about this" card: how important Cmdr's drive index scores the
    folder. "Not ranked" is for folders the ranking leaves out on purpose (caches, build output, hidden, system, and
    temporary folders, app data in the Library); it never says the contents don't matter.
- `aboutThis.importance.because`: Because of {reasons}
  - Where: Muted second line on the "Importance" row of the "Tell me about this" card: the signals behind the score. No
    closing period.
- `aboutThis.importance.reason.projectMarker`: a project file in it
  - Where: One reason in the "Because of …" list on the Importance row: the folder holds a project file (a package.json,
    a Cargo.toml). Lowercase, joins a list.
- `aboutThis.importance.reason.mtimeRecency`: recent changes
  - Where: One reason in the "Because of …" list on the Importance row: files in it changed recently. Lowercase, joins a
    list.
- `aboutThis.importance.reason.visitActivity`: your visits in Cmdr
  - Where: One reason in the "Because of …" list on the Importance row: the user often opens this folder in Cmdr.
    Lowercase, joins a list.
- `aboutThis.importance.reason.lastUsed`: recent use
  - Where: One reason in the "Because of …" list on the Importance row: the system recorded recent use. Lowercase, joins
    a list.
- `aboutThis.importance.reason.extensionDiversity`: many kinds of files
  - Where: One reason in the "Because of …" list on the Importance row: it holds many different kinds of files.
    Lowercase, joins a list.
- `aboutThis.importance.reason.projectRoot`: being a project’s top folder
  - Where: One reason in the "Because of …" list on the Importance row: the folder is the top folder of a software
    project. Lowercase, joins a list.
- `aboutThis.importance.reason.userContent`: holding your own files
  - Where: One reason in the "Because of …" list on the Importance row: it sits among the user's own documents.
    Lowercase, joins a list.
- `aboutThis.importance.floor.nameDenylisted`: Cmdr doesn’t rank folders whose name marks generated files
  - Where: Muted line on the "Importance" row of the "Tell me about this" card, under "Not ranked": Cmdr's folder
    ranking leaves this folder out because its name (like "node_modules" or "cache") marks files a tool generates. It
    describes Cmdr's ranking, never whether the folder's contents matter. No closing period.
- `aboutThis.importance.floor.hiddenOrSystem`: Cmdr doesn’t rank hidden, system, or temporary folders
  - Where: Muted line on the "Importance" row of the "Tell me about this" card, under "Not ranked": Cmdr's folder
    ranking leaves out hidden folders, the system's folders, app data in the Library, and temporary folders. It
    describes Cmdr's ranking, never whether the folder's contents matter. No closing period.
- `aboutThis.importance.floor.underFlooredAncestor`: Cmdr doesn’t rank folders inside one it leaves out
  - Where: Muted line on the "Importance" row of the "Tell me about this" card, under "Not ranked": the folder sits
    inside one Cmdr's folder ranking leaves out (a cache, build output, a hidden folder). It describes Cmdr's ranking,
    never whether the folder's contents matter. No closing period.
- `aboutThis.importance.floor.other`: Cmdr leaves folders like this out of its ranking
  - Where: Muted line on the "Importance" row of the "Tell me about this" card, under "Not ranked", for a reason Cmdr
    has no words for yet. It describes Cmdr's ranking, never whether the folder's contents matter. No closing period.
- `aboutThis.importance.reason.other`: other signals
  - Where: One reason in the "Because of …" list on the Importance row, for a signal Cmdr has no words for yet.
    Lowercase, joins a list.
- `aboutThis.importance.visits`: {count, plural, one {Cmdr visited it once} other {Cmdr visited it {countText} times}}
  - Where: Muted line on the "Importance" row of the "Tell me about this" card: how many times a Cmdr pane opened this
    folder. No closing period.
- `aboutThis.importance.stale`: Scored before the latest changes
  - Where: Muted line on the "Importance" row of the "Tell me about this" card when the score is older than the drive
    index's latest update. No closing period.
- `aboutThis.owner.byBundleId`: {app}
  - Where: Answer on the "Belongs to" row of the "Tell me about this" card when the folder is named after an installed
    app's identifier, so Cmdr is sure. Usually just the app's name; reorder only if your language needs words around it.
- `aboutThis.owner.byBundleIdHow`: The folder carries the app’s identifier, {bundleId}.
  - Where: Muted second line on the "Belongs to" row of the "Tell me about this" card, saying how Cmdr knows the owner:
    the folder's name is the app's identifier.
- `aboutThis.owner.notInstalled`: Left over from an app that isn’t installed ({bundleId})
  - Where: Answer on the "Belongs to" row of the "Tell me about this" card when the folder is named after an app
    identifier and no installed app has it. It doesn't say anything about whether deleting is fine. No closing period.
- `aboutThis.owner.byName`: Probably {app}
  - Where: Answer on the "Belongs to" row of the "Tell me about this" card when exactly one installed app has the
    folder's name, so it's a likely match, not a sure one. No closing period.
- `aboutThis.owner.byNameHow`: Only this app has the folder’s name.
  - Where: Muted second line on the "Belongs to" row of the "Tell me about this" card, saying how Cmdr guessed the
    owner: one installed app has the folder's name.
- `aboutThis.owner.partOfOs`: Part of macOS ({pkgId})
  - Where: Answer on the "Belongs to" row of the "Tell me about this" card when one of Apple's own installer records
    lists the item. No closing period.
- `aboutThis.owner.installedBy`: Installed by the package {pkgId}
  - Where: Answer on the "Belongs to" row of the "Tell me about this" card when an installer package's record lists the
    item. No closing period.
- `aboutThis.owner.homebrew`: Homebrew’s {kind, select, cask {cask} other {formula}} {name}
  - Where: Answer on the "Belongs to" row of the "Tell me about this" card for something Homebrew installed. "Homebrew"
    is the package manager's name, and "cask" and "formula" are its own terms for an app and a command-line tool; keep
    them. No closing period.
- `aboutThis.owner.insideApp`: Part of {app}
  - Where: Answer on the "Belongs to" row of the "Tell me about this" card for something inside an app's bundle. No
    closing period.
- `aboutThis.owner.noReceipt`: No installer record
  - Where: Muted one-line answer on the "Belongs to" row of the "Tell me about this" card when no installer package's
    record lists the item. Ordinary, not a warning. No closing period.
- `aboutThis.owner.ambiguous`: Several installed apps could own this, so Cmdr doesn’t pick one.
  - Where: Answer on the "Belongs to" row of the "Tell me about this" card when several apps match the folder's name (a
    vendor folder like "Google" or "Microsoft").
- `aboutThis.git.repo`: In <repo></repo>{hasBranch, select, yes {, on {branch}} other {, with no branch checked out}}
  - Where: Answer on the Git row of the "Tell me about this" card: the repository the item sits in, and its checked-out
    branch. <repo></repo> becomes the name of the repository's folder, like "cmdr" (its full path shows in a tooltip).
    No closing period.
- `aboutThis.git.copyRepo`: Copy the repository’s path
  - Where: Accessible name and hover hint of the copy button beside the repository's folder on the Git row. It copies
    the full path.
- `aboutThis.git.clean`: Committed, with no changes
  - Where: Second line on the Git row of the "Tell me about this" card: git tracks the item and it matches the last
    commit. No closing period.
- `aboutThis.git.modified`: Has changes no commit has
  - Where: Second line on the Git row of the "Tell me about this" card: the item has changes that aren't committed. No
    closing period.
- `aboutThis.git.untracked`: Git doesn’t track it
  - Where: Second line on the Git row of the "Tell me about this" card: the item is new to git, which doesn't keep a
    copy of it. No closing period.
- `aboutThis.git.ignored`: Git ignores it
  - Where: Second line on the Git row of the "Tell me about this" card: an ignore rule covers the item. No closing
    period.
- `aboutThis.folderSize.known`: {coverage, select, lowerBound {At least <size></size>} other {<size></size>}} in {files,
  plural, one {one file} other {{filesText} files}} and {folders, plural, one {one folder}
  other {{foldersText} folders}}
  - Where: Answer on the Size row of the "Tell me about this" card for a folder: its whole size and how many files and
    folders it holds, from Cmdr's drive index. "At least" when the index hasn't covered all of it. <size></size> becomes
    the size. No closing period.
- `aboutThis.folderSize.stale`: From the drive index’s last pass, so it may have changed since
  - Where: Muted second line on a folder's Size row of the "Tell me about this" card when the index's numbers are older
    than its latest scan. No closing period.
- `aboutThis.folderSize.updating`: The drive index is updating this folder right now
  - Where: Muted second line on a folder's Size row of the "Tell me about this" card while Cmdr's drive index is still
    catching up with changes in it. No closing period.
- `aboutThis.folderSize.symlinks`: Links to other places aren’t counted
  - Where: Muted second line on a folder's Size row of the "Tell me about this" card when the folder holds symbolic
    links, whose targets the size leaves out. No closing period.
- `aboutThis.contents.summary`: {count, plural, =0 {Empty} one {One item} other {{countText} items: {kinds}}}
  - Where: Answer on the Contents row of the "Tell me about this" card for a folder: how many items sit directly in it
    and what kinds they are. No closing period.
- `aboutThis.childKind.folder`: {count, plural, one {{countText} folder} other {{countText} folders}}
  - Where: One part of the list on the Contents row of the "Tell me about this" card: how many items directly in the
    folder are folders. Lowercase, joins a list like "5 folders, 4 images, and 3 documents".
- `aboutThis.childKind.image`: {count, plural, one {{countText} image} other {{countText} images}}
  - Where: One part of the list on the Contents row of the "Tell me about this" card: how many items directly in the
    folder are images. Lowercase, joins a list like "5 folders, 4 images, and 3 documents".
- `aboutThis.childKind.video`: {count, plural, one {{countText} video} other {{countText} videos}}
  - Where: One part of the list on the Contents row of the "Tell me about this" card: how many items directly in the
    folder are videos. Lowercase, joins a list like "5 folders, 4 images, and 3 documents".
- `aboutThis.childKind.audio`: {count, plural, one {{countText} sound file} other {{countText} sound files}}
  - Where: One part of the list on the Contents row of the "Tell me about this" card: how many items directly in the
    folder are sound files. Lowercase, joins a list like "5 folders, 4 images, and 3 documents".
- `aboutThis.childKind.document`: {count, plural, one {{countText} document} other {{countText} documents}}
  - Where: One part of the list on the Contents row of the "Tell me about this" card: how many items directly in the
    folder are documents. Lowercase, joins a list like "5 folders, 4 images, and 3 documents".
- `aboutThis.childKind.archive`: {count, plural,
  one {{countText} archive or disk image} other {{countText} archives or disk images}}
  - Where: One part of the list on the Contents row of the "Tell me about this" card: how many items directly in the
    folder are archives or disk images. Lowercase, joins a list like "5 folders, 4 images, and 3 documents".
- `aboutThis.childKind.code`: {count, plural, one {{countText} code file} other {{countText} code files}}
  - Where: One part of the list on the Contents row of the "Tell me about this" card: how many items directly in the
    folder are code files. Lowercase, joins a list like "5 folders, 4 images, and 3 documents".
- `aboutThis.childKind.app`: {count, plural, one {{countText} app or program} other {{countText} apps or programs}}
  - Where: One part of the list on the Contents row of the "Tell me about this" card: how many items directly in the
    folder are apps or programs. Lowercase, joins a list like "5 folders, 4 images, and 3 documents".
- `aboutThis.childKind.other`: {count, plural, one {{countText} other file} other {{countText} other files}}
  - Where: One part of the list on the Contents row of the "Tell me about this" card: how many items directly in the
    folder are other files. Lowercase, joins a list like "5 folders, 4 images, and 3 documents".
- `aboutThis.folderClass.dependencies`: Installed packages a package manager put here
  - Where: Muted line on the Contents row of the "Tell me about this" card, saying the folder is installed packages
    (like node_modules). Cmdr knows it from the folder's name and the project file beside it. No closing period.
- `aboutThis.folderClass.buildOutput`: Output a build tool wrote
  - Where: Muted line on the Contents row of the "Tell me about this" card, saying the folder is a build tool's output
    (like a Rust project's target folder). Cmdr knows it from the folder's name and the project file beside it. No
    closing period.
- `aboutThis.folderClass.cache`: A cache
  - Where: Muted line on the Contents row of the "Tell me about this" card, saying the folder is a cache that whatever
    wrote it writes again. Cmdr knows it from the folder's name and the project file beside it. No closing period.
- `aboutThis.folderClass.versionControl`: A repository’s history and settings
  - Where: Muted line on the Contents row of the "Tell me about this" card, saying the folder is version-control data
    (like a .git folder). Cmdr knows it from the folder's name and the project file beside it. No closing period.
- `aboutThis.contents.atLeastSize`: at least <size></size>
  - Where: A size on the "Tell me about this" card that may be larger, because Cmdr's index hasn't listed all of the
    folder yet. <size></size> becomes the size, like "2.1 GB".
- `aboutThis.contents.noSize`: Not sized yet
  - Where: Stands in for the size of one folder in a list on the "Tell me about this" card when Cmdr's index has no size
    for it yet. Short, no closing period.
- `aboutThis.contents.inFolder`: in {path}
  - Where: Muted text after a file's name in the "Largest files" list of the "Tell me about this" card: the folder it
    sits in, inside the folder the card is about. No closing period.
- `aboutThis.contents.goTo`: Go to {name}
  - Where: Tooltip on one entry in a list on the "Tell me about this" card: pressing it closes the card and shows that
    file or folder in the file list.
- `aboutThis.contents.notLoaded`: Cmdr lists these once Search has loaded this drive
  - Where: Muted line on the "Largest files" row of the "Tell me about this" card for a folder, when Cmdr's search index
    for this drive isn't in memory yet (it loads when someone uses Search). No closing period.
- `aboutThis.contents.unchanged`: {atLeast, select, yes {At least {percent} of it hasn’t changed in a year
  (<size></size>)} other {{percent} of it hasn’t changed in a year (<size></size>)}}
  - Where: Answer on the "Untouched" row of the "Tell me about this" card for a folder: the share of its bytes in files
    nobody changed in the last year. "At least" when Cmdr counted only the largest of them. <size></size> becomes the
    bytes, like "1.2 GB". No closing period.
- `aboutThis.appData.summary`: {count, plural, =0 {None that Cmdr’s index knows of}
  one {{atLeast, select, yes {At least <size></size> in one folder} other {<size></size> in one folder}}}
  other {{atLeast, select, yes {At least <size></size> in {countText} folders} other {<size></size> in {countText} folders}}}}
  - Where: Answer on the "Its data in your Library" row of the "Tell me about this" card for an app: how much data sits
    in the Library folders named after the app, which stay behind if the app is removed. "At least" when Cmdr's index
    hasn't listed some of it. <size></size> becomes the total, like "2.3 GB". No closing period.
- `aboutThis.appData.notLookedIn`: Cmdr’s index hasn’t listed {places} yet, so there may be more
  - Where: Muted line on the "Its data in your Library" row of the "Tell me about this" card: Library folders Cmdr's
    index hasn't read yet, so some of the app's data may be missing from the list. No closing period.
- `aboutThis.ownerPlace.caches`: Caches
  - Where: The Library folder an app's data sits in, shown muted beside it in the "Its data in your Library" list of the
    "Tell me about this" card. The folder's real name inside the macOS Library folder; keep it as the system names it.
- `aboutThis.ownerPlace.containers`: Containers
  - Where: The Library folder an app's data sits in, shown muted beside it in the "Its data in your Library" list of the
    "Tell me about this" card. The folder's real name inside the macOS Library folder; keep it as the system names it.
- `aboutThis.ownerPlace.groupContainers`: Group Containers
  - Where: The Library folder an app's data sits in, shown muted beside it in the "Its data in your Library" list of the
    "Tell me about this" card. The folder's real name inside the macOS Library folder; keep it as the system names it.
- `aboutThis.ownerPlace.applicationSupport`: Application Support
  - Where: The Library folder an app's data sits in, shown muted beside it in the "Its data in your Library" list of the
    "Tell me about this" card. The folder's real name inside the macOS Library folder; keep it as the system names it.
- `aboutThis.ownerPlace.applicationScripts`: Application Scripts
  - Where: The Library folder an app's data sits in, shown muted beside it in the "Its data in your Library" list of the
    "Tell me about this" card. The folder's real name inside the macOS Library folder; keep it as the system names it.
- `aboutThis.ownerPlace.preferences`: Preferences
  - Where: The Library folder an app's data sits in, shown muted beside it in the "Its data in your Library" list of the
    "Tell me about this" card. The folder's real name inside the macOS Library folder; keep it as the system names it.
- `aboutThis.ownerPlace.savedState`: Saved Application State
  - Where: The Library folder an app's data sits in, shown muted beside it in the "Its data in your Library" list of the
    "Tell me about this" card. The folder's real name inside the macOS Library folder; keep it as the system names it.
- `aboutThis.ownerPlace.logs`: Logs
  - Where: The Library folder an app's data sits in, shown muted beside it in the "Its data in your Library" list of the
    "Tell me about this" card. The folder's real name inside the macOS Library folder; keep it as the system names it.
- `aboutThis.ownerPlace.webData`: HTTPStorages
  - Where: The Library folder an app's data sits in, shown muted beside it in the "Its data in your Library" list of the
    "Tell me about this" card. The folder's real name inside the macOS Library folder; keep it as the system names it.
- `aboutThis.ownerPlace.sandboxHome`: App home
  - Where: The Library folder an app's data sits in, shown muted beside it in the "Its data in your Library" list of the
    "Tell me about this" card. A sandboxed Linux app's own home folder (~/.var/app or ~/snap).
- `aboutThis.removal.uncommittedWork`: It has changes no commit holds, so git can’t bring those back.
  - Where: One line under "What removing it means" on the "Tell me about this" card: the item has uncommitted changes in
    git. A consequence, never advice.
- `aboutThis.removal.untrackedNotInGit`: Git doesn’t track it, so git can’t bring it back.
  - Where: One line under "What removing it means": the item is in a git repository but git keeps no copy of it. A
    consequence, never advice.
- `aboutThis.removal.ignoredByGit`: Git ignores it, so git can’t bring it back.
  - Where: One line under "What removing it means": an ignore rule covers the item, so git keeps no copy. It doesn't
    claim any tool can recreate it. A consequence, never advice.
- `aboutThis.removal.userDocument`: Nothing Cmdr knows of made this or could make it again.
  - Where: One line under "What removing it means": no app, installer, or tool Cmdr knows of created the item or could
    recreate it, so it's likely the person's own. A consequence, never advice.
- `aboutThis.removal.removesApp`: This removes the app. Cmdr can’t tell yet how much of its data stays in your Library.
  - Where: One line under "What removing it means" for an app, when Cmdr doesn't know (yet) which of its folders in the
    user's Library would stay behind. "Library" is the macOS folder of that name.
- `aboutThis.removal.removesAppDataLeft`: {count, plural, =0 {This removes the app. Cmdr’s index knows of none of its
  data in your Library.}
  one {{atLeast, select, yes {This removes the app. Its data in your Library stays: at least <size></size> in one folder.} other {This removes the app. Its data in your Library stays: <size></size> in one folder.}}}
  other {{atLeast, select, yes {This removes the app. Its data in your Library stays: at least <size></size> in {countText} folders.} other {This removes the app. Its data in your Library stays: <size></size> in {countText} folders.}}}}
  - Where: One line under "What removing it means" for an app: how much of the app's data in the user's Library would
    stay behind. <size></size> becomes the total size. "At least" when Cmdr's index hasn't listed some of it.
- `aboutThis.removal.appData`: {certainty, select, probable {It’s probably {app}’s data, and {app} may lose what it
  keeps here.} other {It’s {app}’s data, and {app} may lose what it keeps here.}}
  - Where: One line under "What removing it means": the item holds an installed app's data, and that app may lose
    settings or content. "probably" when Cmdr matched it by name only. A consequence, never advice.
- `aboutThis.removal.installedByReceipt`: {partOfOs, select, yes {macOS installed it, and parts of macOS may need it.}
  other {The package {pkgId} installed it, and what it installed may need it.}}
  - Where: One line under "What removing it means": an installer put the item here, and the software it installed may
    need it. A consequence, never advice.
- `aboutThis.removal.installedByHomebrew`: Homebrew installed it as part of {name}, which may stop working.
  - Where: One line under "What removing it means": Homebrew (a package manager; keep the name) installed the item, and
    that software may stop working. A consequence, never advice.
- `aboutThis.removal.leftoverOfUninstalledApp`: It’s named after {bundleId}, an app that isn’t installed. Cmdr can’t
  tell whether anything else uses it.
  - Where: One line under "What removing it means": the folder is named after an app that isn't installed. The second
    sentence keeps it honest: Cmdr doesn't know whether something else still uses it. Never say it's safe to delete.
- `aboutThis.removal.recordedDownload`: {platform, select, macos {macOS recorded this as downloaded from {host}. Cmdr
  can’t tell whether the site still has it.} other {Your browser recorded this as downloaded from {host}. Cmdr can’t
  tell whether the site still has it.}}
  - Where: One line under "What removing it means" for a downloaded file. It says only what was recorded; it never
    promises the file can be downloaded again.
- `aboutThis.removal.gitCanRestore`: Git can restore it from the last commit.
  - Where: One line under "What removing it means": git tracks the item with no changes, so git can bring it back. A
    consequence, never advice.
- `aboutThis.removal.rebuildablePackageManager`: A package manager can fetch this again, which takes time and a network.
  - Where: One line under "What removing it means" for installed dependencies (like node_modules): a package manager can
    download them again. A consequence, never advice.
- `aboutThis.removal.rebuildableBuildTool`: A build tool can make this again, which takes time.
  - Where: One line under "What removing it means" for build output: the build tool that made it can make it again. A
    consequence, never advice.
- `aboutThis.removal.regeneratedCache`: {hasApp, select, yes {It’s {app}’s cache: {app} writes it again, and is slower
  until it has.} other {It’s a cache: whatever wrote it writes it again, and is slower until it has.}}
  - Where: One line under "What removing it means" for a cache folder. A consequence, never advice.
- `aboutThis.removal.otherNamesRemain`: {count, plural, one {One other name points to the same contents, which stays.}
  other {{countText} other names point to the same contents, which stays.}}
  - Where: One line under "What removing it means" for a file with hard links: removing this name leaves the contents
    reachable through the others. A consequence, never advice.
- `aboutThis.removal.linkOnly`: It’s a link: removing it leaves what it points to.
  - Where: One line under "What removing it means" for a symbolic link. A consequence, never advice.
- `aboutThis.removal.cantTell`: Cmdr can’t tell what depends on this.
  - Where: One line under "What removing it means" when Cmdr doesn't know enough to say. An honest answer, never a
    warning or advice.
- `aboutThis.removal.more`: {count, plural, one {One more} other {{countText} more}}
  - Where: Button under "What removing it means" that shows the rest of the consequences beyond the first two. No
    closing period.
- `aboutThis.removal.less`: Show fewer
  - Where: Button under "What removing it means" that hides the extra consequences again. No closing period.
