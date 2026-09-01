// Android devices over ADB: the device list and connecting one as a volume.
// Contract: `docs/specs/android-adb-backend.md` § "App wiring".

import { commands } from '$lib/ipc/bindings'
import { TypedFailure } from '$lib/ipc/typed-failure'

// ---------------------------------------------------------------------------
// TODO(bindings-shim): DELETE this block once `pnpm bindings:regen` (run in
// `apps/desktop`) emits `AdbDevice`, `AdbDeviceState`, `AdbConnectError`, and the
// two commands from Rust; then import the types from `$lib/ipc/bindings` and
// call `commands.listAdbDevices` / `commands.connectAdbDevice` directly.
// ---------------------------------------------------------------------------

/** What the ADB server reports for a device. Only `ready` can be connected. */
export type AdbDeviceState =
  | 'ready'
  | 'unauthorized'
  | 'offline'
  | 'noPermissions'
  | 'connecting'
  | 'authorizing'
  | 'recovery'
  | 'bootloader'
  | 'sideload'
  | 'unknown'

/** One row of `host:devices-l`. */
export interface AdbDevice {
  serial: string
  state: AdbDeviceState
  product?: string | null
  model?: string | null
  device?: string | null
  transportId?: string | null
}

/** Why a connect didn't produce a volume (`AdbConnectError` in `crates/cmdr-adb`). */
export type AdbConnectError =
  | { type: 'serverUnreachable' }
  | { type: 'adbNotInstalled' }
  | { type: 'deviceGone' }
  | { type: 'unauthorized' }
  | { type: 'deviceTooOld' }
  | { type: 'timedOut' }
  | { type: 'cancelled' }

type AdbResult<T> = { status: 'ok'; data: T } | { status: 'error'; error: AdbConnectError }

interface AdbCommands {
  listAdbDevices(): Promise<AdbDevice[]>
  connectAdbDevice(serial: string): Promise<AdbResult<string>>
}

const adbCommands = commands as unknown as AdbCommands

// ------------------------------- end of shim --------------------------------

/** A connect refusal, still carrying the backend's typed reason. */
export class AdbConnectFailure extends TypedFailure<AdbConnectError> {
  constructor(failure: AdbConnectError) {
    super(failure, `adb connect refused: ${failure.type}`)
    this.name = 'AdbConnectFailure'
  }
}

/** The typed refusal behind a caught value, or `null` when it isn't one. */
export function asAdbConnectError(error: unknown): AdbConnectError | null {
  return error instanceof AdbConnectFailure ? error.failure : null
}

/** Every device the ADB server knows about, whatever its state. */
export async function listAdbDevices(): Promise<AdbDevice[]> {
  return await adbCommands.listAdbDevices()
}

/**
 * Connects a device and registers it as a volume. Resolves to the volume id
 * (`adb-…`); throws {@link AdbConnectFailure} with the typed reason otherwise.
 */
export async function connectAdbDevice(serial: string): Promise<string> {
  const res = await adbCommands.connectAdbDevice(serial)
  if (res.status === 'error') throw new AdbConnectFailure(res.error)
  return res.data
}
