//! Natural language into a [`crate::SearchQuery`]: the classification prompt, the
//! reply's parser, and the deterministic mappings from the model's closed enums to
//! values.
//!
//! No model and no I/O here: a host picks the provider, sends the prompt, and hands
//! the reply back. "Today" and the home folder arrive as values ([`LocalDays`] and a
//! `home` argument), so the same mapping answers on any machine and in tests.
//! `translate/CLAUDE.md` has the must-knows.

mod mappings;
mod parser;
mod prompt;
mod query_builder;
mod types;

#[cfg(any(test, feature = "testing"))]
pub use mappings::FixedZone;
pub use mappings::LocalDays;
pub use parser::{ParsedLlmResponse, fallback_keywords, parse_llm_response};
pub use prompt::build_classification_prompt;
pub use query_builder::{
    build_label, build_search_query, build_translate_display, build_translated_query, generate_caveat,
    iso_date_to_timestamp,
};
pub use types::{TranslateDisplay, TranslatedQuery};
