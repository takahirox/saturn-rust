//! Optional, original-guest dispatch experiment; never a game FPS estimate.
use saturn_rust::kernel::experiment::Experiment;
use std::{hint::black_box, time::Instant};
fn main() {
    let count = std::env::args()
        .nth(1)
        .map(|v| v.parse::<u32>().expect("instruction count"))
        .unwrap_or(1_000_000);
    assert!(count > 0);
    // Component evidence is outside CPU dispatch timing and uses the actual
    // board arbiter/BSC. Its digest can be compared with main-thread/Worker runs.
    eprintln!(
        "owned_read_port={:?}",
        saturn_rust::kernel::verify_read_port_board()
    );
    for memory in [false, true] {
        // Warm both implementations before either timed variant.
        let mut warm = Experiment::new(memory, 1);
        warm.run_kernel(10_000).unwrap();
        warm.run_reference(10_000).unwrap();
        warm.verify().unwrap();
        for round in 0..6 {
            let mut e = Experiment::new(memory, 1);
            let mut ms = [0.0; 2];
            for variant in if round % 2 == 0 { [0, 1] } else { [1, 0] } {
                let start = Instant::now();
                if variant == 0 {
                    e.run_kernel(black_box(count)).unwrap();
                } else {
                    e.run_reference(black_box(count)).unwrap();
                }
                ms[variant] = start.elapsed().as_secs_f64() * 1000.0;
            }
            e.verify().unwrap();
            println!(
                "{{\"memory\":{memory},\"round\":{round},\"instructions_per_cpu\":{count},\"kernel_ms\":{},\"immediate_reference_ms\":{},\"digest\":{:?}}}",
                ms[0],
                ms[1],
                black_box(e.digest())
            );
        }
    }
}
