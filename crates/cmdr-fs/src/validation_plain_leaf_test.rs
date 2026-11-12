//! `validate_plain_leaf`: a name somebody else chose must read as exactly what
//! lands.

use super::{MAX_NAME_BYTES, PlainLeafRefusal, validate_plain_leaf};

#[test]
fn ordinary_names_pass() {
    for name in [
        "2026-09-27 OpenAI invoice Testco.pdf",
        ".gitignore",
        "...",
        "Résumé.pdf",
        "Re\u{301}sume\u{301}.pdf",
        "日本語のファイル",
        "a b",
        &"n".repeat(MAX_NAME_BYTES),
    ] {
        assert_eq!(validate_plain_leaf(name), Ok(()), "{name:?}");
    }
}

#[test]
fn an_empty_or_blank_name_refuses() {
    for name in ["", " ", "\u{3000}"] {
        assert_eq!(validate_plain_leaf(name), Err(PlainLeafRefusal::Empty), "{name:?}");
    }
}

#[test]
fn edge_whitespace_refuses() {
    for name in [" a.pdf", "a.pdf ", "a.pdf\u{A0}", "\u{2003}a.pdf"] {
        assert_eq!(
            validate_plain_leaf(name),
            Err(PlainLeafRefusal::EdgeWhitespace),
            "{name:?}"
        );
    }
}

#[test]
fn a_path_or_a_dot_name_refuses() {
    assert_eq!(validate_plain_leaf("a/b.pdf"), Err(PlainLeafRefusal::Separator));
    assert_eq!(validate_plain_leaf("/abs"), Err(PlainLeafRefusal::Separator));
    assert_eq!(validate_plain_leaf("."), Err(PlainLeafRefusal::DotName));
    assert_eq!(validate_plain_leaf(".."), Err(PlainLeafRefusal::DotName));
}

#[test]
fn a_name_past_the_byte_limit_refuses() {
    let name = "n".repeat(MAX_NAME_BYTES + 1);
    assert_eq!(
        validate_plain_leaf(&name),
        Err(PlainLeafRefusal::TooLong {
            bytes: MAX_NAME_BYTES + 1
        })
    );
}

#[test]
fn a_control_character_refuses_nul_included() {
    for ch in ['\0', '\n', '\t', '\r', '\u{7F}', '\u{1B}', '\u{85}'] {
        let name = format!("in{ch}voice.pdf");
        assert_eq!(validate_plain_leaf(&name), Err(PlainLeafRefusal::Control(ch)), "{ch:?}");
    }
}

#[test]
fn an_invisible_format_character_refuses() {
    let invisible = [
        '\u{202A}', '\u{202B}', '\u{202C}', '\u{202D}', '\u{202E}', // bidi embeddings and overrides
        '\u{2066}', '\u{2067}', '\u{2068}', '\u{2069}', // bidi isolates
        '\u{200E}', '\u{200F}', '\u{061C}', // marks
        '\u{200B}', '\u{200C}', '\u{200D}', '\u{2060}', // zero-width
        '\u{FEFF}', '\u{00AD}', // byte-order mark, soft hyphen
    ];
    for ch in invisible {
        let name = format!("invoice{ch}fdp.exe");
        assert_eq!(
            validate_plain_leaf(&name),
            Err(PlainLeafRefusal::Invisible(ch)),
            "U+{:04X}",
            ch as u32
        );
    }
}

#[test]
fn the_right_to_left_override_spoof_refuses() {
    // Renders as `invoiceexe.pdf`.
    assert_eq!(
        validate_plain_leaf("invoice\u{202E}fdp.exe"),
        Err(PlainLeafRefusal::Invisible('\u{202E}'))
    );
}
