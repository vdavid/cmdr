/**
 * Fixtures for `delete-ai-model` (`$lib/settings/sections/DeleteAiModelDialog.svelte`).
 *
 * In the shipping app this one lives in the SETTINGS window, where its props
 * come from the live AI runtime status. Both of its states are prop-driven, so
 * the gallery renders the same component the settings window does.
 *
 * Raw copy on purpose: this module is dev-only and sits outside the
 * i18n-enforced areas, so fixture strings never reach the message catalog.
 */

export interface DeleteAiModelFixture {
  /** The installed model's size, as the settings section formats it. `null` renders the fallback. */
  modelSizeFormatted: string | null
  /** Mid-delete: the title, body, and both buttons all change. */
  isDeleting: boolean
}

export const deleteAiModelFixtures: Record<string, DeleteAiModelFixture | undefined> = {
  idle: { modelSizeFormatted: '4.1 GB', isDeleting: false },
  deleting: { modelSizeFormatted: '4.1 GB', isDeleting: true },
}

/**
 * Fixtures for `forget-memory` (`$lib/settings/sections/ForgetMemoryDialog.svelte`),
 * the settings-window confirmation over the agent's notes folder. Prop-driven like its
 * neighbour above, so the gallery renders the same component the settings window does.
 */
export interface ForgetMemoryFixture {
  /** Mid-delete: both buttons go dead, and Enter with them. */
  isForgetting: boolean
}

export const forgetMemoryFixtures: Record<string, ForgetMemoryFixture | undefined> = {
  idle: { isForgetting: false },
  forgetting: { isForgetting: true },
}
