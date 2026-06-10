<!--
    Looping keyboard animation that shows a hand pressing the default global
    go-to-latest hotkey (⌃⌥⌘J): the three modifiers press down and glow, then J
    follows a beat later, the way a person actually types the chord.

    Decorative only. The toast already states the keys in text ("⌃⌥⌘J") via a
    ShortcutChip, so this SVG carries no information of its own — it's
    `aria-hidden` and screen readers skip it.

    Hard-coded to ⌃⌥⌘J on purpose. The caller renders this ONLY when the binding
    is still the default; a remapped combo would no longer match the keys lit up
    here, so the toast drops the animation (but keeps the text chip) in that
    case. See `DownloadToastContent.svelte` and the downloads `CLAUDE.md`.

    Colors come from design tokens so the keyboard reads on both the light and
    dark info-toast backgrounds; the pressed state borrows the system accent.
    `prefers-reduced-motion: reduce` swaps the loop for a static frame with the
    four target keys lit, so the "which keys" message survives without motion.
-->
<svg
    class="kbd-anim"
    viewBox="10 115 585 199"
    xmlns="http://www.w3.org/2000/svg"
    aria-hidden="true"
    focusable="false"
>
    <g transform="translate(16, 16)">
        <!-- ROW 2 (QWERTY) -->
        <g transform="translate(0, 60)">
            <rect class="key-shadow" x="0" y="2" width="74" height="54" rx="6" />
            <g><rect class="key-cap" x="0" y="0" width="74" height="54" rx="6" /><text class="key-label-sm" x="6" y="46">tab</text></g>
        </g>
        <g transform="translate(80, 60)"><rect class="key-shadow" x="0" y="2" width="54" height="54" rx="6" /><g><rect class="key-cap" x="0" y="0" width="54" height="54" rx="6" /><text class="key-label" x="27" y="34" text-anchor="middle">Q</text></g></g>
        <g transform="translate(140, 60)"><rect class="key-shadow" x="0" y="2" width="54" height="54" rx="6" /><g><rect class="key-cap" x="0" y="0" width="54" height="54" rx="6" /><text class="key-label" x="27" y="34" text-anchor="middle">W</text></g></g>
        <g transform="translate(200, 60)"><rect class="key-shadow" x="0" y="2" width="54" height="54" rx="6" /><g><rect class="key-cap" x="0" y="0" width="54" height="54" rx="6" /><text class="key-label" x="27" y="34" text-anchor="middle">E</text></g></g>
        <g transform="translate(260, 60)"><rect class="key-shadow" x="0" y="2" width="54" height="54" rx="6" /><g><rect class="key-cap" x="0" y="0" width="54" height="54" rx="6" /><text class="key-label" x="27" y="34" text-anchor="middle">R</text></g></g>
        <g transform="translate(320, 60)"><rect class="key-shadow" x="0" y="2" width="54" height="54" rx="6" /><g><rect class="key-cap" x="0" y="0" width="54" height="54" rx="6" /><text class="key-label" x="27" y="34" text-anchor="middle">T</text></g></g>
        <g transform="translate(380, 60)"><rect class="key-shadow" x="0" y="2" width="54" height="54" rx="6" /><g><rect class="key-cap" x="0" y="0" width="54" height="54" rx="6" /><text class="key-label" x="27" y="34" text-anchor="middle">Y</text></g></g>
        <g transform="translate(440, 60)"><rect class="key-shadow" x="0" y="2" width="54" height="54" rx="6" /><g><rect class="key-cap" x="0" y="0" width="54" height="54" rx="6" /><text class="key-label" x="27" y="34" text-anchor="middle">U</text></g></g>
        <g transform="translate(500, 60)"><rect class="key-shadow" x="0" y="2" width="54" height="54" rx="6" /><g><rect class="key-cap" x="0" y="0" width="54" height="54" rx="6" /><text class="key-label" x="27" y="34" text-anchor="middle">I</text></g></g>
        <g transform="translate(560, 60)"><rect class="key-shadow" x="0" y="2" width="54" height="54" rx="6" /><g><rect class="key-cap" x="0" y="0" width="54" height="54" rx="6" /><text class="key-label" x="27" y="34" text-anchor="middle">O</text></g></g>

        <!-- ROW 3 (Home row) -->
        <g transform="translate(0, 120)">
            <rect class="key-shadow" x="0" y="2" width="94" height="54" rx="6" />
            <g><rect class="key-cap" x="0" y="0" width="94" height="54" rx="6" /><text class="key-label-sm" x="6" y="46">caps lock</text></g>
        </g>
        <g transform="translate(100, 120)"><rect class="key-shadow" x="0" y="2" width="54" height="54" rx="6" /><g><rect class="key-cap" x="0" y="0" width="54" height="54" rx="6" /><text class="key-label" x="27" y="34" text-anchor="middle">A</text></g></g>
        <g transform="translate(160, 120)"><rect class="key-shadow" x="0" y="2" width="54" height="54" rx="6" /><g><rect class="key-cap" x="0" y="0" width="54" height="54" rx="6" /><text class="key-label" x="27" y="34" text-anchor="middle">S</text></g></g>
        <g transform="translate(220, 120)"><rect class="key-shadow" x="0" y="2" width="54" height="54" rx="6" /><g><rect class="key-cap" x="0" y="0" width="54" height="54" rx="6" /><text class="key-label" x="27" y="34" text-anchor="middle">D</text></g></g>

        <!-- F (homing bar) -->
        <g transform="translate(280, 120)">
            <rect class="key-shadow" x="0" y="2" width="54" height="54" rx="6" />
            <g>
                <rect class="key-cap" x="0" y="0" width="54" height="54" rx="6" />
                <text class="key-label" x="27" y="34" text-anchor="middle">F</text>
                <rect class="homing-bar" x="20" y="44" width="14" height="2" rx="1" />
            </g>
        </g>

        <g transform="translate(340, 120)"><rect class="key-shadow" x="0" y="2" width="54" height="54" rx="6" /><g><rect class="key-cap" x="0" y="0" width="54" height="54" rx="6" /><text class="key-label" x="27" y="34" text-anchor="middle">G</text></g></g>
        <g transform="translate(400, 120)"><rect class="key-shadow" x="0" y="2" width="54" height="54" rx="6" /><g><rect class="key-cap" x="0" y="0" width="54" height="54" rx="6" /><text class="key-label" x="27" y="34" text-anchor="middle">H</text></g></g>

        <!-- TARGET: J (homing bar + press animation) -->
        <g transform="translate(460, 120)">
            <rect class="key-shadow" x="0" y="2" width="54" height="54" rx="6" />
            <g class="j-group">
                <rect class="key-cap j-cap" x="0" y="0" width="54" height="54" rx="6" />
                <text class="key-label" x="27" y="34" text-anchor="middle">J</text>
                <rect class="homing-bar j-bar" x="20" y="44" width="14" height="2" rx="1" />
            </g>
        </g>

        <g transform="translate(520, 120)"><rect class="key-shadow" x="0" y="2" width="54" height="54" rx="6" /><g><rect class="key-cap" x="0" y="0" width="54" height="54" rx="6" /><text class="key-label" x="27" y="34" text-anchor="middle">K</text></g></g>
        <g transform="translate(580, 120)"><rect class="key-shadow" x="0" y="2" width="54" height="54" rx="6" /><g><rect class="key-cap" x="0" y="0" width="54" height="54" rx="6" /><text class="key-label" x="27" y="34" text-anchor="middle">L</text></g></g>
        <g transform="translate(640, 120)"><rect class="key-shadow" x="0" y="2" width="54" height="54" rx="6" /><g><rect class="key-cap" x="0" y="0" width="54" height="54" rx="6" /><text class="key-label" x="27" y="34" text-anchor="middle">;</text></g></g>

        <!-- ROW 4 (ZXCV) -->
        <g transform="translate(0, 180)">
            <rect class="key-shadow" x="0" y="2" width="114" height="54" rx="6" />
            <g><rect class="key-cap" x="0" y="0" width="114" height="54" rx="6" /><text class="key-label-sm" x="6" y="46">shift</text></g>
        </g>
        <g transform="translate(120, 180)"><rect class="key-shadow" x="0" y="2" width="54" height="54" rx="6" /><g><rect class="key-cap" x="0" y="0" width="54" height="54" rx="6" /><text class="key-label" x="27" y="34" text-anchor="middle">Z</text></g></g>
        <g transform="translate(180, 180)"><rect class="key-shadow" x="0" y="2" width="54" height="54" rx="6" /><g><rect class="key-cap" x="0" y="0" width="54" height="54" rx="6" /><text class="key-label" x="27" y="34" text-anchor="middle">X</text></g></g>
        <g transform="translate(240, 180)"><rect class="key-shadow" x="0" y="2" width="54" height="54" rx="6" /><g><rect class="key-cap" x="0" y="0" width="54" height="54" rx="6" /><text class="key-label" x="27" y="34" text-anchor="middle">C</text></g></g>
        <g transform="translate(300, 180)"><rect class="key-shadow" x="0" y="2" width="54" height="54" rx="6" /><g><rect class="key-cap" x="0" y="0" width="54" height="54" rx="6" /><text class="key-label" x="27" y="34" text-anchor="middle">V</text></g></g>
        <g transform="translate(360, 180)"><rect class="key-shadow" x="0" y="2" width="54" height="54" rx="6" /><g><rect class="key-cap" x="0" y="0" width="54" height="54" rx="6" /><text class="key-label" x="27" y="34" text-anchor="middle">B</text></g></g>
        <g transform="translate(420, 180)"><rect class="key-shadow" x="0" y="2" width="54" height="54" rx="6" /><g><rect class="key-cap" x="0" y="0" width="54" height="54" rx="6" /><text class="key-label" x="27" y="34" text-anchor="middle">N</text></g></g>
        <g transform="translate(480, 180)"><rect class="key-shadow" x="0" y="2" width="54" height="54" rx="6" /><g><rect class="key-cap" x="0" y="0" width="54" height="54" rx="6" /><text class="key-label" x="27" y="34" text-anchor="middle">M</text></g></g>
        <g transform="translate(540, 180)"><rect class="key-shadow" x="0" y="2" width="54" height="54" rx="6" /><g><rect class="key-cap" x="0" y="0" width="54" height="54" rx="6" /><text class="key-label" x="27" y="34" text-anchor="middle">,</text></g></g>
        <g transform="translate(600, 180)"><rect class="key-shadow" x="0" y="2" width="54" height="54" rx="6" /><g><rect class="key-cap" x="0" y="0" width="54" height="54" rx="6" /><text class="key-label" x="27" y="34" text-anchor="middle">.</text></g></g>

        <!-- ROW 5 (Modifiers + space) -->
        <g transform="translate(0, 240)">
            <rect class="key-shadow" x="0" y="2" width="54" height="54" rx="6" />
            <g><rect class="key-cap" x="0" y="0" width="54" height="54" rx="6" /><text class="key-label-sm" x="6" y="46">fn</text></g>
        </g>

        <!-- TARGET: control ⌃ -->
        <g transform="translate(60, 240)">
            <rect class="key-shadow" x="0" y="2" width="54" height="54" rx="6" />
            <g class="mod-group">
                <rect class="key-cap mod-cap" x="0" y="0" width="54" height="54" rx="6" />
                <text class="key-label-sm" x="6" y="46">control</text>
                <text class="key-label-sym" x="42" y="22">⌃</text>
            </g>
        </g>

        <!-- TARGET: option ⌥ -->
        <g transform="translate(120, 240)">
            <rect class="key-shadow" x="0" y="2" width="54" height="54" rx="6" />
            <g class="mod-group">
                <rect class="key-cap mod-cap" x="0" y="0" width="54" height="54" rx="6" />
                <text class="key-label-sm" x="6" y="46">option</text>
                <text class="key-label-sym" x="40" y="22">⌥</text>
            </g>
        </g>

        <!-- TARGET: command ⌘ -->
        <g transform="translate(180, 240)">
            <rect class="key-shadow" x="0" y="2" width="68" height="54" rx="6" />
            <g class="mod-group">
                <rect class="key-cap mod-cap" x="0" y="0" width="68" height="54" rx="6" />
                <text class="key-label-sm" x="6" y="46">command</text>
                <text class="key-label-sym" x="52" y="22">⌘</text>
            </g>
        </g>

        <!-- Spacebar -->
        <g transform="translate(254, 240)">
            <rect class="key-shadow" x="0" y="2" width="280" height="54" rx="6" />
            <g><rect class="key-cap" x="0" y="0" width="280" height="54" rx="6" /></g>
        </g>

        <!-- Right command ⌘ (layout balance) -->
        <g transform="translate(540, 240)">
            <rect class="key-shadow" x="0" y="2" width="68" height="54" rx="6" />
            <g>
                <rect class="key-cap" x="0" y="0" width="68" height="54" rx="6" />
                <text class="key-label-sm" x="22" y="46">command</text>
                <text class="key-label-sym" x="6" y="22">⌘</text>
            </g>
        </g>

        <!-- Right option ⌥ -->
        <g transform="translate(614, 240)">
            <rect class="key-shadow" x="0" y="2" width="54" height="54" rx="6" />
            <g>
                <rect class="key-cap" x="0" y="0" width="54" height="54" rx="6" />
                <text class="key-label-sm" x="18" y="46">option</text>
                <text class="key-label-sym" x="6" y="22">⌥</text>
            </g>
        </g>
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

    .homing-bar {
        fill: var(--color-text-tertiary);
    }

    /* Motion path: the chord types itself on a loop. Modifiers press first and
       glow, J follows a beat later, both release together. Held out of the
       reduce branch so reduced-motion users get the static frame below. */
    @media (prefers-reduced-motion: no-preference) {
        .mod-group {
            animation: pressMod 4s cubic-bezier(0.4, 0, 0.2, 1) infinite;
        }
        .mod-cap {
            animation: colorMod 4s cubic-bezier(0.4, 0, 0.2, 1) infinite;
        }
        .j-group {
            animation: pressJ 4s cubic-bezier(0.4, 0, 0.2, 1) infinite;
        }
        .j-cap {
            animation: colorJ 4s cubic-bezier(0.4, 0, 0.2, 1) infinite;
        }
        .j-bar {
            animation: barJ 4s cubic-bezier(0.4, 0, 0.2, 1) infinite;
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

        @keyframes pressJ {
            0%, 25% { transform: translateY(0); }
            30%, 40% { transform: translateY(2px); }
            45%, 100% { transform: translateY(0); }
        }

        @keyframes colorJ {
            0%, 25% { fill: var(--color-bg-tertiary); }
            30%, 40% { fill: var(--color-accent-subtle); }
            45%, 100% { fill: var(--color-bg-tertiary); }
        }

        @keyframes barJ {
            0%, 25% { fill: var(--color-text-tertiary); }
            30%, 40% { fill: var(--color-accent); }
            45%, 100% { fill: var(--color-text-tertiary); }
        }
    }

    /* Static frame: no looping press, but the four target keys stay lit so the
       "press these keys" message still lands without any motion. */
    @media (prefers-reduced-motion: reduce) {
        .mod-cap,
        .j-cap {
            fill: var(--color-accent-subtle);
        }
        .j-bar {
            fill: var(--color-accent);
        }
    }
</style>
