<!--
  Link codes page (`/links`): manage the `?r=` short codes used in tracking links. The table lists the
  live map from the api-server admin endpoint; the form adds or edits a code; each row deletes. All
  writes go through SvelteKit form actions in `+page.server.ts`, so the admin token stays server-side
  and never reaches the browser. The layout hides the range/day picker here.
-->
<script lang="ts">
    import { enhance } from '$app/forms'
    import SectionDescription from '$lib/components/SectionDescription.svelte'
    import { exampleLink, isValidCode, type LinkCodeRow } from '$lib/link-codes.js'

    let { data, form } = $props()

    /** The form's working values. Editing a row copies its values in; saving or canceling clears it. */
    let code = $state('')
    let utm_source = $state('')
    let utm_medium = $state('')
    let note = $state('')
    /** True while editing an existing code (so the form titles/buttons reflect edit vs add). */
    let editing = $state(false)

    /** Repopulate the form from a failed save (the action returns the offending values). */
    $effect(() => {
        if (form?.action === 'save' && 'error' in form && form.error) {
            code = form.code ?? code
            utm_source = form.utm_source ?? utm_source
            utm_medium = form.utm_medium ?? utm_medium
            note = form.note ?? note
        }
    })

    function startEdit(row: LinkCodeRow) {
        code = row.code
        utm_source = row.utm_source
        utm_medium = row.utm_medium
        note = row.note
        editing = true
        window.scrollTo({ top: 0, behavior: 'smooth' })
    }

    function resetForm() {
        code = ''
        utm_source = ''
        utm_medium = ''
        note = ''
        editing = false
    }

    /** Live preview of the example link for the code being typed. */
    const previewLink = $derived(isValidCode(code.trim().toLowerCase()) ? exampleLink(code.trim().toLowerCase()) : '')
</script>

