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
 *
 * The disk-backed records (`./disk`) hold BUILDERS rather than data: their props
 * come from the real fixture directory the dev-only Rust command creates, so the
 * numbers those dialogs display are the ones on disk.
 *
 * The store-seeded records (`./store-seeded`) hold PATCHES for a real app store
 * rather than props, and `onboarding` holds a wizard step: those dialogs aren't
 * rendered by the harness at all (see `gallery-registry.ts` § `openedBy`). They
 * live here anyway so the same test proves their state ids resolve.
 */

import type { SoftDialogId } from '$lib/ui/dialog-registry'
import { alertFixtures } from './alert'
import { deleteAiModelFixtures } from './ai-model'
import { archivePasswordFixtures } from './archive-password'
import { crashReportFixtures } from './crash-report'
import { deleteFixtures, goToPathFixtures, mkdirFixtures, newFileFixtures, transferFixtures } from './disk'
import { ptpcameradFixtures } from './devices'
import { expirationFixtures } from './licensing'
import { onboardingFixtures } from './onboarding'
import { extensionChangeFixtures, renameConflictFixtures } from './rename'
import { selectionAddFixtures, selectionRemoveFixtures } from './selection'
import {
  bulkRenameFixtures,
  errorReportFixtures,
  feedbackFixtures,
  operationLogFixtures,
  whatsNewFixtures,
} from './store-seeded'
import { transferErrorFixtures } from './transfer-error'
import { viewerCopyConfirmFixtures, viewerCopyRefuseFixtures } from './viewer-copy'

export const fixtureRecords = {
  alert: alertFixtures,
  'archive-password': archivePasswordFixtures,
  'bulk-rename-review': bulkRenameFixtures,
  'crash-report': crashReportFixtures,
  'delete-ai-model': deleteAiModelFixtures,
  'delete-confirmation': deleteFixtures,
  'error-report': errorReportFixtures,
  expiration: expirationFixtures,
  'extension-change': extensionChangeFixtures,
  feedback: feedbackFixtures,
  'go-to-path': goToPathFixtures,
  'mkdir-confirmation': mkdirFixtures,
  'new-file-confirmation': newFileFixtures,
  onboarding: onboardingFixtures,
  'operation-log': operationLogFixtures,
  ptpcamerad: ptpcameradFixtures,
  'rename-conflict': renameConflictFixtures,
  'selection-add': selectionAddFixtures,
  'selection-remove': selectionRemoveFixtures,
  'transfer-confirmation': transferFixtures,
  'transfer-error': transferErrorFixtures,
  'viewer-copy-confirm': viewerCopyConfirmFixtures,
  'viewer-copy-refuse': viewerCopyRefuseFixtures,
  'whats-new': whatsNewFixtures,
} as const satisfies Partial<Record<SoftDialogId, Record<string, unknown>>>
