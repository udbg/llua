
use super::*;

use core::ops::Deref;

pub use crate::ffi::{
    lua_Number, lua_Integer,
    CFunction, lua_CFunction,
    lua_Alloc, lua_Hook,
    LUA_REGISTRYINDEX,
};
pub type Index = i32;

#[derive(Clone, Copy, Deref)]
pub struct ValRef<'a> {
    #[deref]
    pub state: &'a State,
    pub index: Index
}

impl<'a> ValRef<'a> {
    pub fn new(state: &'a State, index: Index) -> Self {
        ValRef { state, index: state.abs_index(index) }
    }

    #[inline]
    pub fn is_nil(&self) -> bool { self.state.is_nil(self.index) }

    #[inline]
    pub fn is_integer(&self) -> bool { self.state.is_integer(self.index) }

    #[inline]
    pub fn to_bool(&self) -> bool { self.state.to_bool(self.index) }

    #[inline]
    pub fn check_type(&self, ty: Type) { self.state.check_type(self.index, ty); }

    #[inline]
    pub fn cast<T: FromLua>(&self) -> Option<T> { self.state.arg(self.index) }

    #[inline]
    pub fn check_cast<T: FromLua>(&self) -> T { T::check(self.state, self.index) }

    pub fn geti(&self, i: impl Into<lua_Integer>) -> ValRef {
        self.state.geti(self.index, i.into());
        self.val(-1)
    }

    pub fn seti<V: ToLua>(&self, i: impl Into<lua_Integer>, v: V) {
        v.to_lua(self);
        self.state.seti(self.index, i.into());
    }

    pub fn getf(&self, k: &CStr) -> ValRef {
        self.get_field(self.index, k);
        self.val(-1)
    }

    #[inline]
    pub fn rawget<K: ToLua>(&self, k: K) -> Type {
        self.push(k);
        self.raw_get(self.index)
    }

    #[inline]
    pub fn rawlen(&self) -> usize {
        self.raw_len(self.index)
    }

    #[inline]
    pub fn set_field(&self, k: &CStr) {
        self.state.set_field(self.index, k);
    }

    #[inline]
    pub fn setf<V: ToLua>(&self, k: &CStr, v: V) {
        self.push(v);
        self.set_field(k);
    }

    #[inline]
    pub fn getp<T>(&self, p: *const T) -> ValRef {
        self.state.raw_getp(self.index, p);
        self.val(-1)
    }

    #[inline]
    pub fn setp<T, V: ToLua>(&self, k: *const T, v: V) {
        v.to_lua(self.state);
        self.state.raw_setp(self.index, k);
    }

    #[inline]
    pub fn reference<V: ToLua>(&self, v: V) -> Reference {
        v.to_lua(self.state);
        self.state.reference(self.index)
    }

    #[inline]
    pub fn unreference(&self, r: Reference) {
        self.state.unreference(self.index, r);
    }

    #[inline]
    pub fn set<K: ToLua, V: ToLua>(&self, k: K, v: V) {
        if V::IS_TOP {
            self.push(k);
            self.insert(-2);
        } else {
            self.push(k);
            self.push(v);
        }
        self.set_table(self.index);
    }

    #[inline]
    pub fn get<K: ToLua>(&self, k: K) {
        self.push(k);
        self.get_table(self.index);
    }

    #[inline]
    pub fn getopt<K: ToLua, V: FromLua>(&self, k: K) -> Option<V> {
        self.get(k);
        let res = V::from_lua(self.state, -1);
        self.pop(1); res
    }
}

#[derive(Deref)]
pub struct Coroutine(State);

unsafe impl Send for Coroutine {}

impl Coroutine {
    pub fn with_fn(s: &State, i: Index) -> Coroutine {
        s.check_type(i, Type::Function);
        let ns = s.new_thread();
        s.push_value(i);
        s.xmove(&ns, 1);
        assert!(s.type_of(-1) == Type::Thread);
        s.raw_setp(LUA_REGISTRYINDEX, ns.as_ptr());
        Coroutine(ns)
    }

    #[inline(always)]
    pub fn balance_call<T: ToLuaMulti + Copy, R: FromLuaMulti>(&self, args: T) -> Result<R, String> {
        self.push_value(1);
        self.balance_with(|s| s.pcall_trace::<T, R>(args).map_err(|e| e.to_string()))
    }
}

impl FromLua for Coroutine {
    fn from_lua(s: &State, i: Index) -> Option<Self> {
        match s.type_of(i) {
            // maybe cause data race to self.0: lua_State*
            // Type::Thread => s.to_thread(i).map(|x| Self(x)),
            Type::Function => Self::with_fn(s, i).into(),
            _ => None,
        }
    }
}

impl Drop for Coroutine {
    fn drop(&mut self) {
        self.push_nil();
        self.raw_setp(LUA_REGISTRYINDEX, self.as_ptr());
    }
}