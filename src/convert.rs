use super::*;

use crate::{ffi::*, lua_Integer as Integer, lua_Number as Number};
use alloc::sync::Arc;
use core::fmt::Debug;
use core::marker::PhantomData;
use core::mem;
use libc::c_int;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct CRegRef(pub i32);
pub struct NilVal;
pub struct AnyVal;
pub struct TopVal;
pub struct StrictBool(pub bool);
pub struct IterVec<T: ToLua, I: Iterator<Item = T>>(pub I);
pub struct IterMap<K: ToLua, V: ToLua, I: Iterator<Item = (K, V)>>(pub I);
pub struct BoxIter<'a, T>(pub Box<dyn Iterator<Item = T> + 'a>);
// pub struct RsClosure<T, O, F>(pub F, PhantomData<T>, PhantomData<O>);
pub struct RsFn<THIS, T, O, F>(pub F, PhantomData<(THIS, T, O)>);
pub struct UserDataWrapper<T>(pub T, pub Option<InitMetatable>);
#[derive(Default, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Pushed(pub i32);
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct StackRef(pub i32);

impl From<i32> for Pushed {
    #[inline(always)]
    fn from(n: i32) -> Self {
        Self(n)
    }
}

pub trait UserData: Sized {
    /// `__name`
    const TYPE_NAME: &'static str = core::any::type_name::<Self>();
    /// set `__index` to metatable self
    const INDEX_SELF: bool = true;
    /// `__len`
    const RAW_LEN: bool = false;

    const INDEX_USERVALUE: bool = false;
    const GETTER: lua_CFunction = None;
    const IS_POINTER: bool = false;

    unsafe extern "C" fn __gc(l: *mut lua_State) -> c_int {
        let s = State::from_ptr(l);
        if Self::IS_POINTER {
            let u = s.check_userdata_typed::<(*mut Self, Self)>(1);
            if u.0 == &mut u.1 {
                core::ptr::drop_in_place(u.0);
            }
        } else {
            let this = <&mut Self>::check(&s, 1);
            core::ptr::drop_in_place(this);
        }
        0
    }

    unsafe extern "C" fn __len(l: *mut lua_State) -> c_int {
        let s = State::from_ptr(l);
        s.push(s.raw_len(1));
        1
    }

    unsafe extern "C" fn __tostring(l: *mut lua_State) -> c_int
    where
        Self: ToString,
    {
        0
    }

    unsafe extern "C" fn index_getter(l: *mut lua_State) -> c_int {
        let s = State::from_ptr(l);
        let t = s.get_metatable_by(1, s.val(2));
        if t.is_none_or_nil() {
            if t == Type::Nil {
                s.pop(2);
            }
            if let Some(getter) = Self::GETTER {
                return getter(l);
            }
            return 0;
        }
        return 1;
    }

    unsafe extern "C" fn index_uservalue(l: *mut lua_State) -> c_int {
        let s = State::from_ptr(l);
        if s.get_metatable_by(1, s.val(2)).is_none_or_nil() {
            s.get_uservalue(1);
            s.push_value(2);
            s.get_table(-2);
        }
        return 1;
    }

    unsafe extern "C" fn newindex_uservalue(l: *mut lua_State) -> c_int {
        let s = State::from_ptr(l);
        s.get_uservalue(1);
        s.push_value(2);
        s.push_value(3);
        s.set_table(-3);
        return 0;
    }

    fn methods(mt: &ValRef) {}

    fn init_metatable(mt: &ValRef) {
        mt.setf(cstr!("__name"), Self::TYPE_NAME);
        mt.setf(cstr!("__gc"), Self::__gc as CFunction);
        if Self::RAW_LEN {
            mt.setf(cstr!("__len"), Self::__len as CFunction);
        }
        if Self::GETTER.is_some() {
            mt.setf(cstr!("__index"), Self::index_getter as CFunction);
        } else if Self::INDEX_USERVALUE {
            mt.setf(cstr!("__index"), Self::index_uservalue as CFunction);
            mt.setf(cstr!("__newindex"), Self::newindex_uservalue as CFunction);
        } else if Self::INDEX_SELF {
            mt.setf(cstr!("__index"), mt);
        }
        Self::methods(&mt);
    }
}
// impl<T> UserData for Arc<T> {}

