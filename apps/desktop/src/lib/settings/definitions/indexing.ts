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
    // as its title (DriveIndexingSection.svelte). Same key as `indexing.indexSize`.
    cardKey: 'settings.indexing.enabled.label',
    keywords: ['index', 'drive', 'scan', 'size', 'directory', 'folder', 'background'],
    type: 'boolean',
    default: true,
    component: 'switch',
  },
  {
    // Hidden search anchor (not a control). The "Index size / Clear index" action row is
    // hand-rendered in DriveIndexingSection.svelte with no registry entry of its own,
    // so search couldn't reach it and its card couldn't know to show. This anchor gives it
    // a searchable identity: `buildSearchIndex` keeps hidden entries, `buildSectionTree`
    // skips them, so it never adds a nav row. Never read or written. Its `section` MUST
    // equal the hosting page's, or the blank-page fix breaks (the anchor must land in that
    // page's section-scoped match set). Reuses the existing `indexSize` label key (no new
    // string). Additive key, so no SCHEMA_VERSION bump (defaults rebuild from the registry).
    id: 'indexing.indexSize',
    section: ['Indexing', 'Drive indexing'],
    labelKey: 'settings.fileSystemWatching.indexSize',
    cardKey: 'settings.indexing.enabled.label',
    keywords: ['clear index', 'index database'],
    type: 'boolean',
    default: false,
    hidden: true,
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
  // `ImageIndexingSection.svelte`; only `mediaIndex.enabled` is a visible row,
  // the rest back the bespoke slider / network-volume components.
  // ========================================================================
  {
    // Master toggle for image-content (OCR) indexing. Off by default; live-applied to
    // the `media_index` backend scheduler via `set_image_index_enabled`. Its own card
    // in `ImageIndexingSection.svelte`, titled by `cardKey`.
    id: 'mediaIndex.enabled',
    section: ['Indexing', 'Image indexing'],
    labelKey: 'settings.mediaIndex.enabled.label',
    descriptionKey: 'settings.mediaIndex.enabled.description',
    cardKey: 'settings.mediaIndex.card',
    keywords: ['image', 'photo', 'ocr', 'text', 'search', 'index', 'picture', 'screenshot', 'content'],
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
]
