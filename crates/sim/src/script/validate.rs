//! The IWS validator (docs/plans/sim-core.md 11, decision 0001 "Validation before running"; WP9.3).
//!
//! [`validate`] checks every script of a [`GameDef`] without running it and returns issues with
//! file, line and column. The format loader turns them into diagnostics, so the client, the
//! server and `iwbtg pkg check` all refuse the same scripts.
//!
//! What it checks, per script:
//!
//! - the source compiles under the IWS profile (strict variables, `eval` and `**` disabled);
//! - top-level items are only `fn` and `const`;
//! - event functions (`step`, `alarm`, ...) have the arity the host calls them with;
//! - every called function is registered on the engine or defined in the same file, with a
//!   matching argument count;
//! - string literals passed to the API (kinds, sounds, rooms) name something in the package;
//! - lints: a call that draws from the RNG or changes state in a later operand of `&&`/`||`
//!   (Rhai short-circuits, GML does not), `switch` on float case values, a bare number as a
//!   condition (`if this.hp`), and raw `==`/`!=` on a real value.
//!
//! The walk runs on the AST the engine itself compiles (simple optimizer), through `AST::walk`
//! (Rhai `internals`). Case values of a `switch` are hashed away at parse time, so that one lint
//! reads the token stream instead.

use std::collections::{BTreeMap, BTreeSet};

use rhai::{AST, ASTFlags, ASTNode, Expr, FnCallExpr, Position, Stmt, Token};

use super::host::Iws;
use crate::def::GameDef;
use crate::rules::RunnerRules;

/// How serious an [`Issue`] is. Errors refuse the package; warnings only report.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub enum Severity {
    /// The script cannot be used.
    Error,
    /// The script works but probably does not do what its author means.
    Warning,
}

/// One problem found in a script.
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub struct Issue {
    /// Index into `GameDef::scripts`.
    pub script: usize,
    /// 1-based line.
    pub line: u32,
    /// 1-based column.
    pub column: u32,
    /// Error or warning.
    pub severity: Severity,
    /// What is wrong, with the fix when there is one.
    pub message: String,
    /// For a string literal that names nothing: what it should name (`"sound"`) and the text.
    /// The format loader uses it to explain an `engine:<pack>/<id>` name that does not resolve
    /// the same way the physics `[sounds]` do.
    pub unknown: Option<(String, String)>,
}

/// The event functions the host calls, with their argument counts. `draw(d)` and `trigger(name)`
/// run from the client and from triggers; `moment_<name>()` is checked separately.
pub const EVENT_FNS: &[(&str, usize)] = &[
    ("create", 0),
    ("destroy", 0),
    ("begin_step", 0),
    ("step", 0),
    ("end_step", 0),
    ("late", 0),
    ("alarm", 1),
    ("user", 1),
    ("collide", 1),
    ("trigger", 1),
    ("outside_room", 0),
    ("intersect_boundary", 0),
    ("animation_end", 0),
    ("room_start", 0),
    ("room_end", 0),
    ("game_start", 0),
    ("end_of_path", 0),
    ("draw", 1),
];

/// Functions that draw from the RNG.
pub const RNG_FNS: &[&str] = &["random", "random_range", "irandom", "irandom_range", "choose"];

/// Functions that change the world or queue an effect.
pub const STATE_FNS: &[&str] = &[
    "set_alarm",
    "instance_create",
    "destroy",
    "emit",
    "kill_kid",
    "print",
    "debug",
];

/// Registered functions that return a number.
pub const NUMBER_FNS: &[&str] = &[
    "round",
    "floor",
    "ceil",
    "sign",
    "min",
    "max",
    "abs",
    "frac",
    "clamp",
    "lerp",
    "sqrt",
    "sqr",
    "pow",
    "exp",
    "ln",
    "sin",
    "cos",
    "tan",
    "arctan2",
    "degtorad",
    "radtodeg",
    "dsin",
    "dcos",
    "darctan2",
    "point_direction",
    "point_distance",
    "lengthdir_x",
    "lengthdir_y",
    "div",
    "mod",
    "to_float",
    "to_int",
    "tick",
    "room_tick",
    "get_alarm",
    "random",
    "random_range",
    "irandom",
    "irandom_range",
];

