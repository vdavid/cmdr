/**
 * Which sentence the binary-file banner reads, by the keys that run its two suggestions right
 * now: Quick Look (`file.quickLook`, macOS only) and opening in the associated app (`nav.open`).
 * A key the user removed drops out of the sentence, so the banner never shows an empty chip or
 * names a key that runs nothing.
 */
import type { MessageKey } from '#lib/intl/keys.gen.js'

export function binaryWarningKey(keys: { quickLookBound: boolean; openBound: boolean }): MessageKey {
  if (keys.quickLookBound) {
    return keys.openBound ? 'viewer.binaryWarning.bodyWithKeys' : 'viewer.binaryWarning.bodyWithQuickLookKey'
  }
  return keys.openBound ? 'viewer.binaryWarning.bodyWithOpenKey' : 'viewer.binaryWarning.bodyWithoutKeys'
}
