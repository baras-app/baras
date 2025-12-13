use std::time::Instant;

use baras::parse_log_file;

fn main() {
    let path = "test-log-files/50mb/combat_2025-12-10_18_12_15_087604.txt";

    println!("Parsing: {}", path);

    let start = Instant::now();
    let events = parse_log_file(path).expect("Failed to parse log file");
    let duration = start.elapsed();

    println!("Parsed {} events in {:?}", events.len(), duration);

    for event in events.iter().take(37) {
        println!("{:?}", event);
    }
}
