/**
 * The `onboarding` preview, which is neither prop-driven nor store-seeded.
 *
 * `OnboardingWizard` is a hand-rolled `role="dialog"` overlay (not a
 * `ModalDialog`), and its open flag is a LOCAL `let showOnboarding` in
 * `routes/(main)/+page.svelte` — `onboarding-state.svelte.ts` owns the step
 * cursor, not an `open` field. So there's no store to seed and nothing the
 * harness can render: the preview dispatches the app's own re-entry command
 * (`Cmdr > Onboarding…`, the same one the menu and the palette use) and lets
 * `+page.svelte` open the wizard exactly as it does for a user.
 *
 * The wizard then closes on its own terms (it has no Escape affordance by
 * design), so the gallery holds no open-state for it and has nothing to restore.
 */

import type { CommandDispatchArgs, CommandId } from '$lib/commands'
import { setCurrentStep } from '$lib/onboarding/onboarding-state.svelte'
import { closeGalleryDialog } from './gallery-state.svelte'
import { onboardingFixtures } from './fixtures/onboarding'

/**
 * The app's onboarding re-entry command. Typed rather than inlined at the
 * dispatch call, so renaming the id is a compile error here (and so
 * `cmdr/no-raw-command-dispatch` stays satisfied).
 */
const OPEN_ONBOARDING: CommandId = 'cmdr.openOnboarding'

/** The dispatch seam `listener-setup.ts` already holds, narrowed to what this needs. */
type Dispatch = <K extends CommandId>(commandId: K, ...args: CommandDispatchArgs<K>) => Promise<void>

/**
 * Opens the wizard through the real command, then moves the cursor to the step
 * the row asked for.
 *
 * The jump is what makes every page reviewable: on macOS the wizard always
 * opens at step 1, and step 1 refuses to advance until the user commits to
 * Allow (which needs an app restart) or Deny (which persists a real choice). The
 * step's own content is untouched — its variant and banner come from this
 * machine's real FDA state, computed when the wizard opened.
 */
export async function openOnboardingPreview(stateId: string, dispatch: Dispatch): Promise<void> {
  const step = onboardingFixtures[stateId]
  if (step === undefined) return
  // The wizard is a full-window overlay that takes over the screen, and it isn't
  // the gallery's own preview, so whatever the gallery had up would sit hidden
  // behind it and reappear when the wizard closes.
  closeGalleryDialog()
  // Awaited end to end: the handler returns `+page.svelte`'s opener, which loads
  // settings and probes for Full Disk Access before the wizard exists. Setting
  // the step before that lands would be overwritten by `openWizard()`.
  await dispatch(OPEN_ONBOARDING)
  setCurrentStep(step)
}
