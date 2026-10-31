/**
 * The `/trust/accessibility` page's content: Cmdr's accessibility conformance report, based on
 * ITI's VPAT 2.5Rev INT edition (WCAG 2.2 A/AA, Revised Section 508, EN 301 549).
 * `src/pages/trust/accessibility.astro` is a template over this and owns the prose sections.
 *
 * ❗ The evidence behind every row is `docs/notes/accessibility-audit-2026-10.md` (file paths, code
 * references, and contrast math). Remarks here are for buyers: plain, specific, no file paths. When
 * a gap closes, update the row and its remark, and bump `report.date`.
 *
 * Screen-reader evidence: David ran his VoiceOver guide (vault, `projects/Cmdr/2026-10-14 Cmdr
 * VoiceOver test guide.md`, checks V1–V12) by ear and heard every check pass on 2026-10-15. The
 * audit doc's § VoiceOver checks maps each check to the rows it confirms. Remarks that say what
 * VoiceOver reads rest on that run. The audit doc's § Re-hear lists the checks whose code has
 * changed since, and § How to re-evaluate is the procedure for the next pass.
 *
 * Write straight quotes; `smart-quotes.ts` curls them in the built HTML. Remarks are plain text.
 */

export type Conformance =
  | 'supports'
  | 'partially-supports'
  | 'does-not-support'
  | 'not-applicable'
  /** A row the VPAT answers by pointing elsewhere; `seeLabel` says where. */
  | 'see-other'

/** Labels for the page's tables, in the VPAT's own terms. */
export const conformanceLabel: Record<Conformance, string> = {
  supports: 'Supports',
  'partially-supports': 'Partially supports',
  'does-not-support': 'Does not support',
  'not-applicable': 'Not applicable',
  'see-other': 'See remarks',
}

/** What each term means, shown in the report's "Terms" section. */
export const conformanceTerms: { term: Conformance; meaning: string }[] = [
  {
    term: 'supports',
    meaning:
      'The functionality of the product has at least one method that meets the criterion without known defects, or meets it with equivalent facilitation.',
  },
  { term: 'partially-supports', meaning: "Some functionality of the product doesn't meet the criterion." },
  { term: 'does-not-support', meaning: "The majority of product functionality doesn't meet the criterion." },
  { term: 'not-applicable', meaning: "The criterion isn't relevant to the product." },
]

export const report = {
  product: 'Cmdr for macOS',
  version: '0.57.0',
  /** The evaluation date, also the page's "Last updated". */
  date: '2026-10-30',
  edition: 'VPAT® 2.5Rev INT (April 2025)',
}

export interface CriterionRow {
  /** The criterion's number, like `1.4.3` or `502.3.12`. */
  id: string
  name: string
  /** WCAG level. Only set in the WCAG tables. */
  level?: 'A' | 'AA'
  conformance: Conformance
  /** For a `'see-other'` row: the level column's text, like "See WCAG 2.2 tables". */
  seeLabel?: string
  remark: string
  /** GitHub issues in `vdavid/cmdr` that track this row's gaps. */
  issues?: number[]
}

export interface ReportTable {
  /** Anchor id on the page. */
  id: string
  title: string
  /** Optional plain-text note under the heading. */
  note?: string
  rows: CriterionRow[]
}

export interface StandardSection {
  id: string
  title: string
  tables: ReportTable[]
}

