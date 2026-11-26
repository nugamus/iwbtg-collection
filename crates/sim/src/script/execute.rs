//! `script_execute` (#1533; docs/architecture/sim/scripts.md, census group "meta").
//!
//! `script_execute("name", args...)` calls a package script function by name: the name must be a
//! literal, so the validator resolves it (and the argument count) against all the package's
//! scripts; a dynamic or unknown name is a load error. The call shares the caller's run: the same
//! engine means the same step budget, RNG and lent world, and `this` stays the calling instance.

use std::rc::Rc;

use rhai::{Dynamic, Engine, ImmutableString};

use super::api::runtime_error;
use super::docs::Docs;
use super::host::Shared;

type Res<T> = Result<T, Box<rhai::EvalAltResult>>;

fn run(shared: &Rc<Shared>, name: &str, args: Vec<Dynamic>) -> Res<Dynamic> {
    let host = shared
        .host
        .get()
        .ok_or_else(|| runtime_error("script_execute needs a script host"))?;
    host.script_execute(name, args)
        .map_err(|e| runtime_error(e.message))
}

/// Registers `script_execute` for 0 to 8 arguments (Rhai has no variadic native functions).
pub fn register_execute_api(engine: &mut Engine, shared: &Rc<Shared>, docs: &mut Docs) {
    docs.add(
        "script_execute",
        "`script_execute(\"name\", args...)`: calls the package script function `name` with up to 8 arguments, `this` bound to the calling instance and the caller's step budget and world. The name must be a literal; the loader checks it and the argument count, and a dynamic or unknown name is an error.",
    );
    macro_rules! register {
        ($($arg:ident),*) => {{
            let s = Rc::clone(shared);
            engine.register_fn(
                "script_execute",
                move |name: ImmutableString $(, $arg: Dynamic)*| -> Res<Dynamic> {
                    run(&s, &name, vec![$($arg),*])
                },
            );
        }};
    }
    register!();
    register!(a);
    register!(a, b);
    register!(a, b, c);
    register!(a, b, c, d);
    register!(a, b, c, d, e);
    register!(a, b, c, d, e, f);
    register!(a, b, c, d, e, f, g);
    register!(a, b, c, d, e, f, g, h);
}
