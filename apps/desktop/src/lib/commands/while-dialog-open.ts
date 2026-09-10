/**
 * What a command does while a dialog or overlay is up in the main window.
 *
 * Every command declares one: `Command.whileDialogOpen` is required, so a new command
 * doesn't compile until its author has answered. The dispatch core enforces the answer for
 * every road a command comes in by (keyboard, native menu, mouse, palette, the explorer's
 * own controls), so no single road can forget it: `routes/(main)/dialog-command-gate.ts`.
 *
 * Same shape as `$lib/ui/dialog-registry.ts`, which asks the question from the dialog's side
 * (may a file operation START behind me?).
 */

export type WhileDialogOpen =
  /** Refused: the command acts on the panes, or opens more UI in the main window. */
  | { readonly runs: 'never' }
  /** Runs only while focus is in a text input, where the handler edits the text instead of a pane. */
  | { readonly runs: 'inTextInput' }
  /** Runs anyway. Carries its reason, so nobody has to relitigate it. */
  | { readonly runs: 'always'; readonly reason: string }

/** The default: a pane command, or one that opens more UI in the main window. */
export const BLOCKED_BY_DIALOGS: WhileDialogOpen = { runs: 'never' }

/**
 * Cut, copy, paste, and select all. Their handlers branch on `isTextInputFocused()` before
 * touching the explorer, so from a dialog's text field they edit the text. They have to run
 * there: when AppKit consumes ⌘V outright, the menu dispatch is the only path paste has.
 */
export const IN_TEXT_INPUTS_ONLY: WhileDialogOpen = { runs: 'inTextInput' }

/** The opt-out, which has to say why the command is safe to run over a dialog. */
export function runsOverDialogs(reason: string): WhileDialogOpen {
  return { runs: 'always', reason }
}