/// Other registered functions without side effects (conversions, comparisons, queries).
pub const OTHER_PURE_FNS: &[&str] = &[
    "gm_eq",
    "gm_ne",
    "gm_lt",
    "gm_le",
    "gm_gt",
    "gm_ge",
    "gm_bool",
    "gm_repeat",
    "exists",
    "instances",
    "kid",
];

/// Instance properties that read as a real number.
pub const REAL_PROPS: &[&str] = &[
    "x",
    "y",
    "xprevious",
    "yprevious",
    "xstart",
    "ystart",
    "hspeed",
    "vspeed",
    "speed",
    "direction",
    "gravity",
    "gravity_direction",
    "friction",
    "image_index",
    "image_speed",
    "image_xscale",
    "image_yscale",
    "image_angle",
    "image_alpha",
    "depth",
    "id",
];

/// Functions Rhai itself handles (function pointers); they cannot be checked statically.
const ENGINE_FNS: &[&str] = &["Fn", "call", "curry", "is_def_fn", "is_def_var"];

/// What a string literal argument must name.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Names {
    /// A kind (`"@orb"`).
    Kind,
    /// A sound or music id.
    Sound,
    /// A room id.
    Room,
}

/// API functions that take a name, as (function, argument index, what it names). A new API
/// function that takes a kind, sound or room gets a row here.
///
/// A row whose function is not registered yet is inert (the call is already an error); the sound
/// rows are ready for `play_sound` and `play_music`. Pack sounds and music are named
/// `engine:<pack>/<id>` and are in `GameDef::sounds` under that name, like the package's own.
pub const NAME_ARGS: &[(&str, usize, Names)] = &[
    ("instance_create", 0, Names::Kind),
    ("instances", 0, Names::Kind),
    ("play_sound", 0, Names::Sound),
    ("play_music", 0, Names::Sound),
];

/// Whether `name` is something `what` in the package.
#[must_use]
pub fn resolves(def: &GameDef, what: Names, name: &str) -> bool {
    match what {
        Names::Kind => def.kinds.iter().any(|k| k.name == name),
        Names::Sound => def.sounds.iter().any(|s| s.name == name),
        Names::Room => def.rooms.iter().any(|r| r.name == name),
    }
}

impl Names {
    fn noun(self) -> &'static str {
        match self {
            Names::Kind => "kind",
            Names::Sound => "sound",
            Names::Room => "room",
        }
    }
}

/// Checks every script of `def`. Issues come sorted by script, line and column.
#[must_use]
pub fn validate(def: &GameDef) -> Vec<Issue> {
    if def.scripts.is_empty() {
        return Vec::new();
    }
    let iws = Iws::new(RunnerRules::default().num);
    let registry = Registry::new(&iws);
    let mut issues = Vec::new();
    for (index, script) in def.scripts.iter().enumerate() {
        issues.extend(check_script(&iws, &registry, def, index, &script.source));
    }
    issues.sort();
    issues.dedup();
    issues
}

/// Registered functions as name to the argument counts they accept.
struct Registry {
    fns: BTreeMap<String, BTreeSet<usize>>,
}

impl Registry {
    fn new(iws: &Iws) -> Registry {
        let all = iws.engine.collect_fn_metadata(
            None,
            |f| Some((f.metadata.name.to_string(), f.metadata.num_params)),
            true,
        );
        let mut fns: BTreeMap<String, BTreeSet<usize>> = BTreeMap::new();
        for (name, n) in all {
            fns.entry(name).or_default().insert(n);
        }
        Registry { fns }
    }
}