impl<T: UserData> ToLua for T {
    #[inline(always)]
    fn to_lua(self, s: &State) {
        if T::IS_POINTER {
            s.push_userdata_pointer_body(self, Self::init_metatable);
        } else {
            s.push_userdata(self, Some(Self::init_metatable));
        }
        if T::INDEX_USERVALUE {
            s.new_table();
            s.set_uservalue(-2);
        }
    }
}

impl<T: UserData> ToLua for *mut T {
    #[inline(always)]
    fn to_lua(self, s: &State) {
        assert!(T::IS_POINTER);
        s.push_userdata_pointer(self, T::init_metatable);
        if T::INDEX_USERVALUE {
            s.new_table();
            s.set_uservalue(-2);
        }
    }
}

pub trait LuaFn<'a, THIS, ARGS, RET> {
    unsafe extern "C" fn wrapper(l: *mut lua_State) -> c_int;
}

impl<'a, THIS: 'a, T, O, F: LuaFn<'a, THIS, T, O>> RsFn<THIS, T, O, F> {
    pub const fn wrapper(&self) -> lua_CFunction {
        // TODO:
        // assert_eq!(core::mem::size_of::<F>(), 0);
        Some(F::wrapper)
    }
}

impl<'a, T, O, F: LuaFn<'a, (), T, O>> RsFn<(), T, O, F> {
    pub const fn new(f: F) -> Self {
        Self(f, PhantomData)
    }
}

/// Trait for types that can be pushed onto the stack of a Lua s.
///
/// It is important that implementors of this trait ensure that `to_lua`
/// behaves like one of the `lua_push*` functions for consistency.
pub trait ToLua {
    const IS_TOP: bool = false;

    /// Pushes a value of type `Self` onto the stack of a Lua s.
    fn to_lua(self, s: &State);
}

impl<'a> ToLua for () {
    #[inline(always)]
    fn to_lua(self, s: &State) {
        s.push_nil();
    }
}

impl<'a> ToLua for &'a str {
    #[inline(always)]
    fn to_lua(self, s: &State) {
        s.push_string(self);
    }
}

#[cfg(feature = "std")]
impl<'a> ToLua for &'a std::ffi::OsStr {
    #[inline(always)]
    fn to_lua(self, s: &State) {
        s.push(self.to_str());
    }
}

impl ToLua for String {
    #[inline(always)]
    fn to_lua(self, s: &State) {
        s.push_string(&self);
    }
}

impl<'a> ToLua for &'a [u8] {
    #[inline(always)]
    fn to_lua(self, s: &State) {
        s.push_bytes(self);
    }
}

impl ToLua for ValRef<'_> {
    #[inline(always)]
    fn to_lua(self, s: &State) {
        assert_eq!(s, self.state);
        s.push_value(self.index);
    }
}

impl ToLua for &ValRef<'_> {
    #[inline(always)]
    fn to_lua(self, s: &State) {
        assert_eq!(s, self.state);
        s.push_value(self.index);
    }
}

impl ToLua for TopVal {
    const IS_TOP: bool = true;

    #[inline(always)]
    fn to_lua(self, _: &State) {}
}

impl ToLua for InitMetatable {
    #[inline(always)]
    fn to_lua(self, s: &State) {
        s.get_or_init_metatable(self);
    }
}

impl<T> ToLua for UserDataWrapper<T> {
    #[inline(always)]
    fn to_lua(self, s: &State) {
        s.push_userdata(self.0, self.1);
    }
}

impl<T: ToLua, I: Iterator<Item = T>> ToLua for IterVec<T, I> {
    #[inline(always)]
    fn to_lua(self, s: &State) {
        let r = s.table(self.0.size_hint().0 as _, 0);
        let mut i = 1;
        for e in self.0.into_iter() {
            r.seti(i, e);
            i += 1;
        }
    }
}

impl<K: ToLua, V: ToLua, I: Iterator<Item = (K, V)>> ToLua for IterMap<K, V, I> {
    #[inline(always)]
    fn to_lua(self, s: &State) {
        let r = s.table(0, self.0.size_hint().0 as i32);
        for (k, v) in self.0 {
            r.set(k, v);
        }
    }
}

