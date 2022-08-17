use crate::State;
use alloc::boxed::Box;

#[derive(Debug)]
pub(crate) struct LuaInner {
    main: State,
}

#[derive(Debug)]
pub struct Lua(Box<LuaInner>);

impl core::ops::Deref for Lua {
    type Target = State;

    fn deref(&self) -> &Self::Target {
        self.state()
    }
}

impl Lua {
    pub fn new() -> Self {
        let this = Self(Box::new(LuaInner { main: State::new() }));
        // crate::llua::get_extra(this.0.main.as_ptr()).lua = this.0.as_ref();
        this
    }

    pub fn with_open_libs() -> Self {
        let this = Self::new();
        this.0.main.open_libs();
        this
    }

    #[inline(always)]
    pub fn state(&self) -> &State {
        &self.0.main
    }
}

impl Drop for LuaInner {
    fn drop(&mut self) {
        self.main.close();
    }
}
