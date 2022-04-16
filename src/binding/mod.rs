#[cfg(feature = "std")]
pub mod std;

pub fn init_global(s: &crate::State) {
    #[cfg(feature = "std")]
    std::init_global(s);
}
