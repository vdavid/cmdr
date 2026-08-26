# `cmdr/no-confusable-callback-params` violation inventory

The rule (`eslint-plugins/no-confusable-callback-params.js`, tests alongside it) is written and passes its unit tests,
but is deliberately **not wired into any `eslint.config.js` yet** so every commit on the branch that added it stays
green. This doc is the fix-up work plan for whoever wires it to `error` next: 105 flagged locations (103 in
`apps/desktop`, 2 in `apps/api-server`; `apps/website` and `apps/analytics-dashboard` have zero) — 53 in production
code, 52 in test code — split into 27 groups that share one underlying callback/production symbol (fix together) plus a
tail of independent single-occurrence violations (fix independently). Grouping was verified by reading each call site,
not just pattern-matching parameter names.

Fix shape for every group: replace the positional parameters with a single object payload
(`(args: { volumeId: string; volumePath: string; targetPath: string }) => void`), then update every call site.

## Groups: one callback, multiple files, fix together

### 1. `onVolumeChange` (volumeId, volumePath, targetPath)

The flagship "⌘A-shaped" incident. A prop threaded from the pane down through three volume-connection views, plus its
non-Svelte carriers and a renamed variant.

- production: `src/lib/file-explorer/navigation/VolumeBreadcrumb.svelte:93` (`onVolumeChange?`)
- production: `src/lib/file-explorer/pane/FilePane.svelte:102` (`onVolumeChange?`)
- production: `src/lib/file-explorer/pane/MtpConnectionView.svelte:14` (`onVolumeChange?`)
- production: `src/lib/file-explorer/pane/NetworkMountView.svelte:36` (`onVolumeChange?`)
- production: `src/lib/file-explorer/pane/breadcrumb-bar.ts:74` (`onVolumeChange`, `BreadcrumbBarDeps`)
- production: `src/lib/file-explorer/pane/breadcrumb-bar.ts:87` (`handleVolumeChange`, params renamed
  `newVolumeId`/`newVolumePath`/`targetPath` but same shape and the same conceptual event)
- production: `src/lib/file-explorer/pane/listing-loader.ts:129` (`onVolumeChange?`)
- production: `src/lib/file-explorer/pane/navigate.ts:240` (`determineNavigationPath`, the background resolver behind
  the callback; 4 params, the 4th is an object, so redesigning this one needs its own object-payload shape rather than
  reusing the 3-field one verbatim)
- test: `src/lib/file-explorer/pane/navigation-transaction-handlers.test.ts:269` (casts `onVolumeChange` to
  `(v, vp, tp) => void` to invoke it directly)

### 2. `onTransferConfirm` / `onConfirm` / `ConfirmFn` (destination, volumeId, previewId, conflictResolution, operationType, preKnownConflicts)

`DialogManager.svelte` renders `TransferDialog.svelte` with `onConfirm={onTransferConfirm}` (verified at
`DialogManager.svelte:180`); one prop, one call site apart.

- production: `src/lib/file-explorer/pane/DialogManager.svelte:82` (`onTransferConfirm`)
- production: `src/lib/file-operations/transfer/TransferDialog.svelte:67` (`onConfirm`)
- test: `src/lib/file-operations/transfer/TransferDialog.test.ts:159` (`ConfirmFn`; typed `operationType: string` there
  instead of the production `TransferOperationType` union, so the test also flags on `operationType`)

### 3. Transfer/adopted completion `onComplete` (filesProcessed, filesSkipped, bytesProcessed)

`DialogManager.svelte` renders `TransferProgressDialog.svelte` twice: `onComplete={onAdoptedComplete}` (line 204) and
`onComplete={onTransferComplete}` (line 226) (verified). Two outer prop names, one shared inner shape.

- production: `src/lib/file-explorer/pane/DialogManager.svelte:91` (`onTransferComplete`)
- production: `src/lib/file-explorer/pane/DialogManager.svelte:98` (`onAdoptedComplete`)
- production: `src/lib/file-operations/transfer/TransferProgressDialog.svelte:70` (`onComplete`)
- production: `src/lib/file-operations/transfer/transfer-progress-state.svelte.ts:86` (`onComplete`)

### 4. `onSelect` / `handleSelect` (index, shiftKey?, metaKey?)

