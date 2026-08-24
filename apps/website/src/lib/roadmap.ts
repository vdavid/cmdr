/**
 * The roadmap page's content: every shipped milestone and every planned one, in the order they
 * render at `/roadmap`. This is the ONE place to edit roadmap items; `src/pages/roadmap.astro` is
 * a template that maps over it and owns only the layout and styling.
 *
 * `title` and `description` are inline HTML (a handful carry `<em>`, a link, or a `title=` span),
 * so the page renders them with `set:html`. Keep them as plain text unless markup is really needed.
 *
 * Write straight quotes and apostrophes here; `smart-quotes.ts` curls them in the built HTML, and
 * it reaches `set:html` output too. Pre-curled characters would survive, but they'd drift from what
 * every other page writes.
 */
import type { IconName } from '../components/icons/icon-map'

export interface RoadmapMilestone {
  /** The parenthesized left-column label: a ship date ("(Dec 25)") or a guess ("(summer?)"). */
  date: string
  /** Inline HTML. */
  title: string
  /** Inline HTML. */
  description: string
  /** Optional gold glyph rendered right after the title. */
  icon?: IconName
  /** Shipped milestones get a checked box, planned ones an empty one. */
  done: boolean
}

/** One list of milestones, optionally introduced by a month heading. */
export interface RoadmapGroup {
  heading?: string
  milestones: RoadmapMilestone[]
}

export interface RoadmapSection {
  heading: string
  /** Anchor id, so other pages can deep-link (for example `/roadmap#very-soon`). */
  id?: string
  /** Renders a blank spacer paragraph above the heading. */
  spacerAbove?: boolean
  groups: RoadmapGroup[]
}