const wcagA: CriterionRow[] = [
  {
    id: '1.1.1',
    name: 'Non-text content',
    level: 'A',
    conformance: 'supports',
    remark:
      'Icon buttons have names, the image viewer uses the file name as the image\'s text alternative, and the sync-status and symlink badges on file icons are named. Each file\'s name for assistive technology also says its kind (folder, symlink, package, or archive), so the file-type icon needs no text of its own: VoiceOver reads "Photos (folder)" and "backup.zip (archive)". Pictures in a Markdown file keep the text alternative the document gives them.',
  },
  {
    id: '1.2.1',
    name: 'Audio-only and video-only (prerecorded)',
    level: 'A',
    conformance: 'not-applicable',
    remark: 'Cmdr has no audio or video content. Its viewer shows text, images, PDFs, and formatted Markdown.',
  },
  {
    id: '1.2.2',
    name: 'Captions (prerecorded)',
    level: 'A',
    conformance: 'not-applicable',
    remark: 'No audio or video content.',
  },
  {
    id: '1.2.3',
    name: 'Audio description or media alternative (prerecorded)',
    level: 'A',
    conformance: 'not-applicable',
    remark: 'No audio or video content.',
  },
  {
    id: '1.3.1',
    name: 'Info and relationships',
    level: 'A',
    conformance: 'partially-supports',
    remark:
      "Dialogs, Settings, and server forms use real headings, labels, and roles, and the Settings sidebar groups each section's subsections under it. The file list exposes which files are marked and each file's position in the list, and the sort buttons say which column the list is sorted by and which way. The Full view isn't a table: each file's name for assistive technology carries its size and date, but the column headers aren't tied to the cells.",
    issues: [451],
  },
  {
    id: '1.3.2',
    name: 'Meaningful sequence',
    level: 'A',
    conformance: 'supports',
    remark:
      'The reading order matches the visual order in every window, including long file lists. VoiceOver reads dialogs and Settings top to bottom, as they appear on screen.',
  },
  {
    id: '1.3.3',
    name: 'Sensory characteristics',
    level: 'A',
    conformance: 'supports',
    remark:
      'Instructions don\'t rely on shape, position, or sound alone. The two panes are named "Left file pane" and "Right file pane".',
  },
  {
    id: '1.4.1',
    name: 'Use of color',
    level: 'A',
    conformance: 'partially-supports',
    remark:
      "Git status uses letters, errors use labeled symbols, and size and date colors are optional decoration. Marked files carry a bar along their left edge and slightly heavier text besides their red color. By default the focused pane differs from the other by color: its cursor outline and fill are in the accent color where the other pane's are gray, and its active tab has the stronger accent line. With the macOS Differentiate without color or Increase contrast setting on, the other pane's cursor outline also turns dashed, so the panes differ by shape too. Hidden files still differ only by a dimmer color.",
    issues: [452, 454],
  },
  { id: '1.4.2', name: 'Audio control', level: 'A', conformance: 'not-applicable', remark: 'Cmdr plays no audio.' },
  {
    id: '2.1.1',
    name: 'Keyboard',
    level: 'A',
    conformance: 'supports',
    remark:
      'Every core action has a keyboard shortcut and a command palette entry: switching panes, navigating, marking, opening, every file operation, tabs (including reordering them and moving one to the other pane), sorting, view modes, and Settings. "Move focus to the notifications" reaches the buttons in notifications, and Undo after moving files to the trash is also in the Edit menu. Going straight to a Search result\'s folder by clicking part of its path takes the mouse; from the keyboard, open the result, then go up.',
  },
  {
    id: '2.1.2',
    name: 'No keyboard trap',
    level: 'A',
    conformance: 'supports',
    remark: 'Dialogs keep focus inside while open and always release it on Escape or their buttons.',
  },
  {
    id: '2.1.4',
    name: 'Character key shortcuts',
    level: 'A',
    conformance: 'supports',
    remark:
      'Single-key shortcuts in the file list (like * and +) can be remapped and work only while the file list has focus. In the file viewer, single-key shortcuts (W, F, 0–3, + and -) work anywhere in the window, and each one can be remapped or turned off in Settings. Letter keys in dialogs and the queue window (like S for Skip) work only while that dialog or window has focus.',
  },
  {
    id: '2.2.1',
    name: 'Timing adjustable',
    level: 'A',
    conformance: 'supports',
    remark:
      "The only time limits are notifications, which close after 4–10 seconds. Hovering over one or moving keyboard focus into it holds it, and a setting keeps every notification until it's closed. The 15-second countdown when quitting during a file operation is essential: it follows the macOS logout deadline.",
  },
  {
    id: '2.2.2',
    name: 'Pause, stop, hide',
    level: 'A',
    conformance: 'supports',
    remark:
      'Moving content is limited to activity indicators like spinners, which stop under the macOS Reduce motion setting. The live view of a running copy stops following when the user scrolls.',
  },
  {
    id: '2.3.1',
    name: 'Three flashes or below threshold',
    level: 'A',
    conformance: 'supports',
    remark: "Nothing flashes more than once per second: one short warning flash and the text cursor's blink.",
  },
  {
    id: '2.4.1',
    name: 'Bypass blocks',
    level: 'A',
    conformance: 'not-applicable',
    remark: 'Applies to web pages and sets of software. Cmdr is a single app.',
  },
  {
    id: '2.4.2',
    name: 'Page titled',
    level: 'A',
    conformance: 'supports',
    remark:
      'Every window has a title in the app\'s language: "Cmdr" for the main window, the file name in the viewer, and a name for Settings, Keyboard shortcuts, and the queue.',
  },
  {
    id: '2.4.3',
    name: 'Focus order',
    level: 'A',
    conformance: 'supports',
    remark:
      'Dialogs and the command palette move focus in when they open, keep Tab in a logical order, and return focus to where it was when they close.',
  },
  {
    id: '2.4.4',
    name: 'Link purpose (in context)',
    level: 'A',
    conformance: 'supports',
    remark: 'Links and buttons in the app name where they go or what they do.',
  },
  {
    id: '2.5.1',
    name: 'Pointer gestures',
    level: 'A',
    conformance: 'supports',
    remark:
      'Swipe gestures and mouse side buttons for back and forward have keyboard and menu equivalents. Pinch and scroll-wheel zoom in the viewer have keyboard equivalents.',
  },
  {
    id: '2.5.2',
    name: 'Pointer cancellation',
    level: 'A',
    conformance: 'supports',
    remark:
      'Pressing the mouse button on a file only moves the cursor, selects, or starts a drag. Opening needs a double-click, and buttons act on release.',
  },
  {
    id: '2.5.3',
    name: 'Label in name',
    level: 'A',
    conformance: 'supports',
    remark:
      'File list rows, buttons, and other controls have an accessible name that contains their visible text, in order, so speech control can use the words on screen. An automated check tests this on every change, with no exceptions.',
  },
  {
    id: '2.5.4',
    name: 'Motion actuation',
    level: 'A',
    conformance: 'not-applicable',
    remark: 'Cmdr uses no device-motion input.',
  },
  {
    id: '3.1.1',
    name: 'Language of page',
    level: 'A',
    conformance: 'supports',
    remark: 'Every window declares the language the user chose for the app (11 languages).',
  },
  {
    id: '3.2.1',
    name: 'On focus',
    level: 'A',
    conformance: 'supports',
    remark: 'No control changes the context when it gets focus.',
  },
  {
    id: '3.2.2',
    name: 'On input',
    level: 'A',
    conformance: 'supports',
    remark:
      'Settings apply as soon as they change, which changes a setting, not the context. The Settings sidebar switches its section with the arrow keys, like tabs.',
  },
  {
    id: '3.2.6',
    name: 'Consistent help',
    level: 'A',
    conformance: 'supports',
    remark:
      "The macOS Help menu is in the same place in every window: keyboard shortcuts, what's new, feedback, and error reports.",
  },
  {
    id: '3.3.1',
    name: 'Error identification',
    level: 'A',
    conformance: 'supports',
    remark:
      "Rename, new folder, server, license key, and archive and backup password forms mark the field in error and tie the message to it. VoiceOver reads an invalid file name's message as soon as it appears, and a failed license activation is also announced as an alert.",
  },
  {
    id: '3.3.2',
    name: 'Labels or instructions',
    level: 'A',
    conformance: 'supports',
    remark:
      'Settings and server forms have visible labels. The license key, rename, and new folder fields have a visible prompt and an accessible name.',
  },
  {
    id: '3.3.7',
    name: 'Redundant entry',
    level: 'A',
    conformance: 'supports',
    remark:
      'The contact email is remembered and filled in for feedback and error reports. Server passwords are kept in the macOS Keychain.',
  },
  {
    id: '4.1.1',
    name: 'Parsing',
    level: 'A',
    conformance: 'supports',
    remark: 'WCAG 2.2 treats this criterion as always met.',
  },
  {
    id: '4.1.2',
    name: 'Name, role, value',
    level: 'A',
    conformance: 'supports',
    remark:
      'Dialogs, Settings controls, tabs, the command palette, the queue, and in-app menus (submenus included) expose their name, role, and state, and the file list, Search results, and the Settings sidebar convey the highlighted item as you arrow. The file list also conveys which files are marked, the volume switcher is a menu button, and the name field for renaming a file in place is a real text field.',
  },
]

