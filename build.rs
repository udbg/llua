#[cfg(feature = "vendored")]
fn main() {
    let artifacts = lua_src::Build::new().build(lua_src::Lua54);
    artifacts.print_cargo_metadata();
}

#[cfg(not(feature = "vendored"))]
fn main() {}
