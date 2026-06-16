/**
 * Base-locale (en) parity net for the feedback i18n migration.
 *
 * The feedback dialog copy and its success toast moved from hardcoded English
 * into the `feedback.*` catalog (resolved through `t()` / `<Trans>`). This is a
 * behavior-preserving MOVE: every rendered en string must be byte-identical to
 * the pre-migration copy. These goldens are the literals that lived in
 * `FeedbackDialog.svelte` before the move; an intended copy edit lands in the
 * catalog AND here together, never silently.
 *
 * `feedback.dialog.moreWays` is a rich-text `<Trans>` message (two inline
 * links); the parity check below asserts its plain-text shape with the tag
 * markers stripped, which is what the user reads.
 */

import { describe, it, expect, beforeAll, afterAll } from 'vitest'
import { _setLocaleForTests } from '$lib/intl/locale'
import { t, tString } from '$lib/intl/messages.svelte'

beforeAll(() => {
  _setLocaleForTests('en-US')
})
afterAll(() => {
  _setLocaleForTests(null)
})

describe('feedback dialog copy parity (en)', () => {
  it('resolves the static dialog strings', () => {
    expect(tString('feedback.dialog.title')).toBe('Send feedback')
    expect(tString('feedback.dialog.description')).toBe(
      "What's working? What's missing? Your note goes straight to the maker of Cmdr.",
    )
    expect(tString('feedback.dialog.label')).toBe('Your feedback')
    expect(tString('feedback.dialog.placeholder')).toBe("Example: I'd love a shortcut for jumping between tabs.")
    expect(tString('feedback.dialog.invalid')).toBe("That note didn't go through. Shorten it and try again?")
    expect(tString('feedback.dialog.softFailure')).toBe("Sorry, we couldn't send your feedback right now. Try again?")
    expect(tString('feedback.dialog.cancel')).toBe('Cancel')
    expect(tString('feedback.dialog.send')).toBe('Send feedback')
    expect(tString('feedback.dialog.sending')).toBe('Sending…')
    expect(tString('feedback.sentToast')).toBe('Thanks for the feedback! We read every note.')
  })

  it('resolves the interpolated dialog strings', () => {
    expect(t('feedback.dialog.counter', { currentText: '52,000', maxText: '100,000' })).toBe('52,000 / 100,000')
    expect(t('feedback.dialog.tooLong', { maxText: '100,000' })).toBe(
      "Sorry, that's too long. Maximum is 100,000 characters.",
    )
    expect(t('feedback.dialog.attachEmail', { email: 'alex@example.com' })).toBe(
      'Attach my email (alex@example.com) so we can reply',
    )
  })

  it('renders the rich-text moreWays line in order (links stripped to text)', () => {
    // Supply each tag as an identity handler so `format()` returns the parts
    // array; concatenating the inner chunks reconstructs the read text.
    const parts = t('feedback.dialog.moreWays', {
      github: (chunks: unknown[]) => chunks.join(''),
      call: (chunks: unknown[]) => chunks.join(''),
    })
    const text = Array.isArray(parts) ? parts.join('') : String(parts)
    expect(text).toBe('You can also browse and vote on GitHub or book a call with David.')
  })
})
