import type { Component } from 'svelte'
import IconArrowLeft from '~icons/lucide/arrow-left'
import IconCheck from '~icons/lucide/check'
import IconChevronDown from '~icons/lucide/chevron-down'
import IconChevronRight from '~icons/lucide/chevron-right'
import IconChevronUp from '~icons/lucide/chevron-up'
import IconCircle from '~icons/lucide/circle'
import IconCircleAlert from '~icons/lucide/circle-alert'
import IconCircleCheck from '~icons/lucide/circle-check'
import IconCopy from '~icons/lucide/copy'
import IconEye from '~icons/lucide/eye'
import IconEyeOff from '~icons/lucide/eye-off'
import IconFolderInput from '~icons/lucide/folder-input'
import IconGitBranch from '~icons/lucide/git-branch'
import IconGitCommitHorizontal from '~icons/lucide/git-commit-horizontal'
import IconGitFork from '~icons/lucide/git-fork'
import IconHourglass from '~icons/lucide/hourglass'
import IconInfo from '~icons/lucide/info'
import IconLink from '~icons/lucide/link'
import IconList from '~icons/lucide/list'
import IconLock from '~icons/lucide/lock'
import IconMoreHorizontal from '~icons/lucide/more-horizontal'
import IconPause from '~icons/lucide/pause'
import IconPlay from '~icons/lucide/play'
import IconRotateCcw from '~icons/lucide/rotate-ccw'
import IconSearch from '~icons/lucide/search'
import IconShieldCheck from '~icons/lucide/shield-check'
import IconShieldOff from '~icons/lucide/shield-off'
import IconSparkles from '~icons/lucide/sparkles'
import IconTag from '~icons/lucide/tag'
import IconTrash2 from '~icons/lucide/trash-2'
import IconTriangleAlert from '~icons/lucide/triangle-alert'
import IconX from '~icons/lucide/x'
import EjectIcon from './EjectIcon.svelte'

/**
 * The single registry of every inline glyph the app renders through `<Icon>`. Keys are the glyph
 * names (`<Icon name="triangle-alert" />`); values are Svelte components. Most are Lucide glyphs
 * (via `unplugin-icons`); a few are custom (`eject`) where Lucide has no equivalent, authored as
 * local `.svelte` components with the same `<svg {...rest}>` shape so they're interchangeable.
 *
 * Adding a glyph: import it here and add one entry. Everything else (the `IconName` union, the
 * Debug "Graphics" catalog, the no-raw-lucide-import lint) keys off this object, so this stays the
 * one place that grows. Pick names from the Lucide set for visual cohesion (see
 * `docs/guides/icons.md`).
 */
export const ICON_COMPONENTS = {
  'arrow-left': IconArrowLeft,
  check: IconCheck,
  'chevron-down': IconChevronDown,
  'chevron-right': IconChevronRight,
  'chevron-up': IconChevronUp,
  circle: IconCircle,
  'circle-alert': IconCircleAlert,
  'circle-check': IconCircleCheck,
  copy: IconCopy,
  eject: EjectIcon,
  eye: IconEye,
  'eye-off': IconEyeOff,
  'folder-input': IconFolderInput,
  'git-branch': IconGitBranch,
  'git-commit-horizontal': IconGitCommitHorizontal,
  'git-fork': IconGitFork,
  hourglass: IconHourglass,
  info: IconInfo,
  link: IconLink,
  list: IconList,
  lock: IconLock,
  'more-horizontal': IconMoreHorizontal,
  pause: IconPause,
  play: IconPlay,
  'rotate-ccw': IconRotateCcw,
  search: IconSearch,
  'shield-check': IconShieldCheck,
  'shield-off': IconShieldOff,
  sparkles: IconSparkles,
  tag: IconTag,
  'trash-2': IconTrash2,
  'triangle-alert': IconTriangleAlert,
  x: IconX,
} satisfies Record<string, Component>

export type IconName = keyof typeof ICON_COMPONENTS
