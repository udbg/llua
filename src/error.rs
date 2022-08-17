use alloc::boxed::Box;
use alloc::string::String;
use core::fmt::Debug;

#[derive(Debug, From)]
pub enum Error {
    Runtime(String),
    #[from(ignore)]
    Memory(String),
    #[from(ignore)]
    Syntax(String),
    #[from(ignore)]
    Gc(String),
    Yield,
    #[from(ignore)]
    Convert(Box<dyn Debug>),
    ConvertFailed,
    Else(Box<dyn Debug>),
}

unsafe impl Send for Error {}
unsafe impl Sync for Error {}

impl core::fmt::Display for Error {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{self:?}")
    }
}

impl Error {
    pub fn from_debug<D: Debug + 'static>(dbg: D) -> Self {
        Self::Else(Box::new(dbg))
    }

    pub fn convert<D: Debug + 'static>(dbg: D) -> Self {
        Self::Convert(Box::new(dbg))
    }

    pub fn runtime<S: Into<String>>(s: S) -> Self {
        Self::Runtime(s.into())
    }
}

#[cfg(feature = "std")]
impl std::error::Error for Error {}
