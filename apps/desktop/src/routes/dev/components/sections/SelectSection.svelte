<script lang="ts">
    import SectionCard from '$lib/ui/SectionCard.svelte'
    import Select, { type SelectItem } from '$lib/ui/Select.svelte'

    const sizeItems: SelectItem[] = [
        { value: 'auto', label: 'Auto', description: 'Pick the unit that reads best' },
        { value: 'binary', label: 'Binary (KiB, MiB)' },
        { value: 'decimal', label: 'Decimal (KB, MB)' },
    ]
    let sizeValue = $state('auto')

    const encodingItems: SelectItem[] = [
        { value: 'utf-8', label: 'UTF-8 (Detected)', group: 'Unicode' },
        { value: 'utf-16le', label: 'UTF-16 LE', group: 'Unicode' },
        { value: 'utf-16be', label: 'UTF-16 BE', group: 'Unicode' },
        { value: 'windows-1252', label: 'Windows-1252', group: 'Western' },
        { value: 'iso-8859-1', label: 'ISO 8859-1', group: 'Western' },
    ]
    let encodingValue = $state('utf-8')
</script>

<SectionCard id="components-select" label="Select">
    <div class="grid">
        <div class="cell">
            <p class="caption">Flat list with a per-item description. Picks one of a fixed set.</p>
            <div class="control">
                <Select
                    items={sizeItems}
                    value={sizeValue}
                    onChange={(v: string) => {
                        sizeValue = v
                    }}
                    ariaLabel="File size format"
                />
            </div>
        </div>

        <div class="cell">
            <p class="caption">Grouped items (Ark item groups), for example the viewer encoding picker.</p>
            <div class="control">
                <Select
                    items={encodingItems}
                    value={encodingValue}
                    onChange={(v: string) => {
                        encodingValue = v
                    }}
                    ariaLabel="Text encoding"
                />
            </div>
        </div>

        <div class="cell">
            <p class="caption">Disabled.</p>
            <div class="control">
                <Select items={sizeItems} value={sizeValue} onChange={() => {}} ariaLabel="Disabled select" disabled />
            </div>
        </div>

        <div class="cell">
            <p class="caption">Empty value, showing the placeholder.</p>
            <div class="control">
                <Select
                    items={sizeItems}
                    value=""
                    onChange={() => {}}
                    placeholder="Choose a format"
                    ariaLabel="Empty select"
                />
            </div>
        </div>
    </div>
</SectionCard>

<style>
    .grid {
        display: grid;
        grid-template-columns: repeat(auto-fill, minmax(240px, 1fr));
        gap: var(--spacing-lg);
    }

    .caption {
        margin: 0 0 var(--spacing-sm);
        font-size: var(--font-size-xs);
        color: var(--color-text-tertiary);
    }

    .control {
        max-width: 240px;
    }
</style>
