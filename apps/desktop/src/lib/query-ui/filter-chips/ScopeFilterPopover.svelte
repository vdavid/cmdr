<script lang="ts">
    /**
     * Scope ("Search in") popover body: a paths textarea plus the "Hide boring folders" /
     * "Case-sensitive" toggles and the ⌥C "Use current folder" / ⌥V "All folders" footer buttons.
     *
     * Extracted from `FilterChips.svelte`. The parent owns the chip strip, the `openChip` state,
     * the ⌥I opener, and the ⌥C / ⌥V keyboard router (active only while this popover is open). The
     * footer buttons here expose ⌥C / ⌥V as first-class mouse affordances; the keyboard wiring
     * stays in the parent so the dialog-level keymap lives next to the popovers it targets.
     */
    import FilterChipPopover from './FilterChipPopover.svelte'
    import ShortcutChip from '$lib/ui/ShortcutChip.svelte'
    import { tooltip } from '$lib/tooltip/tooltip'
    import './filter-popover.css'

    interface Props {
        /** The Search-in chip element, used by the popover shell for positioning + focus return. */
        anchor: HTMLElement
        /** Whether the popover is shown (owned by the parent's `openChip` state). */
        open: boolean
        /** Fired when the popover wants to close (Esc / click outside). */
        onClose: () => void
        scope: string
        excludeSystemDirs: boolean
        caseSensitive: boolean
        /**
         * D12: smart "current folder" the "Use current folder" button acts on. When the focused
         * pane is a search-results snapshot, this walks back to the most recent real folder; when
         * none exists, the button renders disabled with `disabledReason` as its tooltip.
         */
        searchableFolder: {
            path: string | null
            disabled: boolean
            disabledReason: string
        }
        systemDirExcludeTooltip: string
        onInput: (setter: (v: string) => void, search?: boolean) => (e: Event) => void
        onSetScope: (path: string) => void
        onToggleCaseSensitive: () => void
        onToggleExcludeSystemDirs: () => void
        scheduleSearch: () => void
    }

    const {
        anchor,
        open,
        onClose,
        scope,
        excludeSystemDirs,
        caseSensitive,
        searchableFolder,
        systemDirExcludeTooltip,
        onInput,
        onSetScope,
        onToggleCaseSensitive,
        onToggleExcludeSystemDirs,
        scheduleSearch,
    }: Props = $props()
</script>

<!-- Scope ("Search in") popover -->
<FilterChipPopover {anchor} {open} {onClose} ariaLabel="Search in folders">
    <div class="popover-section scope-popover">
        <label class="popover-label" for="popover-scope">Search in</label>
        <textarea
            id="popover-scope"
            class="popover-textarea"
            placeholder="All folders"
            value={scope}
            oninput={onInput(onSetScope)}
            aria-label="Scope folders"
            spellcheck="false"
            autocomplete="off"
            autocapitalize="off"
            rows="3"
        ></textarea>
        <div class="scope-hint">
            Comma-separated paths. Prefix with <code>!</code> to exclude. Wildcards
            <code>*</code> and <code>?</code> work.
        </div>
        <div class="popover-row scope-toggles">
            <label class="popover-checkbox">
                <input
                    type="checkbox"
                    checked={excludeSystemDirs}
                    onchange={() => {
                        onToggleExcludeSystemDirs()
                    }}
                    aria-label="Hide boring folders"
                />
                <!-- "Hide boring folders" (the label is intentional, not "Hide
                     system folders"). Tooltip lists EVERY exclude (built by the
                     parent from the `get_system_dir_excludes` IPC); no
                     "+30 more" truncation. -->
                <span use:tooltip={{ html: systemDirExcludeTooltip }}>Hide boring folders</span>
            </label>
            <label class="popover-checkbox">
                <input
                    type="checkbox"
                    checked={caseSensitive}
                    onchange={() => {
                        onToggleCaseSensitive()
                    }}
                    aria-label="Case-sensitive matching"
                />
                <span>Case-sensitive</span>
            </label>
        </div>
        <!-- D9: scope shortcuts moved inside the popover. ⌥C "Use current
             folder", ⌥V "All folders". Only active while the popover is open
             (matching the round-2 resolved shortcut allocation: the global
             ⌥F now drives the Filename mode chip instead). -->
        <div class="popover-footer">
            <!-- D12: "Use current folder" renders disabled when the focused
                 pane is a search-results snapshot AND no real-folder history
                 entry is reachable. The button still shows so the user sees
                 the option exists; the tooltip explains why it's off. -->
            <button
                type="button"
                class="footer-button"
                disabled={searchableFolder.disabled}
                use:tooltip={searchableFolder.disabled ? searchableFolder.disabledReason : ''}
                onclick={() => {
                    if (searchableFolder.disabled || !searchableFolder.path) return
                    onSetScope(searchableFolder.path)
                    scheduleSearch()
                }}
            >
                Use current folder
                <ShortcutChip key="⌥C" size="sm" />
            </button>
            <button
                type="button"
                class="footer-button"
                onclick={() => {
                    onSetScope('')
                    scheduleSearch()
                }}
            >
                All folders
                <ShortcutChip key="⌥V" size="sm" />
            </button>
        </div>
    </div>
</FilterChipPopover>

<style>
    /* ===== Scope popover ===== */

    .scope-popover {
        min-width: 320px;
    }

    .popover-textarea {
        width: 100%;
        font-size: var(--font-size-sm);
        font-family: var(--font-system);
        /* stylelint-disable-next-line declaration-property-value-disallowed-list */
        padding: 6px 8px;
        border: 1px solid var(--color-border);
        border-radius: var(--radius-sm);
        background: var(--color-bg-primary);
        color: var(--color-text-primary);
        outline: none;
        resize: vertical;
        line-height: 1.4;
    }

    .popover-textarea:focus {
        border-color: var(--color-accent);
        box-shadow: var(--shadow-focus);
    }

    .scope-hint {
        font-size: var(--font-size-xs);
        color: var(--color-text-tertiary);
        line-height: 1.4;
    }

    .scope-hint code {
        font-family: var(--font-mono);
        font-size: var(--font-size-xs);
        background: var(--color-bg-tertiary);
        /* stylelint-disable-next-line declaration-property-value-disallowed-list */
        padding: 1px 3px;
        border-radius: var(--radius-xs);
    }

    .scope-toggles {
        flex-wrap: wrap;
        gap: var(--spacing-md);
    }

    .popover-row {
        display: flex;
        align-items: center;
        gap: var(--spacing-xs);
    }

    .popover-checkbox {
        display: inline-flex;
        align-items: center;
        gap: var(--spacing-xs);
        font-size: var(--font-size-sm);
        color: var(--color-text-primary);
    }

    .popover-footer {
        display: flex;
        gap: var(--spacing-xs);
        padding-top: var(--spacing-xs);
        border-top: 1px solid var(--color-border-subtle);
        margin-top: var(--spacing-xs);
    }

    .footer-button {
        display: inline-flex;
        align-items: center;
        gap: var(--spacing-xs);
        padding: var(--spacing-xxs) var(--spacing-sm);
        font-size: var(--font-size-sm);
        background: transparent;
        border: 1px solid var(--color-border);
        border-radius: var(--radius-sm);
        color: var(--color-text-secondary);
        line-height: 1;
    }

    .footer-button:not(:disabled):hover {
        background: var(--color-bg-tertiary);
        color: var(--color-text-primary);
    }

    .footer-button:disabled {
        opacity: 0.5;
        cursor: not-allowed;
    }
</style>
