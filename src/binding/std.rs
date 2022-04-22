use crate::serde::*;
use crate::*;

pub mod path {
    use super::*;
    use std::{ffi::OsStr, fs::Metadata, path::*};

    impl ToLua for std::time::SystemTime {
        fn to_lua(self, s: &State) {
            use std::time::UNIX_EPOCH;

            convert::ToLua::to_lua(
                self.duration_since(UNIX_EPOCH)
                    .ok()
                    .map(|dur| dur.as_secs_f64()),
                s,
            )
        }
    }

    impl UserData for Metadata {
        fn getter(fields: &ValRef) {
            fields.register("size", Self::len);
            fields.register("modified", Self::modified);
            fields.register("created", Self::created);
            fields.register("accessed", Self::accessed);
            fields.register("readonly", |this: &Self| this.permissions().readonly());
        }

        fn methods(mt: &ValRef) {
            mt.register("len", Self::len);
            mt.register("is_dir", Self::is_dir);
            mt.register("is_file", Self::is_file);
            mt.register("is_symlink", Self::is_symlink);
        }
    }

    pub fn init(s: &State) {
        let t = s.table(0, 8);
        t.register("dirname", Path::parent);
        t.register("exists", Path::exists);
        t.register("abspath", std::fs::canonicalize::<&str>);
        t.register("isabs", Path::is_absolute);
        t.register("isdir", Path::is_dir);
        t.register("isfile", Path::is_file);
        t.register("issymlink", Path::is_symlink);
        t.register("basename", Path::file_name);
        t.register("withext", Path::with_extension::<&str>);
        t.register("withfilename", Path::with_file_name::<&str>);
        t.set(
            "split",
            cfn!(|s, path: &str| {
                Path::new(path)
                    .parent()
                    .and_then(Path::to_str)
                    .map(|dir| {
                        let name = &path[dir.len()..];
                        s.push(dir);
                        s.push(name.trim_start_matches('\\'));
                        2
                    })
                    .unwrap_or_default()
            }),
        );
        t.set(
            "splitext",
            cfn!(|s, path: &str| {
                Path::new(path)
                    .extension()
                    .and_then(OsStr::to_str)
                    .map(|ext| {
                        let p = &path[..path.len() - ext.len()];
                        s.push(p.trim_end_matches('.'));
                        s.push(ext);
                        2
                    })
                    .unwrap_or_else(|| {
                        s.push_value(1);
                        s.push("");
                        2
                    })
            }),
        );
        t.register("copy", std::fs::copy::<&str, &str>);
        t.register("rename", std::fs::rename::<&str, &str>);
        t.register("removedir", std::fs::remove_dir::<&str>);
        t.register("removefile", std::fs::remove_file::<&str>);
        // t.register("softlink", std::fs::soft_link::<&str, &str>);
        // t.register("hardlink", std::fs::hard_link::<&str, &str>);
        t.register("readlink", Path::read_link);
        t.register("meta", Path::metadata);
        t.set(
            "join",
            cfn!(|s, path: &Path| {
                let mut buf = path.to_path_buf();
                for i in 2..=s.get_top() {
                    if let Some(n) = s.to_str(i) {
                        buf = buf.join(n);
                    } else {
                        break;
                    }
                }
                s.push(buf.to_str());
                1
            }),
        );
        t.set("exepath", RsFn::new(std::env::current_exe));
    }

