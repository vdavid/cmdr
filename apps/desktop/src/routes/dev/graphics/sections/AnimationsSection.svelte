<script lang="ts">
    import SectionCard from '$lib/ui/SectionCard.svelte'
    import ProgressBar from '$lib/ui/ProgressBar.svelte'
    import { tooltip } from '$lib/tooltip/tooltip'

    const PULSE_USAGE =
        'Opacity fades about 1 to 0.3 on a loop. Used for the AI suggestion chip while thinking, the drive-indexing hourglass, and a reduced-motion fallback.'
    const SHAKE_USAGE =
        'Quick horizontal jitter. Used on the volume free-space retry icon, and the inline-rename field on an invalid name.'
    const FLASH_USAGE =
        'Background blips a color once. Used on the failed space-retry row (warning), and the jumped-to shortcuts-list row (accent).'
    const SHIMMER_USAGE = 'A highlight stripe sweeps across. Used on the progress bar fill.'
    const TOAST_SLIDE_USAGE = 'Slides in from the right while fading in. Used for the toast entrance.'
    const FADE_IN_USAGE = 'Fades up from transparent. Used for the loading screen entrance.'
    const RENAME_GLOW_USAGE = 'A quick scale 1.02 to 1 pop. Used on inline-rename field activation.'
    const KEYBOARD_USAGE = 'See the keyboard animation in the Illustrations section above.'
</script>

<SectionCard id="graphics-animations" label="Animations">
    <p class="intro">
        The current hand-rolled CSS animations. The plan is to uniformize them later (likely to animated SVGs). The
        live demos recreate each effect locally and respect <code>prefers-reduced-motion</code>.
    </p>
    <div class="grid">
        <div class="cell" use:tooltip={PULSE_USAGE}>
            <div class="demo-host">
                <div class="swatch demo-pulse"></div>
            </div>
            <p class="caption">pulse</p>
        </div>

        <div class="cell" use:tooltip={SHAKE_USAGE}>
            <div class="demo-host">
                <div class="swatch demo-shake"></div>
            </div>
            <p class="caption">shake</p>
        </div>

        <div class="cell" use:tooltip={FLASH_USAGE}>
            <div class="demo-host">
                <div class="swatch demo-flash"></div>
            </div>
            <p class="caption">flash</p>
        </div>

        <div class="cell" use:tooltip={SHIMMER_USAGE}>
            <div class="demo-host demo-host-wide">
                <ProgressBar value={0.6} ariaLabel="Shimmer demo" />
            </div>
            <p class="caption">shimmer</p>
        </div>

        <div class="cell" use:tooltip={TOAST_SLIDE_USAGE}>
            <div class="demo-host">
                <div class="swatch demo-toast-slide"></div>
            </div>
            <p class="caption">toast-slide-in</p>
        </div>

        <div class="cell" use:tooltip={FADE_IN_USAGE}>
            <div class="demo-host">
                <span class="card-note">No live demo</span>
            </div>
            <p class="caption">fadeIn</p>
        </div>

        <div class="cell" use:tooltip={RENAME_GLOW_USAGE}>
            <div class="demo-host">
                <span class="card-note">No live demo</span>
            </div>
            <p class="caption">rename-glow</p>
        </div>

        <div class="cell" use:tooltip={KEYBOARD_USAGE}>
            <div class="demo-host">
                <span class="card-note">See Illustrations</span>
            </div>
            <p class="caption">keyboard demo</p>
        </div>
    </div>
</SectionCard>

<style>
    .intro {
        margin: 0 0 var(--spacing-lg);
        font-size: var(--font-size-xs);
        color: var(--color-text-tertiary);
    }

    .intro code {
        font-family: var(--font-mono);
        font-size: var(--font-size-xs);
    }

    .grid {
        display: grid;
        grid-template-columns: repeat(auto-fill, minmax(160px, 1fr));
        gap: var(--spacing-lg);
    }

    .cell {
        display: flex;
        flex-direction: column;
        align-items: center;
    }

    .demo-host {
        display: flex;
        align-items: center;
        justify-content: center;
        width: 100%;
        height: 64px;
    }

    .demo-host-wide {
        padding: 0 var(--spacing-md);
    }

    .swatch {
        width: 40px;
        height: 40px;
        border-radius: var(--radius-md);
        background: var(--color-accent);
    }

    .card-note {
        font-size: var(--font-size-xs);
        font-style: italic;
        color: var(--color-text-tertiary);
    }

    .caption {
        margin: var(--spacing-sm) 0 0;
        font-size: var(--font-size-xs);
        font-family: var(--font-mono);
        color: var(--color-text-tertiary);
        text-align: center;
    }

    /* Live demos recreate each effect locally (the originals are component-scoped).
       Gated on no-preference so reduced-motion users see a static swatch. */
    @media (prefers-reduced-motion: no-preference) {
        .demo-pulse {
            animation: demo-pulse 1.4s ease-in-out infinite;
        }

        .demo-shake {
            animation: demo-shake 0.5s ease-in-out infinite;
        }

        .demo-flash {
            background: transparent;
            border: 1px solid var(--color-border);
            animation: demo-flash 1.6s ease-in-out infinite;
        }

        .demo-toast-slide {
            animation: demo-toast-slide 2s ease-in-out infinite;
        }
    }

    @keyframes demo-pulse {
        0%,
        100% {
            opacity: 1;
        }
        50% {
            opacity: 0.3;
        }
    }

    @keyframes demo-shake {
        0%,
        100% {
            transform: translateX(0);
        }
        25% {
            transform: translateX(-3px);
        }
        75% {
            transform: translateX(3px);
        }
    }

    @keyframes demo-flash {
        0%,
        100% {
            background-color: transparent;
        }
        50% {
            background-color: var(--color-warning-bg);
        }
    }

    @keyframes demo-toast-slide {
        0% {
            opacity: 0;
            transform: translateX(20px);
        }
        30%,
        100% {
            opacity: 1;
            transform: translateX(0);
        }
    }
</style>
