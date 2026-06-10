/**
 * Tests for the feedback flow store: the open/close contract that both the Help menu
 * item and the `feedback.send` palette command funnel through.
 */

import { describe, it, expect, beforeEach, vi } from 'vitest'
import { feedbackFlow, openFeedbackDialog, closeFeedbackDialog } from './feedback-flow.svelte'

vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn(() => Promise.resolve()),
}))

beforeEach(() => {
  closeFeedbackDialog()
})

describe('feedback-flow', () => {
  it('starts closed', () => {
    expect(feedbackFlow.open).toBe(false)
  })

  it('openFeedbackDialog flips open to true', () => {
    openFeedbackDialog()
    expect(feedbackFlow.open).toBe(true)
  })

  it('closeFeedbackDialog flips open back to false', () => {
    openFeedbackDialog()
    closeFeedbackDialog()
    expect(feedbackFlow.open).toBe(false)
  })
})
