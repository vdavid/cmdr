// The organization's managed policy (MDM), as the backend reads it. The backend enforces it;
// these only fetch what the UI shows. `src-tauri/src/managed_policy/CLAUDE.md`.

import { type UnlistenFn } from '@tauri-apps/api/event'
import { commands, events, type ManagedPolicyView } from '$lib/ipc/bindings'

/** The policy now: per-setting locks plus the feature-level states. Reads the backend's cache. */
export function getManagedPolicy(): Promise<ManagedPolicyView> {
  return commands.getManagedPolicy()
}

/** The policy changed while Cmdr runs (a profile arrived, changed, or went away). */
export function onManagedPolicyChanged(handler: (policy: ManagedPolicyView) => void): Promise<UnlistenFn> {
  return events.managedPolicyChanged.listen((event) => {
    handler(event.payload.policy)
  })
}
