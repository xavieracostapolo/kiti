//! Coste de un escaneo real (`cargo run --release -p kiti-core --example scan_bench`).
use std::time::Instant;

use kiti_core::adapters::SysinfoScanner;
use kiti_core::domain::dedupe_tree;
use kiti_core::ports::ProcessScanner;

fn main() {
    let mut scanner = SysinfoScanner::new();
    scanner.scan().unwrap(); // primer escaneo: lee argv, exe y cwd de todos
    let runs = 20;
    let start = Instant::now();
    let mut found = 0;
    for _ in 0..runs {
        let snaps = scanner.scan().unwrap();
        found = dedupe_tree(&snaps).len();
    }
    println!(
        "{:.2} ms por escaneo ({} agentes)",
        start.elapsed().as_secs_f64() * 1000.0 / runs as f64,
        found
    );
}
