//! Short ID generation for user-visible report IDs (error reports, crash reports).
//!
//! Produces IDs like `ERR-8F3A2` or `CRASH-K7J4P` from an unambiguous alphabet
//! (`23456789ABCDEFGHJKMNPQRSTUVWXYZ`, no `0`/`O`, no `1`/`I`/`L`). Uses rejection
//! sampling to avoid modulo bias. The alphabet is kept in sync with
//! `apps/api-server/src/license.ts::generateShortId`.

use rand::RngExt;

/// Unambiguous alphabet: no `0`/`O`, no `1`/`I`/`L`. 31 chars.
const ALPHABET: &[u8] = b"23456789ABCDEFGHJKMNPQRSTUVWXYZ";
/// Number of random characters after the prefix and dash.
const SUFFIX_LEN: usize = 5;

/// Generate a short ID like `{prefix}-XXXXX` using rejection sampling.
///
/// `prefix` is something like `"ERR"` or `"CRASH"`. The output shape is
/// `{prefix}-{five-chars-from-alphabet}`. The user sees and reports this ID, so
/// we pick an alphabet that's safe to read aloud or copy by eye.
pub fn generate(prefix: &str) -> String {
    let mut rng = rand::rng();
    let alphabet_len = ALPHABET.len(); // 31
    // 256 - (256 % 31) = 232: bytes at or above this would skew the distribution.
    let max_unbiased = 256 - (256 % alphabet_len);
    let mut out = String::with_capacity(prefix.len() + 1 + SUFFIX_LEN);
    out.push_str(prefix);
    out.push('-');
    let mut remaining = SUFFIX_LEN;
    while remaining > 0 {
        let byte: u8 = rng.random();
        if (byte as usize) < max_unbiased {
            out.push(ALPHABET[(byte as usize) % alphabet_len] as char);
            remaining -= 1;
        }
    }
    out
}

/// True when `candidate` is exactly `{prefix}-XXXXX` with every suffix character drawn
/// from [`ALPHABET`].
///
/// The gate for an id that arrives from outside this process: the error-report dialog
/// previews a report under an id and then hands that id back on send, and an id the app
/// didn't mint has no business becoming a server-side object key. Kept here so the
/// alphabet has exactly one definition.
pub fn matches(prefix: &str, candidate: &str) -> bool {
    let Some(suffix) = candidate.strip_prefix(prefix).and_then(|rest| rest.strip_prefix('-')) else {
        return false;
    };
    // Byte length is the right measure here BECAUSE every byte is then checked against an
    // all-ASCII alphabet: a five-char suffix with a multibyte char is six-plus bytes and
    // fails the length test before the alphabet test ever sees it.
    suffix.len() == SUFFIX_LEN && suffix.bytes().all(|b| ALPHABET.contains(&b))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn matches_accepts_what_generate_produces() {
        for _ in 0..200 {
            let id = generate("ERR");
            assert!(matches("ERR", &id), "`{id}` should match its own prefix");
        }
        assert!(matches("CRASH", &generate("CRASH")));
    }

    #[test]
    fn matches_rejects_anything_off_shape() {
        for bad in [
            "",
            "ERR",
            "ERR-",
            "ERR-AB23",   // too short
            "ERR-AB23XY", // too long
            "ERR-ab23x",  // lowercase
            "ERR-AB2 X",  // space
            "ERR-0O1IL",  // the excluded look-alike characters
            "ERR-AB23X extra",
            "ERRR-AB23X",
            "CRASH-AB23X", // right shape, wrong prefix
            "ERR-ÁB23X",   // multibyte, and five chars but six bytes
            "../ERR-AB23X",
        ] {
            assert!(!matches("ERR", bad), "`{bad}` should not match");
        }
    }

    #[test]
    fn err_prefix_matches_shape() {
        let re = regex::Regex::new("^ERR-[23456789ABCDEFGHJKMNPQRSTUVWXYZ]{5}$").unwrap();
        for _ in 0..200 {
            let id = generate("ERR");
            assert!(re.is_match(&id), "ID `{id}` didn't match");
        }
    }

    #[test]
    fn crash_prefix_matches_shape() {
        let re = regex::Regex::new("^CRASH-[23456789ABCDEFGHJKMNPQRSTUVWXYZ]{5}$").unwrap();
        for _ in 0..200 {
            let id = generate("CRASH");
            assert!(re.is_match(&id), "ID `{id}` didn't match");
        }
    }

    #[test]
    fn ids_are_statistically_unique() {
        let mut seen = HashSet::new();
        for _ in 0..1000 {
            seen.insert(generate("ERR"));
        }
        // 31^5 ≈ 28.6 M ID space → birthday paradox predicts ~0.02 collisions
        // on average per 1000 samples, with tiny variance. Insisting on zero
        // collisions trips ~1.7% of CI runs on a perfectly healthy RNG. Allow
        // up to 10: catches a genuinely broken RNG (hundreds of collisions)
        // without flaking on real entropy.
        assert!(
            seen.len() >= 990,
            "expected at least 990 distinct IDs in 1000 samples, got {}",
            seen.len()
        );
    }
}
