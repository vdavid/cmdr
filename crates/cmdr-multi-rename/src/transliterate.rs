//! Greek to Latin, for file names: ELOT 743 (the Greek standard, close to
//! ISO 843 type 2), the transliteration Greek passports and road signs use.
//!
//! Letter by letter with the few digraphs that read differently together:
//! `ου` → `ou`; `αυ` / `ευ` / `ηυ` → `av` / `ev` / `iv` before a vowel or a
//! voiced consonant and `af` / `ef` / `if` otherwise; `γγ` → `ng`, `γξ` → `nx`,
//! `γχ` → `nch`. Accents (tonos) go; a dialytika (`ϊ`, `ϋ`) keeps the vowel on
//! its own, so `αϋ` is `ay`, not a diphthong. Capitals map to capitalized
//! output (`Θ` → `Th`), or all caps when the next letter is a capital too
//! (`ΘΕΣΗ` → `THESI`). Anything that isn't Greek passes through untouched.

use unicode_normalization::UnicodeNormalization;

const DIALYTIKA: char = '\u{0308}';

/// `text` with every Greek letter written in Latin letters.
pub fn greek_to_latin(text: &str) -> String {
    if !text.chars().any(is_greek) {
        return text.to_string();
    }
    // One Greek letter at a time: its base and whether it carried a dialytika.
    let letters: Vec<(char, bool)> = text
        .chars()
        .map(|c| {
            if !is_greek(c) {
                return (c, false);
            }
            let decomposed: Vec<char> = c.to_string().nfd().collect();
            (decomposed[0], decomposed.contains(&DIALYTIKA))
        })
        .collect();
    let mut out = String::with_capacity(text.len() + 8);
    let mut i = 0;
    while i < letters.len() {
        let (c, _) = letters[i];
        if !is_greek(c) {
            out.push(c);
            i += 1;
            continue;
        }
        let next = letters.get(i + 1).copied();
        let after = letters.get(i + 2).map(|(c, _)| *c);
        let (latin, used) = digraph(c, next, after).unwrap_or_else(|| (single(c).to_string(), 1));
        let upper = c.is_uppercase();
        let next_upper = letters
            .get(i + used)
            .is_some_and(|(n, _)| n.is_uppercase() && n.is_alphabetic());
        out.push_str(&cased(&latin, upper, next_upper));
        i += used;
    }
    out
}

/// A pair (or the `υ` diphthongs) that reads as one sound, and how many letters it used.
fn digraph(c: char, next: Option<(char, bool)>, after: Option<char>) -> Option<(String, usize)> {
    let (n, n_dialytika) = next?;
    let (lc, ln) = (lower(c), lower(n));
    // A dialytika on the second vowel splits the pair.
    if ln == 'υ' && !n_dialytika {
        match lc {
            'ο' => return Some(("ou".into(), 2)),
            'α' | 'ε' | 'η' => {
                let base = match lc {
                    'α' => 'a',
                    'ε' => 'e',
                    _ => 'i',
                };
                let voiced = after.is_some_and(|a| is_vowel(lower(a)) || "βγδζλμνρ".contains(lower(a)));
                let tail = if voiced { 'v' } else { 'f' };
                return Some((format!("{base}{tail}"), 2));
            }
            _ => {}
        }
    }
    if lc == 'γ' {
        return match ln {
            'γ' => Some(("ng".into(), 2)),
            'ξ' => Some(("nx".into(), 2)),
            'χ' => Some(("nch".into(), 2)),
            _ => None,
        };
    }
    None
}

fn single(c: char) -> &'static str {
    match lower(c) {
        'α' => "a",
        'β' => "v",
        'γ' => "g",
        'δ' => "d",
        'ε' => "e",
        'ζ' => "z",
        'η' => "i",
        'θ' => "th",
        'ι' => "i",
        'κ' => "k",
        'λ' => "l",
        'μ' => "m",
        'ν' => "n",
        'ξ' => "x",
        'ο' => "o",
        'π' => "p",
        'ρ' => "r",
        'σ' | 'ς' => "s",
        'τ' => "t",
        'υ' => "y",
        'φ' => "f",
        'χ' => "ch",
        'ψ' => "ps",
        'ω' => "o",
        _ => "",
    }
}

fn cased(latin: &str, upper: bool, next_upper: bool) -> String {
    if !upper {
        return latin.to_string();
    }
    if next_upper || latin.chars().count() == 1 {
        return latin.to_uppercase();
    }
    let mut chars = latin.chars();
    chars
        .next()
        .map(|first| first.to_uppercase().chain(chars).collect())
        .unwrap_or_default()
}

fn lower(c: char) -> char {
    c.to_lowercase().next().unwrap_or(c)
}

fn is_vowel(c: char) -> bool {
    "αεηιουω".contains(c)
}

/// Greek and Coptic letters plus Greek Extended (polytonic), letters only.
fn is_greek(c: char) -> bool {
    matches!(c as u32, 0x0386..=0x03CE | 0x1F00..=0x1FFF) && c.is_alphabetic()
}
