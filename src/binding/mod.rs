#[cfg(feature = "regex")]
pub mod regex;
#[cfg(feature = "std")]
pub mod std;

pub fn init_global(s: &crate::State) {
    #[cfg(feature = "std")]
    self::std::init_global(s);
    #[cfg(feature = "regex")]
    s.requiref(crate::cstr!("regex"), regex::open, false);
}
