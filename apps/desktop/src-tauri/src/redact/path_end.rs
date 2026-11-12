//! Where a path found in prose really ends. The path branches over-match on purpose (spaces are legal in
//! labels and filenames), and this gives back only what a hard delimiter proves isn't path. `DETAILS.md` §
//! "Finding the end of a path".

use super::names::ends_with_unicode_escape;
use super::tokens::is_redacted_segment;

/// Split a greedy path capture into (path, trailing_noise). The regex matches spaces inside
/// paths so both multi-word labels (`/Volumes/My Backup Drive/...`) and multi-word filenames
/// (`Invoice for Acme Corp.pdf`) land whole; it also sweeps up whatever prose follows the path.
///
/// ❗ **Greedy on purpose: the path runs to the next HARD delimiter.** Nothing local tells a
/// name's own words from the prose after it, and every rule that tried (a sentence-ending
/// `)`, a lowercase word, a dotted word) shipped part of a name: `Summer (Lake House) 2026`
/// lost ` 2026` at the `)`. So prose after a path is redacted with it, and only these end it:
/// - the `{path}: {message}` seam, anywhere. macOS forbids `:` in a filename.
/// - a later word that starts a new reference (`… and https://…`, or the `smb:` the regex
///   stopped at before `//`), so the scanner gets to claim that reference.
/// - `, `, `; `, or ` key=` after the LAST separator. Before it, a deeper segment proves the
///   path goes on (`Smith, Test_user_14/x.pdf`), and cutting there would ship the rest.
/// - a ` -> ` arrow, anywhere (`{from} -> {to}`), and the ` -` the regex stopped at before a
///   Windows path's `>`.
/// - a word that's already a token (`…/<dir:…> in 3ms`): this scanner's own output, met again
///   on a second pass. A segment the token leads ends after it; a token after a name ends the
///   path before it.
///
/// Then only characters that carry no name go back: `:<digits>` line markers, and trailing
/// `.` `,` `;` `!` `?` or an UNBALANCED closing bracket (`(under /x/y): …`). `(` and `)` never
/// end a path.
pub(super) fn split_trailing_noise(s: &str) -> (&str, &str) {
    let mut end = s.find(": ").unwrap_or(s.len());
    if let Some(arrow) = s[..end].find(" ->") {
        end = arrow;
    }
    end = new_reference_start(s, end);

    let floor = s[..end].rfind(separator_of(s)).map_or(0, |i| i + 1);
    if let Some(at) = first_delimiter(&s[floor..end]) {
        end = floor + at;
    }
    end = end_at_token_word(s, end);

    end = trim_line_markers(s, end);
    if s[..end].ends_with(" -") {
        end -= 2;
    }
    end = trim_closing_punctuation(s, end);

    // Every cut above lands next to an ASCII byte, so `end` is a char boundary.
    (&s[..end], &s[end..])
}

/// Where the first `, `, `; `, or ` key=` in `tail` sits, if any.
fn first_delimiter(tail: &str) -> Option<usize> {
    let punctuation = [", ", "; "].iter().filter_map(|d| tail.find(d)).min();
    let field = tail
        .match_indices(' ')
        .find(|(i, _)| starts_with_field_key(&tail[i + 1..]))
        .map(|(i, _)| i);
    punctuation.into_iter().chain(field).min()
}

/// `ident=` at the start of `s`.
pub(super) fn starts_with_field_key(s: &str) -> bool {
    let ident_len = s
        .char_indices()
        .take_while(|&(i, c)| c == '_' || c.is_ascii_alphabetic() || (i > 0 && c.is_ascii_digit()))
        .count();
    ident_len > 0 && s[ident_len..].starts_with('=')
}

/// Where a token word ends the path: after it when it leads a segment (`<dir:…> in 3ms`,
/// `<dir:…> $HOME/…`), before its space when it follows a name. Only `<`-led words are looked at,
/// and only an exact token shape counts, so `Secret <draft> Plans` stays one name.
fn end_at_token_word(s: &str, end: usize) -> usize {
    let sep = separator_of(s);
    let mut seg_start = 0usize;
    for seg in s[..end].split(sep) {
        let mut offset = seg_start;
        for (i, word) in seg.split(' ').enumerate() {
            if word.starts_with('<') && is_redacted_segment(word) {
                if i > 0 {
                    return offset - 1;
                }
                if word.len() < seg.len() {
                    return offset + word.len();
                }
            }
            offset += word.len() + 1;
        }
        seg_start += seg.len() + sep.len_utf8();
    }
    end
}

/// The start of the first word after the first that opens a new reference, minus its space:
/// one containing `://` (`https://u:p@a.com and https://x:y@b.com`), or a trailing `scheme:`
/// the match stopped at because `//` followed (`/Volumes/d/f.txt and smb://host/…`). Handing it
/// back is what lets the scanner claim the reference; swallowed, its tail had no pattern left.
fn new_reference_start(s: &str, end: usize) -> usize {
    let mut offset = 0usize;
    for (i, word) in s[..end].split(' ').enumerate() {
        let word_end = offset + word.len();
        let trailing_scheme = word_end == s.len() && is_scheme_colon(word);
        if i > 0 && (word.contains("://") || trailing_scheme) {
            return offset - 1;
        }
        offset = word_end + 1;
    }
    end
}

/// `smb:`, `https:`: a URL scheme and its colon.
fn is_scheme_colon(word: &str) -> bool {
    word.strip_suffix(':').is_some_and(|scheme| {
        scheme.starts_with(|c: char| c.is_ascii_alphabetic())
            && scheme
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.'))
    })
}

/// Step `end` back over `:<digits>` groups (`:42`, `:42:5`, a source location).
fn trim_line_markers(s: &str, mut end: usize) -> usize {
    let bytes = s.as_bytes();
    loop {
        let mut i = end;
        while i > 0 && bytes[i - 1].is_ascii_digit() {
            i -= 1;
        }
        if i < end && i > 0 && bytes[i - 1] == b':' {
            end = i - 1;
        } else {
            return end;
        }
    }
}

/// Step `end` back over trailing sentence punctuation (`.` `,` `;` `!` `?`) and closing
/// brackets the path didn't open, one at a time. None of them carries a name. A balanced `)`
/// stays (`Summer (Lake House)`), and so does the `}` closing a `\u{301}` escape.
fn trim_closing_punctuation(s: &str, mut end: usize) -> usize {
    let bytes = s.as_bytes();
    while end > 0 {
        let path = &s[..end];
        let unbalanced = |open: char, close: char| path.matches(close).count() > path.matches(open).count();
        let trim = match bytes[end - 1] {
            b'.' | b',' | b';' | b'!' | b'?' => true,
            b')' => unbalanced('(', ')'),
            b']' => unbalanced('[', ']'),
            b'}' => !ends_with_unicode_escape(path) && unbalanced('{', '}'),
            b'>' => unbalanced('<', '>'),
            _ => false,
        };
        if !trim {
            break;
        }
        end -= 1;
    }
    end
}

/// The character a path's segments are separated by: `\` for a Windows home (`C:\Users\…`) or
/// a UNC path (`\\host\share\…`), `/` for everything else. On a `/` path a `\` is a `{:?}`
/// escape, never a separator.
fn separator_of(s: &str) -> char {
    let bytes = s.as_bytes();
    let windows_drive = bytes.len() >= 3 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':' && bytes[2] == b'\\';
    if windows_drive || s.starts_with("\\\\") {
        '\\'
    } else {
        '/'
    }
}
