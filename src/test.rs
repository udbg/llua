#![cfg(feature = "vendored")]

use core::any::Any;

use crate::{serde::SerdeValue, *};

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

#[test]
fn userdata() {
    let s = State::new();
    s.open_base();
    s.push(Test { a: 0 });
    let uv = s.val(-1);
    assert_eq!(uv.type_of(), Type::Userdata);
    s.global().set("uv", uv);

    s.do_string("print(getmetatable(uv), type(uv))");
    s.do_string("assert(uv.a == 0)").chk_err(&s);
    s.do_string("uv:inc(); assert(uv.a == 1)").chk_err(&s);
    s.do_string("uv.a = 3; assert(uv.a == 3)").chk_err(&s);
}

#[test]
fn serde() {
    use serde::{Deserialize, Serialize};

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