impl<'a, T: ToLuaMulti> BoxIter<'a, T> {
    unsafe extern "C" fn lua_fn(l: *mut lua_State) -> c_int {
        let s = State::from_ptr(l);
        let p = s.to_userdata(ffi::lua_upvalueindex(1));
        let iter: &mut BoxIter<'a, T> = mem::transmute(p);
        if let Some(v) = iter.0.next() {
            s.pushx(v)
        } else {
            0
        }
    }
}

impl<'a, T: ToLuaMulti> ToLua for BoxIter<'a, T> {
    #[inline(always)]
    fn to_lua(self, s: &State) {
        type BoxIterT = BoxIter<'static, usize>;
        s.push_userdata(self, Some(metatable!([s: State, this: BoxIterT])));
        s.push_cclosure(Some(BoxIter::<'a, T>::lua_fn), 1);
    }
}

impl<'a, THIS, T, O, F: LuaFn<'a, THIS, T, O>> ToLua for RsFn<THIS, T, O, F> {
    #[inline(always)]
    fn to_lua(self, s: &State) {
        if core::mem::size_of::<Self>() == 0 {
            return s.push_cclosure(Some(F::wrapper), 0);
        }
        if core::mem::size_of::<Self>() == core::mem::size_of::<usize>() {
            let pfptr = &self;
            s.push_light_userdata(unsafe { *mem::transmute::<_, *const *mut ()>(pfptr) });
        } else {
            // TODO: metatable __gc
            s.push_userdatauv(self, 0);
        };
        s.push_cclosure(Some(F::wrapper), 1);
    }
}

impl ToLua for fn(State) -> i32 {
    fn to_lua(self, s: &State) {
        unsafe extern "C" fn wrapper(l: *mut lua_State) -> c_int {
            let state = State::from_ptr(l);
            let fp = state.to_pointer(lua_upvalueindex(1));
            let fp: fn(State) -> c_int = mem::transmute(fp);
            fp(state)
        }

        s.push_light_userdata(self as usize as *mut ());
        s.push_cclosure(Some(wrapper), 1);
    }
}

impl ToLua for NilVal {
    #[inline(always)]
    fn to_lua(self, s: &State) {
        s.push_nil();
    }
}

impl ToLua for CRegRef {
    #[inline(always)]
    fn to_lua(self, s: &State) {
        s.c_reg().geti(self.0 as i64);
    }
}

impl ToLua for StackRef {
    #[inline(always)]
    fn to_lua(self, s: &State) {
        s.push_value(self.0);
    }
}

impl ToLua for Number {
    #[inline(always)]
    fn to_lua(self, s: &State) {
        s.push_number(self)
    }
}

impl ToLua for f32 {
    #[inline(always)]
    fn to_lua(self, s: &State) {
        s.push_number(self as Number)
    }
}

impl ToLua for bool {
    #[inline(always)]
    fn to_lua(self, s: &State) {
        s.push_bool(self)
    }
}

impl ToLua for CFunction {
    #[inline(always)]
    fn to_lua(self, s: &State) {
        s.push_fn(Some(self))
    }
}

impl<T: ToLua> ToLua for Option<T> {
    #[inline(always)]
    default fn to_lua(self, s: &State) {
        match self {
            Some(value) => value.to_lua(s),
            None => s.push_nil(),
        }
    }
}

impl ToLua for Vec<u8> {
    #[inline(always)]
    fn to_lua(self, s: &State) {
        s.push_bytes(&self);
    }
}

/// Trait for types that can be taken from the Lua stack.
///
/// It is important that implementors of this trait ensure that `from_lua`
/// behaves like one of the `lua_to*` functions for consistency.
pub trait FromLua: Sized {
    const TYPE_NAME: &'static str = core::any::type_name::<Self>();

    /// Converts the value on top of the stack of a Lua state to a value of type
    /// `Option<Self>`.
    fn from_lua(s: &State, i: Index) -> Option<Self>;
    fn check(s: &State, i: Index) -> Self {
        if let Some(result) = Self::from_lua(s, i) {
            result
        } else {
            let tname = CString::new(Self::TYPE_NAME).unwrap_or_default();
            s.type_error(i, &tname);
        }
    }
}

impl FromLua for AnyVal {
    #[inline(always)]
    fn from_lua(_s: &State, _i: Index) -> Option<AnyVal> {
        Some(AnyVal)
    }
    #[inline(always)]
    fn check(_s: &State, _i: Index) -> Self {
        AnyVal
    }
}

impl FromLua for String {
    #[inline(always)]
    fn from_lua(s: &State, i: Index) -> Option<String> {
        s.to_str(i).map(ToOwned::to_owned)
    }
}

impl FromLua for &str {
    #[inline(always)]
    fn from_lua(s: &State, i: Index) -> Option<&'static str> {
        s.to_str(i)
    }
}

impl FromLua for Vec<u8> {
    #[inline(always)]
    fn from_lua(s: &State, i: Index) -> Option<Vec<u8>> {
        s.to_bytes(i).map(ToOwned::to_owned)
    }
}

impl FromLua for Value {
    #[inline(always)]
    fn from_lua(s: &State, i: Index) -> Option<Value> {
        Some(s.value(i))
    }
}

impl<'a> FromLua for &'a [u8] {
    #[inline(always)]
    fn from_lua(s: &State, i: Index) -> Option<&'a [u8]> {
        s.to_bytes(i).or_else(|| unsafe {
            let p = s.to_userdata(i);
            if p.is_null() {
                None
            } else {
                Some(core::slice::from_raw_parts(
                    p.cast::<u8>(),
                    s.raw_len(i) as _,
                ))
            }
        })
    }
}