`SearchResultsView.svelte`'s doc comment says outright: "Mirrors `FullList`'s signature so the host pane can route to
selection state." Deliberately kept in lockstep; a signature change to one is a signature change to all.

- production: `src/lib/file-explorer/views/BriefList.svelte:100`
- production: `src/lib/file-explorer/views/FullList.svelte:106`
- production: `src/lib/file-explorer/pane/SearchResultsView.svelte:52`
- production: `src/lib/file-explorer/pane/pane-pointer.ts:44` (`handleSelect`, the `PanePointer` interface method these
  views' click handlers ultimately call)

### 5. Visible-range / window shape (start, end)

The virtualized-window range, threaded from the cache through the two list views and the search results view, up to the
pane-level selection-range action.

- production: `src/lib/file-explorer/views/BriefList.svelte:107` (`onVisibleRangeChange?`)
- production: `src/lib/file-explorer/views/FullList.svelte:113` (`onVisibleRangeChange?`)
- production: `src/lib/file-explorer/pane/SearchResultsView.svelte:54` (`onVisibleRangeChange?`)
- production: `src/lib/file-explorer/views/full-list-cache.svelte.ts:87` (`windowRows`, params `startIndex`/`endIndex`)
- production: `src/lib/file-explorer/views/full-list-cache.svelte.ts:89` (`fetch`, params `startItem`/`endItem` — same
  shape, different names; worth unifying naming while fixing the shape)
- production: `src/routes/(main)/explorer-api.ts:118` (`handleSelectionAction`, params `startIndex?`/`endIndex?`
  alongside a non-confusable `action` — same range concept one level up)

### 6. `extendSelection` family (fromIndex, toIndex, overflow[, hasParent]) + its mouse twin

Two parallel keyboard-nav interfaces (Brief/Full pane vs. Search/snapshot pane) intentionally mirror each other's shape,
plus the mouse-driven counterpart on the same pane.

- production: `src/lib/file-explorer/pane/cursor-nav-keys.ts:59` (`extendSelection`, 4 params: `fromIndex`, `toIndex`
  share `number`; `overflow`, `hasParent` share `boolean`, two separate confusable pairs in one signature)
- production: `src/lib/file-explorer/pane/cursor-nav-keys.ts:73` (`applyNavigation`; `shiftKey?`/`overflow?` share
  `boolean`)
- production: `src/lib/file-explorer/pane/search-pane-keys.ts:28` (`extendSelection`, the snapshot-pane twin, 3 params)
- production: `src/lib/file-explorer/pane/pane-pointer.ts:33` (`extendSelectionFromMouse`; `index`/`cursorIndex` share
  `number`, the mouse-driven counterpart of the same gesture)

### 7. `copyPathBetweenPanes` (source, target: `'left' | 'right'`)

`pane-mirror.ts`'s own doc comment names `explorer-api.ts` as the delegate; the two are the same method's public surface
and its interface source.

- production: `src/lib/file-explorer/pane/pane-mirror.ts:34` (`PaneMirror` interface)
- production: `src/routes/(main)/explorer-api.ts:31` (re-declared on `ExplorerApi`)

### 8. `openCopyDialog` / `openMoveDialog` / `openCompressDialog` / `openDeleteDialog` (autoConfirm?/permanent, onConflict?, mcpRequestId?, initiator?)

Four MCP-facing dialog openers on the same `ExplorerApi` interface, all copy-pasted to the same shape (one has an extra
leading `permanent: boolean`, which pairs with `autoConfirm?: boolean` for a second confusable pair there). One file,
one interface: fix all four together or the surviving three stay a trap for the next one.

- production: `src/routes/(main)/explorer-api.ts:139` (`openCopyDialog`)
- production: `src/routes/(main)/explorer-api.ts:148` (`openMoveDialog`)
- production: `src/routes/(main)/explorer-api.ts:154` (`openCompressDialog`)
- production: `src/routes/(main)/explorer-api.ts:167` (`openDeleteDialog`; also flags `permanent`/`autoConfirm` as a
  `boolean` pair)

### 9. `onCancelLoading` / `handleCancelLoading` (cancelledPath, selectName?)

- production: `src/lib/file-explorer/pane/FilePane.svelte:117` (`onCancelLoading?`)
- production: `src/lib/file-explorer/pane/listing-loader.ts:131` (`onCancelLoading?`)
- production: `src/lib/file-explorer/pane/edge-flow-handlers.ts:45` (`handleCancelLoading`; leading non-confusable
  `pane: 'left' | 'right'`, then the same pair)
- test: `src/lib/file-explorer/pane/navigation-transaction-handlers.test.ts:422`, `:437`, `:456` (three casts of
  `onCancelLoading` to `(p, s?) => void` to invoke it directly)

### 10. `loadDirectory` / `navigateToPath` (path, selectName?)

- production: `src/lib/file-explorer/pane/listing-loader.ts:148` (`loadDirectory`, on `ListingLoader`)
- production: `src/lib/file-explorer/pane/listing-loader.ts:150` (`navigateToPath`, same file, same interface)
- production: `src/lib/file-explorer/pane/entry-activation.ts:39` (`loadDirectory`, the dependency-injected signature
  this module calls through)

### 11. `saveAiApiKey` (providerId, apiKey)

Production function `src/lib/tauri-commands/settings.ts:470` (`saveAiApiKey(providerId, apiKey)`), a plain function
declaration so it isn't itself flagged; every test that mocks it re-declares the same confusable shape.

- test: `src/lib/settings/ai-config.test.ts:24`
- test: `src/lib/onboarding/CloudProviderSetup.test.ts:29`
- test: `src/lib/onboarding/StepAi.test.ts:50`

### 12. `checkAiConnection` (baseUrl, providerId)

Production function `src/lib/tauri-commands/settings.ts:461`. Same story as #11: not flagged at the source (plain
function declaration), flagged at both mocks.

