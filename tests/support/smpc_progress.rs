//! Shared native/production-Wasm progress oracle; never seeded by a test run.
use saturn_rust::Machine;

pub fn assert_progress(guest: &str, turns: u64, machine: &Machine) {
    let row = include_str!("../fixtures/smpc-progress.tsv")
        .lines()
        .filter(|line| !line.starts_with('#'))
        .find_map(|line| {
            let (name, values) = line.split_once('\t')?;
            (name == guest).then_some(values)
        })
        .expect("guest must have a shared native/Wasm progress record");
    let expected: Vec<u64> = row
        .split_whitespace()
        .map(|value| value.parse().unwrap())
        .collect();
    assert_eq!(
        [
            turns,
            machine.clocks(),
            machine.cpu_state().instructions,
            machine.cpu_state().exceptions
        ]
        .as_slice(),
        expected,
        "{guest}: native progress must match the shared Wasm oracle"
    );
}
