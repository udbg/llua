use super::error::Error;
use super::ValRef;
use super::{ffi::*, str::*, Index, UserData};

use alloc::borrow::Cow;
use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec::Vec;
use core::mem::MaybeUninit;
use core::{mem, ptr, slice, str};
use libc::{c_char, c_int, c_void, size_t};

pub type InitMetatable = fn(&ValRef);

/// Arithmetic operations for `lua_arith`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Arithmetic {
    Add = LUA_OPADD as isize,
    Sub = LUA_OPSUB as isize,
    Mul = LUA_OPMUL as isize,
    Mod = LUA_OPMOD as isize,
    Pow = LUA_OPPOW as isize,
    Div = LUA_OPDIV as isize,
    IDiv = LUA_OPIDIV as isize,
    BAnd = LUA_OPBAND as isize,
    BOr = LUA_OPBOR as isize,
    BXor = LUA_OPBXOR as isize,
    Shl = LUA_OPSHL as isize,
    Shr = LUA_OPSHR as isize,
    Unm = LUA_OPUNM as isize,
    BNot = LUA_OPBNOT as isize,
}

/// Comparison operations for `lua_compare`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Comparison {
    Eq = LUA_OPEQ as isize,
    Lt = LUA_OPLT as isize,
    Le = LUA_OPLE as isize,
}

/// Status of a Lua state.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ThreadStatus {
    Ok = LUA_OK as isize,
    Yield = LUA_YIELD as isize,
    RuntimeError = LUA_ERRRUN as isize,
    SyntaxError = LUA_ERRSYNTAX as isize,
    MemoryError = LUA_ERRMEM as isize,
    GcError = LUA_ERRGCMM as isize,
    MessageHandlerError = LUA_ERRERR as isize,
    FileError = LUA_ERRFILE as isize,
}

impl ThreadStatus {
    pub(crate) fn from_c_int(i: c_int) -> ThreadStatus {
        match i {
            LUA_OK => ThreadStatus::Ok,
            LUA_YIELD => ThreadStatus::Yield,
            LUA_ERRRUN => ThreadStatus::RuntimeError,
            LUA_ERRSYNTAX => ThreadStatus::SyntaxError,
            LUA_ERRMEM => ThreadStatus::MemoryError,
            LUA_ERRGCMM => ThreadStatus::GcError,
            LUA_ERRERR => ThreadStatus::MessageHandlerError,
            LUA_ERRFILE => ThreadStatus::FileError,
            _ => panic!("Unknown Lua error code: {}", i),
        }
    }

    pub fn is_ok(self) -> bool {
        matches!(self, Self::Ok)
    }

    /// Returns `true` for error statuses and `false` for `Ok` and `Yield`.
    pub fn is_err(self) -> bool {
        match self {
            ThreadStatus::RuntimeError
            | ThreadStatus::SyntaxError
            | ThreadStatus::MemoryError
            | ThreadStatus::GcError
            | ThreadStatus::MessageHandlerError
            | ThreadStatus::FileError => true,
            ThreadStatus::Ok | ThreadStatus::Yield => false,
        }
    }

    pub fn chk_err(self, s: &State) {
        if self != Self::Ok {
            panic!("{}", s.to_str(-1).unwrap_or("<error>"));
        }
    }

    pub fn check(self, s: &State, msg: &str) -> Result<(), String> {
        if self != Self::Ok {
            Err(format!("{}: {}", msg, s.to_str(-1).unwrap_or_default()))
        } else {
            Ok(())
        }
    }
}

/// Options for the Lua garbage collector.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GcOption {
    Stop = LUA_GCSTOP as isize,
    Restart = LUA_GCRESTART as isize,
    Collect = LUA_GCCOLLECT as isize,
    Count = LUA_GCCOUNT as isize,
    CountBytes = LUA_GCCOUNTB as isize,
    Step = LUA_GCSTEP as isize,
    SetPause = LUA_GCSETPAUSE as isize,
    SetStepMul = LUA_GCSETSTEPMUL as isize,
    IsRunning = LUA_GCISRUNNING as isize,
}

/// Represents all possible Lua data types.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Type {
    None = LUA_TNONE as isize,
    Nil = LUA_TNIL as isize,
    Boolean = LUA_TBOOLEAN as isize,
    LightUserdata = LUA_TLIGHTUSERDATA as isize,
    Number = LUA_TNUMBER as isize,
    String = LUA_TSTRING as isize,
    Table = LUA_TTABLE as isize,
    Function = LUA_TFUNCTION as isize,
    Userdata = LUA_TUSERDATA as isize,
    Thread = LUA_TTHREAD as isize,
    Invalid,
}

impl Type {
    fn from_c_int(i: c_int) -> Type {
        match i {
            LUA_TNIL => Type::Nil,
            LUA_TBOOLEAN => Type::Boolean,
            LUA_TLIGHTUSERDATA => Type::LightUserdata,
            LUA_TNUMBER => Type::Number,
            LUA_TSTRING => Type::String,
            LUA_TTABLE => Type::Table,
            LUA_TFUNCTION => Type::Function,
            LUA_TUSERDATA => Type::Userdata,
            LUA_TTHREAD => Type::Thread,
            _ => Type::Invalid,
        }
    }

    pub fn is_none_or_nil(&self) -> bool {
        matches!(*self, Type::None | Type::Nil)
    }
}

#[derive(Debug, Clone, Copy)]
pub enum Value<'a> {
    None,
    Nil,
    Int(LUA_INTEGER),
    Num(LUA_NUMBER),
    Str(&'a str),
    Bool(bool),
    LightUserdata,
    Table,
    Function,
    Userdata,
    Thread,
}

/// Type of Lua references generated through `reference` and `unreference`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Reference(pub c_int);

/// The result of `reference` for `nil` values.
pub const REFNIL: Reference = Reference(LUA_REFNIL);

/// A value that will never be returned by `reference`.
pub const NOREF: Reference = Reference(LUA_REFNIL);

impl Reference {
    /// Returns `true` if this reference is equal to `REFNIL`.
    pub fn is_nil_ref(self) -> bool {
        self == REFNIL
    }

    /// Returns `true` if this reference is equal to `NOREF`.
    pub fn is_no_ref(self) -> bool {
        self == NOREF
    }

    /// Convenience function that returns the value of this reference.
    pub fn value(self) -> c_int {
        let Reference(value) = self;
        value
    }
}

impl From<c_int> for Reference {
    fn from(i: c_int) -> Self {
        Self(i)
    }
}

#[cfg(features = "std")]
bitflags::bitflags! {
    #[doc="Hook point masks for `lua_sethook`."]
    flags HookMask: c_int {
        #[doc="Called when the interpreter calls a function."]
        const MASKCALL  = LUA_MASKCALL,
        #[doc="Called when the interpreter returns from a function."]
        const MASKRET   = LUA_MASKRET,
        #[doc="Called when the interpreter is about to start the execution of a new line of code."]
        const MASKLINE  = LUA_MASKLINE,
        #[doc="Called after the interpreter executes every `count` instructions."]
        const MASKCOUNT = LUA_MASKCOUNT
    }
}

#[derive(Debug, PartialEq, Eq)]
#[repr(C)]
pub struct State(*mut lua_State);

impl State {
    /// Initializes a new Lua state. This function does not open any libraries
    /// by default. Calls `lua_newstate` internally.
    pub fn new() -> State {
        unsafe { State(luaL_newstate()) }
    }

    /// Constructs a wrapper `State` from a raw pointer. This is suitable for use
    /// inside of native functions that accept a `lua_State` to obtain a wrapper.
    #[inline(always)]
    pub unsafe fn from_ptr(L: *mut lua_State) -> State {
        State(L)
    }

