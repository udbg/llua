use llua::*;

#[tokio::test]
async fn llua_async() {
    let s = State::new();
    s.open_libs();

    let g = s.global();
    g.register(
        "echo_async",
        |s: State, n: i32| async move { s.pushed((0, n)) },
    );
    g.register("sleep_async", tokio::time::sleep);

    let co = Coroutine::empty(&s);
    co.load_string(
        "
        print(echo_async(...))
        -- error 'error test'
        sleep_async(0.2)
        print(echo_async(2))
        print(echo_async(3))
        return 1, 2
    ",
    )
    .unwrap();

    let ret = co.call_async::<_, (i32, i32)>(333, None).await.unwrap();
    assert_eq!(ret, (1, 2));
}
