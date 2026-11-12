/**
 * Every reason a connect can stop has words, and they follow the rules for copy
 * a person reads after something didn't work.
 *
 * ❗ Two failures live here. A reason with no catalog key renders the key itself
 * (or an empty pane), and a reason that borrowed another's sentence sends the
 * user to fix the wrong thing: "check your password" for a server that never saw
 * one is the exact case the outcome enum was split to prevent.
 */
import { describe, it, expect, beforeAll, afterAll, onTestFinished } from 'vitest'
import { _setLocaleForTests } from '#lib/intl/locale.js'
import { withPlatform } from '#lib/platform/test-platform.js'
import { refusalField, wordConnectRefusal, wordPaneRefusal } from './connect-refusals'
import type { ConnectRefusalKind } from './connect-refusals'

/** One value per `ConnectRefusalKind`. Adding a kind makes this fail to typecheck. */
const KINDS: ConnectRefusalKind[] = [
  'authentication_rejected',
  'needs_credentials',
  'auth_method_unsupported',
  'certificate_untrusted',
  'certificate_rejected',
  'not_a_webdav_server',
  'invalid_url',
  'timed_out',
  'unreachable',
  'host_key_untrusted',
  'host_key_revoked',
  'start_folder_outside_root',
  'root_not_found',
  'start_folder_not_found',
  'save_unconfirmed',
  'account_not_permitted',
  'address_taken',
  'secret_not_moved',
  'account_changed',
  'operation_running',
  'access_denied',
  'bucket_list_refused',
  'bucket_not_found',
  'region_mismatch',
  'clock_skewed',
  'not_an_s3_endpoint',
  's3_field_malformed',
  'endpoint_malformed',
  'export_not_found',
  'export_refused',
  'reserved_port_required',
  'no_nfs_v3',
]

const subject = { host: 'testnas.local', username: 'test_user_1' }

beforeAll(() => {
  _setLocaleForTests('en-US')
})
afterAll(() => {
  _setLocaleForTests(null)
})

