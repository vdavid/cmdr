//! Settings module for legacy settings loading.

mod legacy;

// Re-export only what's used externally
pub use legacy::load_settings;
