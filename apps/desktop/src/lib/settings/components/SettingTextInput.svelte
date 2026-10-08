<script lang="ts">
    /** Registry-backed single-line text setting. The store follows every edit, so
     * focus changes and window teardown cannot strand a newer value in local UI state. */
    import { onMount, type ComponentProps } from 'svelte'
    import TextInput from '$lib/ui/TextInput.svelte'
    import {
        getSetting,
        getSettingDefinition,
        onSpecificSettingChange,
        setSetting,
        type SettingId,
        type SettingsValues,
    } from '$lib/settings'
    import { useSettingLock } from './setting-lock.svelte'

    type StringSettingId = {
        [K in SettingId]: SettingsValues[K] extends string ? K : never
    }[SettingId]

    type Props = Omit<ComponentProps<typeof TextInput>, 'id' | 'value' | 'oninput'> & {
        id: StringSettingId
        /** Receives user edits and cross-window/reset changes for live previews or derived UI. */
        onValueChange?: (value: string) => void
    }

    const { id, onValueChange, ariaLabel, disabled, 'aria-describedby': ownDescribedBy, ...inputProps }: Props =
        $props()
    const lock = useSettingLock(id)

    const label = getSettingDefinition(id)?.label ?? id
    let value = $state(getSetting(id))

    onMount(() => {
        return onSpecificSettingChange(id, (next) => {
            if (next === value) return
            value = next
            onValueChange?.(next)
        })
    })

    function handleInput(event: Event & { currentTarget: HTMLInputElement }): void {
        value = event.currentTarget.value
        onValueChange?.(value)
        setSetting(id, value)
    }
</script>

<TextInput
    {...inputProps}
    {value}
    oninput={handleInput}
    ariaLabel={ariaLabel ?? label}
    disabled={Boolean(disabled) || lock.locked}
    aria-describedby={lock.describedBy(typeof ownDescribedBy === 'string' ? ownDescribedBy : undefined)}
/>