const wcagAA: CriterionRow[] = [
  {
    id: '1.2.4',
    name: 'Captions (live)',
    level: 'AA',
    conformance: 'not-applicable',
    remark: 'No live audio or video.',
  },
  {
    id: '1.2.5',
    name: 'Audio description (prerecorded)',
    level: 'AA',
    conformance: 'not-applicable',
    remark: 'No video.',
  },
  {
    id: '1.3.4',
    name: 'Orientation',
    level: 'AA',
    conformance: 'supports',
    remark: "Windows don't lock to an orientation.",
  },
  {
    id: '1.3.5',
    name: 'Identify input purpose',
    level: 'AA',
    conformance: 'supports',
    remark: 'Server sign-in fields and every email field declare their purpose, so autofill works.',
  },
  {
    id: '1.4.3',
    name: 'Contrast (minimum)',
    level: 'AA',
    conformance: 'partially-supports',
    remark:
      'An automated check on every change measures every text color against its background in light and dark mode, with all macOS accent colors and pane tints, at 4.5:1 (3:1 for large text). Marked file names measure at least 4.52:1. The check also covers the stronger colors the macOS Increase contrast setting turns on. One exception: the optional "App" date and size colors can put a pale accent color on text (about 2.4:1 with the gold accent).',
    issues: [454],
  },
  {
    id: '1.4.4',
    name: 'Resize text',
    level: 'AA',
    conformance: 'supports',
    remark:
      'App zoom (Command-plus and Command-minus, 75–200%) reaches 200% on its own and combines with the macOS text size setting to go further. Rows grow with the text.',
  },
  {
    id: '1.4.5',
    name: 'Images of text',
    level: 'AA',
    conformance: 'supports',
    remark: 'All text in the app is real text. There are no images of text.',
  },
  {
    id: '1.4.10',
    name: 'Reflow',
    level: 'AA',
    conformance: 'partially-supports',
    remark:
      "Dialogs and Settings are one column wide, and their minimum widths grow with the text size. The main window has a minimum size of 950 by 550 points and a two-pane layout with columns. The file grid needs both dimensions, which WCAG allows, but the controls around it don't reflow. A manual check at large text sizes is pending.",
  },
  {
    id: '1.4.11',
    name: 'Non-text contrast',
    level: 'AA',
    conformance: 'partially-supports',
    remark:
      'Checkbox, switch, and text field borders reach at least 3:1 (about 3.7:1 on white), and the bar on marked files at least 4.5:1. With the macOS Increase contrast or Differentiate without color setting on, the cursor outlines that tell the two panes apart reach at least 3:1, and with Increase contrast every focus ring gains an inner ring that clears 3:1 with any accent color. Without those settings, the cursor outlines are deliberately subtle (about 1.1:1 for the accent one with the yellow accent and 1.4:1 for the gray one, in light mode), and in light mode the focus ring is under 3:1 with the yellow, gold, green, and orange accent colors.',
    issues: [454],
  },
  {
    id: '1.4.12',
    name: 'Text spacing',
    level: 'AA',
    conformance: 'partially-supports',
    remark:
      "File list rows have a fixed height. Larger line spacing fits the default row density but gets cut off in the Compact density, and shortened file names don't account for extra letter or word spacing. A manual text-spacing test is pending.",
  },
  {
    id: '1.4.13',
    name: 'Content on hover or focus',
    level: 'AA',
    conformance: 'supports',
    remark:
      'Tooltips appear on hover and on keyboard focus, stay while the pointer rests on them or their control or while focus stays, and any key closes them.',
  },
  {
    id: '2.4.5',
    name: 'Multiple ways',
    level: 'AA',
    conformance: 'not-applicable',
    remark: 'Applies to sets of web pages and sets of software. Cmdr is a single app.',
  },
  {
    id: '2.4.6',
    name: 'Headings and labels',
    level: 'AA',
    conformance: 'supports',
    remark:
      'Settings uses headings for its sections, dialogs have a heading for their title, and both panes are named. Each file list is named after its pane and folder, and VoiceOver says the new pane and folder, like "Right pane, Documents", after you switch panes or open a folder.',
  },
  {
    id: '2.4.7',
    name: 'Focus visible',
    level: 'AA',
    conformance: 'partially-supports',
    remark:
      "Buttons, links, fields, and menus show a focus ring, and the file cursor has a colored background and outline. The focused pane's cursor outline is in the accent color and the other pane's gray, and the focused pane's active tab carries a stronger accent line than the other pane's. By default these cues rely on color and the line's strength and stay under 3:1 contrast on purpose; the macOS Increase contrast and Differentiate without color settings dash the other pane's outline and take the outlines past 3:1.",
    issues: [454],
  },
  {
    id: '2.4.11',
    name: 'Focus not obscured (minimum)',
    level: 'AA',
    conformance: 'supports',
    remark:
      'Dialogs open in the middle of the window. Notifications appear at the top right and move to the bottom right while they would cover the row the keyboard is on.',
  },
  {
    id: '2.5.7',
    name: 'Dragging movements',
    level: 'AA',
    conformance: 'supports',
    remark:
      'Resizing the panes, reordering tabs, moving a tab to the other pane, and copying or moving files by drag and drop all have keyboard and command palette alternatives. Reordering the queue by dragging has keyboard shortcuts and a right-click menu too.',
  },
  {
    id: '2.5.8',
    name: 'Target size (minimum)',
    level: 'AA',
    conformance: 'supports',
    remark:
      "Column headers, the function key bar, the new-tab button, the tab close button, and the small buttons that clear a field or dismiss a notification are at least 24 by 24 pixels. Path segments above the file list take the full height of their bar and are at least 24 pixels wide, even for a folder with a one-letter name. The divider between the panes has a 24 by 24 pixel grab area at its bottom end, beside the status bar; along the file list it stays 8 pixels wide so it doesn't cover files or the scroll bar. The small arrow beside the date column header opens a menu that a right-click anywhere on the header also opens, and the drag handle on a queue row moves it the way that row's right-click menu does.",
  },
  {
    id: '3.1.2',
    name: 'Language of parts',
    level: 'AA',
    conformance: 'supports',
    remark:
      'The language picker shows each language in its own name and marks which language that name is in, so a screen reader pronounces it right.',
  },
  {
    id: '3.2.3',
    name: 'Consistent navigation',
    level: 'AA',
    conformance: 'not-applicable',
    remark: 'Applies to sets of web pages and sets of software. The macOS menu bar is the same in every window.',
  },
  {
    id: '3.2.4',
    name: 'Consistent identification',
    level: 'AA',
    conformance: 'not-applicable',
    remark: 'Applies to sets of web pages and sets of software. Each command has one name everywhere in the app.',
  },
  {
    id: '3.3.3',
    name: 'Error suggestion',
    level: 'AA',
    conformance: 'supports',
    remark: 'Error messages say what happened and what to try next.',
  },
  {
    id: '3.3.4',
    name: 'Error prevention (legal, financial, data)',
    level: 'AA',
    conformance: 'supports',
    remark:
      'Deleting always asks first, and deleting permanently asks even when confirmations are turned off. Overwrites and name clashes open a dialog, moving to the trash can be undone (from its notification or the Edit menu, which names what it puts back), and copies and moves can be rolled back. Undo and rollback rest on the operation log, which is on by default; while a user turns it off, Cmdr says so wherever it would offer an undo.',
  },
  {
    id: '3.3.8',
    name: 'Accessible authentication (minimum)',
    level: 'AA',
    conformance: 'supports',
    remark:
      'License keys, API keys, and passwords can be pasted, and nothing asks the user to solve a puzzle or remember a code.',
  },
  {
    id: '4.1.3',
    name: 'Status messages',
    level: 'AA',
    conformance: 'supports',
    remark:
      'Notifications, progress and the result of file operations, folder errors, an empty folder, a slow load, the number of marked files, and the result counts of Search (a search that finds nothing included), the file filter, the command palette, and the Settings search are announced without moving focus. Warnings and failures in notifications are announced right away, everything else politely.',
  },
]

