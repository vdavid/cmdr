//! The CLIP byte-pair text tokenizer: a query string → the int32 token-id sequence the
//! CLIP text tower consumes.
//!
//! Wraps [`instant_clip_tokenizer`] (bundles the OpenAI CLIP BPE vocab). The tower's
//! input is a fixed `[1, 77]` int32 tensor: `[<start_of_text>] content [<end_of_text>]`
//! padded with `<end_of_text>` to [`CONTEXT_LENGTH`], exactly what HuggingFace's
//! `CLIPTokenizer` produces (padding with the end-of-text token, not zeros). The padding
//! value doesn't change the embedding — CLIP pools at the FIRST end-of-text position
//! under a causal mask, so tokens after it never affect the result — but matching HF's
//! sequence keeps the tokenization reference exact and unambiguous
//! (`apps/desktop/scripts/convert-clip-model/reference-tokenization.json`).

use std::sync::LazyLock;

use instant_clip_tokenizer::Tokenizer;

/// The CLIP text context length. The tower's input is `[1, CONTEXT_LENGTH]` int32.
pub const CONTEXT_LENGTH: usize = 77;

/// The process-wide tokenizer. [`Tokenizer::new`] parses the bundled BPE vocab once; the
/// result is immutable, so one shared instance serves every query.
static TOKENIZER: LazyLock<Tokenizer> = LazyLock::new(Tokenizer::new);

/// Tokenize `text` into the fixed-length int32 id sequence the CLIP text tower expects:
/// `[<start_of_text>] content… [<end_of_text>]` truncated to fit and padded with
/// `<end_of_text>` up to [`CONTEXT_LENGTH`]. Lowercased before encoding (CLIP is
/// case-insensitive).
pub fn tokenize(text: &str) -> Vec<i32> {
    tokenize_with(&TOKENIZER, text)
}

/// [`tokenize`] against a specific tokenizer instance (so tests can use the same default
/// vocab without depending on the global).
fn tokenize_with(tokenizer: &Tokenizer, text: &str) -> Vec<i32> {
    let eot = tokenizer.end_of_text();
    let mut tokens = vec![tokenizer.start_of_text()];
    tokenizer.encode(text, &mut tokens);
    // Leave room for the end-of-text marker, then append it (matching `tokenize_batch`).
    tokens.truncate(CONTEXT_LENGTH - 1);
    tokens.push(eot);
    let mut ids: Vec<i32> = tokens.iter().map(|t| i32::from(t.to_u16())).collect();
    // Pad with the end-of-text token to the fixed context length.
    ids.resize(CONTEXT_LENGTH, i32::from(eot.to_u16()));
    ids
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The reference token-id sequences from
    /// `apps/desktop/scripts/convert-clip-model/reference-tokenization.json`, produced by
    /// HuggingFace `CLIPTokenizer.from_pretrained("openai/clip-vit-base-patch32")`. These
    /// pin the exact BPE ids, the BOS/EOS markers, and the end-of-text padding, so a
    /// tokenizer-crate change that drifts from canonical CLIP fails here (the text tower
    /// would then see different ids than it was converted against).
    const BOS: i32 = 49406;
    const EOS: i32 = 49407;

    /// Build the expected 77-length sequence from the content ids (between BOS and EOS).
    fn expected(content: &[i32]) -> Vec<i32> {
        let mut v = vec![BOS];
        v.extend_from_slice(content);
        v.push(EOS);
        v.resize(CONTEXT_LENGTH, EOS);
        v
    }

    #[test]
    fn matches_the_clip_reference_tokenization() {
        // (text, content ids between BOS and EOS) — straight from the reference JSON.
        let cases: &[(&str, &[i32])] = &[
            ("a photo of a cat", &[320, 1125, 539, 320, 2368]),
            ("a dog", &[320, 1929]),
            ("beach sunset", &[2117, 3424]),
            ("a video game screenshot", &[320, 1455, 1063, 12646]),
            ("hello", &[3306]),
            ("a red car on a street", &[320, 736, 1615, 525, 320, 2012]),
            ("", &[]),
        ];
        for (text, content) in cases {
            let got = tokenize(text);
            assert_eq!(got.len(), CONTEXT_LENGTH, "always padded to context length for {text:?}");
            assert_eq!(&got, &expected(content), "token ids for {text:?}");
        }
    }

    #[test]
    fn is_case_insensitive() {
        // CLIP lowercases before encoding, so case doesn't change the ids.
        assert_eq!(tokenize("A DOG"), tokenize("a dog"));
    }

    #[test]
    fn over_length_input_truncates_and_still_ends_in_eot() {
        let long = "word ".repeat(200);
        let ids = tokenize(&long);
        assert_eq!(ids.len(), CONTEXT_LENGTH);
        assert_eq!(ids[0], BOS, "starts with start-of-text");
        assert_eq!(ids[CONTEXT_LENGTH - 1], EOS, "the last slot is end-of-text even when truncated");
    }
}