/// Everything one script's checks share.
struct Ctx<'a> {
    def: &'a GameDef,
    registry: &'a Registry,
    script: usize,
    /// Script functions as name to argument counts.
    defined: BTreeMap<String, BTreeSet<usize>>,
    /// Script functions that can draw from the RNG or change state, directly or through a call.
    impure: BTreeSet<String>,
    issues: Vec<Issue>,
}

impl Ctx<'_> {
    fn push(&mut self, severity: Severity, pos: Position, message: String) {
        self.issues.push(Issue {
            script: self.script,
            line: pos.line().and_then(|l| u32::try_from(l).ok()).unwrap_or(1),
            column: pos.position().and_then(|c| u32::try_from(c).ok()).unwrap_or(1),
            severity,
            message,
            unknown: None,
        });
    }

    fn unknown_name(&mut self, pos: Position, what: Names, name: &str) {
        self.error(pos, format!("unknown {} \"{name}\"", what.noun()));
        if let Some(last) = self.issues.last_mut() {
            last.unknown = Some((what.noun().to_owned(), name.to_owned()));
        }
    }

    fn error(&mut self, pos: Position, message: impl Into<String>) {
        self.push(Severity::Error, pos, message.into());
    }

    fn warn(&mut self, pos: Position, message: impl Into<String>) {
        self.push(Severity::Warning, pos, message.into());
    }
}

/// Checks one script's source.
fn check_script(iws: &Iws, registry: &Registry, def: &GameDef, script: usize, source: &str) -> Vec<Issue> {
    let mut cx = Ctx {
        def,
        registry,
        script,
        defined: BTreeMap::new(),
        impure: BTreeSet::new(),
        issues: Vec::new(),
    };
    let ast = match iws.engine.compile(source) {
        Ok(ast) => ast,
        Err(e) => {
            cx.error(e.position(), format!("compile error: {}", e.err_type()));
            return cx.issues;
        }
    };
    for f in ast.iter_fn_def() {
        cx.defined
            .entry(f.name.to_string())
            .or_default()
            .insert(f.params.len());
    }
    let tokens = scan_tokens(iws, source);
    check_top_level(&mut cx, &ast);
    check_event_fns(&mut cx, &tokens);
    cx.impure = impure_fns(&ast, &cx.defined);
    ast.walk(&mut |path: &[ASTNode]| {
        if let Some(node) = path.last() {
            check_node(&mut cx, node);
        }
        true
    });
    for (pos, what) in tokens.float_cases {
        cx.error(
            pos,
            format!(
                "`switch` on a float case value ({what}): Rhai hashes case values and never calls \
                 `==`, so the match is exact. Use `if gm_eq(x, {what}) {{ ... }}` instead"
            ),
        );
    }
    cx.issues
}

// ---- Tokens: function positions and float switch cases ----

struct Tokens {
    /// Where each function is defined, by name.
    fn_pos: BTreeMap<String, Position>,
    /// Float literals in the case position of a `switch`.
    float_cases: Vec<(Position, String)>,
}

/// One open `switch` block while scanning.
struct SwitchFrame {
    /// Bracket depth inside the switch's braces.
    depth: usize,
    /// Whether the scan is at a case pattern (before `=>`).
    pattern: bool,
}

