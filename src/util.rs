use crate::*;

impl State {
    pub fn to_ffi_pointer(&self, i: Index) -> Option<usize> {
        Some(match self.type_of(i) {
            Type::Number => {
                if self.is_integer(i) {
                    self.to_integer(i) as usize
                } else {
                    self.to_number(i) as usize
                }
            }
            Type::String => self.to_string(i) as usize,
            _ => {
                let ptr = self.to_pointer(i);
                if ptr.is_null() {
                    return None;
                }
                ptr as usize
            }
        })
    }

    pub fn init_llua_global(&self) {
        let s = ScopeState::from(self);
        let g = s.global();

        g.setf(cstr!("__llua_psize"), core::mem::size_of::<usize>());
        g.setf(
            cstr!("topointer"),
            RsFn::new(|s: &State| Self::to_ffi_pointer(s, 1)),
        );
        g.setf(
            cstr!("cclosure"),
            RsFn::new(|s: &State| {
                if let Some(f) = s.to_cfunction(1) {
                    s.push_cclosure(Some(f), s.get_top() - 1);
                    Pushed(1)
                } else {
                    s.type_error(1, cstr!("cfunction"));
                }
            }),
        );
        binding::init_global(&s);
    }
}
