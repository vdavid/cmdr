/**
 * What "Tell me about this" sends, one consent line per kind of fact its model projection carries.
 *
 * The kinds are the backend's (`AboutDisclosure`, from `cmdr_about::model`), and every field the
 * projection can emit names one of them in a Rust test. A `Record` over the generated union is
 * what makes a new kind fail the typecheck here until it has words, so the disclosure can't fall
 * behind what leaves the machine. Adding a line is also a `CLOUD_AI_CONSENT_VERSION` bump
 * (`src-tauri/src/ai/cloud_consent.rs`).
 */

import type { AboutDisclosure } from '#lib/ipc/bindings.js'
import type { MessageKey } from '#lib/intl/keys.gen.js'

/** Each kind's line, in the order the disclosure lists them. */
export const ABOUT_THIS_DISCLOSURE: Record<AboutDisclosure, MessageKey> = {
  identity: 'ai.cloudConsent.aboutThis.item.identity',
  realType: 'ai.cloudConsent.aboutThis.item.realType',
  imageIndex: 'ai.cloudConsent.aboutThis.item.imageIndex',
  download: 'ai.cloudConsent.aboutThis.item.download',
  appAndSigner: 'ai.cloudConsent.aboutThis.item.appAndSigner',
  use: 'ai.cloudConsent.aboutThis.item.use',
  cmdrHistory: 'ai.cloudConsent.aboutThis.item.cmdrHistory',
  git: 'ai.cloudConsent.aboutThis.item.git',
  owner: 'ai.cloudConsent.aboutThis.item.owner',
  importance: 'ai.cloudConsent.aboutThis.item.importance',
  inside: 'ai.cloudConsent.aboutThis.item.inside',
  appData: 'ai.cloudConsent.aboutThis.item.appData',
  removal: 'ai.cloudConsent.aboutThis.item.removal',
  contentsStayHome: 'ai.cloudConsent.aboutThis.item.contentsStayHome',
}

/** The lines, in order: every fact kind, then the look inside a file (the verdict's alone). */
export const ABOUT_THIS_LINES: readonly MessageKey[] = [
  ...Object.values(ABOUT_THIS_DISCLOSURE),
  'ai.cloudConsent.aboutThis.lookInside',
]
