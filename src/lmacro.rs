//! Macros for lua binding

#[macro_export]
macro_rules! cfn {
    (@unpack $s:ident $i:expr,) => {};
    (@unpack $s:ident $i:expr, $v:ident: $t:ty, $($rest:tt)*) => {
        let $v: $t = FromLua::check($s, $i);
        cfn!(@unpack $s $i+1, $($rest)*);
    };

    (@define_fn $name:ident $l:ident $body:block) => {
        unsafe extern "C" fn $name($l: *mut $crate::ffi::lua_State) -> i32 $body
    };

    (@define $l:ident $body:block) => {{
        cfn! { @define_fn function $l $body }
        function as CFunction
    }};

    (@body_option $s:ident $body:block) => { #[allow(unused_braces)] $body };
    (@body_option $s:ident push $body:block) => { $s.pushx($body) };
    (@body_option $s:ident throw $body:block) => {{
        #[allow(unused_mut)]
        let mut closure = || -> Result<i32, Box<dyn std::error::Error>> {
            Ok($body)
        };
        match closure() {
            Ok(n) => n,
            Err(e) => $s.raise_error(e),
        }
    }};

    (($s:ident $(,$v:ident : $t:ty)*) $($body_option:ident)? $body:block) => {
        cfn!(@define l {
            let $s = &$crate::State::from_ptr(l);
            cfn!(@unpack $s 1, $($v: $t,)*);
            cfn!{@body_option $s $($body_option)? $body}
        })
    };

    (|$s:ident $(,$v:ident : $t:ty)*| $($body_option:ident)? $body:block) => {
        cfn!(@define l {
            let $s = &$crate::State::from_ptr(l);
            cfn!(@unpack $s 1, $($v: $t,)*);
            cfn!{@body_option $s $($body_option)? $body}
        })
    };

    (|$s:ident $(,$v:ident : $t:ty)*|? $body:block) => {
        cfn!(@define l {
            let $s = &$crate::State::from_ptr(l);
            cfn!(@unpack $s 1, $($v: $t,)*);
            cfn!{@body_option $s throw $body}
        })
    };
}

#[macro_export]
macro_rules! metatable {
    (@method $t:ty, $s:ident, ($($this:tt)*) ($($arg_def:tt)*) $($body_option:ident)? $body:block) => {
        cfn!(@define l {
            let $s = &$crate::State::from_ptr(l);
            metatable!(@unpack-args $t, $s, ($($this)*) $($arg_def)*);
            cfn!(@body_option $s $($body_option)? $body)
        })
    };
    (@tablemethod, ($s:ident, $this:ident, $($v:ident : $a:ty),*) $($body_option:ident)? $body:block) => {
        cfn!(@define l {
            let $s = &$crate::State::from_ptr(l);
            $s.check_type(1, $crate::Type::Table);
            let $this = $crate::Table($s.val(1));
            cfn!(@unpack $s 2, $($v: $a,)*);
            cfn!(@body_option $s $($body_option)? $body)
        })
    };

    (@unpack-args $t:ty, $s:ident, ($($this:tt)*) $($v:ident : $a:ty),*) => {
        metatable!(@get-this $s $($this)*: $t);
        cfn!(@unpack $s 2, $($v: $a,)*);
    };

    (@unpack-args $t:ty, $s:ident, ($($this:tt)*) nocheck, $($v:ident : $a:ty),*) => {
        cfn!(@unpack $s 2, $($v: $a,)*);
    };

    (@unpack-args $t:ty, $s:ident, ($($this:tt)*) nothis, $($v:ident : $a:ty),*) => {
        cfn!(@unpack $s 1, $($v: $a,)*);
    };

    (@get-this $s:ident $this:ident: $t:ty) => {
        let $this = match core::mem::transmute::<_, Option<&mut $t>>($s.to_userdata(1)) {
            Some(r) => r, None => {
                $s.check_type(1, $crate::Type::Userdata);
                $s.raise_error("");
            }
        };
    };

    (@get-this $s:ident $tk:literal $this:ident: $t:ty) => {
        // $s.check_type(1, $crate::Type::Table);
        $s.push($tk);
        $s.raw_get(1);
        $s.check_type(-1, $crate::Type::Userdata);
        let $this: &mut $t = core::mem::transmute($s.to_userdata(-1));
        $s.pop(1);
    };

    (@init-option) => {};
    (@init-option IndexSelf $meta:ident) => {
        $meta.setf($crate::cstr!("__index"), $meta.0);
    };

    // for userdata
    (
        $t:tt($s:ident: State, $this:ident: Self) $($option:ident)?;
        $($name:tt($($arg_def:tt)*) $($body_option:ident)? $body:block)*
    ) => {{
        fn init_metatable(meta: $crate::Table) {
            metatable!(@init-option $($option meta)?);
            meta.setf($crate::cstr!("__name"), stringify!($t));
            meta.setf($crate::cstr!("__gc"), metatable!(@method $t, meta.state, ($this) () {
                core::ptr::drop_in_place($this); 0
            }));
            $(
                meta.setf($crate::cstr!($name), metatable!(
                    @method $t, meta.state, ($this) ($($arg_def)*)
                    $($body_option)? $body
                ));
            )*
        }
        init_metatable
    }};

    // for userdata
    (
        [$s:ident: State, $this:ident: $user_t:ty $(,$init_opt:ident)?]

        $(fn $name:ident ($($arg_def:tt)*) $(@tk:$tk:literal)? $($body_option:ident)? $body:block)*
    ) => {{
        fn init_metatable(meta: &$crate::ValRef) {
            $(metatable!(@init-option $init_opt meta);)?

            meta.setf($crate::cstr!("__name"), stringify!($user_t));
            meta.setf($crate::cstr!("__gc"), metatable!(@method $user_t, $s, ($this) () {
                core::ptr::drop_in_place($this); 0
            }));
            $(
                meta.setf(
                    $crate::cstr!(stringify!($name)),
                    metatable!(
                        @method $user_t, $s, ($($tk)? $this) ($($arg_def)*)
                        $($body_option)? $body
                    )
                );
            )*
        }
        init_metatable
    }};
    (
        [$s:ident: State, *$this:ident: $user_t:ty $(,$init_opt:ident)?]

        $(fn $name:ident ($($arg_def:tt)*) $($body_option:ident)? $body:block)*
    ) => {{
        fn init_metatable(meta: $crate::Table) {
            $(metatable!(@init-option $init_opt meta);)?

            meta.setf($crate::cstr!("__name"), stringify!($user_t));
            meta.setf($crate::cstr!("__gc"), metatable!(@method (*mut $user_t, $user_t), $s, ($this) () {
                if $this.0 == &mut $this.1 {
                    core::ptr::drop_in_place(&mut $this.1);
                }
                return 0;
            }));
            $(
                meta.setf($crate::cstr!(stringify!($name)), metatable!(
                    @method $user_t, $s, (*$this) ($($arg_def)*)
                    $($body_option)? $body
                ));
            )*
        }
        init_metatable
    }};

    // for table
    (
        ($s:ident: State, $this:ident: Self) $($option:ident)?;
        $($name:tt($($arg_def:tt)*) $($body_option:ident)? $body:block)*
    ) => {{
        fn init_metatable(meta: $crate::Table) {
            metatable!(@init-option $($option meta)?);
            $(
                meta.setf($crate::cstr!($name), metatable!(
                    @tablemethod, ($s, $this, $($arg_def)*)
                    $($body_option)? $body
                ));
            )*
        }
        init_metatable
    }};

    (@methods $s:ident: State, $meta:ident: Table, $this:ident: $user_t:ty;
        $(fn $name:tt($($arg_def:tt)*) $($body_option:ident)? $body:block)*
    ) => {{
        $(
            $meta.setf($crate::cstr!(stringify!($name)), metatable!(
                @method $user_t, $s, ($this) ($($arg_def)*)
                $($body_option)? $body
            ));
        )*
    }};

    (const $name:ident = $($rest:tt)*) => { const $name: InitMetatable = metatable!($($rest)*); };
    (pub const $name:ident = $($rest:tt)*) => { pub const $name: InitMetatable = metatable!($($rest)*); };
    // (static $name:ident = $($rest:tt)*) => { static $name: InitMetatable = metatable!($($rest)*); };
}
