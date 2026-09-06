// Android devices over ADB: the device list and connecting one as a volume.
// Contract: `docs/specs/android-adb-backend.md` § "App wiring".

import { commands, type AdbConnectOutcomeError, type AdbDevice, type AdbInstallStatus } from '$lib/ipc/bindings'
import { TypedFailure } from '$lib/ipc/typed-failure'

export type { AdbConnectOutcomeError, AdbDevice, AdbDeviceState, AdbInstallStatus } from '$lib/ipc/bindings'

/** A connect refusal, still carrying the backend's typed reason. */
export class AdbConnectFailure extends TypedFailure<AdbConnectOutcomeError> {
  constructor(failure: AdbConnectOutcomeError) {
    super(failure, `adb connect refused: ${failure.type}`)
    this.name = 'AdbConnectFailure'
  }
}

/** The typed refusal behind a caught value, or `null` when it isn't one. */
export function asAdbConnectError(error: unknown): AdbConnectOutcomeError | null {
  return error instanceof AdbConnectFailure ? error.failure : null
}

/**
 * Applies both ADB settings without a restart: the device tracker comes back
 * under `binaryPath`, or stops and takes its device rows with it.
 *
 * `binaryPath` is `null` for "look for `adb` the usual way".
 */
export async function setAdbSettings(enabled: boolean, binaryPath: string | null): Promise<void> {
  await commands.setAdbSettings(enabled, binaryPath)
}

/** Every device the ADB server knows about, whatever its state. */
export async function listAdbDevices(): Promise<AdbDevice[]> {
  return await commands.listAdbDevices()
}

/**
 * Connects a device and registers it as a volume. Resolves to the volume id
 * (`adb-…`); throws {@link AdbConnectFailure} with the typed reason otherwise.
 *
 * `attemptId` is the caller's own name for this dial, minted BEFORE the call so
 * a cancel button is armed while the phone still shows its "Allow USB
 * debugging?" prompt. {@link cancelAdbConnect} takes the same id.
 */
export async function connectAdbDevice(serial: string, attemptId: string): Promise<string> {
  const res = await commands.connectAdbDevice(serial, attemptId)
  if (res.status === 'error') throw new AdbConnectFailure(res.error)
  return res.data
}

/**
 * Calls off the dial running under `attemptId`, resolving to whether one was.
 * A `false` is ordinary: a cancel racing a dial that just finished finds
 * nothing filed.
 */
export async function cancelAdbConnect(attemptId: string): Promise<boolean> {
  return await commands.cancelAdbConnect(attemptId)
}

/** Where Cmdr found `adb` and whether the device list is live. Reads what is already known. */
export async function getAdbInstallStatus(): Promise<AdbInstallStatus> {
  return await commands.getAdbInstallStatus()
}

/**
 * Looks for `adb` again and revives the device tracker if it turns up: what a
 * "Re-check" button in Settings calls after someone installs platform-tools.
 *
 * ❗ One call per click. This is the only path allowed to retry `adb
 * start-server`, so ❌ never poll it or call it on mount.
 */
export async function recheckAdbInstall(): Promise<AdbInstallStatus> {
  return await commands.recheckAdbInstall()
}
