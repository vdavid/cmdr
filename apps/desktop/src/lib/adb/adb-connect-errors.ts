/**
 * The words for a typed `AdbConnectError`: what connecting a device over ADB
 * says when it doesn't produce a volume. Classification is the backend's
 * (`crates/cmdr-adb/src/errors.rs`); the words all live in the `adb.connect.*`
 * catalog so every locale gets its own.
 */
import { tString } from '$lib/intl/messages.svelte'
import type { AdbConnectError } from '$lib/tauri-commands'

/**
 * One sentence for a connect refusal, or `null` when there is nothing to say
 * (the person cancelled it themselves).
 */
export function adbConnectErrorMessage(error: AdbConnectError): string | null {
  switch (error.type) {
    case 'adbNotInstalled':
      return tString('adb.connect.adbNotInstalled')
    case 'unauthorized':
      return tString('adb.connect.unauthorized')
    case 'deviceTooOld':
      return tString('adb.connect.deviceTooOld')
    case 'deviceGone':
      return tString('adb.connect.deviceGone')
    case 'serverUnreachable':
      return tString('adb.connect.serverUnreachable')
    case 'timedOut':
      return tString('adb.connect.timedOut')
    case 'cancelled':
      return null
  }
}