    impl<'a> FromLua for &'a Path {
        #[inline(always)]
        fn from_lua(s: &State, i: Index) -> Option<&'a Path> {
            Path::new(s.to_str(i)?).into()
        }
    }

    impl ToLua for &Path {
        #[inline(always)]
        fn to_lua(self, s: &State) {
            let p = self.to_string_lossy();
            s.push(p.strip_prefix(r"\\?\").unwrap_or(&p));
        }
    }

    impl ToLua for PathBuf {
        #[inline(always)]
        fn to_lua(self, s: &State) {
            ToLua::to_lua(self.as_path(), s);
        }
    }
}

pub mod process {
    use super::*;
    use std::io::{Read, Write};
    use std::process::{Child, Command, ExitStatus, Stdio};

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
            mt.register("arg", |this: &mut Self, arg: &str| {
                this.arg(arg);
                StackRef(1)
            });
            mt.register("args", |this: &mut Self, arg: SerdeValue<Vec<&str>>| {
                this.args(arg.as_slice());
                StackRef(1)
            });
            mt.register("current_dir", |this: &mut Self, arg: &str| {
                this.current_dir(arg);
                StackRef(1)
            });
            mt.register("env_clear", |this: &mut Self| {
                this.env_clear();
                StackRef(1)
            });
            mt.register("stdin", |this: &mut Self, arg: Stdio| {
                this.stdin(arg);
                StackRef(1)
            });
            mt.register("stdout", |this: &mut Self, arg: Stdio| {
                this.stdout(arg);
                StackRef(1)
            });
            mt.register("stderr", |this: &mut Self, arg: Stdio| {
                this.stderr(arg);
                StackRef(1)
            });
            mt.register("env", |this: &mut Self, k: &str, v: Option<&str>| {
                if let Some(v) = v {
                    this.env(k, v);
                } else {
                    this.env_remove(k);
                }
                StackRef(1)
            });
            mt.register("spawn", Self::spawn);
        }
    }

    impl UserData for Child {
        fn methods(mt: &ValRef) {
            mt.register("id", Self::id);
            mt.register("kill", Self::kill);
            mt.register("wait", Self::wait);
            mt.register("try_wait", |s: &State, this: &mut Self| {
                this.try_wait().map(|e| match e {
                    Some(e) => s.pushed(e),
                    None => 0.into(),
                })
            });
            mt.register(
                "wait_output",
                |s: &State, this: &mut Self| -> Result<Pushed, Box<dyn std::error::Error>> {
                    let status = this.wait()?;
                    let top = s.get_top();
                    s.push(status.success());
                    s.push(status.code());
                    this.stdout
                        .as_mut()
                        .and_then(|o| read_std(o, ReadArg::All).ok())
                        .map(|o| s.push(o.as_slice()));
                    this.stderr
                        .as_mut()
                        .and_then(|o| read_std(o, ReadArg::All).ok())
                        .map(|o| s.push(o.as_slice()));
                    Ok(Pushed(s.get_top() - top))
                },
            );
            mt.register(
                "write",
                |this: &mut Self, data: &[u8]| -> Result<usize, Box<dyn std::error::Error>> {
                    let stdin = this.stdin.as_mut().ok_or("stdin")?;
                    Ok(stdin.write(data)?)
                },
            );
            mt.register(
                "read",
                |s: &State,
                 this: &mut Self,
                 size: ReadArg|
                 -> Result<Pushed, Box<dyn std::error::Error>> {
                    let data = read_std(this.stdout.as_mut().ok_or("stdout")?, size)?;
                    Ok(s.pushed(data.as_slice()))
                },
            );
            mt.register(
                "read_error",
                |s: &State,
                 this: &mut Self,
                 size: ReadArg|
                 -> Result<Pushed, Box<dyn std::error::Error>> {
                    let data = read_std(this.stderr.as_mut().ok_or("stderr")?, size)?;
                    Ok(s.pushed(data.as_slice()))
                },
            );

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

pub fn extend_os(s: &State) {
    s.get_global(cstr!("os"));
    path::init(s);
    s.set_field(-2, cstr!("path"));

    let os = s.val(-1);
    os.set("name", std::env::consts::OS);
    os.set("arch", std::env::consts::ARCH);
    os.set("family", std::env::consts::FAMILY);
    os.set("dllextension", std::env::consts::DLL_EXTENSION);
    os.set("pointersize", core::mem::size_of::<usize>());

    os.register("mkdir", std::fs::create_dir::<&str>);
    os.register("mkdirs", std::fs::create_dir_all::<&str>);
    os.register("rmdir", std::fs::remove_dir::<&str>);

    os.register("chdir", std::env::set_current_dir::<&str>);
    os.register("getcwd", std::env::current_dir);
    os.register("getexe", std::env::current_exe);

    os.register("glob", |pattern: &str| {
        use glob::glob;
        glob(pattern).map(|iter| {
            BoxIter(Box::new(
                iter.filter_map(|e| e.ok())
                    .filter_map(|path| path.to_str().map(|s| s.to_string())),
            ))
        })
    });

    os.register("env", std::env::var::<&str>);
    os.register("putenv", |s: &State, var: &str, val: Option<&str>| {
        if let Some(val) = val {
            std::env::set_var(var, val);
        } else {
            std::env::remove_var(var);
        };
    });

    use std::collections::HashMap;
    use std::process::{Command, Stdio};

    fn init_command(arg: ValRef) -> Command {
        let mut args: SerdeValue<Vec<&str>> = arg.check_cast();
        if args.is_empty() {
            arg.state.error_string("empty command");
        }
        let mut cmd = Command::new(args.remove(0));
        cmd.args(args.as_slice());
        let args = arg;
        args.getopt::<_, Stdio>("stdin").map(|v| cmd.stdin(v));
        args.getopt::<_, Stdio>("stdout").map(|v| cmd.stdout(v));
        args.getopt::<_, Stdio>("stderr").map(|v| cmd.stderr(v));
        args.getopt::<_, &str>("cwd").map(|v| cmd.current_dir(v));
        args.getopt::<_, SerdeValue<HashMap<&str, &str>>>("env")
            .map(|v| {
                for (k, val) in v.iter() {
                    cmd.env(k, val);
                }
            });
        cmd
    }
    os.set(
        "command",
        RsFn::new(|s: &State, arg: Value| match arg {
            Value::Str(cmd) => Command::new(cmd),
            Value::Table => init_command(s.val(1)),
            _ => s.type_error(1, cstr!("string|table")),
        }),
    );
    os.set(
        "spawn_child",
        RsFn::new(|s: &State| init_command(s.val(1)).spawn()),
    );
}

pub fn extend_string(s: &State) {
    s.get_global(cstr!("string"));
    let string = s.val(-1);

    string.set(
        "to_utf16",
        cfn!(|s, t: &str| {
            let mut r = t.encode_utf16().collect::<Vec<_>>();
            r.push(0);
            s.push(core::slice::from_raw_parts(
                r.as_ptr() as *const u8,
                r.len() * 2 - 1,
            ));
            1
        }),
    );
    string.set(
        "from_utf16",
        cfn!(|s, t: &[u8]| push {
            let u = core::slice::from_raw_parts(t.as_ptr() as *const u16, t.len() / 2);
            String::from_utf16_lossy(u)
        }),
    );
    string.set(
        "starts_with",
        cfn!(|s, t: &str, prefix: &str| push {
            t.starts_with(prefix)
        }),
    );
    string.set(
        "ends_with",
        cfn!(|s, t: &str, suffix: &str| push {
            t.ends_with(suffix)
        }),
    );
    string.set(
        "equal",
        cfn!(|s, t1: &str, t2: &str, case_sensitive: bool| push {
            if case_sensitive { t1.eq(t2) } else { t1.eq_ignore_ascii_case(t2) }
        }),
    );
    string.set(
        "wildmatch",
        cfn!(|s, t1: &str, pattern: &str, case_sensitive: bool|? {
            let pattern = glob::Pattern::new(pattern)?;
            let options = glob::MatchOptions {case_sensitive, ..Default::default()};
            s.push(pattern.matches_with(t1, options));
            1
        }),
    );
}

pub fn init_global(s: &State) {
    extend_os(s);
    extend_string(s);

    let g = s.global();
    g.set(
        "readfile",
        cfn!(|s, path: &str| {
            std::fs::read(path)
                .map(|data| s.pushx(data.as_slice()))
                .unwrap_or_default()
        }),
    );
    g.register("writefile", std::fs::write::<&std::path::Path, &[u8]>);
}