impl<'a, T: UserData> FromLua for &'a T {
    const TYPE_NAME: &'static str = T::TYPE_NAME;

    #[inline(always)]
    fn from_lua(s: &State, i: Index) -> Option<&'a T> {
        unsafe {
            if T::IS_POINTER {
                core::mem::transmute(*s.test_userdata_meta_::<*mut T>(i, T::init_metatable))
            } else {
                core::mem::transmute(s.test_userdata_meta_::<T>(i, T::init_metatable))
            }
        }
    }
}

impl<'a, T: UserData> FromLua for &'a mut T {
    const TYPE_NAME: &'static str = T::TYPE_NAME;

    #[inline(always)]
    fn from_lua(s: &State, i: Index) -> Option<&'a mut T> {
        unsafe {
            if T::IS_POINTER {
                core::mem::transmute(*s.test_userdata_meta_::<*mut T>(i, T::init_metatable))
            } else {
                core::mem::transmute(s.test_userdata_meta_::<T>(i, T::init_metatable))
            }
        }
    }
}

impl FromLua for f64 {
    #[inline(always)]
    fn from_lua(s: &State, i: Index) -> Option<f64> {
        s.to_numberx(i)
    }
}

impl FromLua for f32 {
    #[inline(always)]
    fn from_lua(s: &State, i: Index) -> Option<f32> {
        s.to_numberx(i).map(|r| r as f32)
    }
}

impl FromLua for bool {
    #[inline(always)]
    fn from_lua(s: &State, i: Index) -> Option<bool> {
        Some(s.to_bool(i))
    }
}

impl FromLua for StrictBool {
    fn from_lua(s: &State, i: Index) -> Option<StrictBool> {
        if s.is_bool(i) {
            Some(StrictBool(s.to_bool(i)))
        } else {
            None
        }
    }
}

impl<T: FromLua> FromLua for Option<T> {
    #[inline(always)]
    fn from_lua(s: &State, i: Index) -> Option<Option<T>> {
        Some(T::from_lua(s, i))
    }
}

pub struct LuaInt(pub usize);

impl FromLua for LuaInt {
    fn from_lua(s: &State, i: Index) -> Option<Self> {
        if s.is_integer(i) {
            Some(Self(s.to_integer(i) as usize))
        } else if s.is_number(i) {
            Some(Self(s.to_number(i) as usize))
        } else {
            None
        }
    }
}

macro_rules! impl_integer {
    ($($t:ty) *) => {
        $(
        impl ToLua for $t {
            #[inline(always)]
            fn to_lua(self, s: &State) {
                s.push_integer(self as _);
            }
        }

        impl FromLua for $t {
            #[inline(always)]
            fn from_lua(s: &State, i: Index) -> Option<$t> {
                let t = unsafe { lua_type(s.as_ptr(), i) };
                if LUA_TNUMBER == t {
                    Some(s.to_integer(i) as $t)
                } else { None }
            }
        }
        )*
    }
}

