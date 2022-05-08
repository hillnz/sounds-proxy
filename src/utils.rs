use cfg_if::cfg_if;
#[cfg(target_family = "wasm")]
use worker::{wasm_bindgen::prelude::wasm_bindgen, wasm_bindgen_futures::JsFuture};

cfg_if! {
    // https://github.com/rustwasm/console_error_panic_hook#readme
    if #[cfg(feature = "console_error_panic_hook")] {
        extern crate console_error_panic_hook;
        pub use self::console_error_panic_hook::set_once as set_panic_hook;
    } else {
        #[inline]
        pub fn set_panic_hook() {}
    }
}
