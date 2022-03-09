use rucoon::runtime::multithreaded::MultithreadedRuntime;

static RUNTIME: MultithreadedRuntime<100, 2> = MultithreadedRuntime::new();

async fn testing() {
    for _ in 0..10 {
        let thread = std::thread::current();
        println!("{:?}", thread.id());

        for _ in 0..10000 {}

        rucoon::futures::yield_now().await;
    }
}

fn main() {
    println!("Starting");

    for _ in 0..100 {
        RUNTIME.add_task(testing()).unwrap();
    }

    let handles: Vec<_> = RUNTIME
        .run_funcs_with_sleep(|| || {})
        .into_iter()
        .map(|f| std::thread::spawn(f))
        .collect();

    for handle in handles {
        handle.join().unwrap().unwrap();
    }
}
