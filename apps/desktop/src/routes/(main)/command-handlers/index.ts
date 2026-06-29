/**
 * Assembles the family handler modules into one flat `CommandHandlerRecord`.
 *
 * The `: CommandHandlerRecord` annotation is the completeness guarantee: the
 * record is keyed by `Exclude<CommandId, DispatchExemptId>`, so a missing handler
 * is a compile error and a handler for an exempt id is a compile error. The
 * dispatch core (`command-dispatch.ts`) imports this and looks up by id; it never
 * branches per command.
 */
import { appDialogHandlers } from './app-dialog-handlers'
import { viewHandlers } from './view-handlers'
import { paneHandlers } from './pane-handlers'
import { tabHandlers } from './tab-handlers'
import { navHandlers } from './nav-handlers'
import { sortHandlers } from './sort-handlers'
import { fileHandlers } from './file-handlers'
import { clipboardHandlers } from './clipboard-handlers'
import { selectionHandlers } from './selection-handlers'
import { tagHandlers } from './tag-handlers'
import { miscHandlers } from './misc-handlers'
import type { CommandHandlerRecord } from './types'

export const commandHandlers: CommandHandlerRecord = {
  ...appDialogHandlers,
  ...viewHandlers,
  ...paneHandlers,
  ...tabHandlers,
  ...navHandlers,
  ...sortHandlers,
  ...fileHandlers,
  ...clipboardHandlers,
  ...selectionHandlers,
  ...tagHandlers,
  ...miscHandlers,
}

export type { CommandHandlerContext, CommandHandler, CommandHandlerRecord, DispatchExemptId } from './types'
export { DISPATCH_EXEMPT_IDS } from './types'
