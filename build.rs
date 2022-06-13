#[cfg(feature = "vendored")]
fn main() {
    use std::env;
    use std::path::Path;

    const LUA_DIR_NAME: &str = "lua-5.4.4";

    let target_os = env::var("CARGO_CFG_TARGET_OS").unwrap();
    let target_family = env::var("CARGO_CFG_TARGET_FAMILY").unwrap();

    let mut config = lua_builder();
    if target_os == "linux" {
        config.warnings(false).extra_warnings(false);
        config.define("LUA_USE_LINUX", None);
    } else if target_os == "macos" {
        config.define("LUA_USE_MACOSX", None);
    } else if target_family == "unix" {
        config.define("LUA_USE_POSIX", None);
    } else if target_family == "windows" {
        config.define("LUA_USE_WINDOWS", None);
    }
    if cfg!(debug_assertions) {
        config.define("LUA_USE_APICHECK", None);
    }
    println!("cargo:rerun-if-changed={LUA_DIR_NAME}/");
    println!(
        "cargo:luasrc={}",
        Path::new(&env::var("CARGO_MANIFEST_DIR").unwrap())
            .join(LUA_DIR_NAME)
            .to_string_lossy()
    );

    if env::var("CARGO_FEATURE_THREAD").is_ok() {
        config.define("LUA_USER_H", "\"../src/llua.h\"");
    }
    add_files(&mut config, LUA_DIR_NAME, |n| {
        n.ends_with(".c") && !n.ends_with("lua.c") && !n.ends_with("luac.c")
    });
    config.compile("lua54");

    fn add_files(b: &mut cc::Build, dir: &str, cb: fn(&str) -> bool) {
        for entry in std::fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            if path.to_str().map(cb).unwrap_or(false) {
                b.file(path);
            }
        }
    }

    fn lua_builder() -> cc::Build {
        let mut result = cc::Build::new();
        result.include(LUA_DIR_NAME);
        result
    }
}

#[cfg(not(feature = "vendored"))]
fn main() {}