impl_integer!(isize usize u8 u16 u32 u64 i8 i16 i32 Integer);

pub trait ToLuaMulti: Sized {
    fn to_lua(self, _s: &State) -> c_int;
}

pub trait FromLuaMulti: Sized {
    const COUNT: usize = 0;
    fn from_lua(_s: &State, _begin: Index) -> Option<Self> {
        None
    }
}

impl FromLuaMulti for () {
    const COUNT: usize = 0;
    fn from_lua(_s: &State, _begin: Index) -> Option<Self> {
        Some(())
    }
}

impl ToLuaMulti for () {
    #[inline(always)]
    default fn to_lua(self, s: &State) -> c_int {
        0
    }
}

// impl<T: ToLuaMulti> ToLuaMulti for Option<T> {
//     #[inline(always)]
//     default fn to_lua(self, s: &State) -> c_int {
//         match self {
//             Some(val) => val.to_lua(s),
//             None        => 0,
//         }
//     }
// }

impl<T: ToLua> ToLuaMulti for T {
    #[inline(always)]
    default fn to_lua(self, s: &State) -> c_int {
        ToLua::to_lua(self, s);
        1
    }
}

impl ToLuaMulti for Pushed {
    #[inline(always)]
    fn to_lua(self, s: &State) -> c_int {
        self.0
    }
}

impl ToLuaMulti for Option<Pushed> {
    #[inline(always)]
    fn to_lua(self, s: &State) -> c_int {
        match self {
            Some(val) => val.to_lua(s),
            None => 0,
        }
    }
}

impl<T: FromLua> FromLuaMulti for T {
    const COUNT: usize = 1;

    #[inline(always)]
    fn from_lua(s: &State, begin: Index) -> Option<Self> {
        T::from_lua(s, begin)
    }
}

impl<T: ToLuaMulti, E: Debug> ToLuaMulti for Result<T, E> {
    #[inline(always)]
    fn to_lua(self, s: &State) -> c_int {
        match self {
            Ok(val) => val.to_lua(s),
            Err(e) => s.raise_error(e),
        }
    }
}

macro_rules! replace_expr {
    ($_t:tt $sub:expr) => {
        $sub
    };
}

macro_rules! count_tts {
    ($($tts:tt)*) => {0usize $(+ replace_expr!($tts 1usize))*};
}

macro_rules! impl_tuple {
    ($(($x:ident, $i:tt)) +) => (
        impl<$($x,)*> ToLuaMulti for ($($x,)*) where $($x: ToLua,)* {
            #[inline(always)]
            fn to_lua(self, s: &State) -> c_int {
                $(s.push(self.$i);)*
                (count_tts!($($x)*)) as _
            }
        }

        impl<$($x,)*> ToLuaMulti for Option<($($x,)*)> where $($x: ToLua,)* {
            #[inline(always)]
            fn to_lua(self, s: &State) -> c_int {
                match self {
                    Some(val) => val.to_lua(s),
                    None      => 0,
                }
            }
        }

        impl<$($x,)*> FromLuaMulti for ($($x,)*) where $($x: FromLua,)* {
            const COUNT: usize = (count_tts!($($x)*));

            #[inline(always)]
            fn from_lua(s: &State, begin: Index) -> Option<Self> {
                Some(( $($x::from_lua(s, begin + $i)?,)* ))
            }
        }
    );
}

