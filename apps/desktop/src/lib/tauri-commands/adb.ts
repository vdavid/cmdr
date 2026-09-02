// Android devices over ADB: the device list and connecting one as a volume.
// Contract: `docs/specs/android-adb-backend.md` § "App wiring".

import { commands, type AdbConnectOutcomeError, type AdbDevice } from '$lib/ipc/bindings'
import { TypedFailure } from '$lib/ipc/typed-failure'

export type { AdbConnectOutcomeError, AdbDevice, AdbDeviceState } from '$lib/ipc/bindings'

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

/** Every device the ADB server knows about, whatever its state. */
export async function listAdbDevices(): Promise<AdbDevice[]> {
  return await commands.listAdbDevices()
}

/**
 * Connects a device and registers it as a volume. Resolves to the volume id
 * (`adb-…`); throws {@link AdbConnectFailure} with the typed reason otherwise.
 */
export async function connectAdbDevice(serial: string): Promise<string> {
  const res = await commands.connectAdbDevice(serial)
  if (res.status === 'error') throw new AdbConnectFailure(res.error)
  return res.data
}
