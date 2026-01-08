//! Benchmarks for icon fetching performance.
//!
//! Run with: `cargo bench --bench icon_benchmarks`
//! Results are saved to `target/criterion/` with HTML reports.

use criterion::{BenchmarkId, Criterion, criterion_group, criterion_main};
use std::fs;
use std::path::PathBuf;

/// Creates test files with various extensions for benchmarking.
fn setup_test_files(count: usize) -> Vec<PathBuf> {
    let extensions = ["txt", "pdf", "jpg", "png", "rs", "go", "ts", "md", "json", "toml"];
    let temp_dir = std::env::temp_dir().join("cmdr_bench");
    let _ = fs::create_dir_all(&temp_dir);

    (0..count)
        .map(|i| {
            let ext = extensions[i % extensions.len()];
            let path = temp_dir.join(format!("test_file_{}.{}", i, ext));
            if !path.exists() {
                let _ = fs::File::create(&path);
            }
            path
        })
        .collect()
}

/// Benchmarks icon fetching with different numbers of files.
fn bench_icon_fetching(c: &mut Criterion) {
    let mut group = c.benchmark_group("icon_fetch");

    for count in [10, 50, 100, 200] {
        let test_files = setup_test_files(count);
        let paths: Vec<String> = test_files.iter().map(|p| p.to_string_lossy().to_string()).collect();

        // Collect unique extensions
        let extensions: Vec<String> = test_files
            .iter()
            .filter_map(|p| p.extension())
            .map(|e| e.to_string_lossy().to_string())
            .collect::<std::collections::HashSet<_>>()
            .into_iter()
            .collect();

        group.bench_with_input(
            BenchmarkId::new("refresh_directory", count),
            &(paths.clone(), extensions.clone()),
            |b, (dir_paths, exts)| {
                b.iter(|| cmdr_lib::icons::refresh_icons_for_directory(dir_paths.clone(), exts.clone()))
            },
        );
    }

    group.finish();
}

criterion_group!(benches, bench_icon_fetching);
criterion_main!(benches);