- test: `src/lib/onboarding/CloudProviderSetup.test.ts:12` (param named `key` locally)
- test: `src/lib/onboarding/StepAi.test.ts:40` (param named `apiKey` locally)

### 13. `openSettingsWindow` (surface, section?, anchor?)

Production function `src/lib/settings/settings-window.ts:103`, shared by two otherwise-unrelated notification features
that both deep-link into Settings.

- test: `src/lib/downloads/notifications-mode.test.ts:17`
- test: `src/lib/low-disk-space/notifications-mode.test.ts:18`

### 14. Image-search commands: `searchOcr` / `searchSemantic` / `findSimilar` (volumeId, query|sourcePath, limit)

Three real `$lib/tauri-commands` functions, each independently re-mocked in two test files covering different components
of the same search feature (confirmed via each file's own doc comment: one is the gating/toggle test, the other the a11y
pass over the same surfaces).

- test: `src/lib/search/ImageSearchResults.gating.test.ts:19` (`searchOcr`)
- test: `src/lib/search/ImageSearchResults.gating.test.ts:21` (`searchSemantic`)
- test: `src/lib/search/ImageSearchResults.gating.test.ts:25` (`findSimilar`)
- test: `src/lib/search/search.a11y.test.ts:32` (`searchOcr`)
- test: `src/lib/search/search.a11y.test.ts:34` (`searchSemantic`)
- test: `src/lib/search/search.a11y.test.ts:38` (`findSimilar`)

### 15. `executeRenameSaveSpy` / rename-save (target, trimmedName, extensionPolicy, skipExtensionCheck?, volumeId?)

Same mocked production function, re-declared identically in the two rename test suites.

- test: `src/lib/file-explorer/pane/rename-chain.test.ts:22`
- test: `src/lib/file-explorer/pane/rename-flow.test.ts:15`

### 16. `addToastSpy` (content, options?[, pane])

The toast helper's mock type, copy-pasted across pane test files (one carries an extra leading `pane` parameter).

- test: `src/lib/file-explorer/pane/clipboard-operations.test.ts:25`
- test: `src/lib/file-explorer/pane/duplicate-command.test.ts:13`
- test: `src/lib/file-explorer/pane/file-operation-commands.test.ts:22`
- test: `src/lib/file-explorer/pane/paste-clipboard-as-file.test.ts:13` (3-arg variant: `pane, content, options?`)

### 17. `showAlert` (title, message)

- test: `src/lib/file-explorer/pane/clipboard-operations.test.ts:129`
- test: `src/lib/file-explorer/pane/drag-drop-controller.test-fixtures.ts:121`

### 18. `getRecentOperationLogEntries` (limit, offset)

Production function in `src/lib/tauri-commands/operation-log.ts`, mocked in both the feature test and the tauri-commands
wrapper's own test.

