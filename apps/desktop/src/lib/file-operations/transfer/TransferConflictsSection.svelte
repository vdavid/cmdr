<script lang="ts">
    /**
     * The copy/move dialog's "files already exist" block: the "couldn't find out"
     * notice, or the card with the merge and clash lines, the policy radios with
     * their "Remember this choice" box, the remembered-overwrite note, and the
     * mixed-type guardrail.
     *
     * Presentational: the dialog owns the conflict check and every policy rule
     * (`remembered-conflict-policy.ts`); a pick goes back through `onPick`, the
     * box through `onRememberChange`. Kept out of `TransferDialog.svelte`, which
     * keeps the confirm path.
     */
    import type { ConflictResolution } from '#lib/file-explorer/types.js'
    import { formatNumber } from '#lib/file-explorer/selection/selection-info-utils.js'
    import RadioGroup, { type RadioItem } from '#lib/ui/RadioGroup.svelte'
    import SectionCard from '#lib/ui/SectionCard.svelte'
    import Checkbox from '#lib/ui/Checkbox.svelte'
    import InfoTip from '#lib/ui/InfoTip.svelte'
    import Icon from '#lib/ui/Icon.svelte'
    import { t, tString } from '#lib/intl/messages.svelte.js'
    import type { OverwriteNoteReach } from './remembered-conflict-policy'
    import { CONFLICT_POLICY_OPTIONS } from './conflict-policy-labels'

    interface Props {
        /** The conflict check couldn't run: say so, never render a clean destination. */
        conflictCheckUnknown: boolean
        /** The check answered and found a clash or a merge, so the radios show. */
        choiceVisible: boolean
        totalConflictCount: number
        mergeFolderCount: number
        /** A single item about to take an existing folder's name: that folder and the item's own name. */
        namedFolderHint: { target: string; name: string } | null
        hasTypeMismatchConflict: boolean
        policy: ConflictResolution
        /** The person picked a radio. */
        onPick: (policy: ConflictResolution) => void
        /** The "Remember this choice" box: a confirm saves the selected policy as the default. */
        remember: boolean
        onRememberChange: (remember: boolean) => void
        /** Set while the radios sit on a remembered policy that overwrites. */
        rememberedOverwriteReach: OverwriteNoteReach | null
    }

    const {
        conflictCheckUnknown,
        choiceVisible,
        totalConflictCount,
        mergeFolderCount,
        namedFolderHint,
        hasTypeMismatchConflict,
        policy,
        onPick,
        remember,
        onRememberChange,
        rememberedOverwriteReach,
    }: Props = $props()

    // File-conflict policy options for `RadioGroup`, from the list the progress
    // dialog's caption words its policy from (`conflict-policy-labels.ts`). With known clashes the label
    // pluralizes on their count ("Skip" vs "Skip all"). With merges alone nobody knows
    // how many files inside clash (the check is top-level only), so the labels claim no
    // count: "Skip all" of zero read as skipping the folders.
    const conflictPolicyItems = $derived<RadioItem[]>(
        CONFLICT_POLICY_OPTIONS.map((option) => ({
            value: option.value,
            label:
                totalConflictCount > 0
                    ? tString(option.labelKey, { count: totalConflictCount })
                    : option.value === 'stop'
                      ? tString('fileOperations.transferDialog.policyAskEach')
                      : tString(option.labelKey, { count: 1 }),
        })),
    )
</script>

