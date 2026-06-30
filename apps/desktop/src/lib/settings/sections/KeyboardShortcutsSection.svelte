<script lang="ts">
    import { onMount, type Snippet } from 'svelte'
    import { tooltip } from '$lib/tooltip/tooltip'
    import SettingsSection from '../components/SettingsSection.svelte'
    import Button from '$lib/ui/Button.svelte'
    import Icon from '$lib/ui/Icon.svelte'
    import {
        getEffectiveShortcuts,
        isShortcutModified,
        onShortcutChange,
        isNativeShortcutCommand,
        isFixedKeyCommand,
    } from '$lib/shortcuts'
    import GlobalShortcutRow from '$lib/downloads/GlobalShortcutRow.svelte'
    import SectionCard from '$lib/ui/SectionCard.svelte'
    import { fixedKeyMessage, reservedByMacOsMessage, systemShortcutMessage } from './keyboard-shortcuts-banner'
    import { shortcutAnchorId } from '$lib/settings/settings-window'
    import {
        getPendingShortcutHighlight,
        clearPendingShortcutHighlight,
        registerShortcutFilterReset,
        unregisterShortcutFilterReset,
    } from '$lib/settings/pending-shortcut-highlight.svelte'
    import { createKeyboardShortcutsController } from './KeyboardShortcutsSection.controller.svelte'
    import { tString } from '$lib/intl/messages.svelte'
    import Trans from '$lib/intl/Trans.svelte'

    interface Props {
        searchQuery: string
    }

    const { searchQuery }: Props = $props()

    // Business logic (capture/conflict engine, CRUD, filtering, key-filter helpers)
    // lives in the controller; this component keeps the markup, scoped styles, the
    // capture-phase listener, DOM refs, and the deep-link highlight wiring. Passing
    // `() => searchQuery` (an accessor, not a snapshot) keeps the controller's
    // name-search derivation reactive to the parent-driven global search.
    const controller = createKeyboardShortcutsController(() => searchQuery)

    // Purely-UI DOM ref for the key-filter input.
    let keyFilterInput: HTMLInputElement | null = $state(null)

    // Subscribe to shortcut changes; bumping the controller's counter re-runs its
    // derivations and re-keys the rows in the `{#each}` below.
    $effect(() => {
        return onShortcutChange(() => {
            controller.bumpShortcutChangeCounter()
        })
    })

    // Deep-link arrival flash: the settings page sets the target command id in the
    // shared module (`pending-shortcut-highlight`) after scrolling its row into
    // view; we read it here to apply the `flash` class, then clear it once the
    // animation has played out. State-driven (not a direct DOM class) because the
    // rows re-key on `shortcutChangeCounter` — an imperative class would vanish on
    // the next re-render. ~1.6 s matches the CSS animation duration.
    const FLASH_DURATION_MS = 1600
    const highlightedCommandId = $derived(getPendingShortcutHighlight())
    $effect(() => {
        if (highlightedCommandId === null) return
        const timer = setTimeout(() => {
            clearPendingShortcutHighlight()
        }, FLASH_DURATION_MS)
        return () => {
            clearTimeout(timer)
        }
    })

    // Register the controller's filter resetter so a deep link to a row that a
    // leftover filter would hide can clear the filters first (the settings page
    // calls this before its scroll). Unregister on unmount so the page no-ops when
    // the section is gone.
    $effect(() => {
        registerShortcutFilterReset(controller.resetFilters)
        return () => {
            unregisterShortcutFilterReset(controller.resetFilters)
        }
    })

    // Use capture phase listener to intercept key events before they reach +page.svelte's window listener
    // This allows us to stop ESC from closing the settings window when editing shortcuts
    onMount(() => {
        function captureKeyDown(event: KeyboardEvent) {
            if (!controller.editingShortcut) return

            // Handle all key events during editing
            controller.handleKeyDown(event)
        }

        document.addEventListener('keydown', captureKeyDown, true) // true = capture phase
        return () => {
            document.removeEventListener('keydown', captureKeyDown, true)
        }
    })
</script>