- test: `src/lib/operation-log/operation-log-trigger.test.ts:11`
- test: `src/lib/tauri-commands/operation-log.test.ts:13`

### 19. `getOperationLogDetail` (id, limit, offset)

Same story as #18, different command.

- test: `src/lib/operation-log/OperationLogDialog.test.ts:16`
- test: `src/lib/tauri-commands/operation-log.test.ts:14`

### 20. `listAskCmdrConversations` (limit, offset, archived)

- test: `src/lib/ask-cmdr/ask-cmdr-sessions.test.ts:10`
- test: `src/lib/ask-cmdr/ask-cmdr-turn-stream.test.ts:13`

### 21. `onConnect` / `handleSmbUpgradeConnect` (username, password, rememberInKeychain)

`FilePane.svelte:1725` wires `onConnect={smbView.handleSmbUpgradeConnect}` directly (verified); same shape as group 1
and 9's `pane`-prefixed cousins, but its own independent prop.

- production: `src/lib/file-explorer/network/NetworkLoginForm.svelte:33` (`onConnect`)
- production: `src/lib/file-explorer/pane/smb-view-state.svelte.ts:71` (`handleSmbUpgradeConnect`)

### 22. `getMatchIndicesForLabel` / `matchIndices` (label, query)

`setting-components.a11y.test.ts` casts the real `$lib/settings/settings-search` export
(`actual.getMatchIndicesForLabel as (label: string, query: string) => number[]`, line 60) and separately types a stub of
the same shape (`matchIndices`, line 31) that overrides it per-block. Same production function, same file.

- test: `src/lib/settings/components/setting-components.a11y.test.ts:31` (`matchIndices` stub type)
- test: `src/lib/settings/components/setting-components.a11y.test.ts:60` (`realIndices` cast of the actual export)

### 23. `font-metrics` store functions: `storeFontMetrics` / `extendFontMetrics` (fontId, codePoints, widths)

One `vi.fn<...>()` type written twice for two sibling mock declarations back to back; same shape, same file, same
production module (`$lib/font-metrics`).

- test: `src/lib/font-metrics/font-metrics.test.ts:19` (`storeFontMetrics`)
- test: `src/lib/font-metrics/font-metrics.test.ts:20` (`extendFontMetrics`)

### 24. Hono `header(name, value)` CORS helper param (api-server)

**Likely a false positive** — see the note at the bottom of this doc before spending fix effort here. Both are a narrow
structural re-declaration of Hono's own `Context['header']` method, used only for two hardcoded CORS calls each; the
"value" side is never fed from anything the "name" side could plausibly supply.

- production: `apps/api-server/src/website/likes.ts:21` (`likesCors`)
- production: `apps/api-server/src/website/link-codes.ts:78` (`publicCors`)

### 25. `watchSpace` / `watch` (volumeId, path)

`breadcrumb-bar.ts`'s `watchSpace` dependency forwards straight into `volume-space.svelte.ts`'s `watch`: verified at
`FilePane.svelte:1234` (`watchSpace: (id, path) => { diskSpace.watch(id, path) }`).

- production: `src/lib/file-explorer/pane/breadcrumb-bar.ts:78` (`watchSpace`, on `BreadcrumbBarDeps`)
- production: `src/lib/file-explorer/pane/volume-space.svelte.ts:49` (`watch`, on `VolumeSpace`)

### 26. `resolveWriteConflict` (operationId, conflictId, resolution, applyToAll)

Production function `src/lib/tauri-commands/write-operations.ts:221`; the two operation-session test files mock it
identically.

- test: `src/lib/file-operations/operation-session/operation-session-commands.svelte.test.ts:10`
- test: `src/lib/file-operations/operation-session/operation-session.svelte.test.ts:14`

### 27. `caretPositionFromPoint` / `caretRangeFromPoint` (x, y) — DOM polyfill shape

**Also a likely false positive**, same reasoning as group 24: these are structural re-declarations of two STANDARD DOM
`Document` methods (feature-detected browser API surface, not application logic), both taking screen coordinates in the
universal `(x, y)` order. Grouped because they're declared on the same interface for the same reason, not because one
calls the other.

- production: `src/routes/viewer/viewer-pointer.ts:49` (`caretPositionFromPoint`)
- production: `src/routes/viewer/viewer-pointer.ts:50` (`caretRangeFromPoint`)