const section508Ch3: CriterionRow[] = [
  {
    id: '302.1',
    name: 'Without vision',
    conformance: 'partially-supports',
    remark:
      "Cmdr works with VoiceOver. The file list reads the file under the cursor with its kind and position, and says which files are marked and which pane and folder you're in. In-app menus read every row, submenus included, and dialogs, Settings, Search, file operation progress, notifications, and the file viewer read in full. The Full view's columns aren't a table.",
    issues: [451],
  },
  {
    id: '302.2',
    name: 'With limited vision',
    conformance: 'partially-supports',
    remark:
      'App zoom alone takes text to 200%, and the macOS text size setting takes it further. Dark mode is supported, and text contrast is checked automatically. The focused-pane cues lean on color and stay low in contrast, and some focus rings have weak contrast with some accent colors, unless the macOS Increase contrast or Differentiate without color setting is on.',
    issues: [454],
  },
  {
    id: '302.3',
    name: 'Without perception of color',
    conformance: 'partially-supports',
    remark:
      "Marked files carry a bar and heavier text besides their color, and the focused pane's active tab shows a stronger accent line than the other pane's. The two panes' cursor outlines differ only by color unless the macOS Differentiate without color or Increase contrast setting is on, which dashes the other pane's outline. Hidden files still differ only by color. Everything else uses symbols and text.",
    issues: [452, 454],
  },
  { id: '302.4', name: 'Without hearing', conformance: 'supports', remark: 'Cmdr uses no sound.' },
  { id: '302.5', name: 'With limited hearing', conformance: 'supports', remark: 'Cmdr uses no sound.' },
  { id: '302.6', name: 'Without speech', conformance: 'supports', remark: 'Cmdr needs no speech input.' },
  {
    id: '302.7',
    name: 'With limited manipulation',
    conformance: 'supports',
    remark:
      'Everything core works from the keyboard with remappable shortcuts, reordering and moving tabs included, and no shortcut needs more than modifiers plus one key, so Sticky Keys works. Click targets are at least 24 by 24 pixels; see WCAG 2.5.8.',
  },
  {
    id: '302.8',
    name: 'With limited reach and strength',
    conformance: 'supports',
    remark: 'Cmdr is software and needs no physical reach.',
  },
  {
    id: '302.9',
    name: 'With limited language, cognitive, and learning abilities',
    conformance: 'supports',
    remark:
      "Plain, conversational wording, confirmations before risky actions, undo and rollback, the same menus everywhere, and a setting that keeps notifications until they're closed.",
  },
]

