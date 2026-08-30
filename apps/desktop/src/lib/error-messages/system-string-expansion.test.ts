/**
 * Every friendly-error factory must expand the `{system_settings}`-family tokens
 * before the copy reaches a user.
 *
 * The tokens exist so a pane name follows the SYSTEM's language rather than the
 * app's (`system-strings.svelte.ts`): a user running Cmdr in German on an English
 * Mac has to be sent to "System Settings", because that is what their screen
 * says. A factory that returns the raw catalog string instead ships a literal
 * `{system_settings}` to the user, which is why this is an invariant over ALL the
 * factories rather than a test of one string.
 */

import { describe, it, expect } from 'vitest'
import { getGitErrorMessage, type FriendlyGitErrorKind } from './git-error-messages'
import { getProviderSuggestion, type Provider, type ProviderCategory } from './provider-error-messages'
import { getListingErrorMessage } from './listing-error-messages'
import { systemStrings } from '$lib/system-strings.svelte'

/** The tokens `expandSystemStrings` owns. None may survive into rendered copy. */
const TOKENS = [
  '{system_settings}',
  '{privacy_and_security}',
  '{full_disk_access}',
  '{files_and_folders}',
  '{local_network}',
  '{appearance}',
]

const GIT_KINDS: FriendlyGitErrorKind[] = [
  'notARepo',
  'orphanedWorktree',
  'corruptRepo',
  'indexLocked',
  'permissionDenied',
  'bareRepo',
  'blobTooLarge',
  'shallowBoundary',
  'missingObject',
  'gitDirPermissionDenied',
]

const PROVIDERS: Provider[] = [
  'dropbox',
  'googleDrive',
  'oneDrive',
  'box',
  'pCloud',
  'nextcloud',
  'synologyDrive',
  'tresorit',
  'protonDrive',
  'sync',
  'egnyte',
  'macDroid',
  'iCloud',
  'pCloudFuse',
  'macFuse',
  'veraCrypt',
  'cmVolumes',
  'genericCloudStorage',
]

const CATEGORIES: ProviderCategory[] = ['transient', 'needs_action', 'serious']

const expectNoTokens = (text: string, where: string): void => {
  for (const token of TOKENS) expect(`${where}: ${text}`).not.toContain(token)
}

describe('no friendly-error factory leaks a system-string token', () => {
  it('git error copy is expanded', () => {
    for (const kind of GIT_KINDS) {
      const message = getGitErrorMessage(kind)
      expectNoTokens(message.title, `git ${kind} title`)
      expectNoTokens(message.message, `git ${kind} message`)
      expectNoTokens(message.suggestion, `git ${kind} suggestion`)
    }
  })

  it('provider suggestions are expanded', () => {
    for (const provider of PROVIDERS) {
      for (const category of CATEGORIES) {
        expectNoTokens(getProviderSuggestion(provider, category), `provider ${provider}/${category}`)
      }
    }
  })

  it('listing error copy is expanded (the path that already did this)', () => {
    const message = getListingErrorMessage({ reason: 'diskFullErrno' })
    expectNoTokens(message.suggestion, 'listing diskFullErrno suggestion')
  })
})

describe('the expanded copy carries the live pane name', () => {
  it('a git permission denial names the System Settings pane', () => {
    const { suggestion } = getGitErrorMessage('permissionDenied')
    expect(suggestion).toContain(systemStrings.systemSettings)
    expect(suggestion).toContain(systemStrings.privacyAndSecurity)
    expect(suggestion).toContain(systemStrings.filesAndFolders)
  })

  it('a macFUSE mount names the System Settings pane', () => {
    expect(getProviderSuggestion('macFuse', 'needs_action')).toContain(systemStrings.systemSettings)
  })
})
