//! Cross-module integration and stress tests for the indexing subsystem. Unit
//! tests stay colocated in each module's `tests.rs`; these exercise the whole
//! pipeline (scan -> aggregate -> enrich -> watch) or hammer it under load.
//!
//! `external_drive_fixture` is a macOS-only disk-image fixture, NOT a test file;
//! its FSKit-panic-safe attach/detach discipline is load-bearing (see the
//! module and the area DETAILS).

pub(crate) mod stress_test_helpers;

mod integration_tests;
mod stress_tests_concurrency;
mod stress_tests_lifecycle;
mod stress_tests_partial_aggregation;

// Synthetic FAT32/exFAT disk-image fixtures for external-drive indexing tests.
// macOS-only (hdiutil); see the module and the area DETAILS.
#[cfg(target_os = "macos")]
mod external_drive_fixture;
