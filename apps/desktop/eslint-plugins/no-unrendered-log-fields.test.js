import { RuleTester } from 'eslint'
import * as svelteParser from 'svelte-eslint-parser'
import rule from './no-unrendered-log-fields.js'

const ruleTester = new RuleTester({
  languageOptions: { ecmaVersion: 'latest', sourceType: 'module' },
})

const svelteRuleTester = new RuleTester({
  languageOptions: { parser: svelteParser, ecmaVersion: 'latest', sourceType: 'module' },
})

ruleTester.run('no-unrendered-log-fields', rule, {
  valid: [
    // Every field appears as a placeholder: nothing is lost.
    `const log = getAppLogger('x')
     log.error('Trash refused it: {detail}', { detail })`,
    `const log = getAppLogger('x')
     log.info('Loaded {count} of {total}', { count, total })`,
    // No properties argument at all.
    `const log = getAppLogger('x')
     log.info('Started')`,
    // An empty properties object drops nothing.
    `const log = getAppLogger('x')
     log.info('Started', {})`,
    // LogTape trims whitespace inside a placeholder before looking the key up.
    `const log = getAppLogger('x')
     log.info('n={ count }', { count })`,
    // Nested access renders the root field, so `error` is not lost.
    `const log = getAppLogger('x')
     log.error('Boom: {error.message}', { error })`,
    `const log = getAppLogger('x')
     log.error('Boom: {error?.cause}', { error })`,
    `const log = getAppLogger('x')
     log.error('Boom: {items[0]}', { items })`,
    // `{*}` renders the whole properties bag, so nothing can be dropped.
    `const log = getAppLogger('x')
     log.error('Everything: {*}', { error, id })`,
    // A spread makes the key set unknowable; bail rather than guess.
    `const log = getAppLogger('x')
     log.info('x {a}', { a, ...rest })`,
    // A computed key is not statically knowable; skip it, keep checking the rest.
    `const log = getAppLogger('x')
     log.info('x {a}', { a, [k]: v })`,
    // Loggers under their other house names.
    `const logger = getAppLogger('x')
     logger.warn('Slow: {ms}ms', { ms })`,
    `const crashLog = getAppLogger('x')
     crashLog.error('Crashed: {reason}', { reason })`,
    // `console` is not a LogTape logger: its second argument really is printed.
    `console.error('Boom', { error })`,
    // An unrelated object that happens to have an `error` method.
    `emitter.error('Boom {a}', { a, b })`,
    // A logger-shaped call whose binding did not come from `getAppLogger`.
    `const log = makeSomethingElse('x')
     log.error('Boom {a}', { a, b })`,
    // A non-literal template can't be analyzed; bail.
    `const log = getAppLogger('x')
     log.info(buildTemplate(), { a, b })`,
    // Lazy properties that render everything they carry.
    `const log = getAppLogger('x')
     log.error('Boom: {detail}', () => ({ detail }))`,
  ],
  invalid: [
    // The ERR-BPAPM shape: the payload rides along and never reaches the log.
    {
      code: `const log = getAppLogger('x')
             log.error('{op} error: {errorType}', { op, errorType, error })`,
      errors: [{ messageId: 'unrenderedField', data: { field: 'error' } }],
    },
    // Every level drops properties, because the bridge forwards only the message.
    {
      code: `const log = getAppLogger('x')
             log.debug('Loaded', { id })`,
      errors: [{ messageId: 'unrenderedField', data: { field: 'id' } }],
    },
    {
      code: `const log = getAppLogger('x')
             log.info('Loaded', { id })`,
      errors: [{ messageId: 'unrenderedField', data: { field: 'id' } }],
    },
    {
      code: `const log = getAppLogger('x')
             log.warn('Slow', { ms })`,
      errors: [{ messageId: 'unrenderedField', data: { field: 'ms' } }],
    },
    // One report per dropped field.
    {
      code: `const log = getAppLogger('x')
             log.error('Boom {a}', { a, b, c })`,
      errors: [
        { messageId: 'unrenderedField', data: { field: 'b' } },
        { messageId: 'unrenderedField', data: { field: 'c' } },
      ],
    },
    // `{{` escapes a literal brace, so this template renders NO placeholder.
    {
      code: `const log = getAppLogger('x')
             log.info('{{a}}', { a })`,
      errors: [{ messageId: 'unrenderedField', data: { field: 'a' } }],
    },
    // An unterminated brace is literal text, not a placeholder.
    {
      code: `const log = getAppLogger('x')
             log.info('what {a', { a })`,
      errors: [{ messageId: 'unrenderedField', data: { field: 'a' } }],
    },
    // A renamed property is dropped even though its value's name is rendered.
    {
      code: `const log = getAppLogger('x')
             log.error("Couldn't read it: {reason}", { reason: e.message, error: e })`,
      errors: [{ messageId: 'unrenderedField', data: { field: 'error' } }],
    },
    // Lazy properties are checked the same way.
    {
      code: `const log = getAppLogger('x')
             log.error('Boom', () => ({ detail }))`,
      errors: [{ messageId: 'unrenderedField', data: { field: 'detail' } }],
    },
    // Double-quoted templates are no different.
    {
      code: `const logger = getAppLogger('x')
             logger.error("Couldn't push config: {error}", { error: e, config })`,
      errors: [{ messageId: 'unrenderedField', data: { field: 'config' } }],
    },
  ],
})

svelteRuleTester.run('no-unrendered-log-fields (svelte)', rule, {
  valid: [
    {
      code: `<script lang="ts">
               const log = getAppLogger('x')
               function go() { log.error('Boom: {error}', { error }) }
             </script>`,
      filename: 'src/lib/whatever/Thing.svelte',
    },
  ],
  invalid: [
    {
      code: `<script lang="ts">
               const log = getAppLogger('x')
               function go() { log.error('Boom', { error }) }
             </script>`,
      filename: 'src/lib/whatever/Thing.svelte',
      errors: [{ messageId: 'unrenderedField', data: { field: 'error' } }],
    },
  ],
})