describe('wordConnectRefusal', () => {
  it('gives every reason its own sentence', () => {
    const said = KINDS.map((kind) => wordConnectRefusal(kind, subject))
    for (const [i, sentence] of said.entries()) {
      expect(sentence, `${KINDS[i]} has no words`).not.toBe('')
      expect(sentence, `${KINDS[i]} renders its key`).not.toContain('servers.')
      expect(sentence, `${KINDS[i]} left a placeholder unfilled`).not.toMatch(/\{[a-z]/i)
    }
    expect(new Set(said).size).toBe(KINDS.length)
  })

  it('follows the writing rules for copy after something did not work', () => {
    for (const kind of KINDS) {
      const sentence = wordConnectRefusal(kind, subject)
      for (const banned of ['error', 'failed', 'invalid', 'just ']) {
        expect(sentence.toLowerCase(), `${kind} says "${banned}"`).not.toContain(banned)
      }
      // ❌ No backend diagnostics in front of a person.
      for (const jargon of ['PROPFIND', 'Digest', 'rung', 'transport', 'Basic auth']) {
        expect(sentence, `jargon in ${kind}: "${jargon}"`).not.toContain(jargon)
      }
    }
  })

  it('names the account only where the account is what is wrong', () => {
    // "That password didn't work for test_user_1" is about the account; "Cmdr couldn't
    // reach testnas.local" is about the server, and mixing them misdirects the fix.
    expect(wordConnectRefusal('authentication_rejected', subject)).toContain('test_user_1')
    expect(wordConnectRefusal('unreachable', subject)).toContain('testnas.local')
    expect(wordConnectRefusal('timed_out', subject)).toContain('testnas.local')
    expect(wordConnectRefusal('needs_credentials', subject)).not.toContain('test_user_1')
  })

  it('❌ never tells someone who offered nothing that their password is wrong', () => {
    expect(wordConnectRefusal('needs_credentials', subject)).not.toBe(
      wordConnectRefusal('authentication_rejected', subject),
    )
    expect(wordConnectRefusal('auth_method_unsupported', subject)).not.toContain('password')
  })

  it('names the server a folder refusal or an unconfirmed save is about', () => {
    expect(wordConnectRefusal('root_not_found', subject)).toContain('testnas.local')
    expect(wordConnectRefusal('start_folder_not_found', subject)).toContain('testnas.local')
    expect(wordConnectRefusal('save_unconfirmed', subject)).toContain('testnas.local')
  })

  it.each(['macos', 'windows', 'linux'] as const)(
    '❌ never tells anyone on %s to trust a revoked or broken certificate (#1111)',
    (platform) => {
      onTestFinished(withPlatform(platform))
      const rejected = wordConnectRefusal('certificate_rejected', subject)
      expect(rejected).not.toBe(wordConnectRefusal('certificate_untrusted', subject))
      for (const nudge of ['Keychain', 'trusted certificates', 'Add it', 'Manage user certificates']) {
        expect(rejected, `says "${nudge}"`).not.toContain(nudge)
      }
      expect(refusalField('certificate_rejected')).toBe('form')
    },
  )

  it('❌ never blames the password when the account signed in and the place turned it away', () => {
    // The password worked. Pointing at it sends the user to fix the one thing that
    // isn't wrong, and the sentence belongs above the buttons rather than marking
    // the password field invalid.
    const sentence = wordConnectRefusal('account_not_permitted', subject)
    expect(sentence).toContain('test_user_1')
    expect(sentence.toLowerCase()).not.toContain('password')
    expect(refusalField('account_not_permitted')).toBe('form')
  })
})

describe('wordPaneRefusal', () => {
  it('names an unreachable saved server by the name the user gave it', () => {
    const sentence = wordPaneRefusal('unreachable', { ...subject, name: 'Testnas' })
    expect(sentence).toContain('Testnas')
    expect(sentence).not.toContain('testnas.local')
  })

  it('falls back to the host when the place has no name', () => {
    expect(wordPaneRefusal('unreachable', { ...subject, name: '' })).toContain('testnas.local')
  })

  it('words every other reason the way the sheet does', () => {
    for (const kind of KINDS.filter((k) => k !== 'unreachable')) {
      expect(wordPaneRefusal(kind, { ...subject, name: 'Testnas' })).toBe(wordConnectRefusal(kind, subject))
    }
  })
})

describe('S3 refusals', () => {
  const s3 = { host: 's3.eu-west-1.amazonaws.com', username: 'AKIAEXAMPLE', protocol: 's3' as const }

  it('names the region a bucket lives in when the server said, and still reads without one', () => {
    expect(wordConnectRefusal('region_mismatch', { ...s3, region: 'us-east-2' })).toContain('us-east-2')
    const unnamed = wordConnectRefusal('region_mismatch', s3)
    expect(unnamed).not.toMatch(/\{[a-z]/i)
    expect(unnamed).not.toBe('')
  })

  it('says “secret access key” where SFTP and WebDAV say “password”', () => {
    // An S3 account has no password, and a sentence that asks for one sends the reader
    // looking for something their provider never gave them.
    for (const kind of [
      'authentication_rejected',
      'needs_credentials',
      'password_missing',
      'secret_not_stored',
      'saved_secret_not_updated',
      'secret_not_moved',
    ] as const) {
      const sentence = wordConnectRefusal(kind, s3)
      expect(sentence.toLowerCase(), kind).not.toContain('password')
      expect(sentence, kind).toContain('secret access key')
    }
    // Everyone else keeps the password wording.
    expect(wordConnectRefusal('authentication_rejected', subject)).toContain('password')
  })

  it('never names the access key ID as if it were a person', () => {
    for (const kind of ['authentication_rejected', 'password_missing', 'access_denied'] as const) {
      expect(wordConnectRefusal(kind, s3), kind).not.toContain('AKIAEXAMPLE')
    }
  })

  it('points a refused bucket at its name too', () => {
    // R2 answers AccessDenied, never NoSuchBucket, for a mistyped bucket when the key
    // is scoped to one bucket, so the typo is one thing the refusal may mean.
    expect(wordConnectRefusal('access_denied', s3)).toContain('bucket name')
  })

  it('puts each S3 refusal under the field that fixes it', () => {
    expect(refusalField('access_denied')).toBe('secret')
    expect(refusalField('bucket_list_refused')).toBe('bucket')
    expect(refusalField('bucket_not_found')).toBe('bucket')
    expect(refusalField('region_mismatch')).toBe('region')
    expect(refusalField('s3_field_malformed')).toBe('region')
    expect(refusalField('not_an_s3_endpoint')).toBe('address')
    expect(refusalField('endpoint_malformed')).toBe('address')
    // A Mac whose clock is off is nothing any field fixes.
    expect(refusalField('clock_skewed')).toBe('form')
  })
})

describe('NFS refusals', () => {
  it('puts each export refusal under the export, where picking another one fixes it', () => {
    expect(refusalField('export_not_found')).toBe('export')
    expect(refusalField('export_refused')).toBe('export')
    // ❗ The fix is the export's `insecure` option (or another export), ❌ not the address.
    expect(refusalField('reserved_port_required')).toBe('export')
  })

  it('puts a server with no NFSv3 under the address: no export on it can help', () => {
    expect(refusalField('no_nfs_v3')).toBe('address')
  })

  it('names the server each one is about', () => {
    for (const kind of ['export_not_found', 'export_refused', 'reserved_port_required', 'no_nfs_v3'] as const) {
      expect(wordConnectRefusal(kind, subject), kind).toContain('testnas.local')
    }
  })
})

describe('refusalField', () => {
  it('puts each folder refusal under the folder it is about', () => {
    // ❗ "Cmdr can't open this folder" under the start folder sends someone to
    // retype the wrong path, so the root and the start folder keep their own.
    expect(refusalField('root_not_found')).toBe('root')
    expect(refusalField('start_folder_not_found')).toBe('start_folder')
    expect(refusalField('start_folder_outside_root')).toBe('start_folder')
  })

  it('puts an unconfirmed save above the buttons, since no field can fix a server that did not answer', () => {
    expect(refusalField('save_unconfirmed')).toBe('form')
  })

  it('puts a move refusal where the fix is: the address, or the password that stayed behind', () => {
    expect(refusalField('address_taken')).toBe('address')
    expect(refusalField('secret_not_moved')).toBe('secret')
    expect(refusalField('account_changed')).toBe('form')
    // No field fixes a copy that's still running.
    expect(refusalField('operation_running')).toBe('form')
  })
})

describe('a move to an address another saved server holds', () => {
  it('names the server that holds it, so the person knows which one to open or forget', () => {
    expect(wordConnectRefusal('address_taken', { ...subject, takenBy: 'Testnas' })).toContain('Testnas')
  })
})