fn scan_tokens(iws: &Iws, source: &str) -> Tokens {
    let inputs = [source];
    let (stream, _) = iws.engine.lex(&inputs);
    let mut out = Tokens {
        fn_pos: BTreeMap::new(),
        float_cases: Vec::new(),
    };
    let mut depth = 0usize;
    let mut frames: Vec<SwitchFrame> = Vec::new();
    // The bracket depth of a `switch` keyword whose block has not opened yet.
    let mut pending_switch: Option<usize> = None;
    let mut after_fn = false;
    for (tok, pos) in stream {
        if matches!(tok, Token::EOF) {
            break;
        }
        if after_fn {
            if let Token::Identifier(name) = &tok {
                out.fn_pos.entry(name.to_string()).or_insert(pos);
            }
            after_fn = false;
        }
        // Handled before the depth changes, so that a case's own closing brace is seen at the
        // switch's depth.
        let at_switch_level = frames.last().is_some_and(|f| f.depth == depth);
        match &tok {
            Token::Fn => after_fn = true,
            Token::Switch => pending_switch = Some(depth),
            Token::LeftBrace | Token::MapStart | Token::LeftParen | Token::LeftBracket => {
                depth += 1;
                if matches!(tok, Token::LeftBrace) && pending_switch == Some(depth - 1) {
                    pending_switch = None;
                    frames.push(SwitchFrame { depth, pattern: true });
                }
            }
            Token::RightBrace | Token::RightParen | Token::RightBracket => {
                depth = depth.saturating_sub(1);
                if let Some(f) = frames.last_mut() {
                    if depth + 1 == f.depth && matches!(tok, Token::RightBrace) {
                        frames.pop();
                    } else if depth == f.depth && matches!(tok, Token::RightBrace) {
                        // The block of a case arm closed.
                        f.pattern = true;
                    }
                }
            }
            Token::FloatConstant(v) => {
                if at_switch_level && frames.last().is_some_and(|f| f.pattern) {
                    out.float_cases.push((pos, v.1.to_string()));
                }
            }
            Token::DoubleArrow if at_switch_level => {
                if let Some(f) = frames.last_mut() {
                    f.pattern = false;
                }
            }
            Token::Comma if at_switch_level => {
                if let Some(f) = frames.last_mut() {
                    f.pattern = true;
                }
            }
            Token::If if at_switch_level => {
                // A guard (`x if cond => ...`) is not a case value.
                if let Some(f) = frames.last_mut() {
                    f.pattern = false;
                }
            }
            _ => {}
        }
    }
    out
}

// ---- Structure ----

fn check_top_level(cx: &mut Ctx, ast: &AST) {
    for stmt in ast.statements() {
        match stmt {
            Stmt::Var(_, flags, _) if flags.contains(ASTFlags::CONSTANT) => {}
            other => cx.error(
                other.position(),
                "only `fn` and `const` items are allowed at the top level of a script; \
                 put the statement in an event function",
            ),
        }
    }
}

fn check_event_fns(cx: &mut Ctx, tokens: &Tokens) {
    let defined: Vec<(String, BTreeSet<usize>)> =
        cx.defined.iter().map(|(k, v)| (k.clone(), v.clone())).collect();
    for (name, arities) in defined {
        let pos = tokens.fn_pos.get(&name).copied().unwrap_or(Position::NONE);
        let want = if let Some((_, n)) = EVENT_FNS.iter().find(|(e, _)| *e == name) {
            Some(*n)
        } else if name.starts_with("moment_") {
            Some(0)
        } else {
            None
        };
        if let Some(n) = want {
            for a in arities {
                if a != n {
                    cx.error(
                        pos,
                        format!(
                            "the event function `{name}` takes {n} argument{}, not {a}",
                            if n == 1 { "" } else { "s" }
                        ),
                    );
                }
            }
        }
    }
}

// ---- Calls and effects ----

/// A call node: the call, its position and whether it is `receiver.name(args)`.
fn call_of<'a>(node: &ASTNode<'a>) -> Option<(&'a FnCallExpr, Position, bool)> {
    match node {
        ASTNode::Expr(Expr::FnCall(c, p)) => Some((c, *p, false)),
        ASTNode::Expr(Expr::MethodCall(c, p)) => Some((c, *p, true)),
        ASTNode::Stmt(Stmt::FnCall(c, p)) => Some((c, *p, false)),
        _ => None,
    }
}

/// What a node does that the right operand of `&&`/`||` must not do.
enum Effect {
    Rng(String),
    State(String),
    Script(String),
}

