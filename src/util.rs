
pub use corepack;

use crate::{*, lserde::*};
use corepack::{read, error};
use serde::{Serialize, Deserializer};

pub struct LLuaMsgPack<'a>(pub &'a [u8]);

impl<'a> ToLua for LLuaMsgPack<'a> {
    #[inline(always)]
    fn to_lua(self, s: &State) {
        push_corepack(s, self.0)
    }
}

fn push_corepack(s: &State, bytes: &[u8]) {
    let mut position: usize = 0;
    let r = read::BorrowRead::new(|len: usize| if position + len > bytes.len() {
        Err(error::Error::EndOfStream)
    } else {
        let result = &bytes[position..position + len];
        position += len;
        Ok(result)
    });
    let mut der = corepack::Deserializer::new(r);
    push_use_deserializer(s, &mut der);
}

pub fn push_use_deserializer<'de, D: Deserializer<'de>>(s: &State, der: D) {
    der.deserialize_any(LuaVisitor(s)).unwrap();
}

impl State {
    pub fn to_ffi_pointer(&self, i: Index) -> Option<usize> {
        Some(match self.type_of(i) {
            Type::Number => if self.is_integer(i) {
                self.to_integer(i) as usize
            } else { self.to_number(i) as usize },
            Type::String => self.to_string(i) as usize,
            _ => {
                let ptr = self.to_pointer(i);
                if ptr.is_null() { return None; }
                ptr as usize
            }
        })
    }

    pub fn init_llua_global(&self) {
        let s = self.balance();
        let g = s.global();

        g.setf(cstr!("__llua_psize"), core::mem::size_of::<usize>());
        g.setf(cstr!("topointer"), RsFn::new(|s: &State| Self::to_ffi_pointer(s, 1)));
        g.setf(cstr!("cclosure"), RsFn::new(|s: &State| {
            if let Some(f) = s.to_cfunction(1) {
                s.push_cclosure(Some(f), s.get_top() - 1);
                Pushed(1)
            } else {
                s.type_error(1, cstr!("cfunction"));
            }
        }));
    }
}