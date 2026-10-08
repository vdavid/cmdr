/**
 * When onboarding's failed local-model download gets said: never under the wizard, always once
 * it closes, and not at all once the user picked something else.
 */

import { beforeEach, describe, expect, it, vi } from 'vitest'

const { addToast, dismissToast } = vi.hoisted(() => ({
  addToast: vi.fn<(content: unknown, options: { id: string }) => string>(),
  dismissToast: vi.fn<(id: string) => void>(),
}))
vi.mock('$lib/ui/toast', () => ({ addToast, dismissToast }))
vi.mock('./LocalDownloadFailedToastContent.svelte', () => ({ default: {} }))

import {
  LOCAL_DOWNLOAD_FAILED_TOAST_ID,
  clearLocalDownloadFailure,
  noteLocalDownloadFailed,
  resetLocalDownloadNoticeForTesting,
  setWizardShowingForDownloadNotice,
} from './local-download-notice'

beforeEach(() => {
  vi.clearAllMocks()
  resetLocalDownloadNoticeForTesting()
})

describe('the local download notice', () => {
  it('holds a failure while the wizard is up and raises it when the wizard closes', () => {
    setWizardShowingForDownloadNotice(true)
    noteLocalDownloadFailed()
    expect(addToast).not.toHaveBeenCalled()

    setWizardShowingForDownloadNotice(false)

    expect(addToast).toHaveBeenCalledOnce()
    expect(addToast.mock.calls[0][1]).toMatchObject({ id: LOCAL_DOWNLOAD_FAILED_TOAST_ID, level: 'warn' })
  })

  it('raises a failure at once when it lands after the wizard closed', () => {
    setWizardShowingForDownloadNotice(true)
    setWizardShowingForDownloadNotice(false)

    noteLocalDownloadFailed()

    expect(addToast).toHaveBeenCalledOnce()
  })

  it('raises a held failure only once, however often the wizard closes', () => {
    setWizardShowingForDownloadNotice(true)
    noteLocalDownloadFailed()
    setWizardShowingForDownloadNotice(false)
    setWizardShowingForDownloadNotice(true)
    setWizardShowingForDownloadNotice(false)

    expect(addToast).toHaveBeenCalledOnce()
  })

  it('says nothing on close when no download failed', () => {
    setWizardShowingForDownloadNotice(true)
    setWizardShowingForDownloadNotice(false)

    expect(addToast).not.toHaveBeenCalled()
  })

  it('drops a held failure the user moved past, and takes down one already showing', () => {
    setWizardShowingForDownloadNotice(true)
    noteLocalDownloadFailed()

    clearLocalDownloadFailure()
    setWizardShowingForDownloadNotice(false)

    expect(addToast).not.toHaveBeenCalled()
    expect(dismissToast).toHaveBeenCalledWith(LOCAL_DOWNLOAD_FAILED_TOAST_ID)
  })
})
