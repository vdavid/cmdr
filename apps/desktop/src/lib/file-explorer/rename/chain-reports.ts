/**
 * The two running toasts a chained rename speaks through: the names it didn't
 * apply, and the renames a slow volume is still working on.
 *
 * Both are ONE toast per pane, replaced in place as more arrive, because the
 * toast stack holds five and silently DROPS a new one once they're all
 * persistent. A toast per name would lose everything past the fifth with
 * nothing said, which is the failure this reporting exists to prevent.
 *
 * Why two and not one: a slow rename is not a refusal. Saying a file kept its
 * name while the volume is still renaming it would be a lie, and a chain over a
 * slow volume produces both at once, where a shared stack would starve one.
 *
 * Full rationale: `DETAILS.md` § "Saying so, in one toast that grows".
 */

import { addToastForPane, dismissToast, type ToastOriginPane } from '$lib/ui/toast'
import { tString } from '$lib/intl/messages.svelte'
import { formatInteger } from '$lib/intl/number-format'

/** A rename the slow volume is still working on (a rename target fits as is). */
export interface StillRenamingEntry {
  /** The name it had before the rename. */
  originalName: string
  isDirectory: boolean
}

export interface ChainReportsDeps {
  /** Owning pane, so the reports stay pane-scoped. */
  paneId: ToastOriginPane
}

/** What the toast calls a group of renames: the `kind` select in `stillRenamingAndOthers`. */
function kindOf(entries: StillRenamingEntry[]): 'folders' | 'files' | 'mixed' {
  if (entries.every((e) => e.isDirectory)) return 'folders'
  if (entries.every((e) => !e.isDirectory)) return 'files'
  return 'mixed'
}

export function createChainReports(deps: ChainReportsDeps) {
  const keptNamesToastId = `rename-kept-names-${deps.paneId}`
  // Names counted by the toast currently on screen: dismissing it is the user
  // saying they've read it, and the next one starts over.
  let keptNamesCount = 0

  const stillRenamingToastId = `rename-still-renaming-${deps.paneId}`
  // The renames the toast counts, oldest first, each by its own token so two
  // renames of same-named files settle independently.
  let stillRenaming: StillRenamingEntry[] = []
  // Whether that toast is on screen. Dismissing it is the user saying they've
  // read it: the renames run on, but their settles don't bring it back.
  let stillRenamingShown = false

  function showStillRenaming(): void {
    if (stillRenaming.length === 0) {
      dismissToast(stillRenamingToastId)
      stillRenamingShown = false
      return
    }
    const newest = stillRenaming[stillRenaming.length - 1]
    const others = stillRenaming.slice(0, -1)
    const content =
      others.length === 0
        ? tString('fileExplorer.rename.stillRenaming', { name: newest.originalName })
        : tString('fileExplorer.rename.stillRenamingAndOthers', {
            name: newest.originalName,
            kind: kindOf(others),
            others: others.length,
            othersText: formatInteger(others.length),
          })
    addToastForPane(deps.paneId, content, {
      level: 'info',
      dismissal: 'persistent',
      id: stillRenamingToastId,
      onDismiss: () => {
        stillRenamingShown = false
      },
    })
    stillRenamingShown = true
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
     * Says a slow volume is still renaming `target` until `settled` resolves,
     * and hands `settled` back for the caller to report how it ended.
     *
     * A slow rename is NOT a refusal: it may well land. So this never says the
     * file kept its name, and stays a separate message from `keptName` however
     * tempting the shared shape looks. The toast only counts what's still
     * running, and goes once nothing is.
     */
    stillRenaming<T>(target: StillRenamingEntry, settled: Promise<T>): Promise<T> {
      // A copy, so it's this rename's own token even if the caller reuses the object.
      const entry = { originalName: target.originalName, isDirectory: target.isDirectory }
      stillRenaming.push(entry)
      showStillRenaming()
      return settled.finally(() => {
        const before = stillRenaming.length
        stillRenaming = stillRenaming.filter((e) => e !== entry)
        if (stillRenaming.length !== before && stillRenamingShown) showStillRenaming()
      })
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
      // The renames run on; their settles just have nothing left to update.
      dismissToast(stillRenamingToastId)
      stillRenaming = []
      stillRenamingShown = false
    },
  }
}
