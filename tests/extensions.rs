#![feature(thread_is_running)]

mod sleep {
    use std::{
        sync::atomic::{AtomicBool, Ordering},
        time::Duration,
    };

    use rucoon::{
        extensions::time::{Sleep, Timer},
        Runtime,
    };

    #[test]
    fn simple() {
        static RUNTIME: Runtime<10> = Runtime::new();
        static TIMER: Timer<10> = Timer::new(10);

        static REGISTERED: AtomicBool = AtomicBool::new(false);
        static DONE: AtomicBool = AtomicBool::new(false);

        async fn test_func() {
            REGISTERED.store(true, Ordering::SeqCst);
            Sleep::new(&TIMER, Duration::from_millis(50)).await;

            DONE.store(true, Ordering::SeqCst);
        }

        RUNTIME.add_task(test_func()).unwrap();

        let _ = std::thread::spawn(|| {
            RUNTIME.run().unwrap();
        });

        while !REGISTERED.load(Ordering::SeqCst) {}

        for i in 1..=5 {
            println!("Update {}", i);

            assert_eq!(false, DONE.load(Ordering::SeqCst));
            TIMER.update();
            std::thread::sleep(std::time::Duration::from_millis(1));
        }

        assert_eq!(true, DONE.load(Ordering::SeqCst));
    }
}
