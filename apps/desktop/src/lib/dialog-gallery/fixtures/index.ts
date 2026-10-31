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
 * rather than props, `onboarding` holds a wizard step, `drive-index-stale`
 * (`./indexing`) holds the payload of the event that opens it, and
 * `operation-conflict` picks the clash its real copy walks into: those dialogs
 * aren't rendered by the harness at all (see `gallery-registry.ts` § `openedBy`).
 * `transfer-progress` picks the folder its real copy walks, and the harness
 * renders it once its preview resolved that copy. They live here anyway so the
 * same test proves their state ids resolve.
 */

import type { SoftDialogId } from '#lib/ui/dialog-registry.js'
import { alertFixtures } from './alert'
import { deleteAiModelFixtures, deleteAllDataFixtures, forgetMemoryFixtures, smbLinkConsentFixtures } from './ai-model'
import { archivePasswordFixtures } from './archive-password'
import { crashReportFixtures } from './crash-report'
import { deleteFixtures, goToPathFixtures, mkdirFixtures, newFileFixtures, transferFixtures } from './disk'
import { ptpcameradFixtures } from './devices'
import { staleDriveFixtures } from './indexing'
import { expirationFixtures } from './licensing'
import { onboardingFixtures } from './onboarding'
import { operationConflictFixtures } from './operation-conflict'
import { quitFixtures } from './quit'
import { extensionChangeFixtures, renameConflictFixtures } from './rename'
import { rollbackConfirmFixtures } from './rollback'
import { selectionAddFixtures, selectionRemoveFixtures } from './selection'
import { serverSignInFixtures } from './servers'
import { sourcesGoneFixtures } from './sources-gone'
import { lockedSourcesFixtures } from './locked-sources'
import {
  bulkRenameFixtures,
  errorReportFixtures,
  feedbackFixtures,
  operationLogFixtures,
  whatsNewFixtures,
} from './store-seeded'
import { suggestedOpsFixtures } from './suggested-ops'
import { transferErrorFixtures } from './transfer-error'
import { transferProgressFixtures } from './transfer-progress'
import { moveToApplicationsFixtures } from './updates'
import { viewerCopyConfirmFixtures, viewerCopyRefuseFixtures } from './viewer-copy'

export const fixtureRecords = {
  alert: alertFixtures,
  'archive-password': archivePasswordFixtures,
  'bulk-rename-review': bulkRenameFixtures,
  'crash-report': crashReportFixtures,
  'delete-ai-model': deleteAiModelFixtures,
  'delete-all-data': deleteAllDataFixtures,
  'delete-confirmation': deleteFixtures,
  'drive-index-stale': staleDriveFixtures,
  'error-report': errorReportFixtures,
  expiration: expirationFixtures,
  'extension-change': extensionChangeFixtures,
  feedback: feedbackFixtures,
  'forget-memory': forgetMemoryFixtures,
  'smb-link-consent': smbLinkConsentFixtures,
  'go-to-path': goToPathFixtures,
  'mkdir-confirmation': mkdirFixtures,
  'move-to-applications': moveToApplicationsFixtures,
  'new-file-confirmation': newFileFixtures,
  onboarding: onboardingFixtures,
  'operation-conflict': operationConflictFixtures,
  'operation-log': operationLogFixtures,
  ptpcamerad: ptpcameradFixtures,
  'quit-confirmation': quitFixtures,
  'rename-conflict': renameConflictFixtures,
  'rollback-confirmation': rollbackConfirmFixtures,
  'selection-add': selectionAddFixtures,
  'selection-remove': selectionRemoveFixtures,
  'server-sign-in': serverSignInFixtures,
  'sources-gone': sourcesGoneFixtures,
  'suggested-ops': suggestedOpsFixtures,
  'locked-sources': lockedSourcesFixtures,
  'transfer-confirmation': transferFixtures,
  'transfer-error': transferErrorFixtures,
  'transfer-progress': transferProgressFixtures,
  'viewer-copy-confirm': viewerCopyConfirmFixtures,
  'viewer-copy-refuse': viewerCopyRefuseFixtures,
  'whats-new': whatsNewFixtures,
} as const satisfies Partial<Record<SoftDialogId, Record<string, unknown>>>
