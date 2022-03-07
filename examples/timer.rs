use std::time::Duration;

use rucoon::extensions::time;
use rucoon::runtime;

static RUNTIME: runtime::Runtime<10> = runtime::Runtime::new();
static TIMER: time::Timer<10> = time::Timer::new(10);

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
    println!("Testing");

    RUNTIME.add_task(with_sleep()).unwrap();
    RUNTIME.add_task(without_sleep()).unwrap();

    let interval_time = Duration::from_millis(10);
    std::thread::spawn(move || loop {
        TIMER.update();

        std::thread::sleep(interval_time.clone());
    });

    let sleep_time = Duration::from_nanos(250);
    RUNTIME
        .run_with_sleep(|| {
            std::thread::sleep(sleep_time.clone());
        })
        .unwrap();
}
