/**
 * The two running toasts a chained rename speaks through: the names it didn't
 * apply, and the renames the volume never confirmed.
 *
 * Both are ONE toast per pane, replaced in place as more arrive, because the
 * toast stack holds five and silently DROPS a new one once they're all
 * persistent. A toast per name would lose everything past the fifth with
 * nothing said, which is the failure this reporting exists to prevent.
 *
 * Why two and not one: a timeout is not a refusal. Saying a file kept its name
 * when the volume simply never answered would be a lie, and a chain over a slow
 * volume produces both at once, where a shared stack would starve one of them.
 *
 * Full rationale: `DETAILS.md` § "Saying so, in one toast that grows".
 */

import { refreshListing } from '$lib/tauri-commands'
import { addToastForPane, dismissToast, type ToastOriginPane } from '$lib/ui/toast'
import { tString } from '$lib/intl/messages.svelte'
import { formatInteger } from '$lib/intl/number-format'

export interface ChainReportsDeps {
  /** Owning pane, so the reports stay pane-scoped. */
  paneId: ToastOriginPane
  getListingId: () => string
}

/**
 * A volume too slow to answer a rename must not then be asked to list the
 * directory once per unanswered rename. The refresh waits out a quiet spell and
 * runs once; landing AFTER the last straggler is also what makes the listing
 * show the settled truth rather than a half-finished chain.
 */
const UNCONFIRMED_REFRESH_QUIET_MS = 1000

export function createChainReports(deps: ChainReportsDeps) {
  const keptNamesToastId = `rename-kept-names-${deps.paneId}`
  // Names counted by the toast currently on screen: dismissing it is the user
  // saying they've read it, and the next one starts over.
  let keptNamesCount = 0

  const unconfirmedToastId = `rename-unconfirmed-${deps.paneId}`
  // Renames counted by the toast currently on screen, zeroed when the user
  // dismisses it, same as the kept names.
  let unconfirmedCount = 0

  let unconfirmedRefreshTimer: ReturnType<typeof setTimeout> | null = null

  function scheduleUnconfirmedRefresh(): void {
    if (unconfirmedRefreshTimer !== null) clearTimeout(unconfirmedRefreshTimer)
    unconfirmedRefreshTimer = setTimeout(() => {
      unconfirmedRefreshTimer = null
      // Read at fire time: the pane may have moved on, and the listing worth
      // refreshing is the one it is showing now.
      void refreshListing(deps.getListingId())
    }, UNCONFIRMED_REFRESH_QUIET_MS)
  }

  return {
    /**
     * Says which files kept their names when chained renames didn't apply.
     *
     * Persistent on purpose: `handleRenameInput` clears this pane's transient
     * toasts on every keystroke, which is exactly when the user is typing the
     * next name, so a transient one would be gone before it was read.
     *
     * The newest file is the one named, with the reason it kept its name; the
     * others become a count. Holding the arrow through a directory where a dozen
     * names clash is one message that grows, not a dozen fighting for five slots.
     */
    keptName(originalName: string, reason: string): void {
      keptNamesCount += 1
      const others = keptNamesCount - 1
      const content =
        others === 0
          ? tString('fileExplorer.rename.chainKeptOriginalName', { reason, name: originalName })
          : tString('fileExplorer.rename.chainKeptOriginalNameAndOthers', {
              reason,
              name: originalName,
              others,
              othersText: formatInteger(others),
            })
      addToastForPane(deps.paneId, content, {
        level: 'warn',
        dismissal: 'persistent',
        id: keptNamesToastId,
        onDismiss: () => {
          keptNamesCount = 0
        },
      })
    },

    /**
     * Says which renames the volume never confirmed, and refreshes to find out.
     *
     * A timeout is NOT a refusal: the rename may well have landed on disk. So
     * this never says the file kept its name, and stays a separate message from
     * `keptName` however tempting the shared shape looks.
     */
    unconfirmed(name: string): void {
      unconfirmedCount += 1
      const others = unconfirmedCount - 1
      const content =
        others === 0
          ? tString('fileExplorer.rename.unconfirmed', { name })
          : tString('fileExplorer.rename.unconfirmedAndOthers', {
              name,
              others,
              othersText: formatInteger(others),
            })
      addToastForPane(deps.paneId, content, {
        level: 'warn',
        dismissal: 'persistent',
        id: unconfirmedToastId,
        onDismiss: () => {
          unconfirmedCount = 0
        },
      })
      scheduleUnconfirmedRefresh()
    },

    /**
     * Drops both reports, for a pane leaving the directory they name.
     *
     * Carried into the next directory they go on naming a file that isn't on
     * screen any more, and the counts start pooling reasons from directories,
     * and volumes, with nothing to do with each other. The tally belongs to the
     * toast on screen, so dropping the toast is what zeroes it, exactly as
     * dismissing it does.
     *
     * A chain BOUNDARY deliberately doesn't do this: a name nobody has
     * acknowledged is still unacknowledged, and the files are all still there.
     */
    forget(): void {
      dismissToast(keptNamesToastId)
      keptNamesCount = 0
      dismissToast(unconfirmedToastId)
      unconfirmedCount = 0
    },
  }
}
