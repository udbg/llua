#![no_std]
#![feature(const_type_name)]
#![feature(specialization)]
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

#[macro_use]
extern crate cfg_if;
#[macro_use]
extern crate derive_more;

use alloc::borrow::*;
use alloc::boxed::*;
use alloc::string::*;
use alloc::vec;
use alloc::vec::Vec;

cfg_if! {
    if #[cfg(feature = "std")] {
        extern crate std;

        pub use std::ffi::{CStr, CString};
        pub use c_str_macro::c_str as cstr;
    } else {
        pub use cstrptr::{CStr, CString, cstr};
    }
}

pub mod binding;
pub mod ffi;

mod convert;
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
