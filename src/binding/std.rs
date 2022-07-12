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
        t.register("split", |path: &'static str| {
            Path::new(path)
                .parent()
                .and_then(Path::to_str)
                .map(|dir| {
                    let name = &path[dir.len()..];
                    (dir, name.trim_start_matches('\\'))
                })
                .unwrap_or_default()
        });
        t.register("splitext", |s: &State, path: &'static str| {
            Path::new(path)
                .extension()
                .and_then(OsStr::to_str)
                .map(|ext| {
                    let p = &path[..path.len() - ext.len()];
                    s.pushed((p.trim_end_matches('.'), ext))
                })
                .unwrap_or_else(|| {
                    s.push_value(1);
                    s.push("");
                    Pushed(2)
                })
        });
        t.register("copy", std::fs::copy::<&str, &str>);
        t.register("rename", std::fs::rename::<&str, &str>);
        t.register("removedir", std::fs::remove_dir::<&str>);
        t.register("removefile", std::fs::remove_file::<&str>);
        // t.register("softlink", std::fs::soft_link::<&str, &str>);
        // t.register("hardlink", std::fs::hard_link::<&str, &str>);
        t.register("readlink", Path::read_link);
        t.register("meta", Path::metadata);
        t.register("join", |s: &State, path: &Path| {
            let mut buf = path.to_path_buf();
            for i in 2..=s.get_top() {
                if let Some(n) = s.to_str(i) {
                    buf = buf.join(n);
                } else {
                    break;
                }
            }
            buf
        });
        t.set("exepath", RsFn::new(std::env::current_exe));
    }

    impl<'a> FromLua<'a> for &'a Path {
        #[inline(always)]
        fn from_lua(s: &'a State, i: Index) -> Option<&'a Path> {
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

pub mod time {
    use super::*;
    use std::time::Duration;

    impl FromLua<'_> for Duration {
        fn from_lua(s: &State, i: Index) -> Option<Self> {
            Some(match s.value(i) {
                Value::Int(n) => Duration::from_secs(n as _),
                Value::Num(n) => Duration::from_secs_f64(n),
                // TODO: 1s 1ms 1ns
                // Value::Str(_) => todo!(),
                _ => return None,
            })
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

    impl FromLua<'_> for ReadArg {
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

    impl FromLua<'_> for Stdio {
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
        use glob::MatchOptions;

        glob::glob_with(
            pattern,
            MatchOptions {
                case_sensitive: false,
                ..MatchOptions::new()
            },
        )
        .map(|iter| {
            BoxIter(Box::new(
                iter.filter_map(Result::ok)
                    .map(|path| path.to_string_lossy().to_string()),
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

    impl FromLua<'_> for glob::Pattern {
        fn check(s: &State, i: Index) -> Self {
            s.check_result(glob::Pattern::new(s.args(i)))
        }

        fn from_lua(s: &State, i: Index) -> Option<Self> {
            glob::Pattern::new(s.to_str(i)?).ok()
        }
    }

    string.register("to_utf16", |s: &State, t: &str| unsafe {
        let mut r = t.encode_utf16().collect::<Vec<_>>();
        r.push(0);
        s.pushed(core::slice::from_raw_parts(
            r.as_ptr() as *const u8,
            r.len() * 2 - 1,
        ))
    });
    string.register("from_utf16", |t: &[u8]| unsafe {
        let u = core::slice::from_raw_parts(t.as_ptr() as *const u16, t.len() / 2);
        String::from_utf16_lossy(u)
    });
    string.register("starts_with", |t: &str, prefix: &str| t.starts_with(prefix));
    string.register("ends_with", |t: &str, suffix: &str| t.ends_with(suffix));
    string.register("equal", |t1: &str, t2: &str, case_sensitive: bool| {
        if case_sensitive {
            t1.eq(t2)
        } else {
            t1.eq_ignore_ascii_case(t2)
        }
    });
    string.register(
        "wildmatch",
        |t1: &str, pattern: glob::Pattern, case_sensitive: bool| {
            let options = glob::MatchOptions {
                case_sensitive,
                ..Default::default()
            };
            pattern.matches_with(t1, options)
        },
    );
}

#[cfg(feature = "thread")]
mod thread {
    use super::*;

    #[cfg(not(target_os = "windows"))]
    use std::os::unix::thread::{JoinHandleExt, RawPthread as RawHandle};
    #[cfg(target_os = "windows")]
    use std::os::windows::io::{AsRawHandle, RawHandle};
    use std::ptr;
    use std::sync::*;
    use std::thread::{self, JoinHandle};
    use std::time::Duration;

    struct LLuaThread {
        handle: RawHandle,
        join: Option<JoinHandle<()>>,
    }

    impl LLuaThread {
        #[inline]
        fn get(&self) -> Result<&JoinHandle<()>, &'static str> {
            self.join.as_ref().ok_or("thread joined")
        }
    }

    impl UserData for LLuaThread {
        const TYPE_NAME: &'static str = "LLuaThread";

        fn getter(fields: &ValRef) {
            fields.register("handle", |this: &Self| this.handle as usize);
            fields.register("name", |this: &'static Self| {
                this.get().map(|j| j.thread().name())
            });
            fields.register("id", |this: &Self| {
                this.get().map(|j| j.thread().id().as_u64().get())
            });
        }

        fn methods(mt: &ValRef) {
            mt.register("join", |this: &mut Self| {
                this.join.take().ok_or("thread joined").map(|j| j.join())
            });
            mt.register("unpark", |this: &Self| {
                this.get().map(|j| j.thread().unpark())
            });
        }
    }

    #[cfg(target_os = "windows")]
    const RAW_NULL: RawHandle = ptr::null_mut();
    #[cfg(not(target_os = "windows"))]
    const RAW_NULL: RawHandle = 0;

    #[derive(Default, Deref, AsRef)]
    struct LLuaMutex(Mutex<()>);
    struct LLuaMutexGaurd(Option<MutexGuard<'static, ()>>);

    impl UserData for LLuaMutexGaurd {
        const TYPE_NAME: &'static str = "LLuaMutexGaurd";

        fn methods(mt: &ValRef) {
            fn unlock(this: &mut LLuaMutexGaurd) {
                this.0.take();
            }

            mt.register("unlock", unlock);
            mt.register("__close", unlock);
        }
    }

    impl UserData for LLuaMutex {
        const TYPE_NAME: &'static str = "LLuaMutex";

        fn methods(mt: &ValRef) {
            MethodRegistry::<Self, Mutex<()>>::new(mt)
                .register("is_poisoned", Mutex::<()>::is_poisoned);
            mt.register("lock", |this: &'static Self| {
                this.0.lock().map(|g| LLuaMutexGaurd(Some(g)))
            });
            mt.register("try_lock", |this: &'static Self| {
                this.0.try_lock().ok().map(|g| LLuaMutexGaurd(Some(g)))
            });
        }
    }

    #[derive(Default)]
    struct LLuaCondVar {
        lock: Mutex<i32>,
        cvar: Condvar,
    }

    impl UserData for LLuaCondVar {
        const TYPE_NAME: &'static str = "LLuaCondVar";

        fn methods(mt: &ValRef) {
            mt.register("wait", |s: &State, this: &'static Self, tm: Option<u64>| {
                this.wait(s, tm).map(Pushed)
            });
            mt.register("notify_one", |s: &State, this: &Self| {
                this.push_some(s);
                this.cvar.notify_one();
            });
            mt.register("notify_all", |s: &State, this: &Self| {
                this.push_some(s);
                this.cvar.notify_all();
            });
        }
    }

    impl LLuaCondVar {
        fn push_some(&self, s: &State) {
            let mut i = self.lock.lock().unwrap();
            s.unreference(LUA_REGISTRYINDEX, (*i).into());
            s.push_value(2);
            *i = s.reference(LUA_REGISTRYINDEX).0;
        }

        fn wait<'a>(
            &'a self,
            s: &State,
            timeout: Option<u64>,
        ) -> Result<i32, Box<dyn std::error::Error + 'a>> {
            let lock = &self.lock;
            let cvar = &self.cvar;
            if let Some(tm) = timeout {
                let (i, r) = cvar.wait_timeout(lock.lock().unwrap(), Duration::from_millis(tm))?;
                if r.timed_out() {
                    return Ok(0);
                }
                s.raw_geti(LUA_REGISTRYINDEX, (*i) as i64);
            } else {
                let i = cvar.wait(lock.lock().unwrap())?;
                s.raw_geti(LUA_REGISTRYINDEX, (*i) as i64);
            }
            Ok(1)
        }
    }

    pub fn init(s: &State) {
        let t = s.table(0, 4);
        t.register("spawn", |routine: Coroutine, name: Option<&str>| {
            let mut b = thread::Builder::new();
            if let Some(name) = name {
                b = b.name(name.into());
            }
            b.spawn(move || {
                if let Err(err) = routine.pcall_trace::<_, ()>(()) {
                    call_print(
                        &routine,
                        &std::format!(
                            "<thread#{} \"{}\"> {}",
                            thread::current().id().as_u64().get(),
                            thread::current().name().unwrap_or_default(),
                            err
                        ),
                    );
                }
            })
            .map(|join| {
                #[cfg(target_os = "windows")]
                let handle = join.as_raw_handle();
                #[cfg(not(target_os = "windows"))]
                let handle = join.as_pthread_t();
                // let u = s.push_userdata(LLuaThread {join, handle, ref_ud: Cell::new(NOREF)}, Some(LLuaThread::init));
                // u.ref_ud.set({ s.push_value(-1); s.reference(LUA_REGISTRYINDEX) });
                // return 1;
                // TODO: ref_ud
                LLuaThread {
                    join: Some(join),
                    handle,
                }
            })
        });
        t.register("sleep", |time: u64| {
            thread::sleep(Duration::from_millis(time))
        });
        t.register("park", thread::park);
        t.register("id", || thread::current().id().as_u64().get());
        t.register("name", |s: &State| s.pushed(thread::current().name()));
        t.register("yield_now", thread::yield_now);
        t.register("mutex", LLuaMutex::default);
        t.register("condvar", LLuaCondVar::default);

        s.set_global(cstr!("thread"));
    }
}

pub fn init_global(s: &State) {
    extend_os(s);
    extend_string(s);
    #[cfg(feature = "thread")]
    thread::init(s);

    let g = s.global();
    g.set(
        "readfile",
        cfn!(|s, path: &str| {
            std::fs::read(path)
                .map(|data| s.pushx(data.as_slice()))
                .unwrap_or_default()
        }),
    );
    g.set(
        "__file__",
        RsFn::new(|s: &State| {
            s.get_stack(1).and_then(|mut dbg| {
                s.get_info(cstr!("S"), &mut dbg);
                if dbg.source.is_null() {
                    return None;
                }
                let src = unsafe { std::ffi::CStr::from_ptr(dbg.source) };
                let src = src.to_string_lossy();
                Some(src.strip_prefix("@").unwrap_or(&src).to_string())
            })
        })
        .wrapper(),
    );
    g.register("writefile", std::fs::write::<&std::path::Path, &[u8]>);
}

pub fn call_print(s: &State, err: &str) {
    if s.get_global(cstr!("__llua_error")) == Type::Function
        || s.get_global(cstr!("print")) != Type::Function
    {
        s.push(err);
        s.pcall(1, 0, 0);
    } else {
        std::eprintln!("[callback error] {}", err);
    }
}
