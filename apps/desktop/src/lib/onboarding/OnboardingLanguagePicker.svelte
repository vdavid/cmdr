<script lang="ts">
    import Icon from '$lib/ui/Icon.svelte'
    import SettingSelect from '$lib/settings/components/SettingSelect.svelte'
    import { trackLanguageChanged } from '$lib/intl/language-analytics'

    /**
     * The escape hatch: a compact language picker in the onboarding wizard's frame,
     * visible from the very first step.
     *
     * Cmdr picks the user's language from their macOS preferences, so a first launch
     * can land someone in a language they can't read. Every other way out (Settings >
     * Appearance > Language, the command palette) is labeled in that same language, so
     * it has to be reachable from the screen they're already on.
     *
     * Two things make it readable without reading: the globe glyph, and the options
     * themselves. Each locale's row carries its own endonym, resolved in the option's
     * OWN locale by `languageOptions()` (`settings/definitions/appearance.ts`), so the
     * `en` row reads "English" whatever the app currently speaks. Nothing here has to
     * stay untranslated for that to hold.
     *
     * ❌ NOT a wizard step. The step contract is deliberate (step 3's consent page is
     * non-skippable), so this sits in the frame and disturbs no sequence.
     *
     * Wiring is `SettingSelect` on `appearance.language`, exactly what the Settings
     * picker uses: it reads and writes the setting, `settings-applier.ts` live-applies
     * the language, and the wizard re-renders in place with no restart. ❌ Don't fork
     * that into a bespoke handler.
     */
    interface Props {
        /**
         * The wizard's overlay element, so the open menu escapes the panel's
         * `overflow: hidden` while staying inside the focus trap. See
         * `ui/Select.svelte`'s `portalContainer`.
         */
        portalContainer?: HTMLElement
    }

    const { portalContainer }: Props = $props()
</script>

<div class="language-picker">
    <span class="globe"><Icon name="globe" size={16} aria-hidden="true" /></span>
    <SettingSelect
        id="appearance.language"
        {portalContainer}
        onPicked={(value: string) => { trackLanguageChanged('onboarding', value); }}
    />
</div>

<style>
    .language-picker {
        display: flex;
        align-items: center;
        gap: var(--spacing-xs);
        /* Wide enough for the longest endonym plus the resolved "System default
           (…)" label, narrow enough to stay a frame control rather than a field. */
        width: 220px;
    }

    .globe {
        display: inline-flex;
        flex: none;
        color: var(--color-text-secondary);
    }
</style>
