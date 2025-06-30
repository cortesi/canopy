use criterion::{Criterion, black_box, criterion_group, criterion_main};
use std::time::Duration;

fn benchmark_text_rendering(c: &mut Criterion) {
    // Build the example once before benchmarking
    let build_result = std::process::Command::new("cargo")
        .args(["build", "--example", "test_text"])
        .output()
        .expect("Failed to run cargo build");

    if !build_result.status.success() {
        panic!(
            "Failed to build test_text example: {}",
            String::from_utf8_lossy(&build_result.stderr)
        );
    }

    c.bench_function("test_text_render", |b| {
        b.iter(|| {
            // Spawn the test_text example directly without rebuilding
            let bin = concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/../target/debug/examples/test_text"
            )
            .to_string();
            let mut app =
                canopy::tutils::pty::PtyApp::spawn_cmd(&bin, &[]).expect("Failed to spawn example");

            // Wait for initial render - the app should display immediately
            app.expect("Lorem ipsum", Duration::from_millis(100))
                .expect("Failed to see initial text");

            // Force multiple render cycles by sending "r" to trigger full redraws
            for _ in 0..10 {
                app.send("r").expect("Failed to send key");
                // Small delay to allow render to complete
                std::thread::sleep(Duration::from_micros(100));
            }

            // Quit the app
            app.send("q").expect("Failed to send quit");
            app.wait_eof(Duration::from_secs(1))
                .expect("Failed to exit");

            black_box(&app);
        });
    });
}

criterion_group! {
    name = benches;
    config = Criterion::default().sample_size(10);
    targets = benchmark_text_rendering
}
criterion_main!(benches);
