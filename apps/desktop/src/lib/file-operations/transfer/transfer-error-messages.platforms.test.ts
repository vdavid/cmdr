/**
 * The write-error copy that names an OS place reads in that OS's words: the home folder is
 * the "user folder" on Windows, and the trash is "the Recycle Bin". Vocabulary:
 * `docs/style-guide.md` § "Platform vocabulary".
 */

import { describe, expect, it, onTestFinished } from 'vitest'
import { withPlatform } from '#lib/platform/test-platform.js'
import { getUserFriendlyMessage, homeFolderMessage } from './transfer-error-messages'

describe('the home-folder refusal and the trash errors per OS', () => {
  it.each([
    ['macos', 'Please don’t delete your home folder', 'Hey, you can’t delete your own home folder.'],
    ['windows', 'Please don’t delete your user folder', 'Hey, you can’t delete your own user folder.'],
    ['linux', 'Please don’t delete your home folder', 'Hey, you can’t delete your own home folder.'],
  ] as const)('names the home folder in %s’s words', (platform, title, message) => {
    onTestFinished(withPlatform(platform))
    const refusal = homeFolderMessage('/Users/test_user_1', 'isHome', 'delete')
    expect(refusal.title).toBe(title)
    expect(refusal.message).toBe(message)
  })

  it.each([
    ['macos', 'Couldn’t move to trash', 'Couldn’t move the file to trash.'],
    ['windows', 'Couldn’t move to the Recycle Bin', 'Couldn’t move the file to the Recycle Bin.'],
    ['linux', 'Couldn’t move to trash', 'Couldn’t move the file to trash.'],
  ] as const)('names the trash in %s’s words', (platform, title, message) => {
    onTestFinished(withPlatform(platform))
    const friendly = getUserFriendlyMessage({ type: 'io_error', path: '/p', message: 'm' }, 'trash')
    expect(friendly.title).toBe(title)
    expect(friendly.message).toBe(message)
  })
})
