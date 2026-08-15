/**
 * Coverage honesty on screen: what the last run couldn't cover, and the offers that
 * answer it.
 *
 * The note itself is written by whichever run produced it (`search-runners.ts`); this
 * module only reads it and decides what may be OFFERED over it, which is a different
 * question with its own gates:
 *
 *   - **"Index this drive"** shows only for an UNCOVERED gap (an unresolved path sits on
 *     a drive that's already indexed, so there's nothing to turn on), only for a drive
 *     the live volume list can name, never for one the user silenced, and ❌ never while
 *     that drive's first index is already running branch by branch: there is nothing to
 *     turn on, and pressing it would restart the phases. The NOTE still renders (in its
 *     own words for that case): silencing or withholding the offer doesn't make the gap
 *     untrue.
 *   - **"Set up Full Disk Access"** shows only when a run was REFUSED a folder, this is
 *     macOS, and Cmdr doesn't have the permission yet. ❌ Never over `declined` (no
 *     permission opens a NAS snapshot tree, so it would send someone to System Settings
 *     to fix nothing).
 *
 * The TCC probe is the quiet `checkFullDiskAccessQuiet` and runs only when a refusal is
 * on screen: the loud one fires a TCC-registration storm per denial, and this runs per
 * search (`lib/onboarding/CLAUDE.md`).
 */

import { checkFullDiskAccessQuiet } from '$lib/tauri-commands'
import { isMacOS } from '$lib/shortcuts/key-capture'
import { getVolumes } from '$lib/stores/volume-store.svelte'
import { isDriveSilenced, silenceDrive as silenceDrivePref } from '$lib/indexing/drive-index-prefs'
import { isVolumeCoveredInPhases } from '$lib/indexing/index-state.svelte'
import { offersFullDiskAccess, type CoverageNote } from './coverage-note'
import { indexUncoveredDrive } from './coverage-actions'
import { describeVolume } from './search-target-volume'
import type { SearchCta } from './search-analytics'
import { trackCtaOffered, trackCtaUsed } from './search-run-tracking'
import { getCoverageNote } from './search-state.svelte'

export interface CoverageCtaDeps {
  /**
   * The host's Full Disk Access route (the onboarding wizard's step 1), or `undefined`
   * when the host offers none. Read through a getter so a prop change still lands.
   */
  getGrantFullDiskAccess: () => (() => void) | undefined
  /** Closes the dialog: the FDA route goes to System Settings, over this dialog's head. */
  closeDialog: () => void
}

export interface CoverageCtaView {
  readonly note: CoverageNote | null
  /** How to name the drive a gap belongs to, per the volume the BACKEND routed to. */
  readonly driveName: string
  readonly isNetwork: boolean
  /** Whether that drive's first index is running right now, which the note speaks to. */
  readonly isIndexing: boolean
  /** Turns indexing on for the uncovered drive, or `null` when nothing may be offered. */
  readonly indexDrive: (() => void) | null
  /** "Don't ask again" for this drive: the same persisted silence the first-connect prompt honors. */
  silenceDrive: () => void
  /** The Full Disk Access route, or `null` when granting it would change nothing. */
  readonly grantFullDiskAccess: (() => void) | null
}

export function createCoverageCta(deps: CoverageCtaDeps): CoverageCtaView {
  const note = $derived(getCoverageNote())
  /**
   * Looked up by the volume the BACKEND routed to, not the pane's: a typed scope can
   * point at another drive, and offering to index the wrong one would be worse than
   * saying nothing.
   */
  const drive = $derived(describeVolume(getVolumes(), note?.volumeId ?? ''))

  /**
   * Whether this drive's first index is under way right now. It withholds the offer
   * AND changes the note's own wording, so both read as one state rather than as an
   * offer contradicting the sentence above it.
   */
  const isIndexing = $derived(note !== null && note.volumeId !== '' && isVolumeCoveredInPhases(note.volumeId))

  const ctaVolumeId = $derived(
    note && note.uncoveredScopes.length > 0 && note.volumeId !== '' && !isDriveSilenced(note.volumeId) && !isIndexing
      ? note.volumeId
      : null,
  )

  /**
   * Whether Cmdr currently has Full Disk Access. Starts at `true` so nothing is
   * offered before the probe answers: an offer that arrives and then vanishes is
   * worse than one that arrives a moment late, and "already granted" is the state
   * in which the offer would be useless anyway.
   */
  let hasFullDiskAccess = $state(true)

  /** Ask the OS, but only when the answer could change what's on screen. */
  $effect(() => {
    if ((note?.live?.permissionDenied.length ?? 0) === 0) return
    if (!isMacOS()) return
    void checkFullDiskAccessQuiet().then((granted) => {
      hasFullDiskAccess = granted
    })
  })

  /**
   * Closing before routing is deliberate: the wizard is the app's modal and this dialog
   * is a modal over it, and the user who presses this is going to System Settings and
   * then restarting.
   */
  const grantFullDiskAccess = $derived.by(() => {
    const route = deps.getGrantFullDiskAccess()
    if (!route || !offersFullDiskAccess({ note, isMac: isMacOS(), hasFullDiskAccess })) return null
    return () => {
      trackCtaUsed('fullDiskAccess')
      deps.closeDialog()
      route()
    }
  })

  /**
   * The offer is reported from an effect rather than from the run's terminal event
   * because the Full Disk Access one depends on a TCC probe that answers after the run
   * does; reporting at settle time would miss every offer that arrives a moment late and
   * put the conversion rate over 100%.
   */
  let offeredCta = $state<SearchCta>('none')
  $effect(() => {
    const cta: SearchCta =
      ctaVolumeId !== null ? 'indexDrive' : grantFullDiskAccess !== null ? 'fullDiskAccess' : 'none'
    if (cta === offeredCta) return
    offeredCta = cta
    if (cta !== 'none') trackCtaOffered(cta)
  })

  const indexDrive = $derived.by(() => {
    const volumeId = ctaVolumeId
    if (volumeId === null) return null
    return () => {
      trackCtaUsed('indexDrive')
      void indexUncoveredDrive(volumeId, drive.name)
    }
  })

  return {
    get note() {
      return note
    },
    get driveName() {
      return drive.name
    },
    get isNetwork() {
      return drive.isNetwork
    },
    get isIndexing() {
      return isIndexing
    },
    get indexDrive() {
      return indexDrive
    },
    silenceDrive: () => {
      if (note?.volumeId) silenceDrivePref(note.volumeId)
    },
    get grantFullDiskAccess() {
      return grantFullDiskAccess
    },
  }
}
