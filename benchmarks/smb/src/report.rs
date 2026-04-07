//! Output formatting: terminal table and JSON file.

use crate::runner::AllResults;
use std::path::PathBuf;

pub fn print_table(results: &AllResults) {
    println!("\n\n╔══════════════════════════════════════════════════════════════════╗");
    println!("║                        SMB BENCHMARK RESULTS                    ║");
    println!("╚══════════════════════════════════════════════════════════════════╝\n");

    for suite in &results.suites {
        let total_mb =
            (suite.file_count as f64 * suite.file_size_bytes as f64) / (1024.0 * 1024.0);
        println!(
            "Target: {} | Suite: {} — {} files × {} KB ({:.1} MB total)",
            suite.target_name,
            suite.suite_name,
            suite.file_count,
            suite.file_size_bytes / 1024,
            total_mb,
        );
        println!("┌──────────────┬──────────────┬──────────────┬──────────┐");
        println!("│ operation    │ native       │ direct       │ speedup  │");
        println!("├──────────────┼──────────────┼──────────────┼──────────┤");

        for op in &suite.operations {
            let native_ms = op.native_median().as_secs_f64() * 1000.0;
            let direct_ms = op.direct_median().as_secs_f64() * 1000.0;
            let speedup = op.speedup();
            let speedup_str = if speedup >= 100.0 {
                format!("{:.0}x", speedup)
            } else {
                format!("{:.1}x", speedup)
            };
            println!(
                "│ {:<12} │ {:>10} │ {:>10} │ {:>7}  │",
                op.name,
                format_duration_ms(native_ms),
                format_duration_ms(direct_ms),
                speedup_str,
            );
        }
        println!("└──────────────┴──────────────┴──────────────┴──────────┘\n");
    }
}

pub fn save_json(results: &AllResults) {
    let timestamp = chrono::Local::now().format("%Y-%m-%d-%H%M%S");
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("results");
    std::fs::create_dir_all(&dir).expect("create results dir");
    let path = dir.join(format!("results-{timestamp}.json"));

    let json = build_json(results);
    std::fs::write(&path, json).expect("write results JSON");
    println!("Results saved to {}", path.display());
}

fn build_json(results: &AllResults) -> String {
    let mut out = String::from("{\n  \"suites\": [\n");
    for (si, suite) in results.suites.iter().enumerate() {
        out.push_str(&format!(
            "    {{\n      \"target\": \"{}\",\n      \"suite\": \"{}\",\n      \"file_count\": {},\n      \"file_size_bytes\": {},\n      \"operations\": [\n",
            suite.target_name, suite.suite_name, suite.file_count, suite.file_size_bytes
        ));
        for (oi, op) in suite.operations.iter().enumerate() {
            out.push_str(&format!(
                "        {{\n          \"name\": \"{}\",\n          \"native_ms\": [{native}],\n          \"direct_ms\": [{direct}],\n          \"native_median_ms\": {:.2},\n          \"direct_median_ms\": {:.2},\n          \"speedup\": {:.2}\n        }}",
                op.name,
                op.native_median().as_secs_f64() * 1000.0,
                op.direct_median().as_secs_f64() * 1000.0,
                op.speedup(),
                native = op.native_times.iter().map(|d| format!("{:.2}", d.as_secs_f64() * 1000.0)).collect::<Vec<_>>().join(", "),
                direct = op.direct_times.iter().map(|d| format!("{:.2}", d.as_secs_f64() * 1000.0)).collect::<Vec<_>>().join(", "),
            ));
            if oi < suite.operations.len() - 1 {
                out.push(',');
            }
            out.push('\n');
        }
        out.push_str("      ]\n    }");
        if si < results.suites.len() - 1 {
            out.push(',');
        }
        out.push('\n');
    }
    out.push_str("  ]\n}\n");
    out
}

fn format_duration_ms(ms: f64) -> String {
    if ms < 1.0 {
        format!("{:.2} ms", ms)
    } else if ms < 1000.0 {
        format!("{:.0} ms", ms)
    } else {
        format!("{:.2} s", ms / 1000.0)
    }
}
