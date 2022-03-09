//! A Port of the Queue described as in this Post: https://www.1024cores.net/home/lock-free-algorithms/queues/bounded-mpmc-queue

use core::{
    cell::UnsafeCell,
    cmp,
    mem::MaybeUninit,
    sync::atomic::{AtomicUsize, Ordering},
};

use crate::internal;

struct Cell<T> {
    sequence: AtomicUsize,
    data: UnsafeCell<MaybeUninit<T>>,
}

pub struct Queue<T, const N: usize> {
    buffer: [Cell<T>; N],
    enqueue_pos: AtomicUsize,
    dequeue_pos: AtomicUsize,
}

unsafe impl<T, const N: usize> Send for Queue<T, N> {}
unsafe impl<T, const N: usize> Sync for Queue<T, N> {}

pub struct Sender<'t, T> {
    buffer: &'t [Cell<T>],
    enqueue_pos: &'t AtomicUsize,
}

unsafe impl<'t, T> Send for Sender<'t, T> {}
unsafe impl<'t, T> Sync for Sender<'t, T> {}

impl<T, const N: usize> Queue<T, N> {
    pub const fn new() -> Self {
        let mut cells: [Cell<T>; N] = internal::const_array!(
            N,
            Cell<T>,
            Cell {
                sequence: AtomicUsize::new(0),
                data: UnsafeCell::new(MaybeUninit::uninit())
            }
        );

        let mut pos = 0;
        while pos < N {
            cells[pos].sequence = AtomicUsize::new(pos);

            pos += 1;
        }

        Self {
            buffer: cells,
            enqueue_pos: AtomicUsize::new(0),
            dequeue_pos: AtomicUsize::new(0),
        }
    }

    pub const fn sender(&self) -> Sender<'_, T> {
        Sender {
            buffer: &self.buffer,
            enqueue_pos: &self.enqueue_pos,
        }
    }

    pub fn dequeue(&self) -> Result<T, ()> {
        let mut pos = self.dequeue_pos.load(Ordering::SeqCst);
        let mut cell;

        loop {
            cell = self.buffer.get(pos % N).unwrap();
            let seq = cell.sequence.load(Ordering::SeqCst);

            match (pos + 1).cmp(&seq) {
                cmp::Ordering::Equal => {
                    if self
                        .dequeue_pos
                        .compare_exchange(pos, pos + 1, Ordering::SeqCst, Ordering::SeqCst)
                        .is_ok()
                    {
                        break;
                    }
                }
                cmp::Ordering::Greater => return Err(()),
                cmp::Ordering::Less => {
                    pos = self.dequeue_pos.load(Ordering::SeqCst);
                }
            }
        }

        let data_ptr = cell.data.get();
        let data = unsafe { data_ptr.replace(MaybeUninit::uninit()).assume_init() };

        cell.sequence.store(pos + N, Ordering::SeqCst);

        Ok(data)
    }
}

impl<'t, T> Sender<'t, T> {
    pub fn enqueue(&self, data: T) -> Result<(), T> {
        let mut pos = self.enqueue_pos.load(Ordering::SeqCst);
        let mut cell;

        let size = self.buffer.len();
        loop {
            cell = self.buffer.get(pos % size).unwrap();
            let seq = cell.sequence.load(Ordering::SeqCst);

            match pos.cmp(&seq) {
                cmp::Ordering::Equal => {
                    if self
                        .enqueue_pos
                        .compare_exchange(pos, pos + 1, Ordering::SeqCst, Ordering::SeqCst)
                        .is_ok()
                    {
                        break;
                    }
                }
                cmp::Ordering::Greater => return Err(data),
                cmp::Ordering::Less => {
                    pos = self.enqueue_pos.load(Ordering::SeqCst);
                }
            };
        }

        let data_ptr = cell.data.get();
        unsafe {
            data_ptr.write(MaybeUninit::new(data));
        }

        cell.sequence.store(pos + 1, Ordering::SeqCst);

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn create_queue() {
        let _queue = Queue::<usize, 10>::new();
        const _CQUEUE: Queue<usize, 10> = Queue::new();
    }

    #[test]
    fn enqueue_more_than_space() {
        let queue = Queue::<usize, 3>::new();

        assert!(queue.sender().enqueue(0).is_ok());
        assert!(queue.sender().enqueue(1).is_ok());
        assert!(queue.sender().enqueue(2).is_ok());
        assert!(queue.sender().enqueue(3).is_err());
    }

    #[test]
    fn dequeue_empty() {
        let queue = Queue::<usize, 3>::new();

        assert_eq!(Err(()), queue.dequeue());
    }

    #[test]
    fn working_dequeue() {
        let queue = Queue::<usize, 3>::new();

        assert!(queue.sender().enqueue(0).is_ok());
        assert_eq!(Ok(0), queue.dequeue());
    }

    #[test]
    fn mixed_enqueue_dequeue() {
        let queue = Queue::<usize, 2>::new();

        assert!(queue.sender().enqueue(0).is_ok());
        assert!(queue.sender().enqueue(1).is_ok());
        assert!(queue.sender().enqueue(2).is_err());

        assert_eq!(Ok(0), queue.dequeue());

        assert!(queue.sender().enqueue(3).is_ok());

        assert_eq!(Ok(1), queue.dequeue());
        assert_eq!(Ok(3), queue.dequeue());

        assert_eq!(Err(()), queue.dequeue());
    }
}