export const roadmapSections: RoadmapSection[] = [
  {
    heading: '2025',
    spacerAbove: true,
    groups: [
      {
        milestones: [
          {
            date: '(Dec 25)',
            title: 'Start project',
            description: 'Create Rust + Tauri + Svelte boilerplate.',
            icon: 'party-popper',
            done: true,
          },
          {
            date: '(Dec 27)',
            title: 'Build the core',
            description: 'Two-pane view, Full/Brief mode, virtual scrolling, icons.',
            done: true,
          },
          {
            date: '(Dec 29)',
            title: 'Make listing crazy fast',
            description: '350ms to first file for a 50k file folder!',
            done: true,
          },
          { date: '(Dec 30)', title: 'Add file watching', description: 'Live updates, always.', done: true },
          {
            date: '(Dec 31)',
            title: 'Show Dropbox overlays',
            description: 'See synced/syncing/offline-only/online-only statuses.',
            done: true,
          },
        ],
      },
    ],
  },
  {
    heading: '2026',
    groups: [
      {
        heading: 'Jan 2026',
        milestones: [
          {
            date: '(Jan 1)',
            title: 'Add volumes and drag &amp; drop',
            description: 'Volume switching, drag <em>from</em> app.',
            done: true,
          },
          {
            date: '(Jan 5)',
            title: 'Add network drive support!',
            description: 'SMB host discovery via Bonjour, share mounting, authentication.',
            done: true,
          },
          {
            date: '(Jan 7)',
            title: 'Add command palette',
            description: '⌘P to find your actions fast.',
            done: true,
          },
          {
            date: '(Jan 9)',
            title: 'Set up licensing',
            description: "Business planning, set up Paddle. It's free for individuals!",
            done: true,
          },
          {
            date: '(Jan 10)',
            title: 'Ship initial release',
            description: 'And created getcmdr.com!',
            icon: 'ship',
            done: true,
          },
          {
            date: '(Jan 13)',
            title: 'Add MCP server',
            description: 'AI agents can now use the app.',
            done: true,
          },
          {
            date: '(Jan 14)',
            title: 'Add automatic updates',
            description: 'Auto-updater, plus a custom title bar and website redesign.',
            done: true,
          },
          {
            date: '(Jan 16)',
            title: 'Add file selection',
            description: 'Space to toggle, Shift+arrows for range, Cmd+A for all.',
            done: true,
          },
          {
            date: '(Jan 17)',
            title: 'Make SMB access delightful',
            description: 'Show progress: "Opening...", "Loaded N files...", be transparent.',
            done: true,
          },
          {
            date: '(Jan 20)',
            title: 'Add copy! (F5)',
            description: 'Pre-flight check for precise stats, progress, even rollback!',
            done: true,
          },
          {
            date: '(Jan 20)',
            title: 'Add multifile drag &amp; drop',
            description: 'Drag several files out of the app.',
            done: true,
          },
          {
            date: '(Jan 21)',
            title: 'Add copy conflict handling',
            description: 'Skip, overwrite, or rename when files/folders collide.',
            done: true,
          },
          {
            date: '(Jan 22)',
            title: 'Add mkdir (F7)',
            description: 'Folder creation with conflict handling and file watching.',
            done: true,
          },
          {
            date: '(Jan 24)',
            title: 'Add file viewer (F3)',
            description: 'Works for 10+ GB files instantly, with fast search.',
            icon: 'file-text',
            done: true,
          },
          {
            date: '(Jan 24)',
            title: 'Make "copy" operation safe',
            description: 'Check for writability, disk space, inode identity, path limits.',
            done: true,
          },
          {
            date: '(Jan 25)',
            title: 'Add settings dialog',
            description: 'Ark UI components, fuzzy search, 9+ sections.',
            done: true,
          },
          {
            date: '(Jan 26)',
            title: 'Edit keyboard shortcuts',
            description: 'Click-to-edit, conflict detection, searchable.',
            done: true,
          },
          {
            date: '(Jan 27)',
            title: 'Add local AI',
            description: 'Privacy-first on-device LLM, no data leaves your Mac.',
            icon: 'brain',
            done: true,
          },
          {
            date: '(Jan 31)',
            title: 'Wire up Settings',
            description: 'Every setting applies instantly.',
            done: true,
          },
        ],
      },
      {
        heading: 'Feb 2026',
        milestones: [
          {
            date: '(Feb 4)',
            title: 'Add MTP support',
            description: 'Copy files to/from Android phones and cameras very fast!',
            icon: 'smartphone',
            done: true,
          },
          {
            date: '(Feb 10)',
            title: 'Make app a drag &amp; drop target',
            description: 'Drag between panes and from Finder and other apps.',
            done: true,
          },
          {
            date: '(Feb 13)',
            title: 'Add renaming',
            description: 'Inline, with conflict handling and extension-change warnings.',
            done: true,
          },
          {
            date: '(Feb 21)',
            title: 'Calculate folder sizes',
            description: 'See how big folders are, sort by size.',
            done: true,
          },
          {
            date: '(Feb 25)',
            title: 'Add tabs',
            description: 'For easy switching between folders.',
            done: true,
          },
          {
            date: '(Feb 27)',
            title: 'Add move and delete',
            description: 'Implement remaining basic file operations to make this actually useful 😅',
            done: true,
          },
        ],
      },
      {
        heading: 'Mar 2026',
        milestones: [
          {
            date: '(Mar 1)',
            title: 'Add Linux support (alpha) 🐧',
            description: 'Volumes, file ops, trash, inotify, SMB, native icons.',
            done: true,
          },
          {
            date: '(Mar 7)',
            title: 'Add ⌘C ⌘V',
            description: 'Copy and paste files, works with Finder too.',
            done: true,
          },
          {
            date: '(Mar 9)',
            title: 'Add context menu actions',
            description: 'View, Copy, Move, New folder, and Delete from right-click.',
            done: true,
          },
          {
            date: '(Mar 11)',
            title: 'Add cloud AI support',
            description: '15 providers incl. OpenAI, Anthropic, xAI, OpenRouter, and Ollama.',
            icon: 'cloud',
            done: true,
          },
          {
            date: '(Mar 16)',
            title: 'Find files',
            description: 'Find files instantly on all your drives, with AI-powered natural language search.',
            done: true,
          },
          {
            date: '(Mar 23)',
            title: 'Add crash reporting',
            description: 'Opt-in crash reports to help improve stability, no PII.',
            done: true,
          },
        ],
      },
      {
        heading: 'Apr 2026',
        milestones: [
          {
            date: '(Apr 10)',
            title: 'Direct SMB connections',
            description: '~4x faster file ops via native smb2 protocol, manual connections.',
            icon: 'sparkles',
            done: true,
          },
          {
            date: '(Apr 28)',
            title: 'Browse git!',
            description: 'Browse git history, branches, tags, stash, worktrees like folders, copy out files.',
            done: true,
          },
        ],
      },
      {
        heading: 'May 2026',
        milestones: [
          {
            date: '(May 6)',
            title: 'Dynamic text size',
            description: 'Zoom that considers macOS Accessibility. Text, icons, viewer all scale.',
            done: true,
          },
          {
            date: '(May 16)',
            title: 'Settings 1.0',
            description: "Settings had no real structure before. Now it's actually nice to use.",
            done: true,
          },
          {
            date: '(May 19)',
            title: 'Cross-volume operations',
            description: 'Like direct MTP → SMB. Optimized ops, precise progress, cancellation.',
            done: true,
          },
          {
            date: '(May 21)',
            title: 'Polished UI',
            description: 'Main window is flatter and simpler. Settings window got liquid glass design.',
            done: true,
          },
          {
            date: '(May 22)',
            title: 'Search redesign',
            description: 'Overall improved UX, recent searches, "Open in pane".',
            done: true,
          },
          {
            date: '(May 23)',
            title: 'AI-powered smart selection',
            description: 'Support queries like "Select all error logs from last week".',
            icon: 'sparkles',
            done: true,
          },
          {
            date: '(May 24)',
            title: 'Onboarding revamp',
            description: 'Guided first-launch wizard: Full Disk Access, AI provider, and optionals.',
            done: true,
          },
          {
            date: '(May 29)',
            title: 'Downloads watcher',
            description: 'Notification when a download lands; jump to the latest with ⌘J / ⌃⌥⌘J.',
            done: true,
          },
        ],
      },
      {
        heading: 'Jun 2026',
        milestones: [
          {
            date: '(Jun 3)',
            title: 'Go to path',
            description: '⌘G to jump anywhere: paste a path, ~ expansion, recent paths.',
            done: true,
          },
          {
            date: '(Jun 5)',
            title: 'Smarter copy and move',
            description: 'Folders merge, non-conflicting same-volume moves are instant.',
            done: true,
          },
          {
            date: '(Jun 6)',
            title: 'Smarter SMB sign-in',
            description: 'Reuse saved passwords from Finder, re-auth on pw change.',
            done: true,
          },
          {
            date: '(Jun 7)',
            title: 'Low disk space warning',
            description: 'Notifies when a drive runs low.',
            done: true,
          },
          {
            date: '(Jun 10)',
            title: 'Open beta',
            description: 'Feature status page, in-app Beta badges, Send feedback, anonymous usage stats.',
            icon: 'rocket',
            done: true,
          },
          {
            date: '(Jun 14)',
            title: 'Editable favorites',
            description: 'Add/rename/reorder/remove Favorite folders.',
            done: true,
          },
          {
            date: '(Jun 17)',
            title: 'File viewer media support',
            description: 'View images and PDFs in the file viewer.',
            done: true,
          },
          {
            date: '(Jun 18)',
            title: 'Multi-language (i18n)',
            description: 'Support multiple languages (but no translations yet).',
            done: true,
          },
          {
            date: '(Jun 19)',
            title: 'Index network drives and phones',
            description: 'Full drive indexing for SMB shares and MTP devices.',
            done: true,
          },
          {
            date: '(Jun 21)',
            title: '<span title="Localization">L10n</span>',
            description:
              'Chinese, Dutch, French, German, Hungarian, Portuguese, Spanish, Swedish,                                 Vietnamese!',
            done: true,
          },
          {
            date: '(Jun 22)',
            title: 'Transfer queue and pause',
            description: 'Enqueue and pause/resume transfers/deletes.',
            done: true,
          },
          {
            date: '(Jun 30)',
            title: 'macOS Finder tags',
            description: "See and set Finder's colored tags.",
            done: true,
          },
        ],
      },
      {
        heading: 'Jul 2026',
        milestones: [
          {
            date: '(Jul 6)',
            title: 'Handle archives',
            description: 'Browse, create, extract, and edit zips like folders.',
            icon: 'file-archive',
            done: true,
          },
        ],
      },
      {
        milestones: [
          {
            date: '(Jul 8)',
            title: 'Folder importance',
            description: 'Search (and other features) now have a clue which folders are important.',
            done: true,
          },
          {
            date: '(Jul 10)',
            title: 'Operation log and rollback',
            description: 'A durable history of every file change, with rollback.',
            done: true,
          },
          {
            date: '(Jul 10)',
            title: 'Better MCP',
            description: 'Agents can rename, tag, favorite, eject, and queue/dequeue like you.',
            done: true,
          },
          {
            date: '(Jul 13)',
            title: 'Ask Cmdr',
            description: 'Chat with the built-in agent about your files, right in the app.',
            icon: 'brain',
            done: true,
          },
          {
            date: '(Jul 15)',
            title: 'Index external drives',
            description: 'Drive indexing now works for USB sticks, SD cards, and external disks.',
            done: true,
          },
          {
            date: '(Jul 16)',
            title: 'Search your photos',
            description: 'Natural language ("Duck on tree"), OCR ("Pizza receipt"). 100% on-device!',
            done: true,
          },
          {
            date: '(Jul 20)',
            title: 'Natural language bulk rename',
            description: 'After human review, undoable.',
            icon: 'sparkles',
            done: true,
          },
          {
            date: '(Jul 21)',
            title: 'Browse SMB and MTP while indexing',
            description: 'Scans and write ops now pause while navigating.',
            done: true,
          },
          {
            date: '(Jul 22)',
            title: 'Huge indexing revamp',
            description: 'Up to 4x faster and more transparent file+photo indexing',
            done: true,
          },
          {
            date: '(Jul 23)',
            title: 'Big design polish',
            description: 'Dialogs, main window, shared components all much nicer now',
            icon: 'sparkles',
            done: true,
          },
          {
            date: '(Jul 28)',
            title: 'Make search 25x faster',
            description: 'Even a broad query now answers in under 0.5 sec.',
            icon: 'zap',
            done: true,
          },
          {
            date: '(Jul 29)',
            title: 'Better agent',
            description: 'Reasonable context window management and UI, complex actions.',
            icon: 'sparkles',
            done: true,
          },
        ],
      },
      {
        heading: 'Aug 2026',
        milestones: [
          {
            date: '(Aug 1)',
            title: 'Survive a silent NAS',
            description: 'A wedged network transfer now recovers in seconds instead of hanging forever.',
            done: true,
          },
          {
            date: '(Aug 2)',
            title: 'Speed up network copies',
            description: 'Far fewer round trips per file, up to 3.8x faster to a NAS.',
            done: true,
          },
          {
            date: '(Aug 6)',
            title: 'Search any folder',
            description: 'Unindexed folders got a live walk.',
            done: true,
          },
          {
            date: '(Aug 13)',
            title: 'Backgrounded operations',
            description: 'Show/hide running ops at will.',
            done: true,
          },
          {
            date: '(Aug 18)',
            title: 'AI-powered file organization',
            description: 'Clean up Downloads, with 100% human oversight.',
            done: true,
          },
          {
            date: '(Aug 23)',
            title: 'Built-in proactive agent',
            description: 'Suggests actions based on file system changes.',
            done: true,
          },
        ],
      },
    ],
  },
  {
    heading: 'Very soon',
    id: 'very-soon',
    groups: [
      {
        milestones: [
          {
            date: '(summer?)',
            title: 'Support more file systems',
            description: 'S3 buckets, FTP(S), SFTP, SCP, WebDAV, NFS, etc.',
            done: false,
          },
          {
            date: '(summer?)',
            title: 'AI-powered "Tell me about this"',
            description: 'Right-click any file for a quick AI explanation.',
            done: false,
          },
        ],
      },
    ],
  },
  {
    heading: 'Also soon',
    id: 'also-soon',
    groups: [
      {
        milestones: [
          {
            date: '(fall?)',
            title: 'Add plugins',
            description: 'Let you extend Cmdr with your scripts and tools.',
            done: false,
          },
          { date: '(fall?)', title: 'Folder sync', description: 'Compare/sync two folders.', done: false },
          {
            date: '(fall?)',
            title: 'Add disk space visualizer',
            description:
              '<a href="https://grandperspectiv.sourceforge.net/HelpDocumentation/QuickStart.html" target="_blank">GrandPerspective</a>-style treemap built-in',
            done: false,
          },
          {
            date: '(winter?)',
            title: 'Add Windows and true Linux support',
            description: 'Bring Cmdr to more platforms.',
            done: false,
          },
        ],
      },
    ],
  },
]
