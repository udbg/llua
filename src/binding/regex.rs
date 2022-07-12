use crate::{ffi::lua_State, *};
use ::regex::{Captures, Regex};

impl UserData for Captures<'_> {
    const INDEX_METATABLE: bool = false;
    const IS_POINTER: bool = true;
    const TYPE_NAME: &'static str = "RegexCaptures";

    fn methods(mt: &ValRef) {
        mt.register("__len", Captures::len);
        mt.register("__index", |s: &State, this: &Self| {
            let m = if s.is_integer(2) {
                this.get(s.args(2))
            } else {
                this.name(s.args(2))
            };
            m.map(|m| s.pushed((m.as_str(), m.start() + 1, m.end())))
        });
    }
}

impl UserData for Regex {
    const TYPE_NAME: &'static str = "Regex";

    fn methods(mt: &ValRef) {
        mt.register("new", Regex::new);
        mt.register("shortest_match", Regex::shortest_match);
        // https://docs.rs/regex/latest/regex/struct.Regex.html#method.find
        mt.register("find", |this: &Self, text: &str, pos: Option<usize>| {
            pos.map(|p| this.find_at(text, p))
                .unwrap_or_else(|| this.find(text))
                .map(|m| {
                    let (start, end) = (m.start(), m.end());
                    (start + 1, end)
                })
        });
        // https://docs.rs/regex/latest/regex/struct.Regex.html#method.find_iter
        mt.register("gmatch", |this: &'static Self, text: &'static str| {
            let iter = this.find_iter(text);
            BoxIter::from(iter.map(|m| (m.as_str(), m.start() + 1, m.end())))
        });
        // https://docs.rs/regex/latest/regex/struct.Regex.html#method.split
        mt.register("gsplit", |this: &'static Self, text: &'static str| {
            BoxIter::from(this.split(text))
        });
        mt.register("split", |this: &'static Self, text: &'static str| {
            IterVec(this.split(text))
        });
        // https://docs.rs/regex/latest/regex/struct.Regex.html#method.replace
        mt.register("replace", |s: &State, this: &Self, text: &str| {
            let r = if s.is_function(3) {
                this.replace(text, |caps: &Captures| {
                    s.push_value(3);
                    s.push(caps as *const Captures as *mut Captures);
                    s.pcall(1, 1, 0);
                    s.to_str(-1).unwrap_or_default()
                })
            } else {
                let sub: &str = s.args(3);
                this.replace(text, sub)
            };
            s.pushed(r.as_ref())
        });
        // https://docs.rs/regex/latest/regex/struct.Regex.html#method.captures
        mt.register("capture", Regex::captures);
        mt.register("match", |s: &State, this: &Self, text: &str| {
            this.captures(text).map(|cap| {
                let top = s.get_top();
                for m in cap.iter().skip(1).filter_map(|m| m) {
                    s.push(m.as_str());
                }
                Pushed(s.get_top() - top)
            })
        });
    }
}

pub unsafe extern "C" fn open(l: *mut lua_State) -> i32 {
    let s = State::from_ptr(l);
    s.push(Regex::metatable());
    return 1;
}
