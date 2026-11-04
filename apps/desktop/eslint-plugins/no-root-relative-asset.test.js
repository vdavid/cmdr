import { RuleTester } from 'eslint'
import * as svelteParser from 'svelte-eslint-parser'
import rule from './no-root-relative-asset.js'

// The Svelte parser handles plain script too, so one tester covers `.ts`-shaped code and markup.
const ruleTester = new RuleTester({
  languageOptions: { parser: svelteParser, ecmaVersion: 'latest', sourceType: 'module' },
})

const options = [{ staticEntries: ['icons', 'logo.svg', 'favicon.png'] }]
const svelte = 'src/lib/whatever/Row.svelte'
const err = (path) => ({ messageId: 'rootRelativeAsset', data: { path, fixed: path.slice(1) } })

ruleTester.run('no-root-relative-asset', rule, {
  valid: [
    // The intended path.
    { code: `<img src={asset('icons/network-no-hosts.svg')} alt="" />`, filename: svelte, options },
    { code: `<script>const p = asset('icons/sync-synced.svg')</script>`, filename: svelte, options },
    { code: '<script>const p = asset(`icons/${id}.svg`)</script>', filename: svelte, options },
    // Routes and filesystem paths aren't `static/` entries.
    { code: `<script>goto('/settings'); const p = '/Users/me/icon.png'</script>`, filename: svelte, options },
    { code: `<a href="/settings">x</a>`, filename: svelte, options },
    // A look-alike first segment is a different entry.
    { code: `<script>const p = '/icons-cache/x.png'</script>`, filename: svelte, options },
    // Protocol-relative and absolute URLs are someone else's host.
    { code: `<img src="//cdn.example.com/icons/x.svg" alt="" />`, filename: svelte, options },
    { code: `<img src="https://getcmdr.com/logo.svg" alt="" />`, filename: svelte, options },
    // A computed path names no asset on its own.
    { code: `<script>const p = '/' + dir</script>`, filename: svelte, options },
    // A CSS url() with a relative or data URL.
    { code: `<style>.a { background: url(data:image/png;base64,AA) }</style>`, filename: svelte, options },
  ],
  invalid: [
    {
      code: `<img src="/icons/network-no-hosts.svg" alt="" />`,
      filename: svelte,
      options,
      errors: [err('/icons/network-no-hosts.svg')],
    },
    { code: `<img src="/logo.svg" alt="" />`, filename: svelte, options, errors: [err('/logo.svg')] },
    {
      code: `<script>const p = { src: '/icons/mobile-device.svg' }</script>`,
      filename: svelte,
      options,
      errors: [err('/icons/mobile-device.svg')],
    },
    { code: `<img src={'/favicon.png'} alt="" />`, filename: svelte, options, errors: [err('/favicon.png')] },
    {
      code: '<script>const p = `/icons/${id}.svg`</script>',
      filename: svelte,
      options,
      errors: [err('/icons/')],
    },
    {
      code: `<style>.a { background: url('/icons/x.svg') } .b { mask: url(/logo.svg) }</style>`,
      filename: svelte,
      options,
      errors: [err('/icons/x.svg'), err('/logo.svg')],
    },
  ],
})
