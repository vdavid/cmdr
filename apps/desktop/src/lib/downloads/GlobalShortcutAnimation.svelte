<!--
    Looping keyboard animation that shows a hand pressing the global go-to-latest hotkey: the
    chord's modifiers press down and glow, then its key follows a beat later, the way a person
    actually types it.

    Drawn from the live binding on this OS's own keyboard (`global-shortcut-keyboard.ts`): Apple's
    key caps on macOS (⌃⌥⌘J by default), a PC laptop's on Windows and Linux (Ctrl+Alt+Win+J, with
    the Windows logo on the Windows key; Ctrl+Alt+Super+J). The caller renders this ONLY when the keyboard draws every key of the
    binding (`chordOnKeyboard` isn't `null`); a chord on `P`, an F-key, or Space keeps the text
    chip and drops the animation. See `DownloadToastContent.svelte` and the downloads `CLAUDE.md`.

    Decorative only. The toast already states the keys in text via a ShortcutChip, so this SVG
    carries no information of its own: it's `aria-hidden` and screen readers skip it.

    Colors come from design tokens so the keyboard reads on both the light and dark info-toast
    backgrounds; the pressed state borrows the system accent. `prefers-reduced-motion: reduce`
    swaps the loop for a static frame with the chord's keys lit, so the "which keys" message
    survives without motion.
-->
<script lang="ts">
    import type { Platform } from '#lib/platform/index.js'
    import { chordOnKeyboard, keyboardFor, WORD_Y } from './global-shortcut-keyboard'

    interface Props {
        /** The binding to press, in any OS's spelling (`⌃⌥⌘J`, `Ctrl+Alt+Super+J`). */
        binding: string
        /** Whose keyboard to draw. */
        platform: Platform
    }

    const { binding, platform }: Props = $props()

    const keys = $derived(keyboardFor(platform))
    const pressed = $derived(chordOnKeyboard(binding, keys) ?? new Set<number>())
    const wordY = $derived(WORD_Y[platform])
</script>

<svg
    class="kbd-anim"
    viewBox="10 115 585 199"
    xmlns="http://www.w3.org/2000/svg"
    aria-hidden="true"
    focusable="false"
    data-platform={platform}
>
    <g transform="translate(16, 16)">
        {#each keys as key, i (i)}
            {@const isPressed = pressed.has(i)}
            {@const isModifier = isPressed && key.modifier !== undefined}
            {@const isTarget = isPressed && key.modifier === undefined}
            <g transform="translate({key.x}, {key.y})">
                <rect class="key-shadow" x="0" y="2" width={key.width} height="54" rx="6" />
                <g class:mod-group={isModifier} class:target-group={isTarget}>
                    <rect
                        class="key-cap"
                        class:mod-cap={isModifier}
                        class:target-cap={isTarget}
                        x="0"
                        y="0"
                        width={key.width}
                        height="54"
                        rx="6"
                    />
                    {#if key.char !== undefined}
                        <text class="key-label" x={key.width / 2} y="34" text-anchor="middle">{key.char}</text>
                    {/if}
                    {#if key.word !== undefined}
                        <text class="key-label-sm" x={key.wordX ?? 6} y={wordY}>{key.word}</text>
                    {/if}
                    {#if key.logo === 'windows'}
                        <!-- The flat four-pane Windows logo, as Windows keyboards print it: four equal
                             squares, sized and placed like the other PC legends (top left, cap height). -->
                        <g class="key-legend" data-legend="windows-logo" transform="translate(6, 8)">
                            <rect x="0" y="0" width="4.5" height="4.5" />
                            <rect x="5.5" y="0" width="4.5" height="4.5" />
                            <rect x="0" y="5.5" width="4.5" height="4.5" />
                            <rect x="5.5" y="5.5" width="4.5" height="4.5" />
                        </g>
                    {/if}
                    {#if key.symbol !== undefined}
                        <text class="key-label-sym" x={key.symbolX ?? 6} y="22">{key.symbol}</text>
                    {/if}
                    {#if key.homing}
                        <rect class="homing-bar" class:target-bar={isTarget} x="20" y="44" width="14" height="2" rx="1" />
                    {/if}
                </g>
            </g>
        {/each}
    </g>
</svg>

<style>
    .kbd-anim {
        display: block;
        width: 100%;
        height: auto;
    }

    .key-shadow {
        fill: var(--color-border);
    }

    .key-cap {
        fill: var(--color-bg-tertiary);
    }

    .key-label {
        /* stylelint-disable-next-line declaration-property-value-disallowed-list -- SVG user-space geometry (viewBox units), not UI rem text; the --font-size-* tokens would mis-scale inside the fixed 585×199 coordinate space */
        font-size: 16px;
        font-weight: 500;
        fill: var(--color-text-primary);
        user-select: none;
    }

    .key-label-sm {
        /* stylelint-disable-next-line declaration-property-value-disallowed-list -- SVG user-space geometry (viewBox units), not UI rem text */
        font-size: 10px;
        font-weight: 500;
        fill: var(--color-text-tertiary);
        user-select: none;
    }

    .key-label-sym {
        /* stylelint-disable-next-line declaration-property-value-disallowed-list -- SVG user-space geometry (viewBox units), not UI rem text */
        font-size: 14px;
        font-weight: 600;
        fill: var(--color-text-secondary);
        user-select: none;
    }

    /* A drawn legend takes the word legends' color, so it follows light and dark. */
    .key-legend {
        fill: var(--color-text-tertiary);
    }

    .homing-bar {
        fill: var(--color-text-tertiary);
    }

    /* Motion path: the chord types itself on a loop. Modifiers press first and
       glow, the key follows a beat later, both release together. Held out of the
       reduce branch so reduced-motion users get the static frame below. */
    @media (prefers-reduced-motion: no-preference) {
        .mod-group {
            animation: pressMod 4s cubic-bezier(0.4, 0, 0.2, 1) infinite;
        }
        .mod-cap {
            animation: colorMod 4s cubic-bezier(0.4, 0, 0.2, 1) infinite;
        }
        .target-group {
            animation: pressTarget 4s cubic-bezier(0.4, 0, 0.2, 1) infinite;
        }
        .target-cap {
            animation: colorTarget 4s cubic-bezier(0.4, 0, 0.2, 1) infinite;
        }
        .target-bar {
            animation: barTarget 4s cubic-bezier(0.4, 0, 0.2, 1) infinite;
        }

        @keyframes pressMod {
            0%, 10% { transform: translateY(0); }
            15%, 55% { transform: translateY(2px); }
            60%, 100% { transform: translateY(0); }
        }

        @keyframes colorMod {
            0%, 10% { fill: var(--color-bg-tertiary); }
            15%, 55% { fill: var(--color-accent-subtle); }
            60%, 100% { fill: var(--color-bg-tertiary); }
        }

        @keyframes pressTarget {
            0%, 25% { transform: translateY(0); }
            30%, 40% { transform: translateY(2px); }
            45%, 100% { transform: translateY(0); }
        }

        @keyframes colorTarget {
            0%, 25% { fill: var(--color-bg-tertiary); }
            30%, 40% { fill: var(--color-accent-subtle); }
            45%, 100% { fill: var(--color-bg-tertiary); }
        }

        @keyframes barTarget {
            0%, 25% { fill: var(--color-text-tertiary); }
            30%, 40% { fill: var(--color-accent); }
            45%, 100% { fill: var(--color-text-tertiary); }
        }
    }

    /* Static frame: no looping press, but the chord's keys stay lit so the
       "press these keys" message still lands without any motion. */
    @media (prefers-reduced-motion: reduce) {
        .mod-cap,
        .target-cap {
            fill: var(--color-accent-subtle);
        }
        .target-bar {
            fill: var(--color-accent);
        }
    }
</style>
