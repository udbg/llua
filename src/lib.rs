#![no_std]
#![feature(const_type_name)]
#![feature(specialization)]
#![feature(const_fn_trait_bound)]
#![feature(const_fn_fn_ptr_basics)]
#![allow(non_snake_case)]
#![allow(non_camel_case_types)]
#![allow(unused_imports)]
#![allow(dead_code)]
#![allow(unused_variables)]
#![allow(unused_must_use)]
#![allow(unused_braces)]
#![allow(incomplete_features)]
#![allow(non_upper_case_globals)]

extern crate alloc;

#[macro_use] extern crate cfg_if;
#[macro_use] extern crate derive_more;

use alloc::string::*;
use alloc::vec::Vec;
use alloc::vec;
use alloc::borrow::*;
use alloc::boxed::*;

cfg_if! {
    if #[cfg(feature = "std")] {
        extern crate std;

        pub use std::ffi::{CStr, CString};
        pub use c_str_macro::c_str as cstr;
    } else {
        pub use cstrptr::{CStr, CString, cstr};
    }
}

pub mod ffi;
pub mod lserde;

mod util;
mod state;
mod luaconf;
mod convert;
mod value;
mod lmacro;
#[cfg(feature = "std")]
pub mod stdconv;

pub use util::*;
pub use state::*;
pub use value::*;
pub use lmacro::*;
pub use convert::*;