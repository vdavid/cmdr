/**
 * Which sentence the binary-file banner reads, by the keys that run its two suggestions right
 * now: Quick Look (`file.quickLook`, macOS only) and opening in the associated app (`nav.open`).
 * A key the user removed drops out of the sentence, so the banner never shows an empty chip or
 * names a key that runs nothing.
 */
import type { MessageKey } from '#lib/intl/keys.gen.js'

export function binaryWarningKey(keys: { quickLookKey: string | undefined; openKey: string | undefined }): MessageKey {
  if (keys.quickLookKey !== undefined) {
    return keys.openKey !== undefined
      ? 'viewer.binaryWarning.bodyWithKeys'
      : 'viewer.binaryWarning.bodyWithQuickLookKey'
  }
  return keys.openKey !== undefined ? 'viewer.binaryWarning.bodyWithOpenKey' : 'viewer.binaryWarning.bodyWithoutKeys'
}
