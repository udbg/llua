use llua::*;

#[tokio::test]
async fn llua_async() {
    let s = State::new();
    s.open_libs();
    s.init_llua_global();

    let g = s.global();
    g.register("echo_async", |n: i32| async move { (0, n) });
    g.register("sleep_async", tokio::time::sleep);
    s.load_string(
        "
        print(echo_async(1))
        sleep_async(1)
        print(echo_async(2))
        print(echo_async(3))
    ",
    );
    s.call_async().await.unwrap();
}