{#if conflictCheckUnknown}
    <!-- The check couldn't run. Rendering nothing here would show exactly
         what a clean destination shows, and the user is about to make a
         decision about their own files on the strength of it. -->
    <p class="conflicts-unknown" role="status">
        <span class="conflicts-unknown-icon" aria-hidden="true">
            <Icon name="triangle-alert" size={16} />
        </span>
        <span>{tString('fileOperations.transferDialog.conflictsUnknown')}</span>
    </p>
{:else if choiceVisible}
    <!-- A card, not a full-bleed band: it's one more block in the dialog's column,
     so it obeys the same inset as the fields above it. Warning-toned only when a
     known clash can overwrite something; merges alone are a neutral heads-up. -->
    <SectionCard tone={totalConflictCount > 0 ? 'warning' : 'neutral'}>
        {#if namedFolderHint}
            <p class="named-folder-hint">
                {tString('fileOperations.transferDialog.namedFolderHint', namedFolderHint)}
            </p>
        {/if}
        <!-- Folder merges are informational, never a question: same-named
         folders always merge silently. Surfaced so a user who didn't
         expect a same-named folder at the dest gets a visible cue. -->
        {#if mergeFolderCount > 0}
            <p class="merge-info">
                {mergeFolderCount === 1
                    ? tString('fileOperations.transferDialog.mergeInfoSingle')
                    : tString('fileOperations.transferDialog.mergeInfoMany', {
                          countText: formatNumber(mergeFolderCount),
                          count: mergeFolderCount,
                      })}
            </p>
        {/if}
        {#if totalConflictCount > 0}
            <p class="conflicts-summary">
                {t('fileOperations.transferDialog.conflictsSummary', {
                    countText: String(totalConflictCount),
                    count: totalConflictCount,
                })}
            </p>
        {:else}
            <!-- Merges alone: say the radios are about files INSIDE the merged
             folders, or "Skip" reads as skipping the folders. -->
            <p class="merge-question">
                {t('fileOperations.transferDialog.mergeQuestion', { count: mergeFolderCount })}
            </p>
        {/if}
        <!-- The file policy radios show whenever there's a file conflict OR
         a folder merge: a merge can surface file clashes mid-operation
         the upfront check can't see, and the radios pre-answer them. -->
        <div class="conflict-policy">
            <RadioGroup
                items={conflictPolicyItems}
                value={policy}
                onValueChange={(v) => {
                    onPick(v as ConflictResolution)
                }}
                columns={3}
                ariaDescribedBy={rememberedOverwriteReach ? 'transfer-remembered-overwrite' : undefined}
            />
            <!-- Hidden, not disabled, on "Ask later": remembering "ask" is what an
             unticked box already saves, so there's nothing to choose. -->
            {#if policy !== 'stop'}
                <div class="remember-choice">
                    <Checkbox checked={remember} onCheckedChange={onRememberChange}>
                        {tString('fileOperations.transferDialog.rememberChoice')}
                    </Checkbox>
                    <!-- Beside the box, ❌ never inside its label: a click on the glyph would flip it. -->
                    <InfoTip
                        label={tString('fileOperations.transferDialog.rememberChoiceInfoLabel')}
                        text={tString('fileOperations.transferDialog.rememberChoiceHelp')}
                    />
                </div>
            {/if}
            <!-- The dialog opened on a saved policy that overwrites: say so in words
             beside the warning tint, so nobody overwrites without noticing. Gone
             once the person picks anything. -->
            {#if rememberedOverwriteReach}
                <p class="remembered-overwrite-note" id="transfer-remembered-overwrite">
                    <span class="remembered-overwrite-icon" aria-hidden="true">
                        <Icon name="triangle-alert" size={16} />
                    </span>
                    <span>
                        {tString('fileOperations.transferDialog.rememberedOverwriteNote', {
                            reach: rememberedOverwriteReach,
                        })}
                    </span>
                </p>
            {/if}
        </div>

        <!-- Cross-type guardrail: when a clash mixes a file and a same-named
         folder, "Overwrite all" replaces items of a different type and
         deletes folder contents. The per-file dialog already warns on
         this; the bulk path must not be quieter. -->
        {#if hasTypeMismatchConflict && policy === 'overwrite'}
            <p class="conflict-warning" role="alert">
                <span class="conflict-warning-icon" aria-hidden="true">
                    <Icon name="triangle-alert" size={16} />
                </span>
                <span>
                    {tString('fileOperations.transferDialog.typeMismatchWarning')}
                </span>
            </p>
        {/if}
    </SectionCard>
{/if}

<style>
    /* "Couldn't find out": one row, warning-toned text on the dialog's own
       background (a caveat, not a card of its own), like the dialog's
       `.scan-unavailable`. */
    .conflicts-unknown {
        display: flex;
        align-items: center;
        gap: var(--spacing-sm);
        margin: 0;
        font-size: var(--font-size-sm);
        color: var(--color-warning-text);
    }

    .conflicts-unknown-icon {
        display: inline-flex;
        flex: 0 0 auto;
        color: var(--color-warning-text);
    }

    /* The question the card asks. Plain text color: the card's warning tint already
       carries the "pay attention" signal, and coloring the sentence too made it read
       as an error rather than a choice to make. */
    .conflicts-summary,
    .merge-question {
        margin: 0 0 var(--spacing-md);
        font-size: var(--font-size-md);
        color: var(--color-text-primary);
        font-weight: 500;
    }

    /* Folder-merge info line: neutral, not a warning. Folders always merge, so
       this is a heads-up, not a question. */
    .named-folder-hint,
    .merge-info {
        margin: 0 0 var(--spacing-md);
        font-size: var(--font-size-sm);
        color: var(--color-text-secondary);
    }

    /* Cross-type "Overwrite all" guardrail. Mirrors the per-file dialog's red
       warning (icon + sentence in a tinted block) to flag the destructive swap
       before the user confirms a bulk overwrite across mixed types. */
    .conflict-warning {
        display: flex;
        align-items: flex-start;
        gap: var(--spacing-sm);
        margin: var(--spacing-md) 0 0;
        padding: var(--spacing-sm) var(--spacing-md);
        background: var(--color-error-bg);
        color: var(--color-error-text);
        border: 1px solid var(--color-error-border);
        border-radius: var(--radius-md);
        font-size: var(--font-size-sm);
    }

    .conflict-warning-icon {
        flex-shrink: 0;
        display: inline-flex;
        align-items: center;
        color: var(--color-error-text);
        margin-top: 1px;
    }

    /* A saved overwriting policy, flagged under the radios. Warning text with an
       icon and a warning rule on the dialog's own background, so it stands apart
       from the card's softer tint and doesn't rely on color alone. */
    .remembered-overwrite-note {
        display: flex;
        align-items: flex-start;
        gap: var(--spacing-sm);
        margin: var(--spacing-sm) 0 0;
        padding: var(--spacing-sm) var(--spacing-md);
        background: var(--color-bg-primary);
        color: var(--color-warning-text);
        border: 1px solid var(--color-warning);
        border-radius: var(--radius-md);
        font-size: var(--font-size-sm);
        font-weight: 500;
    }

    .remembered-overwrite-icon {
        flex-shrink: 0;
        display: inline-flex;
        align-items: center;
        color: var(--color-warning-text);
        margin-top: 1px;
    }

    /* Wrapper hook for the policy radios (E2E + component tests target
       `.conflict-policy`); `RadioGroup` owns the option layout (`columns={3}`, so the
       five options fill the card as 3 + 2). The column flex stretches the group to
       full width, which is what gives those three columns something to divide. */
    .conflict-policy {
        display: flex;
        flex-direction: column;
    }

    /* The box and its info glyph on one line under the radios. `flex-start` keeps
       the box at its own width, so the empty card space beside it isn't a click
       target. */
    .remember-choice {
        display: flex;
        align-items: center;
        align-self: flex-start;
        gap: var(--spacing-xs);
        margin-top: var(--spacing-sm);
        font-size: var(--font-size-sm);
    }
</style>
