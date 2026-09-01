<script lang="ts">
    import Switch from '$lib/ui/Switch.svelte'
    import type { SettingId } from '$lib/settings'
    import { useBooleanSetting } from './boolean-setting.svelte'

    interface Props {
        id: SettingId
        disabled?: boolean
    }

    const { id, disabled = false }: Props = $props()
    const setting = useBooleanSetting(id)
</script>

<Switch checked={setting.checked} onCheckedChange={setting.set} {disabled} ariaLabel={setting.label} />
