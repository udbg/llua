
use super::*;
use lserde::*;

pub mod path {
    use super::*;
    use std::path::*;

    impl<'a> FromLua for &'a Path {
        #[inline(always)]
        fn from_lua(s: &State, i: Index) -> Option<&'a Path> {
            Path::new(s.to_str(i)?).into()
        }
    }

    impl ToLua for &Path {
        #[inline(always)]
        fn to_lua(self, s: &State) {
            s.push(self.to_str());
        }
    }

    impl ToLua for PathBuf {
        #[inline(always)]
        fn to_lua(self, s: &State) {
            s.push(self.to_str());
        }
    }
}

pub mod process {
    use super::*;
    use std::io::{Write, Read};
    use std::process::{Command, Stdio, Child, ExitStatus};

    enum ReadArg {
        Exact(usize),
        All,
    }

    impl FromLua for ReadArg {
        fn from_lua(s: &State, i: Index) -> Option<Self> {
            if s.is_integer(i) {
                Some(Self::Exact(s.args(i)))
            } else {
                match <&str as FromLua>::from_lua(s, i)? {
                    "a" | "*" | "*a" => Some(Self::All),
                    _ => None,
                }
            }
        }
    }

    impl FromLua for Stdio {
        fn from_lua(s: &State, i: Index) -> Option<Self> {
            Some(match <&str as FromLua>::from_lua(s, i)? {
                "pipe" | "piped" => Stdio::piped(),
                "inherit" => Stdio::inherit(),
                "null" | _ => Stdio::null(),
            })
        }
    }

    impl ToLuaMulti for ExitStatus {
        fn to_lua(self, s: &State) -> i32 {
            s.pushx((self.success(), self.code()))
        }
    }

    impl UserData for Command {
        fn methods(mt: &ValRef) {
            mt.register_fn("arg", |this: &mut Self, arg: &str| { this.arg(arg); StackRef(1) });
            mt.register_fn("args", |this: &mut Self, arg: DeserValue<Vec<&str>>| {
                this.args(arg.as_slice()); StackRef(1)
            });
            mt.register_fn("current_dir", |this: &mut Self, arg: &str| { this.current_dir(arg); StackRef(1) });
            mt.register_fn("env_clear", |this: &mut Self| { this.env_clear(); StackRef(1) });
            mt.register_fn("stdin", |this: &mut Self, arg: Stdio| { this.stdin(arg); StackRef(1) });
            mt.register_fn("stdout", |this: &mut Self, arg: Stdio| { this.stdout(arg); StackRef(1) });
            mt.register_fn("stderr", |this: &mut Self, arg: Stdio| { this.stderr(arg); StackRef(1) });
            mt.register_fn("env", |this: &mut Self, k: &str, v: Option<&str>| {
                if let Some(v) = v {
                    this.env(k, v);
                } else {
                    this.env_remove(k);
                }
                StackRef(1)
            });
            mt.register_fn("spawn", Self::spawn);
        }
    }

    impl UserData for Child {
        fn methods(mt: &ValRef) {
            mt.register_fn("id", Self::id);
            mt.register_fn("kill", Self::kill);
            mt.register_fn("wait", Self::wait);
            mt.register_fn("try_wait", |s: &State, this: &mut Self| {
                this.try_wait().map(|e| match e {
                    Some(e) => s.pushed(e),
                    None => 0.into(),
                })
            });
            mt.register_fn("wait_output", |s: &State, this: &mut Self| -> Result<Pushed, Box<dyn std::error::Error>> {
                let status = this.wait()?;
                let top = s.get_top();
                s.push(status.success());
                s.push(status.code());
                this.stdout.as_mut().and_then(|o| read_std(o, ReadArg::All).ok()).map(|o| s.push(o.as_slice()));
                this.stderr.as_mut().and_then(|o| read_std(o, ReadArg::All).ok()).map(|o| s.push(o.as_slice()));
                Ok(Pushed(s.get_top() - top))
            });
            mt.register_fn("write", |this: &mut Self, data: &[u8]| -> Result<usize, Box<dyn std::error::Error>> {
                let stdin = this.stdin.as_mut().ok_or("stdin")?;
                Ok(stdin.write(data)?)
            });
            mt.register_fn("read", |s: &State, this: &mut Self, size: ReadArg| -> Result<Pushed, Box<dyn std::error::Error>> {
                let data = read_std(this.stdout.as_mut().ok_or("stdout")?, size)?;
                Ok(s.pushed(data.as_slice()))
            });
            mt.register_fn("read_error", |s: &State, this: &mut Self, size: ReadArg| -> Result<Pushed, Box<dyn std::error::Error>> {
                let data = read_std(this.stderr.as_mut().ok_or("stderr")?, size)?;
                Ok(s.pushed(data.as_slice()))
            });

            fn read_std(r: &mut dyn Read, size: ReadArg) -> std::io::Result<Vec<u8>> {
                let mut buf = vec![];
                match size {
                    ReadArg::All => {
                        r.read_to_end(&mut buf)?;
                    }
                    ReadArg::Exact(size) => {
                        buf.resize(size, 0);
                        let len = r.read(buf.as_mut())?;
                        buf.resize(len, 0);
                    }
                }
                Ok(buf)
            }
        }
    }
}