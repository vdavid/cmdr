import type { DataWipeView } from '#lib/settings/sections/delete-all-data.js'

/**
 * Fixtures for `delete-ai-model` (`#lib/settings/sections/DeleteAiModelDialog.svelte`).
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
  modelSizeFormatted: string
  /** Mid-delete: the title, body, and both buttons all change. */
  isDeleting: boolean
}

export const deleteAiModelFixtures: Record<string, DeleteAiModelFixture | undefined> = {
  idle: { modelSizeFormatted: '4.1 GB', isDeleting: false },
  deleting: { modelSizeFormatted: '4.1 GB', isDeleting: true },
}

/**
 * Fixtures for `forget-memory` (`#lib/settings/sections/ForgetMemoryDialog.svelte`),
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

/** Fixtures for `smb-link-consent` (`#lib/file-explorer/network/SmbLinkConsentDialog.svelte`). Prop-driven. */
export interface SmbLinkConsentFixture {
  server: string
  target: string
}

export const smbLinkConsentFixtures: Record<string, SmbLinkConsentFixture | undefined> = {
  default: { server: 'naspolya.local', target: 'fs01.example.com' },
}

/**
 * Fixtures for `delete-all-data` (`#lib/settings/sections/DeleteAllDataDialog.svelte`), the
 * settings-window "Delete all Cmdr data" dialog. Prop-driven: every view is a `DataWipeView`.
 */
export interface DeleteAllDataFixture {
  view: DataWipeView
}

const LICENSED = { totalBytes: 5_812_000_000, modelBytes: 4_310_000_000, hasLicense: true }

export const deleteAllDataFixtures: Record<string, DeleteAllDataFixture | undefined> = {
  confirm: { view: { kind: 'confirm', summary: LICENSED, refusal: null } },
  confirmPersonal: {
    view: { kind: 'confirm', summary: { totalBytes: 1_180_000_000, modelBytes: 0, hasLicense: false }, refusal: null },
  },
  confirmNoSummary: { view: { kind: 'confirm', summary: null, refusal: null } },
  refused: { view: { kind: 'confirm', summary: LICENSED, refusal: { type: 'fileOperationsRunning', count: 2 } } },
  deleting: { view: { kind: 'deleting', summary: LICENSED } },
  done: { view: { kind: 'done', report: { leftovers: [] } } },
  partial: {
    view: {
      kind: 'done',
      report: {
        leftovers: [
          {
            type: 'path',
            path: '/Users/someone/Library/Application Support/com.veszelovszki.cmdr.cmdr-wipe-3f2a9c41d7e84b0c9a1f5e6d7c8b9a0f',
          },
          { type: 'secretItems', count: 2, itemName: 'Cmdr' },
        ],
      },
    },
  },
}
