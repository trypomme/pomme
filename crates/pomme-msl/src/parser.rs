//! A parser for generated MSL: structs (with `[[attributes]]`), constants,
//! functions and entry points, C statements and expressions, member access
//! through `.` and `->`, method calls (`t.sample(s, uv)`) and casts.

use crate::lexer::Tok;

#[derive(Debug, Clone, PartialEq)]
pub struct TypeName {
    pub name: String,
    pub args: Vec<TypeArg>,
    /// Address space (`constant`, `device`, `thread`, `threadgroup`), if given.
    pub space: Option<String>,
    pub is_const: bool,
    pub reference: bool,
    pub pointer: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub enum TypeArg {
    Type(TypeName),
    Int(u64),
}

#[derive(Debug, Clone, PartialEq)]
pub struct Attr {
    pub name: String,
    pub args: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct Field {
    pub ty: TypeName,
    pub name: String,
    pub array: Option<u32>,
    pub attrs: Vec<Attr>,
    /// A parameter's default value (`float b = -0.475`).
    pub default: Option<Expr>,
}

#[derive(Debug, Clone)]
pub struct Func {
    /// `vertex` / `fragment` for entry points.
    pub stage: Option<String>,
    pub ret: TypeName,
    pub name: String,
    pub params: Vec<Field>,
    pub body: Vec<Stmt>,
}

#[derive(Debug, Clone)]
pub enum Item {
    Struct { name: String, fields: Vec<Field> },
    Const { ty: TypeName, name: String, array: Option<u32>, init: Expr },
    Func(Func),
}

#[derive(Debug, Clone)]
pub enum Stmt {
    Decl { ty: TypeName, name: String, array: Option<u32>, init: Option<Expr> },
    Expr(Expr),
    If(Expr, Box<Stmt>, Option<Box<Stmt>>),
    For(Option<Box<Stmt>>, Option<Expr>, Option<Expr>, Box<Stmt>),
    While(Expr, Box<Stmt>),
    DoWhile(Box<Stmt>, Expr),
    Return(Option<Expr>),
    Break,
    Continue,
    Discard,
    Block(Vec<Stmt>),
    Empty,
}

#[derive(Debug, Clone)]
pub enum Expr {
    Ident(String),
    Int(u64, bool),
    Float(f64),
    Bool(bool),
    Unary(&'static str, Box<Expr>),
    /// `x++` / `x--`.
    Postfix(&'static str, Box<Expr>),
    Binary(&'static str, Box<Expr>, Box<Expr>),
    Assign(&'static str, Box<Expr>, Box<Expr>),
    Ternary(Box<Expr>, Box<Expr>, Box<Expr>),
    /// `f(args)`, `float2(args)`, `t.sample(args)` (callee `Member(t, "sample")`).
    Call(Box<Expr>, Vec<Expr>),
    /// `as_type<T>(x)`, `array<T, N>(…)`: a templated name applied to arguments.
    TemplateCall(TypeName, Vec<Expr>),
    Member(Box<Expr>, String),
    Arrow(Box<Expr>, String),
    Index(Box<Expr>, Box<Expr>),
    Cast(TypeName, Box<Expr>),
    /// `{a, b}` (aggregate initializers).
    InitList(Vec<Expr>),
}

pub struct Parser {
    toks: Vec<Tok>,
    pos: usize,
    /// Struct names seen so far (they start declarations like built-in types).
    structs: Vec<String>,
}

const SPACES: &[&str] = &["constant", "device", "thread", "threadgroup"];
const SCALARS: &[&str] = &["void", "bool", "char", "uchar", "short", "ushort", "int", "uint", "long", "ulong", "half", "float", "size_t"];
const TEMPLATES: &[&str] = &["array", "texture2d", "texture1d", "texture3d", "texturecube", "texture2d_array", "depth2d", "as_type", "static_cast", "packed_float2", "vec"];

/// Is `name` a built-in type name (scalars, vectors `float4`, matrices `half3x3`, samplers)?
pub fn is_builtin_type(name: &str) -> bool {
    if SCALARS.contains(&name) || name == "sampler" || name.starts_with("texture") || name.starts_with("packed_") {
        return true;
    }
    let base = name.trim_end_matches(|c: char| c.is_ascii_digit() || c == 'x');
    SCALARS.contains(&base) && base.len() < name.len()
}

impl Parser {
    pub fn new(toks: Vec<Tok>) -> Self {
        Parser { toks, pos: 0, structs: Vec::new() }
    }

    fn peek(&self) -> &Tok {
        &self.toks[self.pos]
    }

    fn peek_at(&self, n: usize) -> &Tok {
        self.toks.get(self.pos + n).unwrap_or(&Tok::Eof)
    }

    fn next(&mut self) -> Tok {
        let t = self.toks[self.pos].clone();
        if self.pos < self.toks.len() - 1 {
            self.pos += 1;
        }
        t
    }

    fn is_punct(&self, p: &str) -> bool {
        matches!(self.peek(), Tok::Punct(q) if *q == p)
    }

    fn eat(&mut self, p: &str) -> bool {
        if self.is_punct(p) {
            self.pos += 1;
            true
        } else {
            false
        }
    }

    fn expect(&mut self, p: &str) -> Result<(), String> {
        if self.eat(p) { Ok(()) } else { Err(format!("expected `{p}`, found {:?} (token {})", self.peek(), self.pos)) }
    }

    fn ident(&mut self) -> Result<String, String> {
        match self.next() {
            Tok::Ident(s) => Ok(s),
            t => Err(format!("expected a name, found {t:?} (token {})", self.pos)),
        }
    }

    fn is_ident(&self, s: &str) -> bool {
        matches!(self.peek(), Tok::Ident(x) if x == s)
    }

    fn is_type_name(&self, name: &str) -> bool {
        is_builtin_type(name) || self.structs.iter().any(|s| s == name) || TEMPLATES.contains(&name)
    }

    /// Does a declaration start here (qualifiers, or a type followed by a name)?
    fn at_declaration(&self) -> bool {
        match self.peek() {
            Tok::Ident(s) if s == "const" || s == "static" || SPACES.contains(&s.as_str()) => true,
            Tok::Ident(s) if self.is_type_name(s) => match self.peek_at(1) {
                Tok::Ident(_) => true,
                Tok::Punct("<") => true,
                Tok::Punct("&") | Tok::Punct("*") => matches!(self.peek_at(2), Tok::Ident(_)),
                _ => false,
            },
            Tok::Ident(_) => matches!(self.peek_at(1), Tok::Ident(_)),
            _ => false,
        }
    }

    pub fn module(&mut self) -> Result<Vec<Item>, String> {
        let mut items = Vec::new();
        while *self.peek() != Tok::Eof {
            if self.is_ident("using") {
                while !self.eat(";") {
                    self.next();
                }
                continue;
            }
            if self.eat(";") {
                continue;
            }
            if self.is_ident("struct") {
                self.next();
                let name = self.ident()?;
                self.structs.push(name.clone());
                let mut fields = Vec::new();
                self.expect("{")?;
                while !self.eat("}") {
                    fields.push(self.field()?);
                    self.expect(";")?;
                }
                self.expect(";")?;
                items.push(Item::Struct { name, fields });
                continue;
            }
            let stage = match self.peek() {
                Tok::Ident(s) if s == "vertex" || s == "fragment" || s == "kernel" => {
                    let s = s.clone();
                    self.next();
                    Some(s)
                }
                _ => None,
            };
            let ty = self.type_name()?;
            let name = self.ident()?;
            if self.eat("(") {
                let mut params = Vec::new();
                while !self.eat(")") {
                    params.push(self.field()?);
                    self.eat(",");
                }
                self.attrs()?; // function attributes, if any
                let body = self.block()?;
                items.push(Item::Func(Func { stage, ret: ty, name, params, body }));
            } else {
                let array = self.array_suffix()?;
                self.expect("=")?;
                let init = self.initializer()?;
                self.expect(";")?;
                items.push(Item::Const { ty, name, array, init });
            }
        }
        Ok(items)
    }

    /// `[[a, b(c)]] [[d]]` …
    fn attrs(&mut self) -> Result<Vec<Attr>, String> {
        let mut out = Vec::new();
        while *self.peek() == Tok::AttrOpen {
            self.next();
            loop {
                let name = self.ident()?;
                let mut args = Vec::new();
                if self.eat("(") {
                    while !self.eat(")") {
                        match self.next() {
                            Tok::Ident(s) => args.push(s),
                            Tok::Int(v, _) => args.push(v.to_string()),
                            Tok::Punct(",") => {}
                            t => args.push(format!("{t:?}")),
                        }
                    }
                }
                out.push(Attr { name, args });
                // `[[a, b]]` and `[[user(locn0) flat]]` both list several.
                if !self.eat(",") && !matches!(self.peek(), Tok::Ident(_)) {
                    break;
                }
            }
            if self.next() != Tok::AttrClose {
                return Err("expected `]]`".into());
            }
        }
        Ok(out)
    }

    fn array_suffix(&mut self) -> Result<Option<u32>, String> {
        if self.eat("[") {
            let n = match self.next() {
                Tok::Int(v, _) => v as u32,
                t => return Err(format!("expected an array size, found {t:?}")),
            };
            self.expect("]")?;
            Ok(Some(n))
        } else {
            Ok(None)
        }
    }

    /// A struct field or parameter: `type name [N] [[attrs]]`.
    fn field(&mut self) -> Result<Field, String> {
        let ty = self.type_name()?;
        let name = self.ident()?;
        let array = self.array_suffix()?;
        let attrs = self.attrs()?;
        let default = if self.eat("=") { Some(self.ternary()?) } else { None };
        Ok(Field { ty, name, array, attrs, default })
    }

    pub fn type_name(&mut self) -> Result<TypeName, String> {
        let mut space = None;
        let mut is_const = false;
        loop {
            match self.peek() {
                Tok::Ident(s) if s == "const" => is_const = true,
                Tok::Ident(s) if s == "static" => {}
                Tok::Ident(s) if SPACES.contains(&s.as_str()) => space = Some(s.clone()),
                _ => break,
            }
            self.next();
        }
        let mut name = self.ident()?;
        while self.eat("::") {
            name = format!("{name}::{}", self.ident()?);
        }
        let mut args = Vec::new();
        if self.is_punct("<") && TEMPLATES.contains(&name.as_str()) {
            self.next();
            loop {
                match self.peek().clone() {
                    Tok::Int(v, _) => {
                        self.next();
                        args.push(TypeArg::Int(v));
                    }
                    _ => args.push(TypeArg::Type(self.type_name()?)),
                }
                if !self.eat(",") {
                    break;
                }
            }
            self.expect(">")?;
        }
        while self.is_ident("const") {
            self.next();
            is_const = true;
        }
        let reference = self.eat("&");
        let pointer = !reference && self.eat("*");
        Ok(TypeName { name, args, space, is_const, reference, pointer })
    }

    fn block(&mut self) -> Result<Vec<Stmt>, String> {
        self.expect("{")?;
        let mut out = Vec::new();
        while !self.eat("}") {
            out.push(self.stmt()?);
        }
        Ok(out)
    }

    fn stmt(&mut self) -> Result<Stmt, String> {
        if self.is_punct("{") {
            return Ok(Stmt::Block(self.block()?));
        }
        if self.eat(";") {
            return Ok(Stmt::Empty);
        }
        if let Tok::Ident(kw) = self.peek().clone() {
            match kw.as_str() {
                "if" => {
                    self.next();
                    self.expect("(")?;
                    let c = self.expr()?;
                    self.expect(")")?;
                    let then = Box::new(self.stmt()?);
                    let els = if self.is_ident("else") {
                        self.next();
                        Some(Box::new(self.stmt()?))
                    } else {
                        None
                    };
                    return Ok(Stmt::If(c, then, els));
                }
                "for" => {
                    self.next();
                    self.expect("(")?;
                    let init = if self.eat(";") {
                        None
                    } else {
                        let s = self.simple_stmt()?;
                        self.expect(";")?;
                        Some(Box::new(s))
                    };
                    let cond = if self.is_punct(";") { None } else { Some(self.expr()?) };
                    self.expect(";")?;
                    let step = if self.is_punct(")") { None } else { Some(self.expr()?) };
                    self.expect(")")?;
                    return Ok(Stmt::For(init, cond, step, Box::new(self.stmt()?)));
                }
                "while" => {
                    self.next();
                    self.expect("(")?;
                    let c = self.expr()?;
                    self.expect(")")?;
                    return Ok(Stmt::While(c, Box::new(self.stmt()?)));
                }
                "do" => {
                    self.next();
                    let body = Box::new(self.stmt()?);
                    if self.ident()? != "while" {
                        return Err("expected `while` after a do body".into());
                    }
                    self.expect("(")?;
                    let c = self.expr()?;
                    self.expect(")")?;
                    self.expect(";")?;
                    return Ok(Stmt::DoWhile(body, c));
                }
                "return" => {
                    self.next();
                    let v = if self.is_punct(";") { None } else { Some(self.expr()?) };
                    self.expect(";")?;
                    return Ok(Stmt::Return(v));
                }
                "break" | "continue" => {
                    self.next();
                    self.expect(";")?;
                    return Ok(if kw == "break" { Stmt::Break } else { Stmt::Continue });
                }
                "discard_fragment" => {
                    self.next();
                    self.expect("(")?;
                    self.expect(")")?;
                    self.expect(";")?;
                    return Ok(Stmt::Discard);
                }
                _ => {}
            }
        }
        let s = self.simple_stmt()?;
        self.expect(";")?;
        Ok(s)
    }

    /// A declaration or an expression (no trailing `;`).
    fn simple_stmt(&mut self) -> Result<Stmt, String> {
        if self.at_declaration() {
            let ty = self.type_name()?;
            let name = self.ident()?;
            let array = self.array_suffix()?;
            // `T x = e`, `T x = {…}` and `T x{…}`.
            let init = if self.eat("=") || self.is_punct("{") { Some(self.initializer()?) } else { None };
            return Ok(Stmt::Decl { ty, name, array, init });
        }
        Ok(Stmt::Expr(self.expr()?))
    }

    fn initializer(&mut self) -> Result<Expr, String> {
        if self.eat("{") {
            let mut items = Vec::new();
            while !self.eat("}") {
                items.push(self.initializer()?);
                self.eat(",");
            }
            return Ok(Expr::InitList(items));
        }
        self.assign()
    }

    pub fn expr(&mut self) -> Result<Expr, String> {
        let mut e = self.assign()?;
        while self.eat(",") {
            // The comma operator: keep the last value (side effects first are rare in generated code).
            e = self.assign()?;
        }
        Ok(e)
    }

    fn assign(&mut self) -> Result<Expr, String> {
        let lhs = self.ternary()?;
        for op in ["=", "+=", "-=", "*=", "/=", "%=", "&=", "|=", "^=", "<<=", ">>="] {
            if self.eat(op) {
                let rhs = self.assign()?;
                let op: &'static str = match op {
                    "=" => "=",
                    "+=" => "+=",
                    "-=" => "-=",
                    "*=" => "*=",
                    "/=" => "/=",
                    "%=" => "%=",
                    "&=" => "&=",
                    "|=" => "|=",
                    "^=" => "^=",
                    "<<=" => "<<=",
                    _ => ">>=",
                };
                return Ok(Expr::Assign(op, Box::new(lhs), Box::new(rhs)));
            }
        }
        Ok(lhs)
    }

    fn ternary(&mut self) -> Result<Expr, String> {
        let c = self.binary(0)?;
        if self.eat("?") {
            let a = self.assign()?;
            self.expect(":")?;
            let b = self.assign()?;
            return Ok(Expr::Ternary(Box::new(c), Box::new(a), Box::new(b)));
        }
        Ok(c)
    }

    fn binary(&mut self, min: u8) -> Result<Expr, String> {
        const LEVELS: &[&[&str]] = &[&["||"], &["&&"], &["|"], &["^"], &["&"], &["==", "!="], &["<", ">", "<=", ">="], &["<<", ">>"], &["+", "-"], &["*", "/", "%"]];
        if min as usize >= LEVELS.len() {
            return self.unary();
        }
        let mut lhs = self.binary(min + 1)?;
        'outer: loop {
            for &op in LEVELS[min as usize] {
                if self.is_punct(op) {
                    self.next();
                    let rhs = self.binary(min + 1)?;
                    lhs = Expr::Binary(op, Box::new(lhs), Box::new(rhs));
                    continue 'outer;
                }
            }
            return Ok(lhs);
        }
    }

    fn unary(&mut self) -> Result<Expr, String> {
        for op in ["-", "+", "!", "~", "&", "*", "++", "--"] {
            if self.is_punct(op) {
                self.next();
                let e = self.unary()?;
                let op: &'static str = match op {
                    "-" => "-",
                    "+" => "+",
                    "!" => "!",
                    "~" => "~",
                    "&" => "&",
                    "*" => "*",
                    "++" => "++",
                    _ => "--",
                };
                return Ok(Expr::Unary(op, Box::new(e)));
            }
        }
        // A cast: `(type) expr`.
        if self.is_punct("(")
            && let Tok::Ident(name) = self.peek_at(1).clone()
            && self.is_type_name(&name)
            && matches!(self.peek_at(2), Tok::Punct(")"))
        {
            self.next();
            let ty = self.type_name()?;
            self.expect(")")?;
            let e = self.unary()?;
            return Ok(Expr::Cast(ty, Box::new(e)));
        }
        self.postfix()
    }

    fn postfix(&mut self) -> Result<Expr, String> {
        let mut e = self.primary()?;
        loop {
            if self.eat("(") {
                let args = self.args()?;
                e = Expr::Call(Box::new(e), args);
            } else if self.eat(".") {
                e = Expr::Member(Box::new(e), self.ident()?);
            } else if self.eat("->") {
                e = Expr::Arrow(Box::new(e), self.ident()?);
            } else if self.eat("[") {
                let i = self.expr()?;
                self.expect("]")?;
                e = Expr::Index(Box::new(e), Box::new(i));
            } else if self.eat("++") {
                e = Expr::Postfix("++", Box::new(e));
            } else if self.eat("--") {
                e = Expr::Postfix("--", Box::new(e));
            } else {
                return Ok(e);
            }
        }
    }

    fn args(&mut self) -> Result<Vec<Expr>, String> {
        let mut args = Vec::new();
        while !self.eat(")") {
            args.push(self.initializer()?);
            if !self.eat(",") {
                self.expect(")")?;
                break;
            }
        }
        Ok(args)
    }

    fn primary(&mut self) -> Result<Expr, String> {
        match self.peek().clone() {
            Tok::Int(v, u) => {
                self.next();
                Ok(Expr::Int(v, u))
            }
            Tok::Float(v) => {
                self.next();
                Ok(Expr::Float(v))
            }
            Tok::Punct("(") => {
                self.next();
                let e = self.expr()?;
                self.expect(")")?;
                Ok(e)
            }
            Tok::Punct("{") => self.initializer(),
            Tok::Ident(name) => {
                if name == "true" || name == "false" {
                    self.next();
                    return Ok(Expr::Bool(name == "true"));
                }
                if TEMPLATES.contains(&name.as_str()) && matches!(self.peek_at(1), Tok::Punct("<")) {
                    let ty = self.type_name()?;
                    if self.eat("(") {
                        let args = self.args()?;
                        return Ok(Expr::TemplateCall(ty, args));
                    }
                    if self.is_punct("{") {
                        let Expr::InitList(items) = self.initializer()? else { unreachable!() };
                        return Ok(Expr::TemplateCall(ty, items));
                    }
                    return Err(format!("expected arguments after {name}<…>"));
                }
                self.next();
                let mut name = name;
                while self.eat("::") {
                    name = format!("{name}::{}", self.ident()?);
                }
                Ok(Expr::Ident(name))
            }
            t => Err(format!("unexpected {t:?} in an expression (token {})", self.pos)),
        }
    }
}

pub fn parse(src: &str) -> Result<Vec<Item>, String> {
    let toks = crate::lexer::lex(src)?;
    Parser::new(toks).module()
}
