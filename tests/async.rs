use llua::*;

#[tokio::test]
async fn llua_async() {
    let s = State::new();
    s.open_libs();
    s.init_llua_global();

    let g = s.global();
    g.register(
        "echo_async",
        |s: State, n: i32| async move { s.pushed((0, n)) },
    );
    g.register("sleep_async", tokio::time::sleep);
    let top = s.get_top();
    s.load_string(
        "
        print(echo_async(...))
        sleep_async(1)
        print(echo_async(2))
        print(echo_async(3))
        return 1, 2
    ",
    );

    // let n = 2;
    // let res = s.raw_call_async(s.pushx(333), n).await.unwrap();
    // assert_eq!(res, n);
    // let ret = s.args::<(i32, i32)>(-2);
    // assert_eq!(ret, (1, 2));

    let ret = s.call_async::<_, (i32, i32)>(333).await.unwrap();
    assert_eq!(ret, (1, 2));
}
