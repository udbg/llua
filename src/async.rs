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
    // #[inline(always)]
    // pub async fn call_async<'a, T: ToLuaMulti, R: FromLuaMulti<'a>>(
    //     &self,
    //     args: T,
    // ) -> Result<R, Error> {
    //     let co = Coroutine::with_fn(self, -1);
    //     // TODO: Coroutine with lifetime
    //     // TODO: FromLuaOwned
    //     let co: &'a Coroutine = unsafe { core::mem::transmute(&co) };
    //     co.call_async::<T, R>(args, Some(self)).await
    // }

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

impl Coroutine {
    #[inline(always)]
    pub async fn call_async<'a, T: ToLuaMulti, R: FromLuaMulti<'a>>(
        &'a self,
        args: T,
        from: Option<&State>,
    ) -> Result<R, Error> {
        let count = R::COUNT as i32;
        self.raw_call_async(from, self.pushx(args), count).await?;
        R::from_lua(self, self.abs_index(-count)).ok_or(Error::ConvertFailed)
    }

    pub async fn raw_call_async(
        &self,
        from: Option<&State>,
        mut nargs: i32,
        nresult: i32,
    ) -> Result<i32, Error> {
        assert!(nargs >= 0 && nresult >= 0);

        let top = self.get_top() - nargs;
        loop {
            let mut nres = nresult;
            match self.resume(from, nargs, &mut nres) {
                ThreadStatus::Yield => {
                    assert_eq!(nres, 1);
                    let task = self
                        .arg::<&mut TaskWrapper>(-1)
                        .ok_or("coroutine task expect a TaskWrapper")
                        .map_err(Error::runtime)?
                        .0
                        .take()
                        .ok_or("task is already moved")
                        .map_err(Error::runtime)?;
                    // pop the TaskWrapper
                    self.pop(1);

                    // execute the task
                    let top = self.get_top();
                    nargs = Box::into_pin(task).await?;

                    // keep the last nargs elements in stack
                    let delta = self.get_top() - top - nargs;
                    if delta > 0 {
                        self.rotate(top, -delta);
                        self.pop(delta);
                    } else {
                        for _ in 0..-delta {
                            self.push_nil();
                        }
                    }
                }
                ThreadStatus::Ok => {
                    // at the end, function in coroutine was also poped
                    self.set_top(top - 1 + nresult);
                    return Ok(nresult);
                }
                err => {
                    return Err(Error::Runtime(
                        self.to_str(-1).unwrap_or_default().to_string(),
                    ));
                }
            }
        }
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
