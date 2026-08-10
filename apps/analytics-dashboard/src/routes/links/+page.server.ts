import type { PageServerLoad, Actions } from './$types'
import { fail } from '@sveltejs/kit'
import { resolveEnv } from '$lib/server/fetch-all.js'
import { fetchLinkCodes, upsertLinkCode, deleteLinkCode } from '$lib/server/sources/link-codes.js'
import { validateLinkCode, toRows } from '$lib/link-codes.js'

export type { LinkCodeRow } from '$lib/link-codes.js'

/**
 * Loads the live `?r=` code map from the api-server admin endpoint, flattened to sorted rows for the
 * table. The admin token resolves server-side (`resolveEnv`) and never reaches the browser; the page
 * gets only the rows and a load error string. No caching: this view must reflect the live KV.
 */
export const load: PageServerLoad = async ({ platform }) => {
  const env = await resolveEnv(platform)
  const result = await fetchLinkCodes(env)
  return {
    rows: result.ok ? toRows(result.data) : [],
    loadError: result.ok ? null : result.error,
  }
}

/**
 * Reads one text field off a submitted form. A `FormData` entry can also be a `File` (any multipart
 * upload), and stringifying one gives `[object Object]`, so a non-string entry counts as absent and
 * falls through to the same validation an empty field gets.
 */
function textField(form: FormData, name: string): string {
  const value = form.get(name)
  return typeof value === 'string' ? value : ''
}

/**
 * Form actions for the page. Both validate + normalize on the server (the source of truth is the
 * api-server, which re-validates), then proxy to the admin CRUD with the server-only bearer token.
 * On a validation or proxy failure they return `fail(...)` with the offending values so the form
 * repopulates and shows the error inline.
 */
export const actions: Actions = {
  save: async ({ request, platform }) => {
    const form = await request.formData()
    const raw = {
      code: textField(form, 'code'),
      utm_source: textField(form, 'utm_source'),
      utm_medium: textField(form, 'utm_medium'),
      note: textField(form, 'note'),
    }

    const validated = validateLinkCode(raw)
    if (!validated.ok) {
      return fail(400, { action: 'save', ...raw, error: validated.error })
    }

    const env = await resolveEnv(platform)
    const result = await upsertLinkCode(env, {
      code: validated.code,
      utm_source: validated.utm_source,
      utm_medium: validated.utm_medium,
      note: validated.note,
    })
    if (!result.ok) {
      return fail(502, { action: 'save', ...raw, error: `Couldn't save: ${result.error}` })
    }
    return { action: 'save', saved: validated.code }
  },

  delete: async ({ request, platform }) => {
    const form = await request.formData()
    const code = textField(form, 'code').trim()
    if (!code) {
      return fail(400, { action: 'delete', error: 'No code to delete.' })
    }

    const env = await resolveEnv(platform)
    const result = await deleteLinkCode(env, code)
    if (!result.ok) {
      return fail(502, { action: 'delete', code, error: `Couldn't delete: ${result.error}` })
    }
    return { action: 'delete', deleted: code }
  },
}
