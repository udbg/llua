#![cfg(feature = "vendored")]

use crate::*;

use alloc::rc::Rc;

struct Test {
    a: i32,
}

impl UserData for Test {
    fn methods(mt: &ValRef) {
        mt.register("inc", |this: &mut Self| this.a += 1);
    }

    fn getter(fields: &ValRef) {
        fields.register("a", |this: &Self| this.a);
    }

    fn setter(fields: &ValRef) {
        fields.register("a", |this: &mut Self, val: i32| this.a = val);
    }
}

impl UserData for Rc<Test> {
    fn key_to_cache(&self) -> *const () {
        self.as_ref() as *const _ as _
    }

    fn getter(fields: &ValRef) {
        fields.register("a", |this: &Self| this.a);
    }
}

#[test]
fn userdata() {
    let s = State::new();
    s.open_base();
    s.push(Test { a: 0 });
    let uv = s.val(-1);
    assert_eq!(uv.type_of(), Type::Userdata);
    s.global().set("uv", uv);
    let test_value = 0x11223344;
    s.global().set("test", RsFn::new(move || test_value));
    s.global().set("toiter", RsFn::new(|| BoxIter::new(0..3)));
    s.do_string(
        r#"
        local iter = toiter()
        assert(iter() == 0)
        assert(iter() == 1)
        assert(iter() == 2)
    "#,
    )
    .chk_err(&s);

    s.do_string("assert(test() == 0x11223344)").chk_err(&s);

    s.do_string("print(getmetatable(uv), type(uv))");
    s.do_string("assert(uv.a == 0)").chk_err(&s);
    s.do_string("uv:inc(); assert(uv.a == 1)").chk_err(&s);
    s.do_string("uv.a = 3; assert(uv.a == 3)").chk_err(&s);

    let test = Rc::new(Test { a: 123 });
    s.global().set("uv", test.clone());
    s.global().set("uv1", test.clone());
    s.do_string("print(uv, uv1)");
    s.do_string("assert(uv == uv1)").chk_err(&s);
    s.do_string("assert(uv.a == 123)").chk_err(&s);
}

#[test]
fn serde() {
    use ::serde::{Deserialize, Serialize};

    #[derive(Serialize, Deserialize, PartialEq, Clone, Debug)]
    struct Test<'a> {
        str: &'a str,
        int: i32,
        flt: f64,
    }

    let s = State::new();
    s.open_base();
    let global = s.global();
    let test = Test {
        str: "abc",
        int: 333,
        flt: 123.0,
    };
    global.set("test", SerdeValue(test.clone()));
    s.do_string("print(test)");
    let t = global.getopt::<_, SerdeValue<Test>>("test").unwrap().0;
    assert_eq!(test, t)
}

#[test]
fn binding() {
    let s = State::new();
    s.open_libs();
    s.init_llua_global();

    s.do_string(
        r"
        testpath = os.getexe()
        local meta = os.path.meta(testpath)
        print(testpath, meta)
        assert(meta.size == meta:len())
        assert(not meta.readonly)
        assert(meta:is_file())
    ",
    )
    .chk_err(&s);
}

#[no_mangle]
extern "C" fn llua_open_libs(_: &State) {}

#[cfg(feature = "regex")]
#[test]
fn regex_binding() {
    let s = State::new();
    s.open_libs();
    s.init_llua_global();

    s.do_string(
        r"
        local re = require 'regex'
        local cap = re.new[[(\w+)\s+(\w+)]]:capture 'abc def'
        assert(cap[1] == 'abc')
        assert(cap[2] == 'def')
    ",
    )
    .chk_err(&s);
}

#[cfg(feature = "thread")]
#[test]
fn test_thread() {
    let s = State::new();
    s.open_libs();
    s.init_llua_global();
    s.do_file("tests/thread.lua").chk_err(&s);
}
