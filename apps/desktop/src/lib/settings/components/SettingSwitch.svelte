<script lang="ts">
    import Switch from '$lib/ui/Switch.svelte'
    import type { SettingId } from '$lib/settings'
    import { useBooleanSetting } from './boolean-setting.svelte'
    import { useSettingLock } from './setting-lock.svelte'

    interface Props {
        id: SettingId
        disabled?: boolean
    }

    const { id, disabled = false }: Props = $props()
    const setting = useBooleanSetting(id)
    const lock = useSettingLock(id)
</script>

<Switch
    checked={setting.checked}
    onCheckedChange={setting.set}
    disabled={disabled || lock.locked}
    ariaLabel={setting.label}
    ariaDescribedBy={lock.describedBy()}
/>
