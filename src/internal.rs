//! Contains some internal Functions and other useful items that can be used in the Crate for
//! various Purpose

/// This macro can be used to easily create an Array with a constant Size
macro_rules! const_array {
    ($size:expr, $entry_ty:ty, $entry_value:expr) => {{
        let mut raw = core::mem::MaybeUninit::uninit();
        let raw_ptr = raw.as_mut_ptr() as *mut $entry_ty;

        let mut i = 0;
        while i < $size {
            let ptr = unsafe { raw_ptr.add(i) };

            unsafe {
                ptr.write($entry_value);
            }

            i += 1;
        }

        unsafe { raw.assume_init() }
    }};
}

pub(crate) use const_array;