<section class="mt-2 rounded-xl border border-border bg-surface p-6">
    <h2 class="text-lg font-semibold text-text-primary">Link codes</h2>
    <SectionDescription
        insight="These are the short codes behind your tracking links. A code like ?r=hn on getcmdr.com expands to a real utm_source (and optional medium) before analytics records the visit, so your links stay short and clean while the channel still gets attributed. New codes work the moment you save them, no deploy needed."
        caveat="Unknown codes still pass through as their own source verbatim, so this map only exists to give a short code a friendlier meaning. Edits take up to about 5 minutes to reach visitors, since the public list is cached at the edge."
    />

    {#if data.loadError}
        <p class="mb-4 rounded-lg border border-danger/40 bg-danger/10 px-3 py-2 text-sm text-danger">
            Couldn't load the codes: {data.loadError}
        </p>
    {/if}

    <!-- Add / edit form -->
    <div class="mb-6 rounded-lg border border-border-subtle bg-surface-elevated p-4">
        <h3 class="mb-3 text-sm font-semibold text-text-primary">
            {editing ? `Edit code “${code}”` : 'Add a code'}
        </h3>
        <form
            method="POST"
            action="?/save"
            use:enhance={() =>
                ({ update }) =>
                    update({ reset: false }).then(() => {
                        if (form?.action === 'save' && form?.saved) resetForm()
                    })}
            class="grid grid-cols-1 gap-3 sm:grid-cols-2 lg:grid-cols-5"
        >
            <label class="flex flex-col gap-1 text-xs text-text-tertiary">
                Code
                <input
                    name="code"
                    bind:value={code}
                    readonly={editing}
                    placeholder="hn"
                    autocomplete="off"
                    spellcheck="false"
                    class="rounded-md border border-border bg-surface px-2 py-1.5 text-sm text-text-primary
                        placeholder:text-text-tertiary read-only:opacity-60 focus:border-accent focus:outline-none"
                />
            </label>
            <label class="flex flex-col gap-1 text-xs text-text-tertiary">
                Source (utm_source)
                <input
                    name="utm_source"
                    bind:value={utm_source}
                    placeholder="hackernews"
                    autocomplete="off"
                    spellcheck="false"
                    class="rounded-md border border-border bg-surface px-2 py-1.5 text-sm text-text-primary
                        placeholder:text-text-tertiary focus:border-accent focus:outline-none"
                />
            </label>
            <label class="flex flex-col gap-1 text-xs text-text-tertiary">
                Medium (optional)
                <input
                    name="utm_medium"
                    bind:value={utm_medium}
                    placeholder="social"
                    autocomplete="off"
                    spellcheck="false"
                    class="rounded-md border border-border bg-surface px-2 py-1.5 text-sm text-text-primary
                        placeholder:text-text-tertiary focus:border-accent focus:outline-none"
                />
            </label>
            <label class="flex flex-col gap-1 text-xs text-text-tertiary sm:col-span-2 lg:col-span-1">
                Note (optional)
                <input
                    name="note"
                    bind:value={note}
                    placeholder="r/macapps launch comment"
                    autocomplete="off"
                    class="rounded-md border border-border bg-surface px-2 py-1.5 text-sm text-text-primary
                        placeholder:text-text-tertiary focus:border-accent focus:outline-none"
                />
            </label>
            <div class="flex items-end gap-2">
                <button
                    type="submit"
                    class="rounded-md bg-accent px-4 py-1.5 text-sm font-medium text-accent-contrast
                        transition-colors hover:bg-accent-hover"
                >
                    {editing ? 'Save changes' : 'Add code'}
                </button>
                {#if editing}
                    <button
                        type="button"
                        onclick={resetForm}
                        class="rounded-md border border-border px-3 py-1.5 text-sm text-text-secondary
                            transition-colors hover:text-text-primary"
                    >
                        Cancel
                    </button>
                {/if}
            </div>
        </form>

        {#if previewLink}
            <p class="mt-3 text-xs text-text-tertiary">
                Example link: <code class="text-text-secondary">{previewLink}</code>
            </p>
        {/if}

        {#if form && 'error' in form && form.error}
            <p class="mt-3 text-sm text-danger">{form.error}</p>
        {/if}
        {#if form?.action === 'save' && 'saved' in form && form.saved}
            <p class="mt-3 text-sm text-success">Saved <code>{form.saved}</code>.</p>
        {/if}
        {#if form?.action === 'delete' && 'deleted' in form && form.deleted}
            <p class="mt-3 text-sm text-success">Deleted <code>{form.deleted}</code>.</p>
        {/if}
    </div>

    <!-- Existing codes -->
    {#if data.rows.length === 0 && !data.loadError}
        <p class="text-sm text-text-secondary">No codes yet. Add one above to get started.</p>
    {:else if data.rows.length > 0}
        <div class="overflow-x-auto">
            <table class="w-full text-left text-sm">
                <thead>
                    <tr class="border-b border-border-subtle text-text-tertiary">
                        <th class="pb-2 pr-4 font-medium">Code</th>
                        <th class="pb-2 pr-4 font-medium">Source</th>
                        <th class="pb-2 pr-4 font-medium">Medium</th>
                        <th class="pb-2 pr-4 font-medium">Note</th>
                        <th class="pb-2 text-right font-medium">Actions</th>
                    </tr>
                </thead>
                <tbody>
                    {#each data.rows as row (row.code)}
                        <tr class="border-b border-border-subtle/50">
                            <td class="py-2 pr-4 font-medium text-text-primary"><code>{row.code}</code></td>
                            <td class="py-2 pr-4 text-text-secondary">{row.utm_source}</td>
                            <td class="py-2 pr-4 text-text-secondary">{row.utm_medium || '–'}</td>
                            <td class="py-2 pr-4 text-text-tertiary">{row.note || '–'}</td>
                            <td class="py-2 text-right">
                                <div class="flex justify-end gap-2">
                                    <button
                                        type="button"
                                        onclick={() => startEdit(row)}
                                        class="rounded-md border border-border px-2.5 py-1 text-xs text-text-secondary
                                            transition-colors hover:text-text-primary"
                                    >
                                        Edit
                                    </button>
                                    <form
                                        method="POST"
                                        action="?/delete"
                                        use:enhance={() =>
                                            ({ update }) =>
                                                update({ reset: false })}
                                    >
                                        <input type="hidden" name="code" value={row.code} />
                                        <button
                                            type="submit"
                                            class="rounded-md border border-danger/40 px-2.5 py-1 text-xs text-danger
                                                transition-colors hover:bg-danger/10"
                                        >
                                            Delete
                                        </button>
                                    </form>
                                </div>
                            </td>
                        </tr>
                    {/each}
                </tbody>
            </table>
        </div>
    {/if}
</section>
