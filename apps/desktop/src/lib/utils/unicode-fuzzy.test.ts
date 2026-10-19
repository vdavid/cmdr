import { describe, expect, it } from 'vitest'
import { foldForSearch, fuzzySearch } from './unicode-fuzzy'

/** The haystack indices uFuzzy returned, in rank order (empty when nothing matched). */
function hits(haystack: string[], query: string): number[] {
  const [idxs, , order] = fuzzySearch(haystack.map(foldForSearch), query)
  if (!idxs) return []
  return order ? order.map((i) => idxs[i]) : idxs
}

describe('foldForSearch', () => {
  it('strips accents from Latin letters', () => {
    expect(foldForSearch('téma')).toBe('tema')
    expect(foldForSearch('Nyelv és régió: ő, ű')).toBe('nyelv es regio: o, u')
    expect(foldForSearch('språk')).toBe('sprak')
  })

  it('strips stacked Vietnamese diacritics', () => {
    expect(foldForSearch('Tiếng Việt')).toBe('tieng viet')
    expect(foldForSearch('ngôn ngữ')).toBe('ngon ngu')
  })

  it('folds Cyrillic and Greek marks', () => {
    expect(foldForSearch('Ёлка')).toBe('елка')
    expect(foldForSearch('άλφα')).toBe('αλφα')
  })

  it('maps letters that are not a base plus a mark', () => {
    expect(foldForSearch('Øresund łódź đường Æsir')).toBe('oresund lodz duong asir')
  })

  it('keeps ß as is, because ss would shift every offset after it', () => {
    expect(foldForSearch('Straße')).toBe('straße')
  })

  it('leaves Hangul and kana alone (NFD would split or strip them)', () => {
    // A Hangul syllable decomposes into two or three jamo (not marks), and `が` would lose its
    // dakuten and become `か`. Neither is accent folding, so both fold to themselves.
    expect(foldForSearch('언어 설정')).toBe('언어 설정')
    expect(foldForSearch('がぎぐ ガギグ')).toBe('がぎぐ ガギグ')
  })

  it('leaves Han characters alone', () => {
    expect(foldForSearch('语言 語言')).toBe('语言 語言')
  })

  it('is one code unit in, one code unit out, so match offsets stay valid', () => {
    const samples = [
      'Tiếng Việt',
      'İstanbul', // `İ`.toLowerCase() is two code units
      'Straße',
      '언어',
      'がぎぐ',
      'emoji 📁 folder', // an astral char is two code units in, two out
      'é', // an already-decomposed accent: the lone mark stays a mark
    ]
    for (const sample of samples) {
      expect(foldForSearch(sample).length).toBe(sample.length)
    }
  })
})

describe('fuzzySearch', () => {
  it('finds Cyrillic words', () => {
    expect(hits(['Оформление › Язык', 'Тема'], 'язык')).toEqual([0])
  })

  it('finds Han words', () => {
    expect(hits(['外观 › 语言', '主题'], '语言')).toEqual([0])
    expect(hits(['外觀 › 語言', '主題'], '語言')).toEqual([0])
  })

  it('matches across accents in both directions', () => {
    expect(hits(['Téma', 'Nyelv'], 'tema')).toEqual([0])
    expect(hits(['Tema', 'Nyelv'], 'téma')).toEqual([0])
    expect(hits(['Ngôn ngữ', 'Giao diện'], 'ngon ngu')).toEqual([0])
  })

  it('keeps typo tolerance for alphabetic scripts', () => {
    expect(hits(['Appearance language'], 'langauge')).toEqual([0])
  })

  it('allows no typo in a CJK query', () => {
    // Under single-error mode a five-character term may differ in one place, which in Chinese
    // is a different word: `使用的语X` would match `使用的语言`.
    const haystack = ['选择 cmdr 使用的语言']
    expect(hits(haystack, '使用的语言')).toEqual([0])
    expect(hits(haystack, '使用的语文')).toEqual([])
    expect(hits(haystack, '语X')).toEqual([])
  })

  it('reports ranges that index the unfolded text', () => {
    const text = 'Giao diện › Ngôn ngữ'
    const [idxs, info] = fuzzySearch([foldForSearch(text)], 'ngon')
    expect(idxs).toEqual([0])
    const [start, end] = info?.ranges[0] ?? []
    expect(text.slice(start, end)).toBe('Ngôn')
  })
})