## Independent single-occurrence violations

Everything below is one file, one signature, no sibling elsewhere in the scanned trees (`explorer-api.ts:167`'s second
flagged pair, `permanent`/`autoConfirm`, is already listed under group 8 and isn't repeated here). Fix each on its own;
no coordination needed.

- `src/lib/ask-cmdr/BulkRenameReviewDialog.a11y.test.ts:23` — `revise(rowId, name)`
- `src/lib/ask-cmdr/BulkRenameReviewDialog.a11y.test.ts:35` — `open(path, volumeId)`
- `src/lib/ask-cmdr/ask-cmdr-context-usage.store.test.ts:12` — `getConversationMock(id, limit, offset)`
- `src/lib/ask-cmdr/ask-cmdr-sessions.test.ts:11` — `searchAskCmdrConversations(q, limit, offset)`
- `src/lib/feedback/FeedbackDialog.a11y.test.ts:16` — `sendFeedback(text, email?)`
- `src/lib/file-operations/mkdir/new-folder-operations.ts:6` —
  `FindFileIndexFn = (listingId, filename, showHiddenFiles)`
- `src/routes/viewer/viewer-keyboard.ts:93` — `selectAll(lastLine, lastLineLength)`
- `src/lib/file-operations/transfer/TransferDialog.test.ts:38` —
  `scanVolumeForConflictsMock(volumeId, sourceItems, destPath, sourceVolumeId?, sourcePaths?)` (a different mock than
  the `ConfirmFn` on line 159, group 2)
- `src/lib/file-operations/transfer/TransferDialog.test.ts:49` — `pathExistsCheckedMock(path, volumeId?)`
- `src/lib/settings/ai-config.test.ts:29` —
  `configureAi(provider, contextSize, cloudProviderId, baseUrl, model, requiresApiKey)` (`configureAi`'s shape isn't
  duplicated elsewhere in the scan)
- `src/lib/settings/sections/KeyboardShortcutsSection.controller.svelte.test.ts:19` — `setShortcut(id, index, combo)`
  (`id`/`combo` share `string`; `index` is `number` so not part of the pair)
- `src/lib/settings/sections/KeyboardShortcutsSection.controller.svelte.test.ts:20` — `addShortcut(id, combo)`, both
  `string`
- `src/lib/settings/sections/KeyboardShortcutsSection.controller.svelte.test.ts:24` —
  `findConflictsForShortcut(combo, scope, id)`
- `src/lib/settings/sections/KeyboardShortcutsSection.controller.svelte.test.ts:25` — `confirmDialog(message, title)`
- `src/lib/settings/settings-store.ts:692` — `SettingChangeListener<K>(id: K, value: SettingsValues[K])`, **the original
  incident's signature.** It's the one production listener type in the codebase written this way.
- `src/lib/suggested-ops/SuggestedOpsDialog.a11y.test.ts:20` — `ensure(id, start)`
- `src/lib/suggested-ops/suggested-ops-trigger.svelte.test.ts:12` — `pageSuggestedOps(groupId, offset, limit)`

## False-positive risk assessment

Two cases read as genuine false positives, both structural re-declarations of a THIRD-PARTY or PLATFORM method signature
rather than application-defined data:

- **Group 24**, Hono's `header(name, value)`: a well-known, extremely stable two-string setter shape (the same shape as
  DOM's `setAttribute`), used only inline for two hardcoded literal calls each. Nobody is going to accidentally swap
  them, and there's no realistic caller-supplied-either-argument path.
- **Group 27**, `caretPositionFromPoint`/`caretRangeFromPoint`'s `(x, y)`: two standard DOM methods, feature-detected
  for Safari/older-browser compatibility. `(x, y)` screen coordinates are a universal, well-understood order; nothing
  about this pairing resembles the incident class the rule targets.

Everything else flagged is either a genuine multi-field domain payload (volume/path/target triples, transfer outcomes,
pagination pairs) or a same-typed pair with a real risk of accidental reordering at a call site outside the file.
Recommend keeping the rule as specified and letting groups 24 and 27 opt out per-line with a reason (e.g.
`// eslint-disable-next-line cmdr/no-confusable-callback-params -- Hono's own two-string header setter, not application data`)
rather than carving out an exception in the rule itself.
