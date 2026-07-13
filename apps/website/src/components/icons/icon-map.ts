// The single place the website imports Lucide glyphs. Everything user-facing renders through
// `<Icon name=… />` (see Icon.astro), which looks the glyph up here. Add a new glyph by importing it
// from `~icons/lucide/<kebab-name>` (unplugin-icons, resolved by the Vite plugin in astro.config.mjs)
// and registering it below under its Lucide name. Browse names at https://lucide.dev/icons.
//
// Mirrors the desktop app's `lib/ui/icons/icon-map.ts` in spirit: one registry, gold line-art, no
// raw emoji or per-site SVG files. Lucide has no penguin, so the roadmap's Linux milestone keeps its
// 🐧 for now (the one deliberate emoji exception).

import AppWindow from '~icons/lucide/app-window'
import Bell from '~icons/lucide/bell'
import Brain from '~icons/lucide/brain'
import ChartPie from '~icons/lucide/chart-pie'
import Cloud from '~icons/lucide/cloud'
import Copy from '~icons/lucide/copy'
import Database from '~icons/lucide/database'
import Eye from '~icons/lucide/eye'
import FileArchive from '~icons/lucide/file-archive'
import FileText from '~icons/lucide/file-text'
import Folder from '~icons/lucide/folder'
import GitBranch from '~icons/lucide/git-branch'
import History from '~icons/lucide/history'
import Keyboard from '~icons/lucide/keyboard'
import ListChecks from '~icons/lucide/list-checks'
import MessageCircleQuestionMark from '~icons/lucide/message-circle-question-mark'
import MessagesSquare from '~icons/lucide/messages-square'
import Monitor from '~icons/lucide/monitor'
import Navigation from '~icons/lucide/navigation'
import PartyPopper from '~icons/lucide/party-popper'
import Pointer from '~icons/lucide/pointer'
import Puzzle from '~icons/lucide/puzzle'
import Rocket from '~icons/lucide/rocket'
import Search from '~icons/lucide/search'
import Server from '~icons/lucide/server'
import Ship from '~icons/lucide/ship'
import Smartphone from '~icons/lucide/smartphone'
import Sparkles from '~icons/lucide/sparkles'
import SquareChevronRight from '~icons/lucide/square-chevron-right'
import Zap from '~icons/lucide/zap'

export const ICONS = {
  'app-window': AppWindow,
  bell: Bell,
  brain: Brain,
  'chart-pie': ChartPie,
  cloud: Cloud,
  copy: Copy,
  database: Database,
  eye: Eye,
  'file-archive': FileArchive,
  'file-text': FileText,
  folder: Folder,
  'git-branch': GitBranch,
  history: History,
  keyboard: Keyboard,
  'list-checks': ListChecks,
  'message-circle-question-mark': MessageCircleQuestionMark,
  'messages-square': MessagesSquare,
  monitor: Monitor,
  navigation: Navigation,
  'party-popper': PartyPopper,
  pointer: Pointer,
  puzzle: Puzzle,
  rocket: Rocket,
  search: Search,
  server: Server,
  ship: Ship,
  smartphone: Smartphone,
  sparkles: Sparkles,
  'square-chevron-right': SquareChevronRight,
  zap: Zap,
} as const

export type IconName = keyof typeof ICONS
