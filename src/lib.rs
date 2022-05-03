#![no_std]
#![feature(once_cell)]
#![feature(const_type_name)]
#![feature(thread_id_value)]
#![feature(min_specialization)]
#![allow(non_snake_case)]
#![allow(non_camel_case_types)]
#![allow(dead_code)]
#![allow(unused_variables)]
#![allow(unused_must_use)]
#![allow(non_upper_case_globals)]

extern crate alloc;
#[cfg(feature = "std")]
extern crate std;
#[macro_use]
extern crate derive_more;

use alloc::borrow::*;
use alloc::boxed::*;
use alloc::string::*;
use alloc::vec;
use alloc::vec::Vec;

pub mod binding;
pub mod ffi;

#[cfg(feature = "std")]
#[macro_export]
macro_rules! cstr {
    ($lit:expr) => {
        unsafe {
            ::std::ffi::CStr::from_ptr(
                concat!($lit, "\0").as_ptr() as *const ::std::os::raw::c_char
            )
        }
    };
}

#[cfg(not(feature = "std"))]
pub use cstrptr::cstr;

#[cfg(feature = "std")]
pub(crate) mod str {
    pub use std::ffi::{CStr, CString};
}

// TODO: try cstr_core
#[cfg(not(feature = "std"))]
pub(crate) mod str {
    pub use cstrptr::{CStr, CString};
    use cty::c_char;

    pub trait CStringExt {
        fn as_ptr(&self) -> *const c_char;
    }

    impl CStringExt for CString {
        fn as_ptr(&self) -> *const c_char {
            self.as_c_str().as_ptr()
        }
    }
}

mod convert;
#[cfg(feature = "thread")]
mod llua;
mod lmacro;
mod luaconf;
mod serde;
mod state;
#[cfg(test)]
mod test;
mod util;
mod value;

pub use self::serde::*;
pub use convert::*;
pub use lmacro::*;
pub use state::*;
pub use util::*;
pub use value::*;

#[cfg(feature = "thread")]
pub mod thread {
    use super::*;
    use parking_lot::Mutex;
    use std::lazy::SyncLazy;

    unsafe impl Send for State {}
    unsafe impl Sync for State {}

    pub static mut GLOBAL_LUA: SyncLazy<Mutex<State>> = SyncLazy::new(|| {
        let state = State::new();
        state.open_libs();
        state.init_llua_global();
        state.into()
    });

    #[derive(derive_more::Deref)]
    pub struct TlsState(pub State);

    impl TlsState {
        fn new() -> TlsState {
            unsafe {
                let s = GLOBAL_LUA.lock();
                let result = Self(s.new_thread());
                s.push_value(-1);
                s.raw_set(LUA_REGISTRYINDEX);
                result
            }
        }
    }

    impl Drop for TlsState {
        fn drop(&mut self) {
            unsafe {
                let s = GLOBAL_LUA.lock();
                s.push_thread();
                s.push_nil();
                s.raw_set(LUA_REGISTRYINDEX);
            }
        }
    }

    std::thread_local! {
        pub static LUA: TlsState = TlsState::new();
    }
}
