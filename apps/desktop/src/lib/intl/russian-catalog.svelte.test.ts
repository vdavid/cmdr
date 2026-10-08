/** Integration coverage for the shipped Russian catalog and its count-sensitive grammar. */
import { afterEach, describe, expect, it } from 'vitest'
import {
  _clearCompiledCacheForTests,
  availableLocales,
  getMessage,
  resolvedCatalogLocale,
  setLocale,
  tString,
} from './messages.svelte'
import { _setLocaleForTests } from './locale'

/** Restore the shared locale, compiled messages, and document language after each test. */
afterEach(() => {
  setLocale(null)
  _setLocaleForTests(null)
  _clearCompiledCacheForTests()
  document.documentElement.lang = 'en'
})

/** Exercise production catalog discovery and formatting without injecting a test catalog. */
describe('the shipped Russian catalog', () => {
  /** A regional OS language must discover Russian and localize both HTML and native menus. */
  it('resolves ru-RU to Russian and exposes translated raw menu labels', () => {
    setLocale('ru-RU')
    expect(availableLocales()).toContain('ru')
    expect(resolvedCatalogLocale('ru-RU')).toBe('ru')
    expect(document.documentElement.lang).toBe('ru')
    expect(getMessage('menu.bar.file')).toBe('Файл')
    expect(getMessage('menu.bar.help')).toBe('Справка')
  })

  /** Russian one includes 21 and 101; every branch must preserve the displayed count. */
  it('renders the real file counter across Russian plural boundaries', () => {
    setLocale('ru')
    // Pin the visible result, including both the number and its grammatical ending.
    const cases: [number, string][] = [
      [1, 'файл'],
      [2, 'файла'],
      [5, 'файлов'],
      [11, 'файлов'],
      [21, 'файл'],
      [22, 'файла'],
      [25, 'файлов'],
      [101, 'файл'],
    ]
    for (const [count, noun] of cases) {
      expect(tString('transfer.delete', { count, countText: String(count) })).toBe(
        `Удаление завершено: ${String(count)} ${noun}`,
      )
    }
  })

  /** Only exactly one operation may use the short singular wording. */
  it('distinguishes exactly one operation from 21 operations', () => {
    setLocale('ru')
    expect(tString('main.quit.title', { count: 1, countText: '1' })).toBe(
      'Завершить работу, пока выполняется операция?',
    )
    expect(tString('main.quit.title', { count: 21, countText: '21' })).toBe(
      'Завершить работу, пока выполняется 21 операция?',
    )
  })

  /** Selection totals after “из” need genitive rather than a nominative count noun. */
  it('uses genitive in a selection total ending in 21', () => {
    setLocale('ru')
    expect(
      tString('fileExplorer.selectionInfo.noSelectionOfMatches', { count: 21, shownText: '3', totalText: '21' }),
    ).toBe('Ничего не выделено, 3 из 21 совпадения.')
    expect(tString('fileExplorer.summary.fileNoun', { count: 21 })).toBe('файла')
  })
})
