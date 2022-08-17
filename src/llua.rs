use crate::{ffi::*, *};
use parking_lot::{lock_api::RawMutex, Mutex};

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
