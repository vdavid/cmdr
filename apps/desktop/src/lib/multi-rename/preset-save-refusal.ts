/** Carrying a typed `PresetSaveRefusal` from `saveMultiRenamePreset` to the name popover that words it. */
import type { PresetSaveRefusal } from '#lib/ipc/bindings.js'
import { TypedFailure, failureOf } from '#lib/ipc/typed-failure.js'

export type { PresetSaveRefusal }

/** An `Error` that still carries the backend's typed preset-save refusal. */
export class PresetSaveFailure extends TypedFailure<PresetSaveRefusal> {
  constructor(failure: PresetSaveRefusal) {
    super(failure, `preset save refused: ${failure.type}`)
    this.name = 'PresetSaveFailure'
  }
}

/** The typed refusal behind a caught value, or `null` when it isn't one. */
export function asPresetSaveRefusal(error: unknown): PresetSaveRefusal | null {
  return failureOf(PresetSaveFailure, error)
}
