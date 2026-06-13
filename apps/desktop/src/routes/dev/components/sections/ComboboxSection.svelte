<script lang="ts">
    import SectionCard from '$lib/ui/SectionCard.svelte'
    import Combobox, { type ComboboxItem } from '$lib/ui/Combobox.svelte'

    const modelItems: ComboboxItem[] = [
        { value: 'gpt-4o', label: 'gpt-4o' },
        { value: 'gpt-4o-mini', label: 'gpt-4o-mini' },
        { value: 'o3-mini', label: 'o3-mini' },
        { value: 'claude-opus-4', label: 'claude-opus-4' },
    ]

    // With suggestions: text stays whatever the user types, even a value not in the list.
    let withListValue = $state('gpt-4o')

    // Cold start: empty items. The field must still show its inputValue and a graceful empty state.
    let emptyValue = $state('my-custom-model')

    // Loading: in-field spinner overlay while the list is fetched.
    let loadingValue = $state('gpt-4o')
</script>

<SectionCard id="components-combobox" label="Combobox">
    <div class="grid">
        <div class="cell">
            <p class="caption">Text field with suggestions. Pick one or type your own (free text persists).</p>
            <div class="control">
                <Combobox
                    items={modelItems}
                    inputValue={withListValue}
                    onInputValueChange={(v: string) => {
                        withListValue = v
                    }}
                    placeholder="Example: gpt-4o"
                    ariaLabel="Model with suggestions"
                />
            </div>
        </div>

        <div class="cell">
            <p class="caption">
                Empty list (cold start). The field keeps its typed value and shows a graceful empty state on focus.
            </p>
            <div class="control">
                <Combobox
                    items={[]}
                    inputValue={emptyValue}
                    onInputValueChange={(v: string) => {
                        emptyValue = v
                    }}
                    placeholder="Type a model name"
                    ariaLabel="Model, no suggestions yet"
                />
            </div>
        </div>

        <div class="cell">
            <p class="caption">Loading: in-field spinner while the suggestions fetch.</p>
            <div class="control">
                <Combobox
                    items={[]}
                    inputValue={loadingValue}
                    onInputValueChange={(v: string) => {
                        loadingValue = v
                    }}
                    loading
                    placeholder="Loading models…"
                    ariaLabel="Model, loading suggestions"
                />
            </div>
        </div>

        <div class="cell">
            <p class="caption">Disabled.</p>
            <div class="control">
                <Combobox
                    items={modelItems}
                    inputValue="gpt-4o"
                    onInputValueChange={() => {}}
                    ariaLabel="Disabled combobox"
                    disabled
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