impl Effect {
    fn describe(&self) -> String {
        match self {
            Effect::Rng(n) => format!("`{n}` draws from the RNG"),
            Effect::State(n) => format!("`{n}` changes state"),
            Effect::Script(n) => {
                format!("`{n}` is a script function that draws from the RNG or changes state")
            }
        }
    }
}

fn effect_of(node: &ASTNode, impure: &BTreeSet<String>) -> Option<(Position, Effect)> {
    if let Some((c, pos, _)) = call_of(node) {
        let name = c.name.as_str();
        if RNG_FNS.contains(&name) {
            return Some((pos, Effect::Rng(name.to_owned())));
        }
        if STATE_FNS.contains(&name) {
            return Some((pos, Effect::State(name.to_owned())));
        }
        if impure.contains(name) {
            return Some((pos, Effect::Script(name.to_owned())));
        }
        return None;
    }
    if let ASTNode::Stmt(Stmt::Assignment(a)) = node
        && !matches!(a.1.lhs, Expr::Variable(..))
    {
        return Some((
            a.0.position(),
            Effect::State("an assignment to a property".to_owned()),
        ));
    }
    None
}

/// The script functions that can draw from the RNG or change state, directly or by calling
/// another one that can.
fn impure_fns(ast: &AST, defined: &BTreeMap<String, BTreeSet<usize>>) -> BTreeSet<String> {
    let mut direct = BTreeSet::new();
    let mut calls: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for name in defined.keys() {
        let sub = ast.clone_functions_only_filtered(|_, _, _, n, _| n == name);
        let mut callees = BTreeSet::new();
        let no_script_fns = BTreeSet::new();
        let mut effect = false;
        sub.walk(&mut |path: &[ASTNode]| {
            if let Some(node) = path.last() {
                if effect_of(node, &no_script_fns).is_some() {
                    effect = true;
                }
                if let Some((c, _, _)) = call_of(node) {
                    callees.insert(c.name.to_string());
                }
            }
            true
        });
        if effect {
            direct.insert(name.clone());
        }
        calls.insert(name.clone(), callees);
    }
    let mut impure = direct;
    loop {
        let more: Vec<String> = calls
            .iter()
            .filter(|(n, callees)| !impure.contains(*n) && callees.iter().any(|c| impure.contains(c)))
            .map(|(n, _)| n.clone())
            .collect();
        if more.is_empty() {
            return impure;
        }
        impure.extend(more);
    }
}

fn check_node(cx: &mut Ctx, node: &ASTNode) {
    if let Some((c, pos, method)) = call_of(node) {
        check_call(cx, c, pos, method);
    }
    match node {
        ASTNode::Expr(Expr::And(ops, _)) => check_operands(cx, ops, "&&", "&"),
        ASTNode::Expr(Expr::Or(ops, _)) => check_operands(cx, ops, "||", "|"),
        ASTNode::Stmt(Stmt::If(f, _) | Stmt::While(f, _) | Stmt::Do(f, _, _)) => {
            check_condition(cx, &f.expr);
        }
        _ => {}
    }
    if let ASTNode::Expr(e) = node {
        check_name_compare(cx, e);
    }
}

/// `a && b`: Rhai skips `b` when `a` decides, GML never does.
fn check_operands(cx: &mut Ctx, ops: &[Expr], op: &str, non_short: &str) {
    for e in ops {
        check_condition(cx, e);
    }
    for operand in ops.iter().skip(1) {
        let mut found = Vec::new();
        let mut path = Vec::new();
        operand.walk(&mut path, &mut |p: &[ASTNode]| {
            if let Some(node) = p.last()
                && let Some(hit) = effect_of(node, &cx.impure)
            {
                found.push(hit);
            }
            true
        });
        for (pos, effect) in found {
            let message = format!(
                "{} in a right operand of `{op}`. Rhai skips that operand when the left one decides, \
                 GML evaluates both. Use `{non_short}` on bools, or evaluate it into a variable first",
                effect.describe()
            );
            cx.error(pos, message);
        }
    }
}

