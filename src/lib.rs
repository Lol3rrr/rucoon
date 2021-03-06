#![no_std]
//
#![deny(unsafe_op_in_unsafe_fn)]
#![warn(missing_docs)]
//
#![feature(const_mut_refs)]
#![feature(const_maybe_uninit_as_mut_ptr)]
#![feature(const_ptr_offset)]
#![feature(const_ptr_write)]

//! Rucoon is a small and simple async Runtime
//!
//! # Idea
//! The Idea is to have a Runtime with a very limited footprint to allow for running it on embedded
//! Devices or other Resource Constrained Environments.
//! For this Purpose the Runtime has several limitations, like only supporting a fixed Number of
//! Tasks or not being able to remove a Task once it has been added, so you have to be aware of the
//! Limitations and determine if they fit your use-case or are a dealbreaker.
//!
//! # no_std
//! This Crate is fully written for use in no_std environments but needs support the alloc Crate as
//! it is currently needed to erase the actual Types of the Futures backing the Tasks, so although
//! this support for alloc is needed, it is not used much and only during very specific Operations
//! (i.e. adding a new Task)
//!
//! # Example Usage:
//! ```no_run
//! // A Runtime with space for 2 Tasks
//! static RUNTIME: rucoon::Runtime<2> = rucoon::Runtime::new();
//!
//! async fn first_task() {
//!     loop {
//!         println!("First Task");
//!         rucoon::futures::yield_now().await;
//!     }
//! }
//! async fn second_task() {
//!     loop {
//!         println!("Second Task");
//!         rucoon::futures::yield_now().await;
//!     }
//! }
//!
//! fn main() {
//!     RUNTIME.add_task(first_task());
//!     RUNTIME.add_task(second_task());
//!
//!     RUNTIME.run().unwrap();
//! }
//!
//! ```

extern crate alloc;

pub mod futures;
pub mod runtime;
pub use runtime::{MultithreadedRuntime, Runtime};
pub mod extensions;

pub(crate) mod internal;
