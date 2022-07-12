use super::*;
use crate::error::Error;
use core::future::Future;
use ffi::*;
use libc::c_int;

pub struct TaskWrapper<'a>(Option<Box<dyn Future<Output = Result<i32, Error>> + 'a>>);

impl UserData for TaskWrapper<'_> {
    const TYPE_NAME: &'static str = "llua::TaskWrapper";
}

impl State {
    pub async fn call_async(&self) -> Result<i32, Error> {
        assert_eq!(self.type_of(-1), Type::Function);

        let state = self.new_thread();
        self.insert(-2);
        self.xmove(&state, 1);

        let mut nres = 0;
        loop {
            match state.resume(Some(self), nres, &mut nres) {
                ThreadStatus::Yield => {
                    let task = state
                        .arg::<&mut TaskWrapper>(-1)
                        .expect("coroutine task expect a TaskWrapper")
                        .0
                        .take()
                        .expect("task moved");
                    state.pop(1);
                    nres = Box::into_pin(task).await?;
                    std::println!("nres: {nres}");
                }
                ThreadStatus::Ok => return Ok(nres),
                err => {
                    return Err(Error::Runtime(
                        state.to_str(-1).unwrap_or_default().to_string(),
                    ))
                }
            }
        }
    }

    #[inline(always)]
    pub(crate) fn yield_task<'a, RET: ToLuaMulti, F: Future<Output = RET> + 'a>(
        &'a self,
        fut: F,
    ) -> ! {
        let state = unsafe { self.copy_state() };
        let top = self.get_top();
        self.yieldk(
            self.pushx(TaskWrapper(Some(Box::new(async move {
                fut.await.to_lua_result(&state)
            })))),
            move |s, status| s.get_top() - top,
        )
    }

    /// Maps to `lua_pcallk`.
    pub fn pcallk<F>(&self, nargs: c_int, nresults: c_int, msgh: c_int, continuation: F) -> c_int
    where
        F: FnOnce(&State, ThreadStatus) -> c_int,
    {
        let func = continue_func::<F>;
        let ctx = Box::into_raw(continuation.into()) as _;
        unsafe {
            // lua_pcallk only returns if no yield occurs, so call the continuation
            func(
                self.as_ptr(),
                lua_pcallk(self.as_ptr(), nargs, nresults, msgh, ctx, Some(func)),
                ctx,
            )
        }
    }

    /// Maps to `lua_yield`.
    pub fn r#yield(&self, nresults: c_int) -> ! {
        unsafe { ffi::lua_yield(self.as_ptr(), nresults) };
        panic!("co_yieldk called in non-coroutine context; check is_yieldable first")
    }

    /// Maps to `lua_yieldk`.
    pub fn yieldk<F>(&self, nresults: c_int, continuation: F) -> !
    where
        F: FnOnce(&State, ThreadStatus) -> c_int,
    {
        let ctx = Box::into_raw(continuation.into()) as _;
        unsafe { ffi::lua_yieldk(self.as_ptr(), nresults, ctx, Some(continue_func::<F>)) };
        panic!("co_yieldk called in non-coroutine context; check is_yieldable first")
    }
}

unsafe extern "C" fn continue_func<F>(
    st: *mut lua_State,
    status: c_int,
    ctx: ffi::lua_KContext,
) -> c_int
where
    F: FnOnce(&State, ThreadStatus) -> c_int,
{
    core::mem::transmute::<_, Box<F>>(ctx)(&State::from_ptr(st), ThreadStatus::from_c_int(status))
}