const section508Ch5: CriterionRow[] = [
  {
    id: '501.1',
    name: 'Scope, incorporation of WCAG 2.0 AA',
    conformance: 'see-other',
    seeLabel: 'See WCAG 2.2 tables',
    remark: 'The WCAG 2.2 tables cover every WCAG 2.0 criterion.',
  },
  {
    id: '502.2.1',
    name: 'User control of accessibility features',
    conformance: 'not-applicable',
    remark: "Cmdr isn't platform software.",
  },
  {
    id: '502.2.2',
    name: 'No disruption of accessibility features',
    conformance: 'supports',
    remark:
      "VoiceOver's keyboard commands pass through, macOS zoom works, and the menus are native. In the main window, the Tab key switches panes (a remappable shortcut), so macOS Full Keyboard Access doesn't move through the pane controls; Control-F5 (also remappable) moves focus to them, and the arrow keys walk them.",
  },
  {
    id: '502.3.1',
    name: 'Object information',
    conformance: 'supports',
    remark:
      'Standard controls expose their name, role, and state to VoiceOver. VoiceOver reads each file row\'s name, kind, and position, like "notes.txt, 3 of 8", and says "selected" on marked files. The name field for renaming a file in place is a real text field.',
  },
  {
    id: '502.3.2',
    name: 'Modification of object information',
    conformance: 'supports',
    remark:
      'Checkboxes, switches, and dropdowns are standard controls that assistive technology can change, and VoiceOver reads the new value after a change.',
  },
  {
    id: '502.3.3',
    name: 'Row, column, and headers',
    conformance: 'partially-supports',
    remark:
      "The queue window is a grid with headers. The Full view of the file list has column headers, but they aren't tied to the cells; each file's name for assistive technology says its size and date instead.",
    issues: [451],
  },
  {
    id: '502.3.4',
    name: 'Values',
    conformance: 'supports',
    remark:
      'Progress bars, sliders, and number fields expose their values. VoiceOver reads file operation progress with the amount, like "1.2 GB of 3 GB".',
  },
  {
    id: '502.3.5',
    name: 'Modification of values',
    conformance: 'supports',
    remark:
      'Sliders, number fields, and dropdowns are standard controls that assistive technology can operate. VoiceOver reads their label and value.',
  },
  {
    id: '502.3.6',
    name: 'Label relationships',
    conformance: 'supports',
    remark:
      "Fields are tied to their labels and error messages, and confirmation dialogs to their message, throughout. VoiceOver reads the rename field's name, and a confirmation dialog's title and message when it opens.",
  },
  {
    id: '502.3.7',
    name: 'Hierarchical relationships',
    conformance: 'partially-supports',
    remark:
      "Dialogs, menus, the tab bar, and the Settings sidebar's sections and subsections are structured. Tabs aren't tied to their content.",
    issues: [451],
  },
  {
    id: '502.3.8',
    name: 'Text',
    conformance: 'supports',
    remark:
      'All text in the app is real text that the macOS accessibility interface can read. VoiceOver reads error messages and text files in the file viewer in full.',
  },
  {
    id: '502.3.9',
    name: 'Modification of text',
    conformance: 'supports',
    remark: 'Text entry uses standard text fields.',
  },
  {
    id: '502.3.10',
    name: 'List of actions',
    conformance: 'supports',
    remark: 'Controls are standard buttons and widgets with standard actions, and VoiceOver reads them.',
  },
  {
    id: '502.3.11',
    name: 'Actions on objects',
    conformance: 'supports',
    remark: 'Assistive technology can press buttons and operate controls, like the buttons of a delete confirmation.',
  },
  {
    id: '502.3.12',
    name: 'Focus cursor',
    conformance: 'supports',
    remark:
      "Focus in dialogs and Settings is real keyboard focus, and the cursor in the file list, Search results, and the Settings sidebar is exposed to assistive technology as the focused list's current item. VoiceOver reads each item as you arrow.",
  },
  {
    id: '502.3.13',
    name: 'Modification of focus cursor',
    conformance: 'partially-supports',
    remark:
      "Assistive technology can move focus between standard controls, and VoiceOver follows Cmdr's cursor as the arrow keys move it through the file list. Moving the VoiceOver cursor onto a file with VoiceOver's own keys likely doesn't move Cmdr's cursor.",
    issues: [453],
  },
  {
    id: '502.3.14',
    name: 'Event notification',
    conformance: 'supports',
    remark: 'See WCAG 4.1.3.',
  },
  {
    id: '502.4',
    name: 'Platform accessibility features',
    conformance: 'not-applicable',
    remark: 'Applies to platforms. Cmdr is an app.',
  },
  {
    id: '503.2',
    name: 'User preferences',
    conformance: 'supports',
    remark:
      'Cmdr follows the macOS system font, light and dark mode, accent color, text size, Reduce transparency, Reduce motion, Increase contrast (stronger borders, secondary text, focus rings, and pane tints), Differentiate without color (stronger outlines and tab line for the focused pane), and Show borders (a visible edge on buttons that otherwise have none), and picks up changes without a restart.',
  },
  {
    id: '503.3',
    name: 'Alternative user interfaces',
    conformance: 'not-applicable',
    remark: 'Cmdr provides no alternative user interface that works as assistive technology.',
  },
  { id: '503.4.1', name: 'Caption controls', conformance: 'not-applicable', remark: 'No video.' },
  { id: '503.4.2', name: 'Audio description controls', conformance: 'not-applicable', remark: 'No video.' },
  {
    id: '504.2',
    name: 'Content creation or editing',
    conformance: 'not-applicable',
    remark: "Cmdr isn't an authoring tool.",
  },
  {
    id: '504.2.1',
    name: 'Preservation of information provided for accessibility in format conversion',
    conformance: 'not-applicable',
    remark: "Cmdr isn't an authoring tool.",
  },
  { id: '504.2.2', name: 'PDF export', conformance: 'not-applicable', remark: "Cmdr isn't an authoring tool." },
  { id: '504.3', name: 'Prompts', conformance: 'not-applicable', remark: "Cmdr isn't an authoring tool." },
  { id: '504.4', name: 'Templates', conformance: 'not-applicable', remark: "Cmdr isn't an authoring tool." },
]