    #[inline(always)]
    pub unsafe fn copy_state(&self) -> State {
        State(self.as_ptr())
    }

    /// Returns an unsafe pointer to the wrapped `lua_State`.
    #[inline(always)]
    pub fn as_ptr(&self) -> *mut lua_State {
        self.0
    }

    /// Maps to `luaL_openlibs`.
    pub fn open_libs(&self) {
        unsafe {
            luaL_openlibs(self.0);
        }
    }

    /// Maps to `luaopen_base`.
    #[inline(always)]
    pub fn open_base(&self) -> c_int {
        unsafe { luaopen_base(self.0) }
    }

    /// Maps to `luaopen_coroutine`.
    #[inline(always)]
    pub fn open_coroutine(&self) -> c_int {
        unsafe { luaopen_coroutine(self.0) }
    }

    /// Maps to `luaopen_table`.
    #[inline(always)]
    pub fn open_table(&self) -> c_int {
        unsafe { luaopen_table(self.0) }
    }

    /// Maps to `luaopen_io`.
    #[inline(always)]
    pub fn open_io(&self) -> c_int {
        unsafe { luaopen_io(self.0) }
    }

    /// Maps to `luaopen_os`.
    #[inline(always)]
    pub fn open_os(&self) -> c_int {
        unsafe { luaopen_os(self.0) }
    }

    /// Maps to `luaopen_string`.
    #[inline(always)]
    pub fn open_string(&self) -> c_int {
        unsafe { luaopen_string(self.0) }
    }

    /// Maps to `luaopen_utf8`.
    #[inline(always)]
    pub fn open_utf8(&self) -> c_int {
        unsafe { luaopen_utf8(self.0) }
    }

    /// Maps to `luaopen_math`.
    #[inline(always)]
    pub fn open_math(&self) -> c_int {
        unsafe { luaopen_math(self.0) }
    }

    /// Maps to `luaopen_debug`.
    #[inline(always)]
    pub fn open_debug(&self) -> c_int {
        unsafe { luaopen_debug(self.0) }
    }

    /// Maps to `luaopen_package`.
    #[inline(always)]
    pub fn open_package(&self) -> c_int {
        unsafe { luaopen_package(self.0) }
    }

    /// Maps to `luaL_dofile`.
    pub fn do_file(&self, filename: &str) -> Result<(), Error> {
        let c_str = CString::new(filename).unwrap();
        let result = unsafe { luaL_dofile(self.0, c_str.as_ptr()) };
        self.to_error(ThreadStatus::from_c_int(result))
    }

    /// Maps to `luaL_dostring`.
    pub fn do_string(&self, s: &str) -> Result<(), Error> {
        let c_str = CString::new(s).unwrap();
        let result = unsafe { luaL_dostring(self.0, c_str.as_ptr()) };
        self.to_error(ThreadStatus::from_c_int(result))
    }

    //===========================================================================
    // State manipulation
    //===========================================================================
    /// Maps to `lua_close`.
    #[inline(always)]
    pub fn close(self) {
        unsafe {
            lua_close(self.0);
        }
    }

    /// [-0, +1, m] Maps to `lua_newthread`.
    #[inline(always)]
    pub fn new_thread(&self) -> State {
        unsafe { State::from_ptr(lua_newthread(self.0)) }
    }

    /// Maps to `lua_atpanic`.
    #[inline(always)]
    pub fn at_panic(&self, panicf: lua_CFunction) -> lua_CFunction {
        unsafe { lua_atpanic(self.0, panicf) }
    }

    /// Maps to `lua_version`.
    pub fn version(state: Option<&mut State>) -> lua_Number {
        let ptr = match state {
            Some(state) => state.0,
            None => ptr::null_mut(),
        };
        unsafe { *lua_version(ptr) }
    }

    //===========================================================================
    // Basic stack manipulation
    //===========================================================================
    /// Maps to `lua_absindex`.
    #[inline(always)]
    pub fn abs_index(&self, idx: Index) -> Index {
        unsafe { lua_absindex(self.0, idx) }
    }

    /// Maps to `lua_gettop`.
    #[inline(always)]
    pub fn get_top(&self) -> Index {
        unsafe { lua_gettop(self.0) }
    }

    /// Maps to `lua_settop`.
    #[inline(always)]
    pub fn set_top(&self, index: Index) {
        unsafe { lua_settop(self.0, index) }
    }

    /// Maps to `lua_pushvalue`.
    #[inline(always)]
    pub fn push_value(&self, index: Index) {
        unsafe { lua_pushvalue(self.0, index) }
    }

    /// Maps to `lua_rotate`.
    #[inline(always)]
    pub fn rotate(&self, idx: Index, n: c_int) {
        unsafe { lua_rotate(self.0, idx, n) }
    }

    /// Maps to `lua_copy`.
    #[inline(always)]
    pub fn copy(&self, from_idx: Index, to_idx: Index) {
        unsafe { lua_copy(self.0, from_idx, to_idx) }
    }

    /// Maps to `lua_checkstack`.
    #[inline(always)]
    pub fn check_stack(&self, extra: c_int) -> bool {
        let result = unsafe { lua_checkstack(self.0, extra) };
        result != 0
    }

    /// Maps to `lua_xmove`.
    #[inline(always)]
    pub fn xmove(&self, to: &State, n: c_int) {
        unsafe { lua_xmove(self.0, to.0, n) }
    }

    //===========================================================================
    // Access functions (stack -> C)
    //===========================================================================
    /// Maps to `lua_isnumber`.
    #[inline(always)]
    pub fn is_number(&self, index: Index) -> bool {
        unsafe { lua_isnumber(self.0, index) == 1 }
    }

    /// Maps to `lua_isstring`.
    #[inline(always)]
    pub fn is_string(&self, index: Index) -> bool {
        unsafe { lua_isstring(self.0, index) == 1 }
    }

    /// Maps to `lua_iscfunction`.
    #[inline(always)]
    pub fn is_native_fn(&self, index: Index) -> bool {
        unsafe { lua_iscfunction(self.0, index) == 1 }
    }

    /// Maps to `lua_isinteger`.
    #[inline(always)]
    pub fn is_integer(&self, index: Index) -> bool {
        unsafe { lua_isinteger(self.0, index) == 1 }
    }

    /// Maps to `lua_isuserdata`.
    #[inline(always)]
    pub fn is_userdata(&self, index: Index) -> bool {
        unsafe { lua_isuserdata(self.0, index) == 1 }
    }

    /// Maps to `lua_type`.
    #[inline(always)]
    pub fn type_of(&self, index: Index) -> Type {
        let result = unsafe { lua_type(self.0, index) };
        Type::from_c_int(result)
    }

    /// Maps to `lua_typename`.
    #[inline(always)]
    pub fn typename_of(&self, tp: Type) -> Cow<str> {
        unsafe {
            let ptr = lua_typename(self.0, tp as c_int);
            let slice = CStr::from_ptr(ptr).to_bytes();
            String::from_utf8_lossy(slice)
        }
    }

    /// Maps to `luaL_typename`.
    #[inline(always)]
    pub fn typename_at(&self, n: Index) -> Cow<str> {
        self.typename_of(self.type_of(n))
    }

    /// Maps to `lua_toboolean`.
    #[inline(always)]
    pub fn to_bool(&self, index: Index) -> bool {
        let result = unsafe { lua_toboolean(self.0, index) };
        result != 0
    }

    // omitted: lua_tolstring

    /// Maps to `lua_rawlen`.
    #[inline(always)]
    pub fn raw_len(&self, index: Index) -> size_t {
        unsafe { lua_rawlen(self.0, index) }
    }

