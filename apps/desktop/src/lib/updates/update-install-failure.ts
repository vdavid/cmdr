/**
 * A refused or failed update install, carried across the throw with its typed reason so the updater can tell a policy
 * refusal (quiet) from a broken install (logged at error). Lives outside `$lib/tauri-commands` for the same reason as
 * `update-download-failure.ts`: the updater's tests mock that barrel and still need the real class.
 */
import type { UpdateInstallError } from '$lib/ipc/bindings'
import { TypedFailure } from '$lib/ipc/typed-failure'

/** An `Error` that still carries the backend's typed install failure. */
export class UpdateInstallFailure extends TypedFailure<UpdateInstallError> {
  constructor(failure: UpdateInstallError) {
    super(failure, failure.type === 'failed' ? `update install: ${failure.detail}` : `update install ${failure.type}`)
    this.name = 'UpdateInstallFailure'
  }
}
