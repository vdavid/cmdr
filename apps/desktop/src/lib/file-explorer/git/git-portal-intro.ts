/**
 * The once-ever intro to the `.git` portal, raised the first time a pane lands on
 * a repo ROOT (a subfolder doesn't count: the `.git` folder the toast points at
 * sits right there in the listing only at the root).
 *
 * The portal is easy to miss: it lives in a hidden folder, and most folks never
 * think to open `.git`. Persistent on purpose, like the open-terminal hint: it shows
 * exactly once, so a four-second toast would be a coin flip on anyone reading it.
 *
 * The flag is spent when the toast is RAISED, never when it's answered (the
 * `nudges/` rule): a crash mid-toast then costs one intro, the other order risks
 * repeating it forever. Spending it synchronously, before `addToast`, is also what
 * keeps two panes landing on two repos at startup from both raising it.
 */

import { getSetting, setSetting } from '#lib/settings/index.js'
import { addToast } from '#lib/ui/toast/index.js'
import { isE2eRun } from '#lib/app-mode.js'
import GitPortalIntroToastContent from './GitPortalIntroToastContent.svelte'

/** Auto-rendered in Settings > Advanced's "Hints and warnings" card; off shows the intro once more. */
export const GIT_PORTAL_INTRO_SEEN_KEY = 'fileExplorer.git.portalIntroSeen'

const TOAST_ID = 'git-portal-intro'

/** Wide enough for the three action buttons on one line, longer locales included. */
const INTRO_TOAST_WIDTH_PX = 480

export interface GitPortalIntroArgs {
  /** The pane's canonical path (`~` expanded), or `null` before it's known. */
  panePath: string | null
  /** The repo the pane's path belongs to (`RepoInfo.repoRoot`). */
  repoRoot: string
  /** Takes the pane into `<repoRoot>/.git`. */
  goToGitDir: () => void
}

/** Raises the intro if this is its moment. Returns whether it did. */
export function maybeShowGitPortalIntro({ panePath, repoRoot, goToGitDir }: GitPortalIntroArgs): boolean {
  // A Playwright shard or the screenshot pass: a persistent toast would sit over
  // whatever the next spec looks at.
  if (isE2eRun()) return false
  if (panePath !== repoRoot) return false
  if (getSetting(GIT_PORTAL_INTRO_SEEN_KEY)) return false
  // Nothing to introduce while the portal is switched off.
  if (!getSetting('fileExplorer.git.showVirtualGitPortal')) return false

  setSetting(GIT_PORTAL_INTRO_SEEN_KEY, true)
  addToast(GitPortalIntroToastContent, {
    id: TOAST_ID,
    level: 'info',
    dismissal: 'persistent',
    props: { goToGitDir },
    // Three buttons in one row: at the default 360 px their labels wrapped mid-word.
    widthPx: INTRO_TOAST_WIDTH_PX,
  })
  return true
}
