/**
 * Every fixture-backed dialog's state-id → fixture lookup, in one place.
 *
 * `DialogGallery.svelte`'s switch reads its fixtures from here (each entry keeps
 * its own value type, so the props stay typed), and `fixtures.test.ts` walks the
 * same object to prove each record's keys are exactly the state ids its
 * `gallery-registry.ts` row advertises. Sharing the object is what makes that
 * test meaningful: a separate list in the test could drift from the switch and
 * pass while a button opened nothing.
 *
 * Dialogs that take callbacks only (`about`, `license`, `commercial-reminder`,
 * `connect-to-server`, `mtp-permission`) have no fixture data and aren't here.
 */

import type { SoftDialogId } from '$lib/ui/dialog-registry'
import { alertFixtures } from './alert'
import { archivePasswordFixtures } from './archive-password'
import { crashReportFixtures } from './crash-report'
import { ptpcameradFixtures } from './devices'
import { expirationFixtures } from './licensing'
import { extensionChangeFixtures, renameConflictFixtures } from './rename'
import { selectionAddFixtures, selectionRemoveFixtures } from './selection'
import { transferErrorFixtures } from './transfer-error'
import { viewerCopyConfirmFixtures, viewerCopyRefuseFixtures } from './viewer-copy'

export const fixtureRecords = {
  alert: alertFixtures,
  'archive-password': archivePasswordFixtures,
  'crash-report': crashReportFixtures,
  expiration: expirationFixtures,
  'extension-change': extensionChangeFixtures,
  ptpcamerad: ptpcameradFixtures,
  'rename-conflict': renameConflictFixtures,
  'selection-add': selectionAddFixtures,
  'selection-remove': selectionRemoveFixtures,
  'transfer-error': transferErrorFixtures,
  'viewer-copy-confirm': viewerCopyConfirmFixtures,
  'viewer-copy-refuse': viewerCopyRefuseFixtures,
} as const satisfies Partial<Record<SoftDialogId, Record<string, unknown>>>