impl_tuple!((A, 0));
impl_tuple!((A, 0)(B, 1));
impl_tuple!((A, 0)(B, 1)(C, 2));
impl_tuple!((A, 0)(B, 1)(C, 2)(D, 3));
impl_tuple!((A, 0)(B, 1)(C, 2)(D, 3)(E, 4));
impl_tuple!((A, 0)(B, 1)(C, 2)(D, 3)(E, 4)(F, 5));
impl_tuple!((A, 0)(B, 1)(C, 2)(D, 3)(E, 4)(F, 5)(G, 6));
impl_tuple!((A, 0)(B, 1)(C, 2)(D, 3)(E, 4)(F, 5)(G, 6)(H, 7));
impl_tuple!((A, 0)(B, 1)(C, 2)(D, 3)(E, 4)(F, 5)(G, 6)(H, 7)(I, 8));
impl_tuple!((A, 0)(B, 1)(C, 2)(D, 3)(E, 4)(F, 5)(G, 6)(H, 7)(I, 8)(J, 9));
impl_tuple!((A, 0)(B, 1)(C, 2)(D, 3)(E, 4)(F, 5)(G, 6)(H, 7)(I, 8)(J, 9)(K, 10));
impl_tuple!((A, 0)(B, 1)(C, 2)(D, 3)(E, 4)(F, 5)(G, 6)(H, 7)(I, 8)(J, 9)(K, 10)(L, 11));
impl_tuple!((A, 0)(B, 1)(C, 2)(D, 3)(E, 4)(F, 5)(G, 6)(H, 7)(I, 8)(J, 9)(K, 10)(L, 11)(M, 12));

macro_rules! getfn {
    ($s:ident, $f:ident) => {
        #[allow(unused_assignments)]
        let mut pfn = core::mem::transmute(1usize);
        if core::mem::size_of::<Self>() == core::mem::size_of::<usize>() {
            pfn = $s.to_userdata(ffi::lua_upvalueindex(1));
            $f = core::mem::transmute(&pfn)
        } else if core::mem::size_of::<Self>() == 0 {
            $f = core::mem::transmute(pfn);
        } else {
            pfn = $s.to_userdata(ffi::lua_upvalueindex(1));
            $f = core::mem::transmute(pfn)
        };
    };
}

macro_rules! impl_luafn {
    ($(($x:ident, $i:tt)) *) => (
        impl<'a, FN: Fn($($x,)*)->RET + 'static, $($x: FromLua+'a,)* RET: ToLuaMulti+'a> LuaFn<'a, (), ($($x,)*), RET> for FN {
            unsafe extern "C" fn wrapper(l: *mut lua_State) -> c_int {
                let s = State::from_ptr(l);
                let f: &Self;
                getfn!(s, f);
                s.pushx(f($($x::check(&s, 1 + $i),)*))
            }
        }

        impl<'a, FN: Fn(&State, $($x,)*)->RET + 'static, $($x: FromLua,)* RET: ToLuaMulti> LuaFn<'a, (), (State, $($x,)*), RET> for FN {
            unsafe extern "C" fn wrapper(l: *mut lua_State) -> c_int {
                let s = State::from_ptr(l);
                let f: &Self;
                getfn!(s, f);
                s.pushx(f(&s, $($x::check(&s, 1 + $i),)*))
            }
        }

        impl<'a, FN: Fn(THIS, &State, $($x,)*)->RET + 'static, THIS: FromLua, $($x: FromLua,)* RET: ToLuaMulti> LuaFn<'a, (), (THIS, State, $($x,)*), RET> for FN {
            unsafe extern "C" fn wrapper(l: *mut lua_State) -> c_int {
                let s = State::from_ptr(l);
                let f: &Self;
                getfn!(s, f);
                s.pushx(f(THIS::check(&s, 1), &s, $($x::check(&s, 2 + $i),)*))
            }
        }

        #[allow(unused_parens)]
        impl<'a, FN: for<'r> Fn(&'r T $(,$x)*)->RET, T: ?Sized, THIS: UserData+AsRef<T>+'a, $($x: FromLua,)* RET: ToLuaMulti> LuaFn<'a, (THIS, &'a T), ($($x,)*), RET> for FN {
            unsafe extern "C" fn wrapper(l: *mut lua_State) -> c_int {
                let s = State::from_ptr(l);
                let f: &Self;
                getfn!(s, f);
                let this = <&THIS as FromLua>::check(&s, 1);
                s.pushx(f(this.as_ref(), $($x::check(&s, 2 + $i),)*))
            }
        }

        #[allow(unused_parens)]
        impl<'a, FN: for<'r> Fn(&'r mut T $(,$x)*)->RET, T: ?Sized, THIS: UserData+AsMut<T>+'a, $($x: FromLua,)* RET: ToLuaMulti> LuaFn<'a, (THIS, &'a mut T), ($($x,)*), RET> for FN {
            unsafe extern "C" fn wrapper(l: *mut lua_State) -> c_int {
                let s = State::from_ptr(l);
                let f: &Self;
                getfn!(s, f);
                let this = <&mut THIS as FromLua>::check(&s, 1);
                s.pushx(f(this.as_mut(), $($x::check(&s, 2 + $i),)*))
            }
        }
    );
}

