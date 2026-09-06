/**
 * Indexing section settings (data only). Logic lives in `../settings-registry.ts`,
 * which concatenates this array into the full registry in section order.
 *
 * Two subsections:
 *   - `Drive indexing`: the background file-system indexer (rendered by
 *     `DriveIndexingSection.svelte`).
 *   - `Image indexing`: on-device image-content (OCR) search, the `mediaIndex.*`
 *     family (rendered by `ImageIndexingSection.svelte`).
 */

import type { SettingDefinitionSource } from '../types'

export const indexingSettings: SettingDefinitionSource[] = [
  // ========================================================================
  // Indexing › Drive indexing
  //
  // The background file-system indexer: it scans drives so search can find
  // files fast. Rendered by `DriveIndexingSection.svelte`.
  // ========================================================================
  {
    id: 'indexing.enabled',
    section: ['Indexing', 'Drive indexing'],
    labelKey: 'settings.indexing.enabled.label',
    descriptionKey: 'settings.indexing.enabled.description',
    // The Drive-indexing card has no dedicated `card*` key; it reuses this row-label key
    // as its title (DriveIndexingSection.svelte), and so do the card's searchable rows
    // (`DriveIndexingSection.rows.ts`).
    cardKey: 'settings.indexing.enabled.label',
    keywords: ['index', 'drive', 'scan', 'size', 'directory', 'folder', 'background'],
    type: 'boolean',
    default: true,
    component: 'switch',
  },
  {
    // Gates the per-drive first-connect "turn on indexing?" notification (D6).
    id: 'indexing.askForEachDrive',
    section: ['Indexing', 'Drive indexing'],
    labelKey: 'settings.indexing.askForEachDrive.label',
    descriptionKey: 'settings.indexing.askForEachDrive.description',
    cardKey: 'settings.indexing.enabled.label',
    keywords: ['drive', 'index', 'ask', 'prompt', 'notification', 'connect', 'network', 'smb', 'usb'],
    type: 'boolean',
    default: true,
    component: 'switch',
  },
  {
    // Gates the one-time "your drive went stale" dialog (D2). The yellow badge
    // shows regardless of this toggle.
    id: 'indexing.staleNotify',
    section: ['Indexing', 'Drive indexing'],
    labelKey: 'settings.indexing.staleNotify.label',
    descriptionKey: 'settings.indexing.staleNotify.description',
    cardKey: 'settings.indexing.enabled.label',
    keywords: ['drive', 'index', 'stale', 'outdated', 'notify', 'notification', 'disconnect', 'network', 'smb'],
    type: 'boolean',
    default: true,
    component: 'switch',
  },
  {
    // Internal (FE-owned): JSON array of volume ids the user silenced via
    // "Don't ask again for this drive". Never a UI row; the "Re-enable
    // notifications for all drives" button resets it to "[]".
    id: 'indexing.silencedDrives',
    section: ['Indexing', 'Drive indexing'],
    labelKey: 'settings.indexing.silencedDrives.label',
    descriptionKey: 'settings.indexing.silencedDrives.description',
    keywords: [],
    type: 'string',
    default: '[]',
    component: 'text-input',
    hidden: true,
  },
  {
    // Internal (FE-owned): whether the one-time stale dialog has fired once.
    id: 'indexing.firstStaleDialogShown',
    section: ['Indexing', 'Drive indexing'],
    labelKey: 'settings.indexing.firstStaleDialogShown.label',
    descriptionKey: 'settings.indexing.firstStaleDialogShown.description',
    keywords: [],
    type: 'boolean',
    default: false,
    component: 'switch',
    hidden: true,
  },

  // ========================================================================
  // Indexing › Image indexing
  //
  // On-device image-content (OCR) search. Runs entirely on the user's Mac via
  // Apple's Vision framework — no cloud, no AI provider, no API key. Rendered by
  // `ImageIndexingSection.svelte`: `mediaIndex.enabled`, the two display toggles
  // (`showFileStatusIcons`, `showInSearch`), and `parallelism` are visible rows in
  // its "Enable indexing" card; the rest are `hidden`, backing the bespoke scope /
  // slider / chosen-folder / network-volume / CLIP components.
  // ========================================================================
  {
    // Master toggle for image-content (OCR) indexing. Off by default; live-applied to
    // the `media_index` backend scheduler via `set_image_index_enabled`. Lives in the
    // "Enable indexing" card of `ImageIndexingSection.svelte`, so `cardKey` is the key
    // that card's title renders (searching the visible title has to reach the row).
    id: 'mediaIndex.enabled',
    section: ['Indexing', 'Image indexing'],
    labelKey: 'settings.mediaIndex.enabled.label',
    descriptionKey: 'settings.mediaIndex.enabled.description',
    cardKey: 'settings.mediaIndex.cards.enable',
    keywords: ['image', 'photo', 'ocr', 'text', 'search', 'index', 'picture', 'screenshot', 'content'],
    type: 'boolean',
    default: false,
    component: 'switch',
  },
  {
    // Whether the file list shows a small per-file image-index status badge (indexed /
    // pending / stale / excluded / couldn't-index). FE-only render toggle: when off, the
    // overlay is neither fetched nor drawn. Rendered under the master toggle in
    // `ImageIndexingSection.svelte`, gated on `mediaIndex.enabled`.
    id: 'mediaIndex.showFileStatusIcons',
    section: ['Indexing', 'Image indexing'],
    labelKey: 'settings.mediaIndex.showFileStatusIcons.label',
    descriptionKey: 'settings.mediaIndex.showFileStatusIcons.description',
    cardKey: 'settings.mediaIndex.cards.enable',
    keywords: ['image', 'photo', 'badge', 'icon', 'overlay', 'indicator', 'status', 'indexed'],
    type: 'boolean',
    default: true,
    component: 'switch',
  },
  {
    // Whether the Search dialog shows its grid of matching images above the file results.
    // FE-only render toggle, read by `search/ImageSearchResults.svelte`: off means the grid
    // renders nothing AND fires no `media.db` IPC per keystroke, so there's no Tauri command
    // and no `settings-applier.ts` case. Default OFF because match quality isn't good enough
    // yet to take that space unasked; indexing, the file-list badges, and Ask Cmdr / MCP
    // photo search are unaffected either way. Rendered under the status-badges row in
    // `ImageIndexingSection.svelte`, gated on `mediaIndex.enabled`. A new key is additive,
    // so SCHEMA_VERSION doesn't move.
    id: 'mediaIndex.showInSearch',
    section: ['Indexing', 'Image indexing'],
    labelKey: 'settings.mediaIndex.showInSearch.label',
    descriptionKey: 'settings.mediaIndex.showInSearch.description',
    cardKey: 'settings.mediaIndex.cards.enable',
    keywords: ['image', 'photo', 'search', 'results', 'grid', 'ocr', 'semantic', 'show', 'hide'],
    type: 'boolean',
    default: false,
    component: 'switch',
  },
  {
    // Internal (FE-owned): JSON array of volume ids opted into background network (SMB)
    // image enrichment (network enrichment). Off by default per volume; the per-network-volume rows in
    // `ImageIndexingSection`'s "Image indexing" card toggle it, persisting here AND
    // calling `media_index_set_network_volume_enabled`. Read by the Rust loader as an array.
    id: 'mediaIndex.networkVolumes',
    section: ['Indexing', 'Image indexing'],
    labelKey: 'settings.mediaIndex.networkVolumes.label',
    descriptionKey: 'settings.mediaIndex.networkVolumes.description',
    keywords: [],
    type: 'string-array',
    default: [],
    hidden: true,
  },
  {
    // Internal (FE-owned): JSON array of volume ids marked "always index" (enrich
    // regardless of importance). Toggled by the per-network-volume rows; persisted here
    // AND pushed via `media_index_set_always_index_volume`.
    id: 'mediaIndex.alwaysIndexVolumes',
    section: ['Indexing', 'Image indexing'],
    labelKey: 'settings.mediaIndex.alwaysIndexVolumes.label',
    descriptionKey: 'settings.mediaIndex.alwaysIndexVolumes.description',
    keywords: [],
    type: 'string-array',
    default: [],
    hidden: true,
  },
  {
    // WHICH folders image indexing may cover. `chosen` (the default) indexes only the
    // folders and drives the user named; `importance` adds every folder scoring at or
    // above the threshold slider, which is shown only in that mode. Rendered as a radio
    // group by the bespoke `MediaIndexScope.svelte` inside the "Image indexing" card (not
    // an auto row), so `hidden`. An install that already had image indexing on migrates
    // to `importance` (settings-store `migrateSettings`, schema 3) so its behavior
    // doesn't change under it; the Rust `gate::scope_from_settings` applies the same rule
    // on the launch before that migration writes the key.
    // Live-applied via the `settings-applier.ts` passthrough → `media_index_set_scope`.
    id: 'mediaIndex.scope',
    section: ['Indexing', 'Image indexing'],
    labelKey: 'settings.mediaIndex.scope.label',
    descriptionKey: 'settings.mediaIndex.scope.description',
    keywords: ['image', 'photo', 'index', 'folders', 'scope', 'coverage', 'which', 'choose'],
    type: 'enum',
    default: 'chosen',
    component: 'radio',
    hidden: true,
    constraints: {
      options: [
        { value: 'chosen', labelKey: 'settings.mediaIndex.scope.opt.chosen' },
        { value: 'importance', labelKey: 'settings.mediaIndex.scope.opt.importance' },
      ],
    },
  },
  {
    // Internal (FE-owned): JSON array of absolute OS-mount folder paths marked "always
    // index" — the chosen folders. In the `chosen` scope these ARE the coverage. Managed
    // by `MediaIndexChosenFolders.svelte`; persisted here AND pushed via
    // `media_index_set_always_index_folder` (which kicks a pass when a folder is added).
    id: 'mediaIndex.alwaysIndexFolders',
    section: ['Indexing', 'Image indexing'],
    labelKey: 'settings.mediaIndex.alwaysIndexFolders.label',
    descriptionKey: 'settings.mediaIndex.alwaysIndexFolders.description',
    keywords: [],
    type: 'string-array',
    default: [],
    hidden: true,
  },
  {
    // Internal (FE-owned): JSON array of absolute OS folder paths EXCLUDED from image
    // indexing (the privacy veto). Set by the folder context-menu "Don't index images
    // in this folder" item; persisted here AND pushed via `media_index_set_excluded_folder`
    // (which also retro-deletes the folder's existing rows). Read by the Rust loader as
    // an array.
    id: 'mediaIndex.excludedFolders',
    section: ['Indexing', 'Image indexing'],
    labelKey: 'settings.mediaIndex.excludedFolders.label',
    descriptionKey: 'settings.mediaIndex.excludedFolders.description',
    keywords: [],
    type: 'string-array',
    default: [],
    hidden: true,
  },
  {
    // The image-index importance threshold (`0.0..=1.0`): the lowest folder-importance
    // level the scheduler enriches. Rendered as named buckets by the bespoke
    // `MediaIndexImportanceSlider.svelte` inside the "Image indexing" card (not an auto
    // row), so `hidden`. Default `0.0` matches the backend `DEFAULT_IMPORTANCE_THRESHOLD`
    // (enrich every scored folder — non-regressive vs the OCR slice, junk is floored out anyway), so
    // the UI and a sparse (unpersisted) store agree without eagerly writing a default.
    // Live-applied via the `settings-applier.ts` passthrough → `media_index_set_importance_threshold`.
    id: 'mediaIndex.importanceThreshold',
    section: ['Indexing', 'Image indexing'],
    labelKey: 'settings.mediaIndex.importanceThreshold.label',
    descriptionKey: 'settings.mediaIndex.importanceThreshold.description',
    keywords: ['image', 'photo', 'index', 'importance', 'folders', 'coverage', 'depth'],
    type: 'number',
    default: 0,
    hidden: true,
  },
  {
    // How many parallel workers image indexing runs. Default 1 = today's single worker
    // (a parallelism spike measured a ~1.25x ceiling on current Apple Silicon: the ANE serializes
    // inference, so more workers help modestly and only up to ~2). Rendered by `SettingSlider`
    // inside the "Enable indexing" card with a RUNTIME max = this machine's CPU count
    // (`media_index_max_parallelism`); the `constraints.max` here is only a static fallback
    // for search. `hidden` because it's hand-rendered, not an auto row; `hidden` doesn't mean
    // unsearchable, so it still carries the card title the user reads above it. Live-applied via the
    // `settings-applier.ts` passthrough → `media_index_set_parallelism` (the backend clamps to
    // `1..=CPU-count` and a running pass resizes its pool between images).
    id: 'mediaIndex.parallelism',
    section: ['Indexing', 'Image indexing'],
    labelKey: 'settings.mediaIndex.parallelism.label',
    descriptionKey: 'settings.mediaIndex.parallelism.description',
    cardKey: 'settings.mediaIndex.cards.enable',
    keywords: ['image', 'photo', 'index', 'parallel', 'workers', 'speed', 'performance', 'cpu', 'cores'],
    type: 'number',
    default: 1,
    constraints: { min: 1, max: 16, step: 1 },
    hidden: true,
  },
  {
    // Whether CLIP semantic search ("search photos by description") is on. Default `true`:
    // once the on-device model is installed, covered images become findable by description.
    // Rendered as a `SettingSwitch` inside the "Semantic search" card by `MediaIndexClipModel`
    // (not an auto row), so `hidden`. Live-applied via the `settings-applier.ts` passthrough →
    // `media_index_set_semantic_search_enabled`, which gates both the read (search returns `[]`
    // when off) and the CLIP embedding writes (no new CLIP work when off). The backend seeds it
    // at startup, so a sparse (unpersisted) store and the UI agree on the `true` default.
    id: 'mediaIndex.semanticSearch.enabled',
    section: ['Indexing', 'Image indexing'],
    labelKey: 'settings.mediaIndex.semanticSearch.label',
    descriptionKey: 'settings.mediaIndex.clip.description',
    cardKey: 'settings.mediaIndex.clip.title',
    keywords: ['image', 'photo', 'semantic', 'clip', 'search', 'describe', 'description', 'natural language', 'ai'],
    type: 'boolean',
    default: true,
    component: 'switch',
    hidden: true,
  },
]