const section508Ch6: CriterionRow[] = [
  {
    id: '602.2',
    name: 'Accessibility and compatibility features',
    conformance: 'supports',
    remark:
      "The accessibility guide (Help > Accessibility guide, also on getcmdr.com) describes Cmdr's accessibility features, how to turn each one on, and the known gaps. The Keyboard shortcuts window, also in the Help menu, lists every keyboard command.",
  },
  {
    id: '602.3',
    name: 'Electronic support documentation',
    conformance: 'supports',
    remark:
      "Every main page of getcmdr.com, this report and the accessibility guide included, is tested automatically for WCAG 2 AA issues in light and dark mode on every change, and the home page passes Lighthouse's accessibility audit on every change too.",
  },
  {
    id: '602.4',
    name: 'Alternate formats for non-electronic support documentation',
    conformance: 'not-applicable',
    remark: 'All documentation is electronic.',
  },
  {
    id: '603.2',
    name: 'Information on accessibility and compatibility features',
    conformance: 'supports',
    remark:
      'Support answers by email and through in-app feedback, and points to the accessibility guide for what Cmdr offers and how to turn it on.',
  },
  {
    id: '603.3',
    name: 'Accommodation of communication needs',
    conformance: 'supports',
    remark: 'All support is written, by email or in-app feedback. No phone call is needed.',
  },
]

