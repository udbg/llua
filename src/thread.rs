use crate::{ffi::*, *};
use core::cell::Cell;
use parking_lot::{lock_api::RawMutex, Mutex};
use std::sync::LazyLock;

unsafe impl Send for State {}
unsafe impl Sync for State {}

pub static mut GLOBAL_LUA: LazyLock<Mutex<Lua>> = LazyLock::new(|| {
    let lua = Lua::with_open_libs();
    lua.init_llua_global();
    lua.into()
});

#[derive(derive_more::Deref)]
pub struct TlsState(Cell<*mut lua_State>);

impl TlsState {
    fn new() -> TlsState {
        Self(Cell::new(core::ptr::null_mut()))
    }

    fn get(&self) -> State {
        if self.0.get().is_null() {
            unsafe {
                let s = GLOBAL_LUA.lock();
                let t = s.new_thread();
                s.push_value(-1);
                s.raw_set(ffi::LUA_REGISTRYINDEX);
                self.0.set(t.as_ptr());
            }
        }
        unsafe { State::from_ptr(self.0.get()) }
    }
}

impl Drop for TlsState {
    fn drop(&mut self) {
        if !self.0.get().is_null() {
            let s = self.get();
            s.push_thread();
            s.push_nil();
            s.raw_set(ffi::LUA_REGISTRYINDEX);
        }
    }
}

std::thread_local! {
    static LUA: TlsState = TlsState::new();
}

pub fn state() -> State {
    LUA.with(TlsState::get)
}

// llua impl

#[cfg(feature = "vendored")]
pub mod llua {
    use super::*;

    #[repr(C)]
    pub struct Extra {
        mutex: Mutex<()>,
        pub(crate) lua: *const LuaInner,
    }

    #[inline(always)]
    pub fn get_extra<'a>(l: *mut lua_State) -> &'a mut Extra {
        unsafe { *core::mem::transmute::<_, *mut &mut Extra>(lua_getextraspace(l)) }
    }

    #[no_mangle]
    unsafe extern "C" fn llua_lock(l: *mut lua_State) {
        let extra = get_extra(l);
        extra.mutex.raw().lock();
    }

    #[no_mangle]
    unsafe extern "C" fn llua_unlock(l: *mut lua_State) {
        get_extra(l).mutex.force_unlock()
    }

    #[no_mangle]
    unsafe extern "C" fn llua_userstateopen(l: *mut lua_State) {
        let extra = Box::new(Extra {
            mutex: Mutex::new(()),
            lua: core::ptr::null(),
        });
        *core::mem::transmute::<_, *mut *mut Extra>(lua_getextraspace(l)) = Box::into_raw(extra);
    }

    #[no_mangle]
    unsafe extern "C" fn llua_userstateclose(l: *mut lua_State) {
        let e = get_extra(l);
        e.lua = core::ptr::null();
        drop(Box::from_raw(e));
    }
}
