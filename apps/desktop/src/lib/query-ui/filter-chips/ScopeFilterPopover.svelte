<script lang="ts">
    /**
     * Scope ("Search in") popover body: a paths textarea plus the "Hide boring folders" /
     * "Case-sensitive" toggles and the ⌥C "Use current folder" / ⌥V "This volume" footer buttons.
     *
     * A search covers at most one volume, so those two buttons are the whole ladder: the
     * current folder (also what an EMPTY box means, shown as the placeholder) and the volume
     * it lives on. There is no "all folders" rung any more.
     *
     * Extracted from `FilterChips.svelte`. The parent owns the chip strip, the `openChip` state,
     * the ⌥I opener, and the ⌥C / ⌥V keyboard router (active only while this popover is open). The
     * footer buttons here expose ⌥C / ⌥V as first-class mouse affordances; the keyboard wiring
     * stays in the parent so the dialog-level keymap lives next to the popovers it targets.
     */
    import Checkbox from '#lib/ui/Checkbox.svelte'
    import TextArea from '#lib/ui/TextArea.svelte'
    import FilterPopover from '#lib/ui/FilterPopover.svelte'
    import Button from '#lib/ui/Button.svelte'
    import { tooltip } from '#lib/tooltip/tooltip.js'
    import { tString } from '#lib/intl/messages.svelte.js'
    import Trans from '#lib/intl/Trans.svelte'
    import type { ScopePresets } from '../query-dialog-config'
    import { queryKeyAria, queryKeyLabel } from '../query-keys'
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
         * The two scope presets the footer buttons set. `currentFolder` is `null` when the
         * focused pane is a search-results snapshot with no real folder behind it; the button
         * then renders blocked (`ariaDisabled`) with `currentFolderUnavailableReason` as its tooltip.
         */
        scopePresets: ScopePresets
        /** What an empty box means: the path a defaulted search actually runs against. */
        defaultScopePath: string
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
        scopePresets,
        defaultScopePath,
        systemDirExcludeTooltip,
        onInput,
        onSetScope,
        onToggleCaseSensitive,
        onToggleExcludeSystemDirs,
        scheduleSearch,
    }: Props = $props()
</script>

<!--
  One code-styled span per `<tag>` in the scope-hint message. Each renders the
  fixed literal character (''!'', ''*'', ''?'') in a code style, not the tag''s inner
  content; the characters also live in the message so translators see them in
  context. `children` is intentionally ignored.
-->
{#snippet bangCode()}<code>!</code>{/snippet}
{#snippet starCode()}<code>*</code>{/snippet}
{#snippet questionCode()}<code>?</code>{/snippet}

<!-- Scope ("Search in") popover -->
<FilterPopover
    {anchor}
    {open}
    {onClose}
    label={tString('queryUi.scope.popover.label')}
    labelFor="popover-scope"
    ariaLabel={tString('queryUi.scope.popover.aria')}
    sectionClass="scope-popover"
>
        <TextArea
            id="popover-scope"
            radius="sm"
            placeholder={defaultScopePath}
            value={scope}
            oninput={onInput(onSetScope)}
            ariaLabel={tString('queryUi.scope.textareaAria')}
            spellcheck="false"
            autocomplete="off"
            autocapitalize="off"
            rows={3}
        />
        <div class="scope-hint">
            <Trans
                key="queryUi.scope.hint"
                snippets={{ bang: bangCode, star: starCode, question: questionCode }}
            />
        </div>
        <div class="popover-row scope-toggles">
            <Checkbox
                checked={excludeSystemDirs}
                onCheckedChange={() => {
                    onToggleExcludeSystemDirs()
                }}
            >
                <!-- "Hide boring folders" (the label is intentional, not "Hide
                     system folders"). Tooltip lists EVERY exclude (built by the
                     parent from the `get_system_dir_excludes` IPC); no
                     "+30 more" truncation. -->
                <span use:tooltip={{ html: systemDirExcludeTooltip }}>{tString('queryUi.scope.toggle.hideBoring')}</span>
            </Checkbox>
            <Checkbox
                checked={caseSensitive}
                onCheckedChange={() => {
                    onToggleCaseSensitive()
                }}
                ariaLabel={tString('queryUi.scope.toggle.caseSensitiveAria')}
            >
                {tString('queryUi.scope.toggle.caseSensitive')}
            </Checkbox>
        </div>
        <!-- D9: scope shortcuts live inside the popover. ⌥C "Use current
             folder", ⌥V "This volume". Only active while the popover is open
             (matching the round-2 resolved shortcut allocation: the global
             ⌥F now drives the Filename mode chip instead). -->
        <div class="popover-footer">
            <!-- "Use current folder" renders blocked when the focused pane is
                 a search-results snapshot AND no real-folder history entry is
                 reachable. The button still shows so the user sees the option
                 exists, and stays hoverable and focusable so its tooltip can
                 say why it's off. -->
            <Button
                size="mini"
                ariaDisabled={scopePresets.currentFolder === null}
                tooltipContent={scopePresets.currentFolder === null
                    ? scopePresets.currentFolderUnavailableReason
                    : undefined}
                keyHint={queryKeyLabel('scopeCurrentFolder')}
                aria-keyshortcuts={queryKeyAria('scopeCurrentFolder')}
                onclick={() => {
                    if (!scopePresets.currentFolder) return
                    onSetScope(scopePresets.currentFolder)
                    scheduleSearch()
                }}
            >
                {tString('queryUi.scope.useCurrentFolder')}
            </Button>
            <Button
                size="mini"
                tooltipContent={scopePresets.volumeRoot}
                keyHint={queryKeyLabel('scopeVolume')}
                aria-keyshortcuts={queryKeyAria('scopeVolume')}
                onclick={() => {
                    onSetScope(scopePresets.volumeRoot)
                    scheduleSearch()
                }}
            >
                {tString('queryUi.scope.thisVolume')}
            </Button>
        </div>
</FilterPopover>

<style>
    /* ===== Scope popover ===== */
    /* `.scope-popover` (the section min-width) lives in the shared `filter-popover.css` because
       the wrapper element is rendered by `FilterPopover`, not by this component's own markup. */

    .scope-hint {
        font-size: var(--font-size-xs);
        color: var(--color-text-tertiary);
        line-height: var(--font-line-height-normal);
    }

    .scope-hint code {
        font-family: var(--font-mono);
        font-size: var(--font-size-xs);
        background: var(--color-bg-tertiary);
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

    .popover-footer {
        display: flex;
        gap: var(--spacing-xs);
        padding-top: var(--spacing-xs);
        border-top: 1px solid var(--color-border-subtle);
        margin-top: var(--spacing-xs);
    }
</style>
