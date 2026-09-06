/**
 * Every reason a connect can stop has words, and they follow the rules for copy
 * a person reads after something didn't work.
 *
 * ❗ Two failures live here. A reason with no catalog key renders the key itself
 * (or an empty pane), and a reason that borrowed another's sentence sends the
 * user to fix the wrong thing: "check your password" for a server that never saw
 * one is the exact case the outcome enum was split to prevent.
 */
import { describe, it, expect, beforeAll, afterAll } from 'vitest'
import { _setLocaleForTests } from '$lib/intl/locale'
import { wordConnectRefusal } from './connect-refusals'
import type { ConnectRefusalKind } from './connect-refusals'

/** One value per `ConnectRefusalKind`. Adding a kind makes this fail to typecheck. */
const KINDS: ConnectRefusalKind[] = [
  'authentication_rejected',
  'needs_credentials',
  'auth_method_unsupported',
  'certificate_untrusted',
  'not_a_webdav_server',
  'invalid_url',
  'timed_out',
  'unreachable',
  'host_key_untrusted',
  'host_key_revoked',
]

const subject = { host: 'nas.local', username: 'ada' }

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
    // "That password didn't work for ada" is about the account; "Cmdr couldn't
    // reach nas.local" is about the server, and mixing them misdirects the fix.
    expect(wordConnectRefusal('authentication_rejected', subject)).toContain('ada')
    expect(wordConnectRefusal('unreachable', subject)).toContain('nas.local')
    expect(wordConnectRefusal('timed_out', subject)).toContain('nas.local')
    expect(wordConnectRefusal('needs_credentials', subject)).not.toContain('ada')
  })

  it('❌ never tells someone who offered nothing that their password is wrong', () => {
    expect(wordConnectRefusal('needs_credentials', subject)).not.toBe(
      wordConnectRefusal('authentication_rejected', subject),
    )
    expect(wordConnectRefusal('auth_method_unsupported', subject)).not.toContain('password')
  })
})
