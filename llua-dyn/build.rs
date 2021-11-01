
fn main() {
    let target_family = std::env::var("CARGO_CFG_TARGET_FAMILY").unwrap();
    if target_family == "windows" {
        println!("cargo:rustc-link-lib=dylib=lua54.dll");
    } else {
        println!("cargo:rustc-link-lib=dylib=lua54");
    }
}