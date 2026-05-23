//! AI-powered selection query translation pipeline.
//!
//! Maps natural-language intent into a glob or regex that the JS-side matcher uses
//! to select files in the focused folder. Mirrors `crate::search::ai` in structure
//! but narrower: no scope, no system-dir excludes, no type/keyword/folders enums.
//!
//! The prompt receives a sampled folder listing so the model can ground the pattern
//! in what's actually in the folder (the user's intent often refers to filename
//! conventions like "all rymdskottkärra files" that the model can't infer without
//! seeing the names).

pub mod parser;
pub mod prompt;
pub mod query_builder;

#[cfg(test)]
mod real_llm_eval_test;

pub use parser::{ParsedSelectionLlmResponse, parse_selection_response};
pub use prompt::{build_classification_prompt, format_sample_block};
pub use query_builder::{SelectionTranslateResult, build_selection_translate_result, generate_caveat};
