use core::sync::atomic::{AtomicU8, Ordering};

/// The Status of a single Slot in the TaskList
pub enum SlotStatus {
    /// The Slot is Empty
    Empty,
    /// The Slot is currently locked and being modified
    Locked,
    /// The Slot is Set and not being accessed currently
    Set,
    /// The Slot is Set and being accessed
    Accessed,
}

#[derive(Debug)]
pub struct AtomicStatus(AtomicU8);

impl SlotStatus {
    const fn to_u8(&self) -> u8 {
        match self {
            Self::Empty => 0,
            Self::Locked => 1,
            Self::Set => 2,
            Self::Accessed => 3,
        }
    }

    const fn from_u8(raw: u8) -> Self {
        match raw {
            0 => Self::Empty,
            1 => Self::Locked,
            2 => Self::Set,
            3 => Self::Accessed,
            _ => panic!(""),
        }
    }
}

impl AtomicStatus {
    pub const fn new(val: SlotStatus) -> Self {
        Self(AtomicU8::new(val.to_u8()))
    }

    pub fn store(&self, value: SlotStatus, ordering: Ordering) {
        self.0.store(value.to_u8(), ordering);
    }

    pub fn compare_exchange(
        &self,
        current: SlotStatus,
        new: SlotStatus,
        success: Ordering,
        failure: Ordering,
    ) -> Result<SlotStatus, SlotStatus> {
        match self
            .0
            .compare_exchange(current.to_u8(), new.to_u8(), success, failure)
        {
            Ok(raw) => Ok(SlotStatus::from_u8(raw)),
            Err(prev) => Err(SlotStatus::from_u8(prev)),
        }
    }
}