const enClause4: CriterionRow[] = [
  {
    id: '4.2.1',
    name: 'Usage without vision',
    conformance: 'partially-supports',
    remark: 'See Section 508 302.1.',
    issues: [451],
  },
  {
    id: '4.2.2',
    name: 'Usage with limited vision',
    conformance: 'partially-supports',
    remark: 'See Section 508 302.2.',
    issues: [454],
  },
  {
    id: '4.2.3',
    name: 'Usage without perception of colour',
    conformance: 'partially-supports',
    remark: 'See Section 508 302.3.',
    issues: [452, 454],
  },
  { id: '4.2.4', name: 'Usage without hearing', conformance: 'supports', remark: 'Cmdr uses no sound.' },
  { id: '4.2.5', name: 'Usage with limited hearing', conformance: 'supports', remark: 'Cmdr uses no sound.' },
  {
    id: '4.2.6',
    name: 'Usage with no or limited vocal capability',
    conformance: 'supports',
    remark: 'Cmdr needs no speech input.',
  },
  {
    id: '4.2.7',
    name: 'Usage with limited manipulation or strength',
    conformance: 'supports',
    remark: 'See Section 508 302.7.',
  },
  {
    id: '4.2.8',
    name: 'Usage with limited reach',
    conformance: 'supports',
    remark: 'Cmdr is software and needs no physical reach.',
  },
  { id: '4.2.9', name: 'Minimize photosensitive seizure triggers', conformance: 'supports', remark: 'See WCAG 2.3.1.' },
  {
    id: '4.2.10',
    name: 'Usage with limited cognition, language or learning',
    conformance: 'supports',
    remark: 'See Section 508 302.9.',
  },
  {
    id: '4.2.11',
    name: 'Privacy',
    conformance: 'supports',
    remark:
      "Accessibility features don't expose private data. Passwords and keys use masked fields and are stored in the macOS Keychain.",
  },
]

const enClause5: CriterionRow[] = [
  {
    id: '5.1.2–5.1.7',
    name: 'Closed functionality',
    conformance: 'not-applicable',
    remark: 'Cmdr is open to assistive technology.',
  },
  {
    id: '5.2',
    name: 'Activation of accessibility features',
    conformance: 'supports',
    remark:
      'Zoom works from the keyboard, the View menu, and Settings. macOS accessibility settings apply with no extra steps.',
  },
  { id: '5.3', name: 'Biometrics', conformance: 'not-applicable', remark: 'Cmdr uses no biometric identification.' },
  {
    id: '5.4',
    name: 'Preservation of accessibility information during conversion',
    conformance: 'not-applicable',
    remark: 'Cmdr copies and archives files without converting their formats.',
  },
  { id: '5.5.1', name: 'Means of operation', conformance: 'not-applicable', remark: 'Applies to hardware.' },
  { id: '5.5.2', name: 'Operable parts discernibility', conformance: 'not-applicable', remark: 'Applies to hardware.' },
  {
    id: '5.6.1',
    name: 'Tactile or auditory status',
    conformance: 'not-applicable',
    remark: 'Cmdr has no locking or toggle keys of its own.',
  },
  {
    id: '5.6.2',
    name: 'Visual status',
    conformance: 'not-applicable',
    remark: 'Cmdr has no locking or toggle keys of its own.',
  },
  { id: '5.7', name: 'Key repeat', conformance: 'not-applicable', remark: 'Cmdr uses the macOS key repeat settings.' },
  {
    id: '5.8',
    name: 'Double-strike key acceptance',
    conformance: 'not-applicable',
    remark: 'Cmdr uses the macOS keyboard settings.',
  },
  {
    id: '5.9',
    name: 'Simultaneous user actions',
    conformance: 'supports',
    remark:
      'Shortcuts are modifiers plus one key, which Sticky Keys can enter one at a time. Nothing needs two pointer actions at once.',
  },
]

const enClause11: CriterionRow[] = [
  {
    id: '11.1–11.4',
    name: 'Software criteria that map to WCAG',
    conformance: 'see-other',
    seeLabel: 'See WCAG 2.2 tables',
    remark: 'Each of these criteria matches a WCAG 2.2 criterion of the same number, applied to software.',
  },
  {
    id: '11.5.1',
    name: 'Closed functionality',
    conformance: 'not-applicable',
    remark: 'Cmdr is open to assistive technology.',
  },
  {
    id: '11.5.2.1–11.5.2.3',
    name: 'Platform accessibility service support and use',
    conformance: 'see-other',
    seeLabel: 'See 11.5.2.5–11.5.2.17',
    remark:
      "Cmdr uses the macOS accessibility interface through WebKit, Apple's web engine. The rows below say how well each part works.",
  },
  {
    id: '11.5.2.4',
    name: 'Assistive technology',
    conformance: 'not-applicable',
    remark: "Cmdr isn't assistive technology.",
  },
  {
    id: '11.5.2.5',
    name: 'Object information',
    conformance: 'supports',
    remark: 'See Section 508 502.3.1.',
  },
  {
    id: '11.5.2.6',
    name: 'Row, column, and headers',
    conformance: 'partially-supports',
    remark: 'See Section 508 502.3.3.',
    issues: [451],
  },
  {
    id: '11.5.2.7',
    name: 'Values',
    conformance: 'supports',
    remark: 'See Section 508 502.3.4.',
  },
  {
    id: '11.5.2.8',
    name: 'Label relationships',
    conformance: 'supports',
    remark: 'See Section 508 502.3.6.',
  },
  {
    id: '11.5.2.9',
    name: 'Parent-child relationships',
    conformance: 'partially-supports',
    remark: 'See Section 508 502.3.7.',
    issues: [451],
  },
  {
    id: '11.5.2.10',
    name: 'Text',
    conformance: 'supports',
    remark: 'See Section 508 502.3.8.',
  },
  {
    id: '11.5.2.11',
    name: 'List of available actions',
    conformance: 'supports',
    remark: 'See Section 508 502.3.10.',
  },
  {
    id: '11.5.2.12',
    name: 'Execution of available actions',
    conformance: 'supports',
    remark: 'See Section 508 502.3.11.',
  },
  {
    id: '11.5.2.13',
    name: 'Tracking of focus and selection attributes',
    conformance: 'supports',
    remark: 'See Section 508 502.3.12. Marked files are exposed as selected, and VoiceOver says "selected" on them.',
  },
  {
    id: '11.5.2.14',
    name: 'Modification of focus and selection attributes',
    conformance: 'partially-supports',
    remark: 'See Section 508 502.3.13.',
    issues: [453],
  },
  {
    id: '11.5.2.15',
    name: 'Change notification',
    conformance: 'supports',
    remark: 'See WCAG 4.1.3.',
  },
  {
    id: '11.5.2.16',
    name: 'Modifications of states and properties',
    conformance: 'supports',
    remark: 'See Section 508 502.3.2.',
  },
  {
    id: '11.5.2.17',
    name: 'Modifications of values and text',
    conformance: 'supports',
    remark: 'See Section 508 502.3.5 and 502.3.9.',
  },
  {
    id: '11.6.1',
    name: 'User control of accessibility features',
    conformance: 'not-applicable',
    remark: "Cmdr isn't platform software.",
  },
  {
    id: '11.6.2',
    name: 'No disruption of accessibility features',
    conformance: 'supports',
    remark: 'See Section 508 502.2.2.',
  },
  {
    id: '11.7',
    name: 'User preferences',
    conformance: 'supports',
    remark: 'See Section 508 503.2.',
  },
  {
    id: '11.8.1–11.8.5',
    name: 'Authoring tools',
    conformance: 'not-applicable',
    remark: "Cmdr isn't an authoring tool.",
  },
]

