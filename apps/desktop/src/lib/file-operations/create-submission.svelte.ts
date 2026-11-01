/**
 * One new-folder / new-file dialog's submit, from OK to how the create really
 * ended. Shared by `mkdir/NewFolderDialog.svelte` and `mkfile/NewFileDialog.svelte`.
 *
 * A slow volume can hold a create for seconds. The backend answers "still
 * running" past its reply deadline and reports the real end later
 * (`#lib/tauri-commands/mutation-reply.ts`), so this never says the create
 * didn't happen when it may still land (ERR-AREUV). The phases:
 *
 * - `editing`: the person is naming it.
 * - `creating`: OK was pressed; the dialog goes busy at once.
 * - `stillCreating`: the volume is being slow, so the dialog says so.
 *
 * Closing the dialog doesn't stop the create (an in-flight write can't be taken
 * back). After that, a landing steers nothing (the person has moved on, and the
 * listing shows the entry), and a refusal is the one end that still speaks, in a
 * toast, since nothing else would tell them.
 */

import type { MutationWaitOptions } from '#lib/tauri-commands/index.js'
import { addToast } from '#lib/ui/toast/index.js'
import { tString } from '#lib/intl/messages.svelte.js'
import { getAppLogger } from '#lib/logging/logger.js'
import { asMutationError } from './mutation-error'
import { renderMutationError } from './mutation-error-messages'
import type { ReportIncident } from '#lib/error-reporter/report-incident.js'
import type { CreatePress } from '#lib/file-explorer/pane/dialog-props.js'
import type { NewEntryNameCheck } from './new-entry-name-check.svelte'

const log = getAppLogger('fileOperations')

export type CreatePhase = 'editing' | 'creating' | 'stillCreating'

/** A create the volume turned down: the sentence to show, and what reporting it sends. */
export interface CreateRefusal {
  message: string
  incident: ReportIncident
}

export interface CreateSubmissionDeps {
  kind: 'folder' | 'file'
  /** The create itself, handed the hooks that report a slow volume. */
  create: (name: string, wait: MutationWaitOptions) => Promise<void>
  /** It landed while the dialog is still up. */
  onCreated: (name: string) => void
  /** It was refused while the dialog is still up: the message (and its report offer) under the name. */
  onRefused: (refusal: CreateRefusal) => void
}

export class CreateSubmission {
  phase = $state<CreatePhase>('editing')
  /** The name being created, as submitted (the field may have changed since). */
  submittedName = $state('')
  /** When OK was pressed, in `Date.now()` time: a slowness report measures from here. */
  submittedAt = $state(0)
  #closed = false
  readonly #deps: CreateSubmissionDeps

  constructor(deps: CreateSubmissionDeps) {
    this.#deps = deps
  }

  /** Whether a create is in flight, so OK and Enter stand down. */
  get busy(): boolean {
    return this.phase !== 'editing'
  }

  /** Creates `name`. A second submit while one is in flight is ignored. */
  async submit(name: string): Promise<void> {
    await this.#create(name, () => {})
  }

  /**
   * The MCP `dialog_confirm`: OK, with the checks a person gets from the field and the button.
   * Answers when the create ended or a slow volume is still on it, for the tool's reply.
   */
  async pressFromTool(name: string, check: NewEntryNameCheck): Promise<CreatePress> {
    await check.validate(name)
    if (name === '') return { kind: 'refused', reason: 'The name is empty.' }
    if (check.errorMessage) return { kind: 'refused', reason: check.errorMessage }
    if (this.busy) return { kind: 'stillRunning' }
    return new Promise((resolve) => {
      void this.#create(name, () => {
        resolve({ kind: 'stillRunning' })
      }).then((end) => {
        resolve(end === 'created' ? { kind: 'done' } : { kind: 'refused', reason: check.errorMessage })
      })
    })
  }

  async #create(name: string, onStillRunning: () => void): Promise<'created' | 'refused' | 'ignored'> {
    if (this.busy) return 'ignored'
    this.submittedName = name
    this.submittedAt = Date.now()
    this.phase = 'creating'
    try {
      await this.#deps.create(name, {
        onStillRunning: () => {
          if (!this.#closed) this.phase = 'stillCreating'
          onStillRunning()
        },
      })
    } catch (e) {
      this.#refused(name, e)
      return 'refused'
    }
    if (!this.#closed) this.#deps.onCreated(name)
    return 'created'
  }

  /** The dialog is going away. A create still running carries on without it. */
  close(): void {
    this.#closed = true
  }

  #refused(name: string, e: unknown): void {
    const { kind } = this.#deps
    const failure = asMutationError(e)
    if (!failure) log.warn('A create call threw an untyped value: {error}', { error: e })
    const typed = failure ?? { type: 'unexpected', detail: '' }
    const reason = renderMutationError(typed, kind, name)
    const incident: ReportIncident = { kind: 'createRefused', entry: kind, refusal: typed.type, reason }
    if (this.#closed) {
      // Persistent: the person left the dialog, so this may be the only word
      // they get that the entry isn't there.
      const key = kind === 'folder' ? 'fileOperations.mkdir.notCreatedToast' : 'fileOperations.mkfile.notCreatedToast'
      addToast(tString(key, { name, reason }), { level: 'error', dismissal: 'persistent', report: incident })
      return
    }
    this.phase = 'editing'
    this.#deps.onRefused({ message: reason, incident })
  }
}
