//! A Collection of useful Futures to be used when developing applications using Rucoon

mod yielder;
pub use yielder::{yield_now, Yielder};
