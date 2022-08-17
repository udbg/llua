use llua::*;

#[test]
fn coroutine() {
    let lua = Lua::with_open_libs();
    let s = lua.state();

    s.load_file("tests/co.lua").unwrap();
    let mut co = Coroutine::with_fn(s, -1);
    let res = co.resume::<_, Value>((1, 2)).unwrap();
    assert_eq!(res, Value::Int(1));
}