impl_luafn!();
impl_luafn!((A, 0));
impl_luafn!((A, 0)(B, 1));
impl_luafn!((A, 0)(B, 1)(C, 2));
impl_luafn!((A, 0)(B, 1)(C, 2)(D, 3));
impl_luafn!((A, 0)(B, 1)(C, 2)(D, 3)(E, 4));
impl_luafn!((A, 0)(B, 1)(C, 2)(D, 3)(E, 4)(F, 5));
impl_luafn!((A, 0)(B, 1)(C, 2)(D, 3)(E, 4)(F, 5)(G, 6));
impl_luafn!((A, 0)(B, 1)(C, 2)(D, 3)(E, 4)(F, 5)(G, 6)(H, 7));
impl_luafn!((A, 0)(B, 1)(C, 2)(D, 3)(E, 4)(F, 5)(G, 6)(H, 7)(I, 8));
impl_luafn!((A, 0)(B, 1)(C, 2)(D, 3)(E, 4)(F, 5)(G, 6)(H, 7)(I, 8)(J, 9));
impl_luafn!((A, 0)(B, 1)(C, 2)(D, 3)(E, 4)(F, 5)(G, 6)(H, 7)(I, 8)(J, 9)(K, 10));
impl_luafn!((A, 0)(B, 1)(C, 2)(D, 3)(E, 4)(F, 5)(G, 6)(H, 7)(I, 8)(J, 9)(K, 10)(L, 11));
impl_luafn!((A, 0)(B, 1)(C, 2)(D, 3)(E, 4)(F, 5)(G, 6)(H, 7)(I, 8)(J, 9)(K, 10)(L, 11)(M, 12));

impl State {
    #[inline(always)]
    pub fn pushed<T: ToLuaMulti>(&self, t: T) -> Pushed {
        Pushed(self.pushx(t))
    }
}

impl ValRef<'_> {
    #[inline(always)]
    pub fn register_fn<'a, K: ToLua, V: LuaFn<'a, (), ARGS, RET>, ARGS, RET>(
        &self,
        k: K,
        v: V,
    ) -> &Self {
        self.push(k);
        self.push(RsFn::new(v));
        self.set_table(self.index);
        self
    }
}

pub struct MethodRegistry<'a, T, D: ?Sized>(ValRef<'a>, PhantomData<(T, D)>);

impl<'a, 'b, T: AsRef<D> + 'b, D> MethodRegistry<'a, T, D>
where
    D: ?Sized + 'b,
{
    pub fn new(mt: &'a ValRef) -> MethodRegistry<'a, T, D> {
        Self(*mt, PhantomData)
    }

    #[inline]
    pub fn register<K, V, ARGS, RET>(&self, k: K, v: V) -> &Self
    where
        K: ToLua,
        V: LuaFn<'b, (T, &'b D), ARGS, RET>,
    {
        self.0.push(k);
        self.0.push(RsFn(v, PhantomData));
        self.0.set_table(self.0.index);
        self
    }
}

pub struct MethodRegistryMut<'a, T, D: ?Sized>(ValRef<'a>, PhantomData<(T, D)>);

impl<'a, 'b, T: AsMut<D> + 'b, D> MethodRegistryMut<'a, T, D>
where
    D: ?Sized + 'b,
{
    pub fn new(mt: &'a ValRef) -> MethodRegistryMut<'a, T, D> {
        Self(*mt, PhantomData)
    }

    #[inline]
    pub fn register<K, V, ARGS, RET>(&self, k: K, v: V) -> &Self
    where
        K: ToLua,
        V: LuaFn<'b, (T, &'b mut D), ARGS, RET>,
    {
        self.0.push(k);
        self.0.push(RsFn(v, PhantomData));
        self.0.set_table(self.0.index);
        self
    }
}