    /// Maps to `lua_tocfunction`.
    #[inline(always)]
    pub fn to_native_fn(&self, index: Index) -> lua_CFunction {
        let result = unsafe { lua_tocfunction(self.0, index) };
        result
    }

    /// Maps to `lua_touserdata`.
    #[inline(always)]
    pub fn to_userdata(&self, index: Index) -> *mut c_void {
        unsafe { lua_touserdata(self.0, index) }
    }

    /// Convenience function that calls `to_userdata` and performs a cast.
    //#[unstable(reason="this is an experimental function")]
    pub unsafe fn to_userdata_typed<'a, T>(&'a self, index: Index) -> Option<&'a mut T> {
        mem::transmute(self.to_userdata(index))
    }

    pub unsafe fn check_userdata_typed<'a, T>(&'a self, index: Index) -> &'a mut T {
        luaL_checktype(self.0, index, LUA_TUSERDATA);
        mem::transmute(self.to_userdata(index))
    }

    #[inline(always)]
    pub fn get_userdata_by_size<'a, T>(&'a self, index: Index) -> Option<&'a mut T> {
        unsafe {
            if self.type_of(index) == Type::Userdata
                && self.raw_len(index) as usize == mem::size_of::<T>()
            {
                Some(mem::transmute(self.to_userdata(index)))
            } else {
                None
            }
        }
    }

    /// Maps to `lua_tothread`.
    #[inline]
    pub fn to_thread(&self, index: Index) -> Option<State> {
        let state = unsafe { lua_tothread(self.0, index) };
        if state.is_null() {
            None
        } else {
            Some(unsafe { State::from_ptr(state) })
        }
    }

    /// Maps to `lua_topointer`.
    #[inline(always)]
    pub fn to_pointer(&self, index: Index) -> *const c_void {
        unsafe { lua_topointer(self.0, index) }
    }

    //===========================================================================
    // Comparison and arithmetic functions
    //===========================================================================
    /// Maps to `lua_arith`.
    #[inline(always)]
    pub fn arith(&self, op: Arithmetic) {
        unsafe { lua_arith(self.0, op as c_int) }
    }

    /// Maps to `lua_rawequal`.
    #[inline(always)]
    pub fn raw_equal(&self, idx1: Index, idx2: Index) -> bool {
        let result = unsafe { lua_rawequal(self.0, idx1, idx2) };
        result != 0
    }

    /// Maps to `lua_compare`.
    #[inline(always)]
    pub fn compare(&self, idx1: Index, idx2: Index, op: Comparison) -> bool {
        let result = unsafe { lua_compare(self.0, idx1, idx2, op as c_int) };
        result != 0
    }

    //===========================================================================
    // Push functions (C -> stack)
    //===========================================================================
    /// Maps to `lua_pushnil`.
    #[inline(always)]
    pub fn push_nil(&self) {
        unsafe { lua_pushnil(self.0) }
    }

    /// Maps to `lua_pushnumber`.
    #[inline(always)]
    pub fn push_number(&self, n: lua_Number) {
        unsafe { lua_pushnumber(self.0, n) }
    }

    /// Maps to `lua_pushinteger`.
    #[inline(always)]
    pub fn push_integer(&self, i: lua_Integer) {
        unsafe { lua_pushinteger(self.0, i) }
    }

    // omitted: lua_pushstring

    /// Maps to `lua_pushlstring`.
    #[inline(always)]
    pub fn push_string(&self, s: &str) {
        unsafe { lua_pushlstring(self.0, s.as_ptr() as *const _, s.len() as size_t) };
    }

    /// Maps to `lua_pushlstring`.
    #[inline(always)]
    pub fn push_bytes(&self, s: &[u8]) {
        unsafe { lua_pushlstring(self.0, s.as_ptr() as *const _, s.len() as size_t) };
    }

    // omitted: lua_pushvfstring
    // omitted: lua_pushfstring

    /// Maps to `lua_pushcclosure`.
    #[inline(always)]
    pub fn push_cclosure(&self, f: lua_CFunction, n: c_int) {
        unsafe { lua_pushcclosure(self.0, f, n) }
    }

    /// Maps to `lua_pushboolean`.
    #[inline(always)]
    pub fn push_bool(&self, b: bool) {
        unsafe { lua_pushboolean(self.0, b as c_int) }
    }

    /// Maps to `lua_pushlightuserdata`. The Lua state will receive a pointer to
    /// the given value. The caller is responsible for cleaning up the data. Any
    /// code that manipulates the userdata is free to modify its contents, so
    /// memory safety is not guaranteed.
    #[inline(always)]
    pub fn push_light_userdata<T>(&self, ud: *mut T) {
        unsafe { lua_pushlightuserdata(self.0, mem::transmute(ud)) }
    }

    /// Maps to `lua_pushthread`.
    pub fn push_thread(&self) -> bool {
        let result = unsafe { lua_pushthread(self.0) };
        result != 1
    }

    //===========================================================================
    // Get functions (Lua -> stack)
    //===========================================================================
    /// [-0, +1, -] `lua_getglobal`.
    #[inline(always)]
    pub fn get_global(&self, name: &CStr) -> Type {
        Type::from_c_int(unsafe { lua_getglobal(self.0, name.as_ptr()) })
    }

    /// Maps to `lua_gettable`.
    #[inline(always)]
    pub fn get_table(&self, index: Index) -> Type {
        let ty = unsafe { lua_gettable(self.0, index) };
        Type::from_c_int(ty)
    }

    /// Maps to `lua_getfield`.
    #[inline(always)]
    pub fn get_field(&self, index: Index, k: &CStr) -> Type {
        Type::from_c_int(unsafe { lua_getfield(self.0, index, k.as_ptr()) })
    }

    /// Maps to `lua_geti`.
    #[inline(always)]
    pub fn geti(&self, index: Index, i: lua_Integer) -> Type {
        let ty = unsafe { lua_geti(self.0, index, i) };
        Type::from_c_int(ty)
    }

    /// [-1, +1, -] `lua_rawget`.
    #[inline(always)]
    pub fn raw_get(&self, index: Index) -> Type {
        let ty = unsafe { lua_rawget(self.0, index) };
        Type::from_c_int(ty)
    }

    /// Maps to `lua_rawgeti`.
    #[inline(always)]
    pub fn raw_geti(&self, index: Index, n: lua_Integer) -> Type {
        let ty = unsafe { lua_rawgeti(self.0, index, n) };
        Type::from_c_int(ty)
    }

    /// [0, +1, -] `lua_rawgetp`.
    #[inline(always)]
    pub fn raw_getp<T>(&self, index: Index, p: *const T) -> Type {
        let ty = unsafe { lua_rawgetp(self.0, index, mem::transmute(p)) };
        Type::from_c_int(ty)
    }

    /// Maps to `lua_createtable`.
    #[inline(always)]
    pub fn create_table(&self, narr: c_int, nrec: c_int) {
        unsafe { lua_createtable(self.0, narr, nrec) }
    }

    /// Maps to `lua_newuserdata`. The pointer returned is owned by the Lua state
    /// and it will be garbage collected when it is no longer in use or the state
    /// is closed. To specify custom cleanup behavior, use a `__gc` metamethod.
    #[inline(always)]
    pub fn new_userdata(&self, sz: size_t) -> *mut c_void {
        unsafe { lua_newuserdata(self.0, sz) }
    }

    #[inline(always)]
    pub fn new_userdatauv(&self, sz: size_t, n: i32) -> *mut c_void {
        unsafe { lua_newuserdatauv(self.0, sz, n) }
    }

    /// [-0, +(0|1), –] `lua_getmetatable`.
    #[inline(always)]
    pub fn get_metatable(&self, objindex: Index) -> bool {
        let result = unsafe { lua_getmetatable(self.0, objindex) };
        result != 0
    }

    /// Maps to `lua_getuservalue`.
    #[inline(always)]
    pub fn get_uservalue(&self, idx: Index) -> Type {
        let result = unsafe { lua_getuservalue(self.0, idx) };
        Type::from_c_int(result)
    }

    /// [-0, +1, –] Maps to `lua_getiuservalue`.
    #[inline(always)]
    pub fn get_iuservalue(&self, idx: Index, n: i32) -> Type {
        let result = unsafe { lua_getiuservalue(self.0, idx, n) };
        Type::from_c_int(result)
    }

    //===========================================================================
    // Set functions (stack -> Lua)
    //===========================================================================
    /// Maps to `lua_setglobal`.
    #[inline(always)]
    pub fn set_global(&self, var: &CStr) {
        unsafe { lua_setglobal(self.0, var.as_ptr()) }
    }

    /// Maps to `lua_settable`.
    #[inline(always)]
    pub fn set_table(&self, idx: Index) {
        unsafe { lua_settable(self.0, idx) }
    }

    /// Maps to `lua_setfield`.
    #[inline(always)]
    pub fn set_field(&self, idx: Index, k: &CStr) {
        unsafe { lua_setfield(self.0, idx, k.as_ptr()) }
    }

    /// Maps to `lua_seti`.
    #[inline(always)]
    pub fn seti(&self, idx: Index, n: lua_Integer) {
        unsafe { lua_seti(self.0, idx, n) }
    }

    /// [-2, +0, m] `lua_rawset`.
    #[inline(always)]
    pub fn raw_set(&self, idx: Index) {
        unsafe { lua_rawset(self.0, idx) }
    }

    /// Maps to `lua_rawseti`.
    #[inline(always)]
    pub fn raw_seti(&self, idx: Index, n: lua_Integer) {
        unsafe { lua_rawseti(self.0, idx, n) }
    }

    /// Maps to `lua_rawsetp`.
    #[inline(always)]
    pub fn raw_setp<T>(&self, idx: Index, p: *const T) {
        unsafe { lua_rawsetp(self.0, idx, mem::transmute(p)) }
    }

    /// [-1, +0, -] `lua_setmetatable`.
    #[inline(always)]
    pub fn set_metatable(&self, objindex: Index) {
        unsafe { lua_setmetatable(self.0, objindex) };
    }

    /// [-1, +0, -] `lua_setuservalue`.
    #[inline(always)]
    pub fn set_uservalue(&self, idx: Index) {
        unsafe {
            lua_setuservalue(self.0, idx);
        }
    }

    /// [-1, +0, -] Maps to `lua_setiuservalue`.
    #[inline(always)]
    pub fn set_iuservalue(&self, idx: Index, n: i32) {
        unsafe {
            lua_setiuservalue(self.0, idx, n);
        }
    }

    //===========================================================================
    // 'load' and 'call' functions (load and run Lua code)
    //===========================================================================
    /// Maps to `lua_callk`.
    // pub fn callk<F>(&self, nargs: c_int, nresults: c_int, continuation: F)
    //     where F: FnOnce(&mut State, ThreadStatus) -> c_int
    //     {
    //         let func = continue_func::<F>;
    //         unsafe {
    //             let ctx = mem::transmute(Box::new(continuation));
    //             lua_callk(self.0, nargs, nresults, ctx, Some(func));
    //             // no yield occurred, so call the continuation
    //             func(self.0, LUA_OK, ctx);
    //         }
    //     }

    /// Maps to `lua_call`.
    #[inline(always)]
    pub fn call(&self, nargs: c_int, nresults: c_int) {
        unsafe { lua_call(self.0, nargs, nresults) }
    }

    /// Maps to `lua_pcall`.
    #[inline(always)]
    pub fn pcall(&self, nargs: c_int, nresults: c_int, msgh: c_int) -> ThreadStatus {
        let result = unsafe { lua_pcall(self.0, nargs, nresults, msgh) };
        ThreadStatus::from_c_int(result)
    }

    //===========================================================================
    // Coroutine functions
    //===========================================================================
    /// Maps to `lua_resume`.
    pub fn resume(&self, from: Option<&State>, nargs: c_int, nresults: &mut c_int) -> ThreadStatus {
        let from_ptr = from.map(|s| s.0).unwrap_or(ptr::null_mut());
        let result = unsafe { lua_resume(self.0, from_ptr, nargs, nresults) };
        ThreadStatus::from_c_int(result)
    }

    /// Maps to `lua_status`.
    #[inline(always)]
    pub fn status(&self) -> ThreadStatus {
        let result = unsafe { lua_status(self.0) };
        ThreadStatus::from_c_int(result)
    }

    /// Maps to `lua_isyieldable`.
    #[inline(always)]
    pub fn is_yieldable(&self) -> bool {
        let result = unsafe { lua_isyieldable(self.0) };
        result != 0
    }

    //===========================================================================
    // Garbage-collection function
    //===========================================================================
    // TODO: return typing?
    /// Maps to `lua_gc`.
    #[inline(always)]
    pub fn gc(&self, what: GcOption, data: c_int) -> c_int {
        unsafe { lua_gc(self.0, what as c_int, data) }
    }

    //===========================================================================
    // Miscellaneous functions
    //===========================================================================
    /// Maps to `lua_error`.
    #[inline(always)]
    pub fn error(&self) -> ! {
        unsafe { lua_error(self.0) };
        unreachable!()
    }

    /// Maps to `lua_next`.
    #[inline(always)]
    pub fn next(&self, idx: Index) -> bool {
        let result = unsafe { lua_next(self.0, idx) };
        result != 0
    }

    /// Maps to `lua_concat`.
    #[inline(always)]
    pub fn concat(&self, n: c_int) {
        unsafe { lua_concat(self.0, n) }
    }

    /// Maps to `lua_len`.
    #[inline(always)]
    pub fn len(&self, idx: Index) {
        unsafe { lua_len(self.0, idx) }
    }

    /// Maps to `lua_stringtonumber`.
    pub fn string_to_number(&self, s: &str) -> size_t {
        let c_str = CString::new(s).unwrap();
        unsafe { lua_stringtonumber(self.0, c_str.as_ptr()) }
    }

    /// Maps to `lua_getallocf`.
    #[inline(always)]
    pub fn get_alloc_fn(&self) -> (lua_Alloc, *mut c_void) {
        let mut slot = ptr::null_mut();
        (unsafe { lua_getallocf(self.0, &mut slot) }, slot)
    }

    /// Maps to `lua_setallocf`.
    #[inline(always)]
    pub fn set_alloc_fn(&self, f: lua_Alloc, ud: *mut c_void) {
        unsafe { lua_setallocf(self.0, f, ud) }
    }

    /// Maps to `lua_tonumber`.
    #[inline(always)]
    pub fn to_number(&self, index: Index) -> lua_Number {
        unsafe { lua_tonumber(self.0, index) }
    }

    /// Maps to `lua_tonumberx`.
    #[inline(always)]
    pub fn to_numberx(&self, index: Index) -> Option<lua_Number> {
        let mut suc = 0i32;
        let r = unsafe { lua_tonumberx(self.0, index, &mut suc) };
        if suc > 0 {
            Some(r)
        } else {
            None
        }
    }

    /// Maps to `lua_tointeger`.
    #[inline(always)]
    pub fn to_integer(&self, index: Index) -> lua_Integer {
        unsafe { lua_tointeger(self.0, index) }
    }

    /// Maps to `lua_tointegerx`.
    #[inline(always)]
    pub fn to_integerx(&self, index: Index) -> Option<lua_Integer> {
        let mut isnum: c_int = 0;
        let r = unsafe { lua_tointegerx(self.0, index, &mut isnum) };
        if isnum == 0 {
            None
        } else {
            Some(r)
        }
    }

    /// Maps to `lua_pop`.
    #[inline(always)]
    pub fn pop(&self, n: c_int) {
        unsafe { lua_pop(self.0, n) }
    }

    /// Maps to `lua_newtable`.
    #[inline(always)]
    pub fn new_table(&self) {
        unsafe { lua_newtable(self.0) }
    }

    /// Maps to `lua_register`.
    #[inline(always)]
    pub fn register(&self, n: &str, f: CFunction) {
        let c_str = CString::new(n).unwrap();
        unsafe { lua_register(self.0, c_str.as_ptr(), Some(f)) }
    }

    /// Maps to `lua_pushcfunction`.
    #[inline(always)]
    pub fn push_fn(&self, f: lua_CFunction) {
        unsafe { lua_pushcfunction(self.0, f) }
    }

    /// Maps to `lua_isfunction`.
    #[inline(always)]
    pub fn is_function(&self, index: Index) -> bool {
        unsafe { lua_isfunction(self.0, index) == 1 }
    }

    /// Maps to `lua_istable`.
    #[inline(always)]
    pub fn is_table(&self, index: Index) -> bool {
        unsafe { lua_istable(self.0, index) == 1 }
    }

    /// Maps to `lua_islightuserdata`.
    #[inline(always)]
    pub fn is_light_userdata(&self, index: Index) -> bool {
        unsafe { lua_islightuserdata(self.0, index) == 1 }
    }

    /// Maps to `lua_isnil`.
    #[inline(always)]
    pub fn is_nil(&self, index: Index) -> bool {
        unsafe { lua_isnil(self.0, index) == 1 }
    }

    /// Maps to `lua_isboolean`.
    #[inline(always)]
    pub fn is_bool(&self, index: Index) -> bool {
        unsafe { lua_isboolean(self.0, index) == 1 }
    }

    /// Maps to `lua_isthread`.
    #[inline(always)]
    pub fn is_thread(&self, index: Index) -> bool {
        unsafe { lua_isthread(self.0, index) == 1 }
    }

    /// Maps to `lua_isnone`.
    #[inline(always)]
    pub fn is_none(&self, index: Index) -> bool {
        unsafe { lua_isnone(self.0, index) == 1 }
    }

    /// Maps to `lua_isnoneornil`.
    #[inline(always)]
    pub fn is_none_or_nil(&self, index: Index) -> bool {
        unsafe { lua_isnoneornil(self.0, index) == 1 }
    }

    // omitted: lua_pushliteral

    /// Maps to `lua_pushglobaltable`.
    #[inline(always)]
    pub fn push_global_table(&self) {
        unsafe { lua_pushglobaltable(self.0) };
    }

    /// Maps to `lua_insert`.
    #[inline(always)]
    pub fn insert(&self, idx: Index) {
        unsafe { lua_insert(self.0, idx) }
    }

    /// Maps to `lua_remove`.
    #[inline(always)]
    pub fn remove(&self, idx: Index) {
        unsafe { lua_remove(self.0, idx) }
    }

    /// Maps to `lua_replace`.
    #[inline(always)]
    pub fn replace(&self, idx: Index) {
        unsafe { lua_replace(self.0, idx) }
    }

    //===========================================================================
    // Debug API
    //===========================================================================
    /// Maps to `lua_getstack`.
    pub fn get_stack(&self, level: c_int) -> Option<lua_Debug> {
        let mut ar: lua_Debug = unsafe { MaybeUninit::uninit().assume_init() };
        let result = unsafe { lua_getstack(self.0, level, &mut ar) };
        if result == 1 {
            Some(ar)
        } else {
            None
        }
    }

    /// Maps to `lua_getinfo`.
    pub fn get_info(&self, what: &CStr, ar: &mut lua_Debug) -> i32 {
        unsafe { lua_getinfo(self.0, what.as_ptr(), ar) }
    }

    /// Maps to `lua_getlocal`.
    pub fn get_local(&self, ar: &lua_Debug, n: c_int) -> Option<&str> {
        let ptr = unsafe { lua_getlocal(self.0, ar, n) };
        if ptr.is_null() {
            None
        } else {
            let slice = unsafe { CStr::from_ptr(ptr).to_bytes() };
            str::from_utf8(slice).ok()
        }
    }

    /// Maps to `lua_setlocal`.
    pub fn set_local(&self, ar: &lua_Debug, n: c_int) -> Option<&str> {
        let ptr = unsafe { lua_setlocal(self.0, ar, n) };
        if ptr.is_null() {
            None
        } else {
            let slice = unsafe { CStr::from_ptr(ptr).to_bytes() };
            str::from_utf8(slice).ok()
        }
    }

    /// Maps to `lua_getupvalue`.
    pub fn get_upvalue(&self, funcindex: Index, n: c_int) -> Option<&str> {
        let ptr = unsafe { lua_getupvalue(self.0, funcindex, n) };
        if ptr.is_null() {
            None
        } else {
            let slice = unsafe { CStr::from_ptr(ptr).to_bytes() };
            str::from_utf8(slice).ok()
        }
    }

    /// Maps to `lua_setupvalue`.
    pub fn set_upvalue(&self, funcindex: Index, n: c_int) -> Option<&str> {
        let ptr = unsafe { lua_setupvalue(self.0, funcindex, n) };
        if ptr.is_null() {
            None
        } else {
            let slice = unsafe { CStr::from_ptr(ptr).to_bytes() };
            str::from_utf8(slice).ok()
        }
    }

    /// Maps to `lua_upvalueid`.
    pub fn upvalue_id(&self, funcindex: Index, n: c_int) -> *mut c_void {
        unsafe { lua_upvalueid(self.0, funcindex, n) }
    }

    /// Maps to `lua_upvaluejoin`.
    pub fn upvalue_join(&self, fidx1: Index, n1: c_int, fidx2: Index, n2: c_int) {
        unsafe { lua_upvaluejoin(self.0, fidx1, n1, fidx2, n2) }
    }

    #[cfg(features = "std")]
    /// Maps to `lua_sethook`.
    pub fn set_hook(&self, func: lua_Hook, mask: HookMask, count: c_int) {
        unsafe { lua_sethook(self.0, func, mask.bits(), count) }
    }

    /// Maps to `lua_gethook`.
    pub fn get_hook(&self) -> lua_Hook {
        unsafe { lua_gethook(self.0) }
    }

    #[cfg(features = "std")]
    /// Maps to `lua_gethookmask`.
    pub fn get_hook_mask(&self) -> HookMask {
        let result = unsafe { lua_gethookmask(self.0) };
        HookMask::from_bits_truncate(result)
    }

    /// Maps to `lua_gethookcount`.
    pub fn get_hook_count(&self) -> c_int {
        unsafe { lua_gethookcount(self.0) }
    }

    //===========================================================================
    // Auxiliary library functions
    //===========================================================================
    /// Maps to `luaL_checkversion`.
    pub fn check_version(&self) {
        unsafe { luaL_checkversion(self.0) }
    }

    /// Maps to `luaL_getmetafield`.
    #[inline(always)]
    pub fn get_metafield(&self, obj: Index, e: &CStr) -> bool {
        let result = unsafe { luaL_getmetafield(self.0, obj, e.as_ptr()) };
        result != 0
    }

    /// Maps to `luaL_callmeta`.
    #[inline(always)]
    pub fn call_meta(&self, obj: Index, e: &CStr) -> bool {
        let result = unsafe { luaL_callmeta(self.0, obj, e.as_ptr()) };
        result != 0
    }

    /// [-0, +0, -]
    #[inline(always)]
    pub fn to_string(&self, index: Index) -> *const c_char {
        unsafe { lua_tolstring(self.0, index, ptr::null_mut()) }
    }

    /// [-0, +0, -]
    #[inline(always)]
    pub fn tolstring(&self, index: Index, size: &mut usize) -> *const c_char {
        unsafe { lua_tolstring(self.0, index, size as *mut usize) }
    }

    /// [-0, +0, -]
    #[inline(always)]
    pub fn to_cfunction(&self, index: Index) -> lua_CFunction {
        unsafe { lua_tocfunction(self.0, index) }
    }

    /// Maps to `luaL_tolstring`.
    /// [-0, +1, -]
    #[inline(always)]
    pub fn cast_string(&self, index: Index) -> Option<&[u8]> {
        let mut len = 0;
        let ptr = unsafe { luaL_tolstring(self.0, index, &mut len) };
        if ptr.is_null() {
            None
        } else {
            Some(unsafe { slice::from_raw_parts(ptr as *const u8, len as usize) })
        }
    }

    #[inline(always)]
    pub fn to_str<'a>(&'a self, index: Index) -> Option<&'a str> {
        self.to_bytes(index)
            .map(|r| unsafe { str::from_utf8_unchecked(r) })
    }

    /// Maps to `lua_tolstring`, but allows arbitrary bytes.
    /// This function returns a reference to the string at the given index,
    /// on which `to_owned` may be called.
    pub fn to_bytes(&self, index: Index) -> Option<&[u8]> {
        let mut len = 0;
        let ptr = unsafe { lua_tolstring(self.0, index, &mut len) };
        if ptr.is_null() {
            None
        } else {
            Some(unsafe { slice::from_raw_parts(ptr as *const u8, len as usize) })
        }
    }

    /// Maps to `luaL_argerror`.
    pub fn arg_error(&self, arg: Index, extramsg: &CStr) -> ! {
        unsafe { luaL_argerror(self.0, arg, extramsg.as_ptr()) };
        unreachable!()
    }

    /// Maps to `luaL_typeerror`.
    #[inline(always)]
    pub fn type_error(&self, arg: Index, tname: &CStr) -> ! {
        unsafe { luaL_typeerror(self.0, arg, tname.as_ptr()) };
        unreachable!()
    }

    // omitted: luaL_checkstring
    // omitted: luaL_optstring

    /// Maps to `luaL_checknumber`.
    #[inline(always)]
    pub fn check_number(&self, arg: Index) -> lua_Number {
        unsafe { luaL_checknumber(self.0, arg) }
    }

    /// Maps to `luaL_optnumber`.
    #[inline(always)]
    pub fn opt_number(&self, arg: Index, def: lua_Number) -> lua_Number {
        unsafe { luaL_optnumber(self.0, arg, def) }
    }

    /// Maps to `luaL_checkinteger`.
    #[inline(always)]
    pub fn check_integer(&self, arg: Index) -> lua_Integer {
        unsafe { luaL_checkinteger(self.0, arg) }
    }

    /// Maps to `luaL_optinteger`.
    #[inline(always)]
    pub fn opt_integer(&self, arg: Index, def: lua_Integer) -> lua_Integer {
        unsafe { luaL_optinteger(self.0, arg, def) }
    }

    /// Maps to `luaL_checkstack`.
    pub fn check_stack_msg(&self, sz: c_int, msg: &str) {
        let c_str = CString::new(msg).unwrap();
        unsafe { luaL_checkstack(self.0, sz, c_str.as_ptr()) }
    }

    /// Maps to `luaL_checktype`.
    #[inline(always)]
    pub fn check_type(&self, arg: Index, t: Type) {
        unsafe { luaL_checktype(self.0, arg, t as c_int) }
    }

    /// Maps to `luaL_checkany`.
    #[inline(always)]
    pub fn check_any(&self, arg: Index) {
        unsafe { luaL_checkany(self.0, arg) }
    }

    /// Maps to `luaL_newmetatable`.
    #[inline(always)]
    pub fn new_metatable(&self, tname: &CStr) -> bool {
        unsafe { luaL_newmetatable(self.0, tname.as_ptr()) != 0 }
    }

    /// Maps to `luaL_setmetatable`.
    #[inline(always)]
    pub fn set_metatable_from_registry(&self, tname: &CStr) {
        unsafe { luaL_setmetatable(self.0, tname.as_ptr()) }
    }

    /// Maps to `luaL_testudata`.
    #[inline(always)]
    pub fn test_userdata(&self, arg: Index, tname: &CStr) -> *mut c_void {
        unsafe { luaL_testudata(self.0, arg, tname.as_ptr()) }
    }

    /// Convenience function that calls `test_userdata` and performs a cast.
    //#[unstable(reason="this is an experimental function")]
    #[inline(always)]
    pub unsafe fn test_userdata_typed<'a, T>(
        &'a mut self,
        arg: Index,
        tname: &CStr,
    ) -> Option<&'a mut T> {
        mem::transmute(self.test_userdata(arg, tname))
    }

    /// Maps to `luaL_checkudata`.
    #[deprecated]
    #[inline(always)]
    pub fn checkudata<'a, T>(&'a self, arg: Index, tname: &CStr) -> &'a mut T {
        unsafe { mem::transmute(luaL_checkudata(self.0, arg, tname.as_ptr())) }
    }

    /// Maps to `luaL_where`. `where` is a reserved keyword.
    #[inline(always)]
    pub fn location(&self, lvl: c_int) {
        unsafe { luaL_where(self.0, lvl) }
    }

    // omitted: luaL_error

    /// Maps to `luaL_checkoption`.
    pub fn check_option(&self, arg: Index, def: Option<&str>, lst: &[&str]) -> usize {
        let mut vec: Vec<*const c_char> = Vec::with_capacity(lst.len() + 1);
        let cstrs: Vec<CString> = lst.iter().map(|ent| CString::new(*ent).unwrap()).collect();
        for ent in cstrs.iter() {
            vec.push(ent.as_ptr());
        }
        vec.push(ptr::null());
        let result = match def {
            Some(def) => unsafe {
                let c_str = CString::new(def).unwrap();
                luaL_checkoption(self.0, arg, c_str.as_ptr(), vec.as_ptr())
            },
            None => unsafe { luaL_checkoption(self.0, arg, ptr::null(), vec.as_ptr()) },
        };
        result as usize
    }

    /// luaL_ref [-1, +0, m]
    #[inline(always)]
    pub fn reference(&self, t: Index) -> Reference {
        let result = unsafe { luaL_ref(self.0, t) };
        Reference(result)
    }

    /// Maps to `luaL_unref`.
    #[inline(always)]
    pub fn unreference(&self, t: Index, reference: Reference) {
        unsafe { luaL_unref(self.0, t, reference.value()) }
    }

    /// Maps to `luaL_loadfilex`.
    pub fn load_filex(&self, filename: &str, mode: &str) -> Result<(), Error> {
        let result = unsafe {
            let filename_c_str = CString::new(filename).unwrap();
            let mode_c_str = CString::new(mode).unwrap();
            luaL_loadfilex(self.0, filename_c_str.as_ptr(), mode_c_str.as_ptr())
        };
        self.to_error(ThreadStatus::from_c_int(result))
    }

    /// Maps to `luaL_loadfile`.
    pub fn load_file(&self, filename: &str) -> Result<(), Error> {
        let c_str = CString::new(filename).unwrap();
        let result = unsafe { luaL_loadfile(self.0, c_str.as_ptr()) };
        self.to_error(ThreadStatus::from_c_int(result))
    }

    /// Maps to `luaL_loadbufferx`.
    pub fn load_bufferx(&self, buff: &[u8], name: &str, mode: &str) -> Result<(), Error> {
        let name_c_str = CString::new(name).unwrap();
        let mode_c_str = CString::new(mode).unwrap();
        let result = unsafe {
            luaL_loadbufferx(
                self.0,
                buff.as_ptr() as *const _,
                buff.len() as size_t,
                name_c_str.as_ptr(),
                mode_c_str.as_ptr(),
            )
        };
        self.to_error(ThreadStatus::from_c_int(result))
    }

    fn to_error(&self, ts: ThreadStatus) -> Result<(), Error> {
        match ts {
            ThreadStatus::Ok => Ok(()),
            ThreadStatus::Yield => Err(Error::Yield),
            _ => {
                let err = self.to_str(-1).unwrap_or_default().to_string();
                match ts {
                    ThreadStatus::RuntimeError | ThreadStatus::MessageHandlerError => {
                        Err(Error::runtime(err))
                    }
                    ThreadStatus::GcError => Err(Error::Gc(err)),
                    ThreadStatus::SyntaxError => Err(Error::Syntax(err)),
                    ThreadStatus::MemoryError => Err(Error::Memory(err)),
                    ThreadStatus::FileError => Err(Error::runtime(err)),
                    _ => unreachable!(),
                }
            }
        }
    }

    /// Maps to `luaL_loadstring`.
    pub fn load_string(&self, source: &str) -> Result<(), Error> {
        let c_str = CString::new(source).unwrap();
        let result = unsafe { luaL_loadstring(self.0, c_str.as_ptr()) };
        self.to_error(ThreadStatus::from_c_int(result))
    }

    /// Maps to `lua_dump`.
    #[inline]
    pub fn dump(&self, mut writer: impl FnMut(&[u8]), strip: bool) -> c_int {
        use core::mem::transmute;
        unsafe extern "C" fn dump_wrapper(
            L: *mut lua_State,
            p: *const c_void,
            sz: usize,
            ud: *mut c_void,
        ) -> c_int {
            let callback = transmute::<_, &mut &mut dyn FnMut(&[u8])>(ud);
            callback(core::slice::from_raw_parts(p as *const u8, sz));
            0
        }
        let writer: &mut dyn FnMut(&[u8]) = &mut writer;
        unsafe {
            lua_dump(
                self.0,
                Some(dump_wrapper),
                transmute(&writer),
                strip as c_int,
            )
        }
    }

    /// Maps to `luaL_len`.
    pub fn len_direct(&self, index: Index) -> lua_Integer {
        unsafe { luaL_len(self.0, index) }
    }

    /// Maps to `luaL_gsub`.
    pub fn gsub(&self, s: &str, p: &str, r: &str) -> &str {
        let s_c_str = CString::new(s).unwrap();
        let p_c_str = CString::new(p).unwrap();
        let r_c_str = CString::new(r).unwrap();
        let ptr =
            unsafe { luaL_gsub(self.0, s_c_str.as_ptr(), p_c_str.as_ptr(), r_c_str.as_ptr()) };
        let slice = unsafe { CStr::from_ptr(ptr).to_bytes() };
        str::from_utf8(slice).unwrap()
    }

    /// Maps to `luaL_setfuncs`.
    pub fn set_fns(&self, l: &[(&str, lua_CFunction)], nup: c_int) {
        let mut reg: Vec<luaL_Reg> = Vec::with_capacity(l.len() + 1);
        let ents: Vec<(CString, lua_CFunction)> = l
            .iter()
            .map(|&(s, f)| (CString::new(s).unwrap(), f))
            .collect();
        for &(ref s, f) in ents.iter() {
            reg.push(luaL_Reg {
                name: s.as_ptr(),
                func: f,
            });
        }
        reg.push(luaL_Reg {
            name: ptr::null(),
            func: None,
        });
        unsafe { luaL_setfuncs(self.0, reg.as_ptr(), nup) }
    }

    /// Maps to `luaL_getsubtable`.
    #[inline(always)]
    pub fn get_subtable(&self, idx: Index, fname: &CStr) -> bool {
        unsafe { luaL_getsubtable(self.0, idx, fname.as_ptr()) != 0 }
    }

    /// Maps to `luaL_traceback`.
    #[inline(always)]
    pub fn traceback(&self, state: &State, msg: &CStr, level: c_int) {
        unsafe { luaL_traceback(self.0, state.0, msg.as_ptr(), level) }
    }

    /// Maps to `luaL_requiref`.
    #[inline(always)]
    pub fn requiref(&self, modname: &CStr, openf: CFunction, glb: bool) {
        unsafe { luaL_requiref(self.0, modname.as_ptr(), Some(openf), glb as c_int) }
    }

    /// Maps to `luaL_newlibtable`.
    pub fn new_lib_table(&self, l: &[(&str, lua_CFunction)]) {
        self.create_table(0, l.len() as c_int)
    }

    /// Maps to `luaL_newlib`.
    pub fn new_lib(&self, l: &[(&str, lua_CFunction)]) {
        self.check_version();
        self.new_lib_table(l);
        self.set_fns(l, 0)
    }

    /// Maps to `luaL_argcheck`.
    #[inline(always)]
    pub fn arg_check(&self, cond: bool, arg: Index, extramsg: &str) {
        let c_str = CString::new(extramsg).unwrap();
        unsafe { luaL_argcheck(self.0, cond as c_int, arg, c_str.as_ptr()) }
    }

    /// Maps to `luaL_checklstring`.
    pub fn check_string(&self, n: Index) -> &str {
        let mut size = 0;
        let ptr = unsafe { luaL_checklstring(self.0, n, &mut size) };
        let slice = unsafe { slice::from_raw_parts(ptr as *const u8, size as usize) };
        str::from_utf8(slice).unwrap()
    }

    /// Maps to `luaL_optlstring`.
    pub fn opt_string<'a>(&'a mut self, n: Index, default: &'a str) -> &'a str {
        let mut size = 0;
        let c_str = CString::new(default).unwrap();
        let ptr = unsafe { luaL_optlstring(self.0, n, c_str.as_ptr(), &mut size) };
        if ptr == c_str.as_ptr() {
            default
        } else {
            let slice = unsafe { slice::from_raw_parts(ptr as *const u8, size as usize) };
            str::from_utf8(slice).unwrap()
        }
    }

    // omitted: luaL_checkint (use .check_integer)
    // omitted: luaL_optint (use .opt_integer)
    // omitted: luaL_checklong (use .check_integer)
    // omitted: luaL_optlong (use .opt_integer)

    // luaL_dofile and luaL_dostring implemented above

    /// Maps to `luaL_getmetatable`.
    #[inline(always)]
    pub fn get_metatable_from_registry(&self, tname: &str) {
        let c_str = CString::new(tname).unwrap();
        unsafe { luaL_getmetatable(self.0, c_str.as_ptr()) }
    }

    // omitted: luaL_opt (undocumented function)

    //===========================================================================
    // Wrapper functions
    //===========================================================================
    #[inline(always)]
    pub fn val(&self, i: Index) -> ValRef {
        ValRef::new(self, i)
    }

    /// [-0, +0, -]
    #[inline(always)]
    pub fn upval(&self, i: Index) -> ValRef {
        ValRef::new(self, lua_upvalueindex(i))
    }

    /// [-0, +0, -]
    #[inline(always)]
    pub fn c_reg(&self) -> ValRef {
        self.val(LUA_REGISTRYINDEX)
    }

    /// [-0, +1, -]
    #[inline(always)]
    pub fn global(&self) -> ValRef {
        unsafe {
            lua_rawgeti(self.0, LUA_REGISTRYINDEX, LUA_RIDX_GLOBALS);
        }
        self.val(-1)
    }

    #[inline(always)]
    pub fn table(&self, narr: c_int, nrec: c_int) -> ValRef {
        self.create_table(narr, nrec);
        self.val(-1)
    }

    /// Register a metatable of UserData into the C registry
    #[inline(always)]
    pub fn register_usertype<U: UserData>(&self) {
        self.get_or_init_metatable(U::init_metatable);
        self.pop(1);
    }

    #[inline(always)]
    pub fn push_userdatauv<T>(&self, data: T, n: i32) -> &mut T {
        let result: &mut T = unsafe { mem::transmute(self.new_userdatauv(mem::size_of::<T>(), n)) };
        mem::forget(mem::replace(result, data));
        result
    }

    /// [-0, +1, -]
    #[inline(always)]
    pub fn push_userdata<T>(&self, data: T, metatable: Option<InitMetatable>) -> &mut T {
        let result: &mut T = unsafe { mem::transmute(self.new_userdata(mem::size_of::<T>())) };
        mem::forget(mem::replace(result, data));
        if let Some(m) = metatable {
            self.set_or_init_metatable(m);
        }
        result
    }

    /// [-0, +1, -]
    pub fn push_userdata_pointer<T: UserData>(&self, data: *mut T, metatable: InitMetatable) {
        let result: &mut *mut T = unsafe {
            mem::transmute(self.new_userdatauv(
                mem::size_of_val(&data),
                data.as_ref().unwrap().uservalue_count(self),
            ))
        };
        mem::replace(result, data);
        self.set_or_init_metatable(metatable);
    }

    /// [-0, +1, -]
    pub fn push_userdata_pointer_body<T: UserData>(
        &self,
        data: T,
        metatable: InitMetatable,
    ) -> &mut T {
        let result: &mut (*mut T, T) = unsafe {
            mem::transmute(
                self.new_userdatauv(mem::size_of::<(*mut T, T)>(), data.uservalue_count(self)),
            )
        };
        mem::forget(mem::replace(result, (ptr::null_mut(), data)));
        result.0 = &mut result.1;
        self.set_or_init_metatable(metatable);
        &mut result.1
    }

    pub fn check_udata<T>(&self, i: Index, name: &CStr) -> &mut T {
        unsafe { mem::transmute(luaL_checkudata(self.0, i, name.as_ptr())) }
    }

    #[inline(always)]
    pub fn test_userdata_meta_<T>(&self, i: Index, meta: InitMetatable) -> *mut T {
        if self.get_metatable(i) && {
            self.raw_getp(LUA_REGISTRYINDEX, meta as *const ());
            self.raw_equal(-1, -2)
        } {
            self.pop(2);
            self.to_userdata(i) as _
        } else {
            core::ptr::null_mut()
        }
    }

    pub fn test_userdata_meta<T>(&self, i: Index, meta: InitMetatable) -> Option<&mut T> {
        unsafe { core::mem::transmute(self.test_userdata_meta_::<T>(i, meta)) }
    }

    pub fn check_userdata<T>(&self, i: Index, meta: InitMetatable) -> &mut T {
        let p = self.test_userdata_meta_::<T>(i, meta);
        if p.is_null() {
            let tname = CString::new(core::any::type_name::<Self>()).unwrap_or_default();
            self.type_error(i, &tname);
        } else {
            unsafe { core::mem::transmute(p) }
        }
    }

    /// [-0, +1, -]
    pub fn load_buffer<F: AsRef<[u8]>>(&self, source: F, chunk_name: Option<&str>) -> ThreadStatus {
        let buffer = source.as_ref();
        let chunk = match chunk_name {
            Some(name) => name.as_ptr(),
            None => ptr::null(),
        };
        ThreadStatus::from_c_int(unsafe {
            luaL_loadbuffer(
                self.0,
                buffer.as_ptr() as *const c_char,
                buffer.len(),
                chunk as *const c_char,
            )
        })
    }

    /// [-0, +1, -]
    pub fn get_or_init_metatable(&self, callback: InitMetatable) {
        let reg = self.c_reg();
        let p = callback as *const usize;
        let metatable = reg.getp(p);
        if metatable.is_nil() {
            let mt = self.table(0, 0);
            self.balance_with(|s| callback(&mt));
            assert!(self.type_of(-1) == Type::Table);

            if self.get_field(-1, cstr!("__name")) == Type::String {
                self.push_value(-2);
                self.set_table(reg.index);
            } else {
                self.pop(1);
            }

            reg.setp(p, mt);
            self.replace(-2);
        }
    }

    /// [-0, +0, -]
    #[inline]
    pub fn set_or_init_metatable(&self, callback: InitMetatable) {
        let ty = self.type_of(-1);
        assert!(ty == Type::Userdata || ty == Type::Table);
        self.get_or_init_metatable(callback);
        self.set_metatable(-2);
    }

    /// [-1, +1, -]
    pub fn trace_error(&self, s: Option<&State>) -> &str {
        let err = self.to_str(-1).unwrap_or("");
        self.pop(1);
        unsafe {
            let thread = s.unwrap_or(self);
            luaL_traceback(self.0, thread.0, err.as_ptr() as *const c_char, 0);
        }
        self.to_str(-1).unwrap_or("")
    }

    #[inline(always)]
    pub fn balance_with<'a, T: 'a, F: FnOnce(&'a State) -> T>(&'a self, callback: F) -> T {
        let top = self.get_top();
        let result = callback(self);
        self.set_top(top);
        result
    }

    #[inline(always)]
    pub fn balance(&self) -> BalanceState {
        BalanceState::new(self)
    }

    #[inline(always)]
    pub fn error_string(&self, e: impl AsRef<str>) -> ! {
        self.push_string(e.as_ref());
        core::mem::drop(e);
        self.error()
    }

    #[inline(always)]
    pub fn raise_error(&self, e: impl core::fmt::Debug) -> ! {
        self.error_string(format!("{e:?}"))
    }

    #[inline(always)]
    pub fn check_result<T>(&self, r: Result<T, impl core::fmt::Debug>) -> T {
        match r {
            Ok(v) => v,
            Err(e) => self.raise_error(e),
        }
    }

    pub unsafe extern "C" fn traceback_c(l: *mut lua_State) -> i32 {
        let s = State::from_ptr(l);
        luaL_traceback(l, l, s.to_string(1), 1);
        1
    }

    pub fn value(&self, i: Index) -> Value {
        match unsafe { lua_type(self.0, i) } {
            LUA_TNONE => Value::None,
            LUA_TNIL => Value::Nil,
            LUA_TNUMBER => {
                if self.is_integer(i) {
                    Value::Int(self.to_integer(i))
                } else {
                    Value::Num(self.to_number(i))
                }
            }
            LUA_TSTRING => Value::Str(self.to_str(i).unwrap()),
            LUA_TBOOLEAN => Value::Bool(self.to_bool(i)),
            LUA_TLIGHTUSERDATA => Value::LightUserdata,
            LUA_TTABLE => Value::Table,
            LUA_TFUNCTION => Value::Function,
            LUA_TUSERDATA => Value::Userdata,
            LUA_TTHREAD => Value::Thread,
            _ => panic!(""),
        }
    }
}

#[derive(Deref)]
pub struct BalanceState<'a> {
    #[deref]
    state: &'a State,
    pub top: i32,
}

impl<'a> BalanceState<'a> {
    pub fn new(state: &'a State) -> Self {
        Self {
            state,
            top: state.get_top(),
        }
    }
}

impl Drop for BalanceState<'_> {
    fn drop(&mut self) {
        self.set_top(self.top);
    }
}