fn check_call(cx: &mut Ctx, c: &FnCallExpr, pos: Position, method: bool) {
    let name = c.name.as_str();
    let args = c.args.len() + usize::from(method);
    // Operators (`&`, `|`, `+`) on built-in types are the engine's own.
    if ENGINE_FNS.contains(&name) || !name.starts_with(|c: char| c.is_ascii_alphabetic()) {
        return;
    }
    let script_arities = cx.defined.get(name);
    let reg_arities = cx.registry.fns.get(name);
    let known =
        script_arities.is_some_and(|s| s.contains(&args)) || reg_arities.is_some_and(|s| s.contains(&args));
    if !known {
        let message = match (script_arities, reg_arities) {
            (None, None) => format!(
                "unknown function `{name}`: it is not part of the script API and this file does not define it"
            ),
            (s, r) => {
                let mut have: BTreeSet<usize> = BTreeSet::new();
                have.extend(s.into_iter().flatten());
                have.extend(r.into_iter().flatten());
                let list: Vec<String> = have.iter().map(ToString::to_string).collect();
                format!(
                    "`{name}` is called with {args} argument{}, but takes {}",
                    if args == 1 { "" } else { "s" },
                    list.join(" or ")
                )
            }
        };
        cx.error(pos, message);
    }
    if !method {
        for (f, index, what) in NAME_ARGS {
            if *f == name
                && let Some(Expr::StringConstant(s, p)) = c.args.get(*index)
                && !resolves(cx.def, *what, s)
            {
                cx.unknown_name(*p, *what, s);
            }
        }
    }
}

/// The property a read ends in, for `a.b.c` the name `c`.
fn last_property(e: &Expr) -> Option<&str> {
    match e {
        Expr::Dot(b, _, _) => last_property(&b.rhs),
        Expr::Property(p, _) => Some(p.2.as_str()),
        _ => None,
    }
}

fn real_property(e: &Expr) -> Option<&str> {
    last_property(e).filter(|n| REAL_PROPS.contains(n))
}

/// Whether `e` is certainly a number: a literal, a real property or a call of a number function.
fn number_name(e: &Expr) -> Option<String> {
    match e {
        Expr::IntegerConstant(i, _) => Some(i.to_string()),
        Expr::FloatConstant(f, _) => Some(f.to_string()),
        Expr::FnCall(c, _) if NUMBER_FNS.contains(&c.name.as_str()) => Some(format!("{}(...)", c.name)),
        other => real_property(other).map(str::to_owned),
    }
}

fn check_condition(cx: &mut Ctx, e: &Expr) {
    if let Some(what) = number_name(e) {
        cx.error(
            e.position(),
            format!(
                "`{what}` is a number used as a condition. Rhai has no truthiness; \
                 write `gm_bool({what})` or compare it"
            ),
        );
    }
}

fn is_float_literal(e: &Expr) -> bool {
    matches!(e, Expr::FloatConstant(..))
}

