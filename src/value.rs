use crate::{
    error::Error,
    ffi::{lua_Integer, lua_Number, LUA_REGISTRYINDEX},
    str::CStr,
    FromLua, FromLuaMulti, Reference, State, ThreadStatus, ToLua, ToLuaMulti, Type,
};
pub type Index = i32;

#[derive(Clone, Copy)]
pub struct ValRef<'a> {
    pub state: &'a State,
    pub index: Index,
}

impl<'a> ValRef<'a> {
    pub fn new(state: &'a State, index: Index) -> Self {
        ValRef {
            state,
            index: state.abs_index(index),
        }
    }

    #[inline]
    pub fn type_of(&self) -> Type {
        self.state.type_of(self.index)
    }

    #[inline]
    pub fn is_nil(&self) -> bool {
        self.state.is_nil(self.index)
    }

    #[inline]
    pub fn is_integer(&self) -> bool {
        self.state.is_integer(self.index)
    }

    #[inline]
    pub fn to_bool(&self) -> bool {
        self.state.to_bool(self.index)
    }

    #[inline]
    pub fn check_type(&self, ty: Type) {
        self.state.check_type(self.index, ty);
    }

    #[inline]
    pub fn cast<T: FromLua<'a>>(&'a self) -> Option<T> {
        self.state.arg(self.index)
    }

    #[inline]
    pub fn check_cast<T: FromLua<'a>>(&'a self) -> T {
        T::check(self.state, self.index)
    }

    pub fn raw_geti(&self, i: impl Into<lua_Integer>) -> ValRef {
        self.state.raw_geti(self.index, i.into());
        self.state.val(-1)
    }

    pub fn geti(&self, i: impl Into<lua_Integer>) -> ValRef {
        self.state.geti(self.index, i.into());
        self.state.val(-1)
    }

    pub fn seti<V: ToLua>(&self, i: impl Into<lua_Integer>, v: V) {
        v.to_lua(self.state);
        self.state.seti(self.index, i.into());
    }

    pub fn getf(&self, k: &CStr) -> ValRef {
        self.state.get_field(self.index, k);
        self.state.val(-1)
    }

    #[inline]
    pub fn rawget<K: ToLua>(&self, k: K) -> Type {
        self.state.push(k);
        self.state.raw_get(self.index)
    }

    #[inline]
    pub fn rawlen(&self) -> usize {
        self.state.raw_len(self.index)
    }

    #[inline]
    pub fn set_field(&self, k: &CStr) {
        self.state.set_field(self.index, k);
    }

    #[inline]
    pub fn setf<V: ToLua>(&self, k: &CStr, v: V) {
        self.state.push(v);
        self.set_field(k);
    }

    #[inline]
    pub fn getp<T>(&self, p: *const T) -> ValRef {
        self.state.raw_getp(self.index, p);
        self.state.val(-1)
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
            self.state.push(k);
            self.state.insert(-2);
        } else {
            self.state.push(k);
            self.state.push(v);
        }
        self.state.set_table(self.index);
    }

    #[inline]
    pub fn get<K: ToLua>(&self, k: K) -> ValRef<'a> {
        self.state.push(k);
        self.state.get_table(self.index);
        self.state.val(-1)
    }

    #[inline]
    pub fn getopt<K: ToLua, V: FromLua<'a>>(&self, k: K) -> Option<V> {
        self.get(k);
        let res = V::from_lua(self.state, -1);
        self.state.pop(1);
        res
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Value<'a> {
    None,
    Nil,
    Int(lua_Integer),
    Num(lua_Number),
    Str(&'a str),
    Bool(bool),
    LightUserdata,
    Table,
    Function,
    Userdata,
    Thread,
}

impl State {
    pub fn value(&self, i: Index) -> Value {
        match self.type_of(i) {
            Type::None => Value::None,
            Type::Nil => Value::Nil,
            Type::Number => {
                if self.is_integer(i) {
                    Value::Int(self.to_integer(i))
                } else {
                    Value::Num(self.to_number(i))
                }
            }
            Type::String => Value::Str(self.to_str(i).unwrap()),
            Type::Boolean => Value::Bool(self.to_bool(i)),
            Type::LightUserdata => Value::LightUserdata,
            Type::Table => Value::Table,
            Type::Function => Value::Function,
            Type::Userdata => Value::Userdata,
            Type::Thread => Value::Thread,
            _ => unreachable!(),
        }
    }
}

#[derive(Deref)]
pub struct Coroutine {
    #[deref]
    pub(crate) state: State,
    nres: i32,
}

unsafe impl Send for Coroutine {}

impl Coroutine {
    // [-0, +0]
    pub fn empty(s: &State) -> Self {
        let result = s.new_thread();
        assert!(s.type_of(-1) == Type::Thread);
        let result = Self::new(result);
        s.pop(1);
        result
    }

    fn new(s: State) -> Self {
        s.push_thread();
        s.raw_setp(LUA_REGISTRYINDEX, s.as_ptr());
        Self { state: s, nres: 0 }
    }

    pub fn with_fn(s: &State, i: Index) -> Self {
        s.check_type(i, Type::Function);

        let result = Self::empty(s);
        s.push_value(i);
        s.xmove(&result, 1);
        result
    }

    pub fn resume<'a, A: ToLuaMulti, R: FromLuaMulti<'a>>(
        &'a mut self,
        args: A,
    ) -> Result<R, Error> {
        // FIXME: args are maybe located in the stack will be poped
        self.pop(self.nres);
        match self.state.resume(None, self.pushx(args), &mut self.nres) {
            ThreadStatus::Ok | ThreadStatus::Yield => {
                let fidx = self.get_top() - self.nres;
                self.set_top(fidx + R::COUNT as i32);
                self.nres = R::COUNT as i32;
                R::from_lua(self, self.abs_index(-(R::COUNT as i32))).ok_or(Error::ConvertFailed)
            }
            err => Err(self.to_error(err).unwrap_err()),
        }
    }
}

impl FromLua<'_> for Coroutine {
    fn from_lua(s: &State, i: Index) -> Option<Self> {
        match s.type_of(i) {
            Type::Thread => s.to_thread(i).map(Self::new),
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
