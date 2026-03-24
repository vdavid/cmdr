//! AI-powered search query translation pipeline.
//!
//! Parses natural language queries via an LLM classification prompt, then maps
//! the structured response into deterministic `SearchQuery` fields.

pub(crate) mod mappings;
pub mod parser;
pub mod prompt;
pub mod query_builder;

// Re-exports for convenience
pub use parser::{ParsedLlmResponse, fallback_keywords, parse_llm_response};
pub use prompt::build_classification_prompt;
pub use query_builder::{
    build_search_query, build_translate_display, build_translated_query, generate_caveat, iso_date_to_timestamp,
};