/// `==`/`!=` calls: float lint and `.kind` against a literal.
fn check_name_compare(cx: &mut Ctx, e: &Expr) {
    let Expr::FnCall(c, pos) = e else { return };
    let name = c.name.as_str();
    if !matches!(name, "==" | "!=") || c.args.len() != 2 {
        return;
    }
    let (a, b) = (&c.args[0], &c.args[1]);
    for (x, y) in [(a, b), (b, a)] {
        if last_property(x) == Some("kind")
            && let Expr::StringConstant(s, p) = y
            && !resolves(cx.def, Names::Kind, s)
        {
            cx.error(*p, format!("unknown kind \"{s}\""));
        }
    }
    let real = |e: &Expr| is_float_literal(e) || real_property(e).is_some();
    if real(a) || real(b) {
        let gm = if name == "==" { "gm_eq" } else { "gm_ne" };
        cx.warn(
            *pos,
            format!(
                "raw `{name}` on a real value is an exact comparison; GM compares with an epsilon. \
                 Use `{gm}(a, b)`"
            ),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classified_functions_cover_the_registry() {
        let iws = Iws::new(RunnerRules::default().num);
        let ours: Vec<String> =
            iws.engine
                .collect_fn_metadata(None, |f| Some(f.metadata.name.to_string()), false);
        let known: BTreeSet<&str> = RNG_FNS
            .iter()
            .chain(STATE_FNS)
            .chain(NUMBER_FNS)
            .chain(OTHER_PURE_FNS)
            .copied()
            .collect();
        for name in ours {
            if name.contains('$') || !name.starts_with(|c: char| c.is_ascii_alphabetic()) {
                continue;
            }
            assert!(
                known.contains(name.as_str()),
                "registered function `{name}` is not classified in script/validate.rs"
            );
        }
        for name in known {
            assert!(
                Registry::new(&iws).fns.contains_key(name) || name == "print" || name == "debug",
                "classified function `{name}` is not registered"
            );
        }
    }

    #[test]
    fn real_props_match_the_getters() {
        let iws = Iws::new(RunnerRules::default().num);
        let getters: BTreeSet<String> = iws
            .engine
            .collect_fn_metadata(None, |f| Some(f.metadata.name.to_string()), false)
            .into_iter()
            .filter_map(|n| n.strip_prefix("get$").map(str::to_owned))
            .collect();
        for p in REAL_PROPS {
            assert!(getters.contains(*p), "`{p}` is not a registered getter");
        }
        for g in &getters {
            if !["visible", "solid", "kind"].contains(&g.as_str()) {
                assert!(
                    REAL_PROPS.contains(&g.as_str()),
                    "getter `{g}` is not in REAL_PROPS"
                );
            }
        }
    }

    /// `play_sound` and `play_music` are not in the API yet; stubs let the name rows be tested.
    #[test]
    fn sound_and_music_names_resolve_including_engine_pack_names() {
        use crate::def::testing::GameDefBuilder;
        use crate::def::{RoomDef, SoundDef, SoundKind};
        use crate::num::Real;
        let mut b = GameDefBuilder::new("t373");
        let room = b.room(RoomDef::new("room", 320, 240));
        b.start_room(room);
        let mut def = b.build().expect("def");
        for name in ["boom", "engine:a1/jump"] {
            def.sounds.push(SoundDef {
                name: name.into(),
                kind: SoundKind::default(),
                length_ticks: 10,
                volume: Real::new(1.0),
                pan: Real::new(0.0),
            });
        }
        let mut iws = Iws::new(RunnerRules::default().num);
        iws.engine
            .register_fn("play_sound", |_: rhai::ImmutableString| {});
        iws.engine
            .register_fn("play_music", |_: rhai::ImmutableString| {});
        let registry = Registry::new(&iws);
        let check = |src: &str| check_script(&iws, &registry, &def, 0, src);
        assert!(check("fn step() { play_sound(\"boom\"); play_sound(\"engine:a1/jump\"); play_music(\"engine:a1/jump\"); }").is_empty());
        let i = check("fn step() { play_sound(\"engine:a1/gone\"); play_music(\"nope\"); }");
        assert_eq!(i.len(), 2, "{i:#?}");
        assert_eq!(
            i[0].unknown,
            Some(("sound".to_owned(), "engine:a1/gone".to_owned()))
        );
        assert_eq!(i[0].message, "unknown sound \"engine:a1/gone\"");
    }

    #[test]
    fn host_events_are_valid_event_functions() {
        use crate::events::EventKind;
        for ev in [
            EventKind::Create,
            EventKind::Step,
            EventKind::Alarm(0),
            EventKind::OutsideRoom,
        ] {
            let (name, arg) = super::super::host::event_call(&ev).expect("a script event");
            let arity = usize::from(arg.is_some());
            assert!(EVENT_FNS.contains(&(name, arity)), "{name}/{arity}");
        }
    }
}
