use crate::{ffi::*, *};
use parking_lot::{lock_api::RawMutex, Mutex};

#[repr(C)]
pub struct Extra {
    mutex: Mutex<()>,
}

#[inline(always)]
pub fn get_extra(l: *mut lua_State) -> &'static mut Extra {
    unsafe { *core::mem::transmute::<_, *mut &'static mut Extra>(lua_getextraspace(l)) }
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
    });
    *core::mem::transmute::<_, *mut *mut Extra>(lua_getextraspace(l)) = Box::into_raw(extra);
}

#[no_mangle]
unsafe extern "C" fn llua_userstateclose(l: *mut lua_State) {
    let e = get_extra(l);
    Box::from_raw(e);
}
