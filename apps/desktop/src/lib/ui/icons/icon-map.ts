import type { Component } from 'svelte'
import IconArchive from '~icons/lucide/archive'
import IconArchiveRestore from '~icons/lucide/archive-restore'
import IconArrowLeft from '~icons/lucide/arrow-left'
import IconArrowRight from '~icons/lucide/arrow-right'
import IconCheck from '~icons/lucide/check'
import IconChevronDown from '~icons/lucide/chevron-down'
import IconChevronRight from '~icons/lucide/chevron-right'
import IconChevronUp from '~icons/lucide/chevron-up'
import IconChevronsUpDown from '~icons/lucide/chevrons-up-down'
import IconCircle from '~icons/lucide/circle'
import IconCircleAlert from '~icons/lucide/circle-alert'
import IconCircleCheck from '~icons/lucide/circle-check'
import IconCircleDashed from '~icons/lucide/circle-dashed'
import IconCircleDot from '~icons/lucide/circle-dot'
import IconCircleSlash from '~icons/lucide/circle-slash'
import IconCircleX from '~icons/lucide/circle-x'
import IconClock from '~icons/lucide/clock'
import IconCopy from '~icons/lucide/copy'
import IconCornerDownLeft from '~icons/lucide/corner-down-left'
import IconEye from '~icons/lucide/eye'
import IconEyeOff from '~icons/lucide/eye-off'
import IconFile from '~icons/lucide/file'
import IconFileArchive from '~icons/lucide/file-archive'
import IconFilePlus from '~icons/lucide/file-plus'
import IconFolder from '~icons/lucide/folder'
import IconFolderInput from '~icons/lucide/folder-input'
import IconFolderPlus from '~icons/lucide/folder-plus'
import IconGitBranch from '~icons/lucide/git-branch'
import IconGitCommitHorizontal from '~icons/lucide/git-commit-horizontal'
import IconGitFork from '~icons/lucide/git-fork'
import IconGlobe from '~icons/lucide/globe'
import IconHourglass from '~icons/lucide/hourglass'
import IconInfo from '~icons/lucide/info'
import IconKey from '~icons/lucide/key'
import IconLink from '~icons/lucide/link'
import IconList from '~icons/lucide/list'
import IconLock from '~icons/lucide/lock'
import IconMessagesSquare from '~icons/lucide/messages-square'
import IconMonitor from '~icons/lucide/monitor'
import IconMoon from '~icons/lucide/moon'
import IconMoreHorizontal from '~icons/lucide/more-horizontal'
import IconPaperclip from '~icons/lucide/paperclip'
import IconPause from '~icons/lucide/pause'
import IconPencil from '~icons/lucide/pencil'
import IconPlay from '~icons/lucide/play'
import IconRotateCcw from '~icons/lucide/rotate-ccw'
import IconRotateCw from '~icons/lucide/rotate-cw'
import IconSearch from '~icons/lucide/search'
import IconShieldCheck from '~icons/lucide/shield-check'
import IconShieldOff from '~icons/lucide/shield-off'
import IconSparkles from '~icons/lucide/sparkles'
import IconDownload from '~icons/lucide/download'
import IconSquare from '~icons/lucide/square'
import IconSun from '~icons/lucide/sun'
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
  archive: IconArchive,
  'archive-restore': IconArchiveRestore,
  'arrow-left': IconArrowLeft,
  'arrow-right': IconArrowRight,
  check: IconCheck,
  'chevron-down': IconChevronDown,
  'chevron-right': IconChevronRight,
  'chevron-up': IconChevronUp,
  'chevrons-up-down': IconChevronsUpDown,
  circle: IconCircle,
  'circle-alert': IconCircleAlert,
  'circle-check': IconCircleCheck,
  'circle-dashed': IconCircleDashed,
  'circle-dot': IconCircleDot,
  'circle-slash': IconCircleSlash,
  'circle-x': IconCircleX,
  clock: IconClock,
  copy: IconCopy,
  'corner-down-left': IconCornerDownLeft,
  eject: EjectIcon,
  eye: IconEye,
  'eye-off': IconEyeOff,
  file: IconFile,
  'file-archive': IconFileArchive,
  'file-plus': IconFilePlus,
  folder: IconFolder,
  'folder-input': IconFolderInput,
  'folder-plus': IconFolderPlus,
  'git-branch': IconGitBranch,
  'git-commit-horizontal': IconGitCommitHorizontal,
  'git-fork': IconGitFork,
  globe: IconGlobe,
  hourglass: IconHourglass,
  info: IconInfo,
  key: IconKey,
  link: IconLink,
  list: IconList,
  lock: IconLock,
  'messages-square': IconMessagesSquare,
  monitor: IconMonitor,
  moon: IconMoon,
  'more-horizontal': IconMoreHorizontal,
  paperclip: IconPaperclip,
  pause: IconPause,
  pencil: IconPencil,
  play: IconPlay,
  'rotate-ccw': IconRotateCcw,
  'rotate-cw': IconRotateCw,
  search: IconSearch,
  'shield-check': IconShieldCheck,
  'shield-off': IconShieldOff,
  sparkles: IconSparkles,
  download: IconDownload,
  square: IconSquare,
  sun: IconSun,
  tag: IconTag,
  'trash-2': IconTrash2,
  'triangle-alert': IconTriangleAlert,
  x: IconX,
} satisfies Record<string, Component>

export type IconName = keyof typeof ICON_COMPONENTS
