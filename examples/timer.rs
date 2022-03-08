use std::time::{Duration, Instant};

use rucoon::extensions::time;
use rucoon::runtime;

static RUNTIME: runtime::Runtime<10> = runtime::Runtime::new();
static TIMER: time::Timer<10> = time::Timer::new(1);

async fn without_sleep() {
    println!("Without Sleep");
}

async fn with_sleep() {
    println!("With Sleep Before");

    let before = std::time::Instant::now();
    time::Sleep::new(&TIMER, Duration::from_millis(50)).await;

    println!("With Sleep After");
    println!("Elapsed: {:?}", before.elapsed());
}

fn main() {
    println!("Starting");

    RUNTIME.add_task(with_sleep()).unwrap();
    RUNTIME.add_task(without_sleep()).unwrap();

    let interval_time = Duration::from_millis(1);
    std::thread::spawn(move || loop {
        let start = Instant::now();

        TIMER.update();

        let sleep_time = interval_time.saturating_sub(start.elapsed());
        std::thread::sleep(sleep_time);
    });

    let sleep_time = Duration::from_nanos(250);
    RUNTIME
        .run_with_sleep(|| {
            std::thread::sleep(sleep_time.clone());
        })
        .unwrap();
}
