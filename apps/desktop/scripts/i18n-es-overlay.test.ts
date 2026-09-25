/**
 * Drift guard for the Latin American Spanish overlay, `es-419` over `es`.
 *
 * `es` is Spain Spanish, and `es-419` forks only the keys where Latin America
 * reads differently. Coverage, parity, and stale all compare a fork against the
 * `es` value it overrides, so none of them notices the failure that matters here:
 * a NEW `es` string (or an edited one) using a Spain-only form that `es-419` never
 * forked, which a Mexican or Argentine reader then sees verbatim. This sweeps every
 * `es` value for each Spain-only form the overlay replaces and fails on any key
 * the overlay skipped, plus any Spain-only form left inside a forked value.
 *
 * The fix for a failure is almost always "fork the key in `es-419`"; the forms and
 * their evidence are in `docs/i18n/es-419/style.md`.
 */
import { describe, expect, it } from 'vitest'

import { isRawKey, loadCatalog, visibleLiterals } from './i18n-catalog-lib.ts'

const es = loadCatalog('es').messages
const la = loadCatalog('es-419').messages

/** The text a reader sees: no placeholder, tag, or plural/select identifiers. */
function visibleTextOf(key: string, value: string): string {
  if (isRawKey(key)) return value.replaceAll(/\{[^{}]*\}/g, ' ')
  return visibleLiterals(value, 'es') ?? value.replaceAll(/\{[^{}]*\}/g, ' ')
}

/**
 * Every Spain-only form `es-419` replaces, as it appears in `es` (`u` flag, so
 * `\p{L}` bounds a word even next to an accented letter). `except` lists keys where
 * the same letters mean something that doesn't fork, with the reason.
 */
const SPAIN_ONLY: readonly { term: string; pattern: RegExp; except?: readonly string[] }[] = [
  { term: 'papelera → basurero', pattern: /(?<!\p{L})papelera/iu },
  { term: 'añadir → agregar', pattern: /(?<!\p{L})añ[aá]d/iu },
  {
    term: 'Ajustes → Configuración',
    pattern: /(?<!\p{L})ajustes?(?!\p{L})/iu,
    // `Ajuste de línea` is word wrap, not a setting.
    except: [
      'menu.viewer.wordWrap',
      'settings.viewer.wordWrap.label',
      'viewer.statusBar.badge.wrap',
      'viewer.statusBar.hint.text',
    ],
  },
  { term: 'pulsar → presionar', pattern: /(?<!\p{L})p[uú]ls(?!\p{L}*ión)/iu },
  { term: 'pulsación → tecla presionada', pattern: /(?<!\p{L})pulsación/iu },
  { term: 'ordenador → computadora', pattern: /(?<!\p{L})ordenador/iu },
  { term: 'coste → costo', pattern: /(?<!\p{L})costes?(?!\p{L})/iu },
  { term: 'merece la pena → vale la pena', pattern: /merezca la pena|merece la pena/iu },
  { term: 'por omisión → predeterminado', pattern: /por omisión/iu },
  { term: 'copia de seguridad → respaldo', pattern: /copias? de seguridad/iu },
  { term: 'gestionar → administrar', pattern: /(?<!\p{L})gesti[oó]n|(?<!\p{L})gestor/iu },
  { term: 'icono → ícono', pattern: /(?<!\p{L})iconos?(?!\p{L})/iu },
  // The verb only: `Introducción` is Cmdr's name for onboarding and doesn't fork.
  { term: 'introducir → ingresar', pattern: /(?<!\p{L})introd[uú](?!cci)/iu },
  { term: 'informe → reporte', pattern: /(?<!\p{L})informes?(?!\p{L})/iu },
  { term: 'caducar → vencer', pattern: /(?<!\p{L})caduc/iu },
  { term: 'Acceso total al disco → Acceso completo al disco', pattern: /acceso total al disco/iu },
  { term: 'sitio → lugar', pattern: /(?<!\p{L})sitios?(?! web)(?!\p{L})/iu },
  { term: 'vosotros → ustedes', pattern: /(?<!\p{L})(vosotr|vuestr)/iu },
  // A finished action takes the preterite: the loudest compound-perfect frames.
  { term: 'no se ha podido → no se pudo', pattern: /ha(?:n)? podido|ha ido mal/iu },
]

describe('es-419 forks every Spain-only form in es', () => {
  for (const { term, pattern, except = [] } of SPAIN_ONLY) {
    it(term, () => {
      const unforked: string[] = []
      const leftover: string[] = []
      for (const [key, value] of Object.entries(es)) {
        if (except.includes(key) || !pattern.test(visibleTextOf(key, value))) continue
        if (!(key in la)) unforked.push(key)
        else if (pattern.test(visibleTextOf(key, la[key]))) leftover.push(key)
      }
      expect(unforked, `es keys using "${term.split(' → ')[0]}" that es-419 doesn't fork`).toEqual([])
      expect(leftover, `es-419 forks still using "${term.split(' → ')[0]}"`).toEqual([])
    })
  }
})