const enClause12: CriterionRow[] = [
  {
    id: '12.1.1',
    name: 'Accessibility and compatibility features',
    conformance: 'supports',
    remark: 'See Section 508 602.2.',
  },
  { id: '12.1.2', name: 'Accessible documentation', conformance: 'supports', remark: 'See Section 508 602.3.' },
  {
    id: '12.2.2',
    name: 'Information on accessibility and compatibility features',
    conformance: 'supports',
    remark: 'See Section 508 603.2.',
  },
  { id: '12.2.3', name: 'Effective communication', conformance: 'supports', remark: 'See Section 508 603.3.' },
  { id: '12.2.4', name: 'Accessible documentation', conformance: 'supports', remark: 'See Section 508 602.3.' },
]

/** The report's tables, grouped by standard, in the VPAT's order. */
export const standards: StandardSection[] = [
  {
    id: 'wcag',
    title: 'WCAG 2.2 report',
    tables: [
      { id: 'wcag-a', title: 'Table 1: Success criteria, Level A', rows: wcagA },
      { id: 'wcag-aa', title: 'Table 2: Success criteria, Level AA', rows: wcagAA },
      {
        id: 'wcag-aaa',
        title: 'Table 3: Success criteria, Level AAA',
        note: 'Not evaluated. None of the standards in this report requires Level AAA.',
        rows: [],
      },
    ],
  },
  {
    id: 'section-508',
    title: 'Revised Section 508 report',
    tables: [
      { id: 's508-ch3', title: 'Chapter 3: Functional performance criteria', rows: section508Ch3 },
      { id: 's508-ch4', title: 'Chapter 4: Hardware', note: 'Not applicable. Cmdr is software.', rows: [] },
      { id: 's508-ch5', title: 'Chapter 5: Software', rows: section508Ch5 },
      { id: 's508-ch6', title: 'Chapter 6: Support documentation and services', rows: section508Ch6 },
    ],
  },
  {
    id: 'en-301-549',
    title: 'EN 301 549 report',
    tables: [
      { id: 'en-4', title: 'Clause 4: Functional performance statements', rows: enClause4 },
      { id: 'en-5', title: 'Clause 5: Generic requirements', rows: enClause5 },
      {
        id: 'en-6-8',
        title: 'Clauses 6–8: Two-way voice communication, ICT with video capabilities, and hardware',
        note: 'Not applicable. Cmdr has no voice or video communication and is software.',
        rows: [],
      },
      {
        id: 'en-9',
        title: 'Clause 9: Web',
        note: 'Not applicable. Cmdr is a desktop app, not web content. The getcmdr.com website is covered as documentation under clause 12.',
        rows: [],
      },
      {
        id: 'en-10',
        title: 'Clause 10: Non-web documents',
        note: 'Not applicable. Cmdr ships no non-web documents.',
        rows: [],
      },
      { id: 'en-11', title: 'Clause 11: Software', rows: enClause11 },
      { id: 'en-12', title: 'Clause 12: Documentation and support services', rows: enClause12 },
      {
        id: 'en-13',
        title: 'Clause 13: ICT providing relay or emergency service access',
        note: 'Not applicable. Cmdr provides no relay or emergency services.',
        rows: [],
      },
    ],
  },
]

/** The open issues behind the report's gaps, most important first. */
export const gapIssues: { number: number; title: string }[] = [
  {
    number: 451,
    title: "The Full view's columns aren't a table, and tabs aren't tied to the file list they show",
  },
  {
    number: 453,
    title: "Moving the VoiceOver cursor onto a file may not move Cmdr's cursor",
  },
  {
    number: 454,
    title:
      'By default, the focused-pane cues, some focus rings, and the optional App date and size colors are under 3:1 contrast',
  },
  { number: 452, title: 'Hidden files differ from other files only by a dimmer color' },
]
