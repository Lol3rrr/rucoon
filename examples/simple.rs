use rucoon::runtime;

static RUNTIME: runtime::Runtime<10> = runtime::Runtime::new();

async fn without_yield() {
    println!("Without Yield");
}

async fn with_yield() {
    println!("With Yield Before");

    rucoon::futures::Yielder::new().await;

    println!("With Yield After");
}

fn main() {
    println!("Testing");

    RUNTIME.add_task(with_yield()).unwrap();
    RUNTIME.add_task(without_yield()).unwrap();

    let sleep_time = std::time::Duration::from_nanos(250);
    RUNTIME
        .run_with_sleep(|| {
            std::thread::sleep(sleep_time.clone());
        })
        .unwrap();
}