<SettingsSection title={tString('shortcuts.section.title')}>
    <div class="shortcuts-header">
        <div class="search-fields">
            <input
                type="text"
                class="search-input"
                placeholder={tString('shortcuts.section.searchByNamePlaceholder')}
                value={searchQuery.trim() ? searchQuery : controller.localNameSearchQuery}
                oninput={(e) => {
                    const target = e.target
                    if (target instanceof HTMLInputElement) controller.localNameSearchQuery = target.value
                }}
                disabled={!!searchQuery.trim()}
                autocomplete="off"
                autocapitalize="off"
                spellcheck="false"
            />
            <div class="key-search-wrapper">
                <input
                    type="text"
                    class="search-input key-search"
                    placeholder={tString('shortcuts.section.filterByKeysPlaceholder')}
                    bind:value={controller.keySearchQuery}
                    bind:this={keyFilterInput}
                    onkeydown={controller.handleKeyFilterKeyDown}
                    onkeyup={controller.handleKeyFilterKeyUp}
                    autocomplete="off"
                    autocapitalize="off"
                    spellcheck="false"
                />
                <span class="key-search-hint" class:visible={!!controller.keySearchQuery}
                    >{tString('shortcuts.section.pressEscToClear')}</span
                >
            </div>
        </div>

        <div class="filters">
            <button
                class="filter-chip"
                class:active={controller.activeFilter === 'all'}
                onclick={() => (controller.activeFilter = 'all')}
            >
                {tString('shortcuts.section.filterAll')}
            </button>
            <button
                class="filter-chip"
                class:active={controller.activeFilter === 'modified'}
                onclick={() => (controller.activeFilter = 'modified')}
            >
                {tString('shortcuts.section.filterModified')}
            </button>
            <button
                class="filter-chip"
                class:active={controller.activeFilter === 'conflicts'}
                onclick={() => (controller.activeFilter = 'conflicts')}
            >
                {tString('shortcuts.section.filterConflicts')}
                {#if controller.conflictCount > 0}
                    <span class="conflict-badge">{controller.conflictCount}</span>
                {/if}
            </button>
        </div>
    </div>

    {#if controller.conflictWarning}
        <!-- `{@const}` snapshot so the kind checks narrow the union (a re-read of the
             `$state` field doesn't narrow, and `SystemConflict` has no `command`). -->
        {@const conflictWarning = controller.conflictWarning}
        {@const conflict = conflictWarning.conflict}
        <div class="conflict-warning">
            <span class="warning-icon"><Icon name="triangle-alert" size={18} aria-hidden="true" /></span>
            {#if conflict.kind === 'native'}
                <!-- macOS owns this combo: it can never reach Cmdr, so we don't offer
                     "Remove from other" or "Keep both" (both would be a lie). -->
                <span class="warning-text">
                    {reservedByMacOsMessage(conflictWarning.shortcut, conflict.command)}
                </span>
                <div class="warning-actions">
                    <Button variant="secondary" size="mini" onclick={controller.cancelEdit}
                        >{tString('shortcuts.section.cancel')}</Button
                    >
                </div>
            {:else if conflict.kind === 'system'}
                <!-- macOS usually intercepts this combo before Cmdr sees it, but the user
                     may have disabled the system shortcut — warn and let them decide. -->
                <span class="warning-text">
                    {systemShortcutMessage(conflictWarning.shortcut, conflict.label)}
                </span>
                <div class="warning-actions">
                    <Button variant="secondary" size="mini" onclick={controller.handleKeepBoth}
                        >{tString('shortcuts.section.useAnyway')}</Button
                    >
                    <Button variant="secondary" size="mini" onclick={controller.cancelEdit}
                        >{tString('shortcuts.section.cancel')}</Button
                    >
                </div>
            {:else if conflict.kind === 'fixed'}
                <!-- The combo is hardcoded in a component: it can't be removed there and
                     would keep firing, so we don't offer "Remove from other" or "Keep both". -->
                <span class="warning-text">
                    {fixedKeyMessage(conflictWarning.shortcut, conflict.command)}
                </span>
                <div class="warning-actions">
                    <Button variant="secondary" size="mini" onclick={controller.cancelEdit}
                        >{tString('shortcuts.section.cancel')}</Button
                    >
                </div>
            {:else if conflict.kind === 'normal'}
                {#snippet comboStrong(children: Snippet)}
                    <strong>{@render children()}</strong>
                {/snippet}
                <span class="warning-text">
                    <Trans
                        key="shortcuts.section.alreadyBound"
                        snippets={{ b: comboStrong }}
                        params={{ combo: conflictWarning.shortcut, command: conflict.command.name }}
                    />
                </span>
                <div class="warning-actions">
                    <Button variant="secondary" size="mini" onclick={controller.handleRemoveFromOther}
                        >{tString('shortcuts.section.removeFromOther')}</Button
                    >
                    <Button variant="secondary" size="mini" onclick={controller.handleKeepBoth}
                        >{tString('shortcuts.section.keepBoth')}</Button
                    >
                    <Button variant="secondary" size="mini" onclick={controller.cancelEdit}
                        >{tString('shortcuts.section.cancel')}</Button
                    >
                </div>
            {/if}
        </div>
    {/if}

    <div class="commands-list">
        {#each controller.groupedCommands as group (group.scope)}
            <SectionCard label={group.title}>
                {#each group.commands as command (`${command.id}-${String(controller.shortcutChangeCounter)}`)}
                    {@const shortcuts = getEffectiveShortcuts(command.id)}
                    {@const isModified = isShortcutModified(command.id)}
                    {@const hasConflicts = controller.conflictingIds.has(command.id)}
                    {@const isNative = isNativeShortcutCommand(command.id)}
                    {@const isFixed = isFixedKeyCommand(command.id)}
                    {@const isAddingHere =
                        controller.editingShortcut !== null &&
                        controller.editingShortcut.commandId === command.id &&
                        controller.editingShortcut.index === shortcuts.length}
                    <div
                        id={shortcutAnchorId(command.id)}
                        class="command-row"
                        class:has-conflicts={hasConflicts}
                        class:flash={command.id === highlightedCommandId}
                    >
                        <div class="command-info">
                            {#if isModified}
                                <span class="modified-dot" use:tooltip={tString('shortcuts.section.modifiedTooltip')}
                                ></span>
                            {/if}
                            {#if hasConflicts}
                                <span class="conflict-icon" use:tooltip={tString('shortcuts.section.hasConflictsTooltip')}
                                    ><Icon name="triangle-alert" size={14} aria-hidden="true" /></span
                                >
                            {/if}
                            <span class="command-name">{command.name}</span>
                        </div>
                        <div class="command-shortcuts">
                            {#if isNative || isFixed}
                                <!-- Read-only rows. Native: macOS owns both the behavior and the
                                     accelerator (PredefinedMenuItem), so Cmdr can neither rebind nor
                                     intercept it. Fixed: the key is hardcoded in the owning component's
                                     keydown handler and never consults the store, so a customization
                                     would be a no-op illusion. Both render plain pills and a badge, with
                                     no +/×/reset and no add slot. -->
                                {#if shortcuts.length > 0}
                                    {#each shortcuts as shortcut (shortcut)}
                                        <span class="shortcut-pill static">{shortcut}</span>
                                    {/each}
                                {:else}
                                    <span class="no-shortcut">{tString('shortcuts.section.noneShortcut')}</span>
                                {/if}
                                {#if isNative}
                                    <span class="readonly-badge" use:tooltip={tString('shortcuts.section.macOsTooltip')}
                                        >{tString('shortcuts.section.macOsBadge')}</span
                                    >
                                {:else}
                                    <span class="readonly-badge" use:tooltip={tString('shortcuts.section.fixedTooltip')}
                                        >{tString('shortcuts.section.fixedBadge')}</span
                                    >
                                {/if}
                            {:else}
                                {#if shortcuts.length > 0}
                                {#each shortcuts as shortcut, i (i)}
                                    {@const isEditing =
                                        controller.editingShortcut !== null &&
                                        controller.editingShortcut.commandId === command.id &&
                                        controller.editingShortcut.index === i}
                                    <button
                                        class="shortcut-pill"
                                        class:editing={isEditing}
                                        class:pending-conflict={isEditing && controller.conflictWarning !== null}
                                        class:empty={!shortcut && !isEditing}
                                        onclick={() => {
                                            controller.startEditingShortcut(command.id, i)
                                        }}
                                    >
                                        {#if isEditing}
                                            {controller.pendingKey || tString('shortcuts.section.pressKeys')}
                                        {:else if shortcut}
                                            {shortcut}
                                            <span
                                                class="remove-shortcut"
                                                use:tooltip={tString('shortcuts.section.removeShortcutTooltip')}
                                                role="button"
                                                tabindex="-1"
                                                onclick={(e) => {
                                                    e.stopPropagation()
                                                    controller.handleRemoveShortcutAtIndex(command.id, i)
                                                }}
                                                onkeydown={(e) => {
                                                    if (e.key === 'Enter' || e.key === ' ') {
                                                        e.stopPropagation()
                                                        controller.handleRemoveShortcutAtIndex(command.id, i)
                                                    }
                                                }}>×</span
                                            >
                                        {:else}
                                            {tString('shortcuts.section.noneShortcut')}
                                        {/if}
                                    </button>
                                {/each}
                            {:else if !isAddingHere}
                                <span class="no-shortcut">{tString('shortcuts.section.noneShortcut')}</span>
                            {/if}
                            {#if isAddingHere}
                                <!-- Synthetic add-slot pill: UI-only until a key is captured and
                                     confirmed. Until then nothing reaches the store, so abandoning
                                     the add leaks no junk entry. -->
                                <button
                                    class="shortcut-pill editing"
                                    class:pending-conflict={controller.conflictWarning !== null}
                                    onclick={() => {
                                        controller.resetPendingCapture()
                                    }}
                                >
                                    {controller.pendingKey || tString('shortcuts.section.pressKeys')}
                                </button>
                            {/if}
                            <button
                                class="add-shortcut"
                                aria-label={tString('shortcuts.section.addShortcutTooltip')}
                                use:tooltip={tString('shortcuts.section.addShortcutTooltip')}
                                onclick={() => {
                                    controller.handleAddShortcut(command.id)
                                }}
                            >
                                +
                            </button>
                            {#if isModified}
                                <button
                                    class="reset-shortcut"
                                    aria-label={tString('shortcuts.section.resetToDefaultTooltip')}
                                    use:tooltip={tString('shortcuts.section.resetToDefaultTooltip')}
                                    onclick={(e) => {
                                        e.stopPropagation()
                                        controller.handleResetShortcut(command.id)
                                    }}
                                >
                                    <Icon name="rotate-ccw" size={14} aria-hidden="true" />
                                </button>
                            {/if}
                            {/if}
                        </div>
                    </div>
                {/each}
            </SectionCard>
        {/each}
        {#if controller.showGlobalGoToLatestRow}
            <SectionCard label={tString('downloads.shortcutRow.scopeTitle')}>
                <GlobalShortcutRow />
            </SectionCard>
        {/if}
    </div>

    <div class="shortcuts-footer">
        <Button variant="secondary" size="mini" onclick={controller.handleResetAll}
            >{tString('shortcuts.section.resetAll')}</Button
        >
    </div>
</SettingsSection>

<style>
    .shortcuts-header {
        display: flex;
        flex-direction: column;
        gap: var(--spacing-sm);
        margin-bottom: var(--spacing-lg);
    }

    .search-fields {
        display: flex;
        gap: var(--spacing-sm);
    }

    .search-input {
        flex: 1;
        padding: var(--spacing-xs) var(--spacing-sm);
        border: 1px solid var(--color-border);
        border-radius: var(--radius-sm);
        background: var(--color-bg-primary);
        color: var(--color-text-primary);
        font-size: var(--font-size-sm);
    }

    .search-input:focus {
        outline: none;
        border-color: var(--color-accent);
        box-shadow: var(--shadow-focus);
    }

    .key-search-wrapper {
        flex: 0.5;
        position: relative;
    }

    .key-search {
        width: 100%;
    }

    .key-search-hint {
        position: absolute;
        top: 100%;
        left: 0;
        margin-top: var(--spacing-xxs);
        font-size: var(--font-size-xs);
        color: var(--color-text-tertiary);
        opacity: 0;
        transition: opacity 0.15s;
    }

    .key-search-hint.visible {
        opacity: 1;
    }

    .filters {
        display: flex;
        gap: var(--spacing-xs);
    }

    .filter-chip {
        padding: var(--spacing-xs) var(--spacing-sm);
        border: 1px solid var(--color-border);
        border-radius: var(--radius-full);
        background: var(--color-bg-primary);
        color: var(--color-text-secondary);
        font-size: var(--font-size-xs);
        cursor: default;
        display: flex;
        align-items: center;
        gap: var(--spacing-xs);
    }

    .filter-chip.active {
        background: var(--color-accent);
        color: var(--color-accent-fg);
        border-color: var(--color-accent);
    }

    .filter-chip.active:hover {
        background: var(--color-accent-hover);
        border-color: var(--color-accent-hover);
    }

    .conflict-badge {
        background: var(--color-warning-bg-solid);
        color: var(--color-warning-text);
        font-size: var(--font-size-xs);
        padding: 1px 5px;
        border-radius: var(--radius-lg);
        font-weight: 600;
    }

    .conflict-warning {
        background: var(--color-warning-bg);
        border: 1px solid var(--color-warning);
        border-radius: var(--radius-sm);
        padding: var(--spacing-sm);
        margin-bottom: var(--spacing-lg);
        display: flex;
        flex-wrap: wrap;
        align-items: center;
        gap: var(--spacing-sm);
    }

    .warning-icon {
        display: inline-flex;
        align-items: center;
        color: var(--color-warning-text);
    }

    .warning-text {
        flex: 1;
        font-size: var(--font-size-sm);
    }

    .warning-actions {
        display: flex;
        gap: var(--spacing-xs);
    }

    .commands-list {
        max-height: 400px;
        overflow-y: auto;
        /* Keep the row's trailing controls (the + add button, the macOS badge) out
           from under the scrollbar. macOS overlay scrollbars float above content and
           take no layout space (`scrollbar-gutter` alone does nothing for them), so
           styling `::-webkit-scrollbar` below opts this scroller into a classic,
           space-taking scrollbar; the gutter then keeps the width stable whether or
           not the list is long enough to scroll. */
        scrollbar-gutter: stable;
    }

    /* Styling any `::-webkit-scrollbar` part switches WebKit from overlay to classic
       scrollbars for this element — that switch is the actual fix; the colors just
       keep it native-looking. */
    .commands-list::-webkit-scrollbar {
        width: 8px;
    }

    .commands-list::-webkit-scrollbar-thumb {
        background: var(--color-border);
        border-radius: var(--radius-full);
    }

    .commands-list::-webkit-scrollbar-thumb:hover {
        background: var(--color-text-tertiary);
    }

    .commands-list::-webkit-scrollbar-track {
        background: transparent;
    }

    .command-row {
        display: flex;
        align-items: center;
        justify-content: space-between;
        padding: var(--spacing-xs) 0;
        border-bottom: 1px solid var(--color-border-subtle);
    }

    .command-row:last-child {
        border-bottom: none;
    }

    .command-row.has-conflicts {
        background: var(--color-warning-bg);
    }

    /* Deep-link arrival flash: two gentle accent-subtle background pulses so the
       eye lands on the row the user clicked a shortcut chip to customize. The
       `border-radius` rounds the highlight so it reads as a deliberate pulse, not
       a full-bleed row-state change. State-driven via `class:flash` (the row
       re-keys on `shortcutChangeCounter`, so a direct DOM class wouldn't survive).
       The settings page clears the state after the animation; the class drops then. */
    .command-row.flash {
        border-radius: var(--radius-sm);
        animation: shortcut-flash 1.5s ease-in-out;
    }

    @keyframes shortcut-flash {
        0%,
        100% {
            background: transparent;
        }

        20%,
        65% {
            background: var(--color-accent-subtle);
        }

        42% {
            background: transparent;
        }
    }

    /* Reduced motion: skip the pulses, show a static highlight that fades out once. */
    @media (prefers-reduced-motion: reduce) {
        .command-row.flash {
            animation: shortcut-flash-static 1.5s ease-out;
        }

        @keyframes shortcut-flash-static {
            0% {
                background: var(--color-accent-subtle);
            }

            100% {
                background: transparent;
            }
        }
    }

    .command-info {
        flex: 1;
        display: flex;
        align-items: center;
        gap: var(--spacing-xs);
    }

    .modified-dot {
        width: 6px;
        height: 6px;
        border-radius: var(--radius-full);
        background: var(--color-accent);
    }

    .conflict-icon {
        display: inline-flex;
        align-items: center;
        color: var(--color-warning);
    }

    .command-name {
        font-size: var(--font-size-sm);
        color: var(--color-text-primary);
    }

    .command-shortcuts {
        display: flex;
        align-items: center;
        gap: var(--spacing-xs);
    }

    .shortcut-pill {
        display: inline-flex;
        align-items: center;
        gap: var(--spacing-xs);
        padding: var(--spacing-xxs) var(--spacing-sm);
        background: var(--color-bg-tertiary);
        border: 1px solid var(--color-border);
        border-radius: var(--radius-sm);
        font-size: var(--font-size-xs);
        font-family: var(--font-system) sans-serif;
        color: var(--color-text-primary);
        cursor: default;
        min-width: 40px;
        text-align: center;
    }

    .shortcut-pill.editing {
        background: var(--color-accent);
        color: var(--color-accent-fg);
        border-color: var(--color-accent);
    }

    .shortcut-pill.editing:hover {
        background: var(--color-accent-hover);
        border-color: var(--color-accent-hover);
    }

    /* A pending-decision pill: the user pressed a conflicting combo and the warning
       banner is up awaiting their choice. Tint it like the warning (also on hover,
       overriding the accent hover above) so it reads as "this combo is in question",
       not as a saved binding. */
    .shortcut-pill.editing.pending-conflict,
    .shortcut-pill.editing.pending-conflict:hover {
        background: var(--color-warning-bg);
        color: var(--color-text-primary);
        border-color: var(--color-warning);
    }

    .shortcut-pill.empty {
        color: var(--color-text-tertiary);
        border-style: dashed;
    }

    /* A read-only pill for macOS-native commands: same chip shape as an editable
       pill, but rendered as a plain span (no hover, no click) so it reads as
       "shown, not editable". */
    .shortcut-pill.static {
        color: var(--color-text-secondary);
    }

    /* "macOS" / "Fixed" badge marking a read-only row (OS-owned or hardcoded in
       its component). Tinted with the muted/secondary surface tokens so it reads
       as an informational tag, not an action. */
    .readonly-badge {
        display: inline-flex;
        align-items: center;
        padding: var(--spacing-xxs) var(--spacing-xs);
        border: 1px solid var(--color-border);
        border-radius: var(--radius-sm);
        background: var(--color-bg-secondary);
        color: var(--color-text-tertiary);
        font-size: var(--font-size-xs);
        cursor: default;
    }

    .remove-shortcut {
        width: 12px;
        height: 12px;
        border-radius: var(--radius-full);
        background: var(--color-text-tertiary);
        color: var(--color-bg-primary);
        font-size: var(--font-size-xs);
        font-weight: 600;
        cursor: default;
        display: none;
        align-items: center;
        justify-content: center;
        line-height: 1;
        flex-shrink: 0;
    }

    .shortcut-pill:hover .remove-shortcut {
        display: flex;
    }

    .no-shortcut {
        color: var(--color-text-tertiary);
        font-size: var(--font-size-sm);
    }

    .add-shortcut,
    .reset-shortcut {
        width: 20px;
        height: 20px;
        padding: 0;
        border: 1px dashed var(--color-border);
        border-radius: var(--radius-sm);
        background: transparent;
        color: var(--color-text-tertiary);
        font-size: var(--font-size-md);
        cursor: default;
        display: flex;
        align-items: center;
        justify-content: center;
    }

    .reset-shortcut {
        font-size: var(--font-size-sm);
    }

    .reset-shortcut:hover {
        color: var(--color-accent-text);
        border-color: var(--color-accent);
    }

    .shortcuts-footer {
        margin-top: var(--spacing-lg);
        padding-top: var(--spacing-sm);
        border-top: 1px solid var(--color-border);
    }
</style>
