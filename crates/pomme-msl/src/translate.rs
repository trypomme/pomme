//! Typed translation of parsed MSL to WGSL.
//!
//! - MSL converts implicitly (int ↔ float, scalar → vector); WGSL doesn't, so
//!   every expression is typed and conversions are written out.
//! - `half` is `f32` (no f16 requirement on the host GPU).
//! - Entry points become a wrapper (WGSL I/O structs, builtins, loading
//!   uniforms) around a plain function holding the original body.
//! - Uniform buffers are bound as raw words and loaded field by field at the
//!   offsets *Metal* gives them, so the app's bytes are read exactly as on a
//!   device (half4 is 8 bytes there, float3 is padded to 16, …).
//! - A struct holding resources (SkSL's `Globals`: pointers to uniforms,
//!   textures, samplers) is dissolved: `_globals.x` names a module-scope resource.

use crate::parser::{Expr, Field, Func, Item, Stmt, TypeArg, TypeName};
use std::collections::{BTreeMap, HashMap};
use std::fmt::Write;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Scalar {
    F,
    I,
    U,
    B,
}

#[derive(Clone, PartialEq, Debug)]
pub enum Ty {
    Void,
    Scalar(Scalar),
    Vec(Scalar, u8),
    /// Columns, rows.
    Mat(u8, u8),
    Struct(String),
    Array(Box<Ty>, u32),
    Texture,
    Sampler,
    /// A dissolved resource struct (`Globals`).
    Resources,
    /// A texture and its sampler (SkSL's `sampler2D` struct): their resource names.
    Combined(String, String),
}

impl Ty {
    fn wgsl(&self) -> String {
        match self {
            Ty::Void => "void".into(),
            Ty::Scalar(s) => scalar_name(*s).into(),
            Ty::Vec(s, n) => format!("vec{n}<{}>", scalar_name(*s)),
            Ty::Mat(c, r) => format!("mat{c}x{r}<f32>"),
            Ty::Struct(n) => n.clone(),
            Ty::Array(t, n) => format!("array<{}, {n}>", t.wgsl()),
            Ty::Texture => "texture_2d<f32>".into(),
            Ty::Sampler => "sampler".into(),
            Ty::Resources | Ty::Combined(..) => "/* resources */".into(),
        }
    }

    fn scalar(&self) -> Option<Scalar> {
        match self {
            Ty::Scalar(s) | Ty::Vec(s, _) => Some(*s),
            Ty::Mat(..) => Some(Scalar::F),
            _ => None,
        }
    }

    fn width(&self) -> Option<u8> {
        match self {
            Ty::Scalar(_) => Some(1),
            Ty::Vec(_, n) => Some(*n),
            _ => None,
        }
    }

    fn with_scalar(&self, s: Scalar) -> Ty {
        match self {
            Ty::Scalar(_) => Ty::Scalar(s),
            Ty::Vec(_, n) => Ty::Vec(s, *n),
            t => t.clone(),
        }
    }
}

fn scalar_name(s: Scalar) -> &'static str {
    match s {
        Scalar::F => "f32",
        Scalar::I => "i32",
        Scalar::U => "u32",
        Scalar::B => "bool",
    }
}

fn shaped(s: Scalar, n: u8) -> Ty {
    if n == 1 { Ty::Scalar(s) } else { Ty::Vec(s, n) }
}

/// Scalar kind and shape of a built-in type name: `half3` → (F, Vec 3).
fn builtin(name: &str) -> Option<Ty> {
    let scalar = |b: &str| -> Option<Scalar> {
        Some(match b {
            "float" | "half" | "double" => Scalar::F,
            "int" | "short" | "char" | "long" => Scalar::I,
            "uint" | "ushort" | "uchar" | "ulong" | "size_t" => Scalar::U,
            "bool" => Scalar::B,
            _ => return None,
        })
    };
    let name = name.strip_prefix("packed_").unwrap_or(name);
    if name == "void" {
        return Some(Ty::Void);
    }
    if name == "sampler" {
        return Some(Ty::Sampler);
    }
    if let Some(s) = scalar(name) {
        return Some(Ty::Scalar(s));
    }
    let base = name.trim_end_matches(|c: char| c.is_ascii_digit() || c == 'x');
    let dims = &name[base.len()..];
    let s = scalar(base)?;
    if let Some((c, r)) = dims.split_once('x') {
        return Some(Ty::Mat(c.parse().ok()?, r.parse().ok()?));
    }
    Some(Ty::Vec(s, dims.parse().ok()?))
}

// ---- Metal layout ----

/// Size and alignment of a scalar type name in Metal.
fn scalar_size(name: &str) -> u32 {
    match name {
        "half" | "short" | "ushort" => 2,
        "char" | "uchar" | "bool" => 1,
        "long" | "ulong" | "double" => 8,
        _ => 4,
    }
}

const WGSL_RESERVED: &[&str] = &[
    "alias", "bitcast", "break", "case", "const", "const_assert", "continue", "continuing", "default", "diagnostic", "discard", "else", "enable", "false", "fn", "for", "if",
    "let", "loop", "override", "requires", "return", "struct", "switch", "true", "var", "while", "NULL", "Self", "abstract", "active", "alignas", "alignof", "as", "asm",
    "async", "attribute", "auto", "await", "become", "cast", "catch", "class", "coherent", "column_major", "common", "compile", "concept", "constexpr", "crate", "debugger",
    "decltype", "delete", "demote", "do", "enum", "explicit", "export", "extends", "extern", "external", "fallthrough", "filter", "final", "finally", "friend", "from", "get",
    "goto", "highp", "impl", "implements", "import", "inline", "instanceof", "interface", "layout", "lowp", "macro", "match", "mediump", "meta", "mod", "module", "move",
    "mut", "mutable", "namespace", "new", "nil", "noexcept", "noinline", "null", "nullptr", "of", "operator", "package", "partition", "pass", "patch", "precise",
    "precision", "priv", "protected", "pub", "public", "readonly", "ref", "register", "require", "resource", "restrict", "self", "set", "shared", "sizeof", "smooth",
    "snorm", "static", "super", "target", "template", "this", "throw", "trait", "try", "type", "typedef", "typeid", "typename", "typeof", "union", "unless", "unorm",
    "unsafe", "unsized", "use", "using", "varying", "virtual", "volatile", "wgsl", "where", "with", "writeonly", "yield", "f32", "f16", "i32", "u32", "vec2", "vec3",
    "vec4", "mat2x2", "mat3x3", "mat4x4", "array", "atomic", "ptr", "sampler", "texture_2d", "in", "out", "inout", "input", "output", "texture", "sample",
];

fn ident(name: &str) -> String {
    let n = if let Some(rest) = name.strip_prefix("__") { format!("u_{rest}") } else { name.to_string() };
    if WGSL_RESERVED.contains(&n.as_str()) { format!("{n}_") } else { n }
}

// ---- reflection ----

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Stage {
    Vertex,
    Fragment,
}

/// A Metal binding (`[[buffer(n)]]` …) and where it is in the WGSL module.
#[derive(Clone, Debug, PartialEq)]
pub struct Binding {
    pub index: u32,
    pub group: u32,
    pub binding: u32,
    /// Bytes (Metal layout) the shader reads, for buffers.
    pub size: u32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct VertexInput {
    pub location: u32,
    pub scalar: Scalar,
    pub components: u8,
}

#[derive(Clone, Debug)]
pub struct EntryPoint {
    pub name: String,
    pub stage: Stage,
    pub buffers: Vec<Binding>,
    pub textures: Vec<Binding>,
    pub samplers: Vec<Binding>,
    pub inputs: Vec<VertexInput>,
    pub color_outputs: Vec<u32>,
    /// Color attachments read through framebuffer fetch (bound at FRAMEBUFFER_BASE + n).
    pub framebuffer_inputs: Vec<u32>,
}

/// Where Metal slots land: one bind group per stage (Metal binds stages
/// separately), buffers at their index, textures at 32+, samplers at 64+.
pub fn group_for(stage: Stage) -> u32 {
    match stage {
        Stage::Vertex => 0,
        Stage::Fragment => 1,
    }
}
pub const TEXTURE_BASE: u32 = 32;
pub const SAMPLER_BASE: u32 = 64;
/// Framebuffer fetch (`[[color(n)]]` inputs) reads a copy of color attachment n bound here (+n).
pub const FRAMEBUFFER_BASE: u32 = 96;

pub struct Translation {
    pub wgsl: String,
    pub entry_points: Vec<EntryPoint>,
}

// ---- the translator ----

#[derive(Clone)]
struct Var {
    ty: Ty,
    /// An inout parameter: a pointer to dereference.
    pointer: bool,
}

#[derive(Clone)]
struct FnSig {
    ret: Ty,
    /// (type, by pointer, dropped (a resource struct))
    params: Vec<(Ty, bool, bool)>,
    /// Default values of trailing parameters.
    defaults: Vec<Option<Expr>>,
}

struct Tr {
    structs: HashMap<String, Vec<Field>>,
    resource_structs: Vec<String>,
    /// Structs holding exactly a texture and a sampler (SkSL's `sampler2D`).
    combined_structs: Vec<String>,
    /// Resource-struct field → (texture, sampler) for combined samplers.
    combined_fields: HashMap<String, (String, String)>,
    /// Resource-struct field → module-scope resource name.
    resource_fields: HashMap<String, String>,
    consts: HashMap<String, Ty>,
    /// Module-scope resources (uniform values, textures, samplers).
    resources: HashMap<String, Ty>,
    funcs: HashMap<String, FnSig>,
    scopes: Vec<HashMap<String, Var>>,
    helpers: BTreeMap<String, String>,
    ret: Ty,
    out: String,
    /// Entry-point parameters waiting for the body's scope.
    pending: Vec<(String, Ty)>,
    /// Function constants given a value by the specialization.
    defined_constants: Vec<String>,
    /// Template functions: name → (parameters, returned expression).
    templates: HashMap<String, (Vec<String>, Expr)>,
}

type R<T> = Result<T, String>;

pub fn translate(items: &[Item], constants: &HashMap<u32, f64>) -> R<Translation> {
    let mut t = Tr {
        structs: HashMap::new(),
        resource_structs: Vec::new(),
        combined_structs: Vec::new(),
        combined_fields: HashMap::new(),
        resource_fields: HashMap::new(),
        consts: HashMap::new(),
        resources: HashMap::new(),
        funcs: HashMap::new(),
        scopes: Vec::new(),
        helpers: BTreeMap::new(),
        ret: Ty::Void,
        out: String::new(),
        pending: Vec::new(),
        defined_constants: Vec::new(),
        templates: HashMap::new(),
    };
    for item in items {
        if let Item::Struct { name, fields } = item
            && fields.len() == 2
            && fields[0].ty.name.starts_with("texture")
            && fields[1].ty.name == "sampler"
        {
            t.combined_structs.push(name.clone());
        }
    }
    for item in items {
        if let Item::Struct { name, fields } = item {
            let is_resources = !t.combined_structs.contains(name)
                && fields.iter().any(|f| f.ty.pointer || f.ty.name.starts_with("texture") || f.ty.name == "sampler" || t.combined_structs.contains(&f.ty.name));
            if is_resources {
                t.resource_structs.push(name.clone());
            }
            t.structs.insert(name.clone(), fields.clone());
        }
    }
    // Which resource each resource-struct field refers to: from `Globals _globals{&u, t, s}`.
    for item in items {
        if let Item::Func(f) = item {
            t.find_resource_inits(&f.body);
        }
    }
    let mut body = String::new();
    let mut entry_points = Vec::new();
    for item in items {
        match item {
            Item::Struct { name, fields } if !t.resource_structs.contains(name) && !t.combined_structs.contains(name) => {
                let s = t.struct_def(name, fields)?;
                body += &s;
            }
            Item::Struct { .. } => {}
            Item::Const { ty, name, array, init } => {
                let ty = t.ty(ty, *array)?;
                let (code, ety) = t.expr_expect(init, &ty)?;
                let code = t.convert(code, &ety, &ty, Some(init));
                t.consts.insert(name.clone(), ty.clone());
                writeln!(body, "const {}: {} = {code};", ident(name), ty.wgsl()).unwrap();
            }
            // SkSL's `sample(sampler2D, …)` overloads: calls to them are mapped directly.
            Item::Func(f) if f.params.iter().any(|p| t.combined_structs.contains(&p.ty.name)) => {}
            Item::Template { name, params, body } => {
                t.templates.insert(name.clone(), (params.clone(), body.clone()));
            }
            Item::FunctionConstant { ty, name, index } => {
                let ty = t.ty(ty, None)?;
                let value = constants.get(index).copied();
                if value.is_some() {
                    t.defined_constants.push(name.clone());
                }
                let v = value.unwrap_or(0.0);
                let lit = match ty {
                    Ty::Scalar(Scalar::B) => (v != 0.0).to_string(),
                    Ty::Scalar(Scalar::I) => format!("{}i", v as i64),
                    Ty::Scalar(Scalar::U) => format!("{}u", v as u64),
                    _ => float_lit(v),
                };
                t.consts.insert(name.clone(), ty.clone());
                writeln!(body, "const {}: {} = {lit};", ident(name), ty.wgsl()).unwrap();
            }
            Item::Func(f) if f.stage.is_none() => {
                let s = t.function(f, &f.name)?;
                body += &s;
            }
            Item::Func(f) => {
                let (s, ep) = t.entry_point(f)?;
                body += &s;
                entry_points.push(ep);
            }
        }
    }
    let mut wgsl = String::from("diagnostic(off, derivative_uniformity);\n\n");
    for h in t.helpers.values() {
        wgsl += h;
        wgsl += "\n";
    }
    wgsl += &t.out;
    wgsl += &body;
    Ok(Translation { wgsl, entry_points })
}

impl Tr {
    fn find_resource_inits(&mut self, stmts: &[Stmt]) {
        for s in stmts {
            match s {
                Stmt::Decl { ty, init: Some(Expr::InitList(items)), .. } if self.resource_structs.contains(&ty.name) => {
                    let fields = self.structs[&ty.name].clone();
                    for (f, e) in fields.iter().zip(items) {
                        let target = match e {
                            Expr::Unary("&", inner) => inner.as_ref(),
                            e => e,
                        };
                        if let Expr::Ident(n) = target {
                            self.resource_fields.insert(f.name.clone(), n.clone());
                        }
                        if let Expr::InitList(pair) = target
                            && let [Expr::Ident(tex), Expr::Ident(smp)] = pair.as_slice()
                        {
                            self.combined_fields.insert(f.name.clone(), (tex.clone(), smp.clone()));
                        }
                    }
                }
                Stmt::Block(b) => self.find_resource_inits(b),
                _ => {}
            }
        }
    }

    fn ty(&self, tn: &TypeName, array: Option<u32>) -> R<Ty> {
        let base = if tn.name == "array" {
            let (Some(TypeArg::Type(el)), Some(TypeArg::Int(n))) = (tn.args.first(), tn.args.get(1)) else {
                return Err(format!("unsupported array type {tn:?}"));
            };
            Ty::Array(Box::new(self.ty(el, None)?), *n as u32)
        } else if tn.name.starts_with("texture") {
            Ty::Texture
        } else if let Some(t) = builtin(&tn.name) {
            t
        } else if self.resource_structs.contains(&tn.name) {
            Ty::Resources
        } else if self.structs.contains_key(&tn.name) {
            Ty::Struct(tn.name.clone())
        } else {
            return Err(format!("unknown type {}", tn.name));
        };
        Ok(match array {
            Some(n) => Ty::Array(Box::new(base), n),
            None => base,
        })
    }

    fn struct_def(&self, name: &str, fields: &[Field]) -> R<String> {
        let mut s = format!("struct {} {{\n", ident(name));
        if fields.is_empty() {
            s += "    _unused: u32,\n";
        }
        for f in fields {
            writeln!(s, "    {}: {},", ident(&f.name), self.ty(&f.ty, f.array)?.wgsl()).unwrap();
        }
        s += "}\n";
        Ok(s)
    }

    fn lookup(&self, name: &str) -> Option<Var> {
        for scope in self.scopes.iter().rev() {
            if let Some(v) = scope.get(name) {
                return Some(v.clone());
            }
        }
        if let Some(t) = self.consts.get(name) {
            return Some(Var { ty: t.clone(), pointer: false });
        }
        self.resources.get(name).map(|t| Var { ty: t.clone(), pointer: false })
    }

    fn declare(&mut self, name: &str, ty: Ty, pointer: bool) {
        self.scopes.last_mut().unwrap().insert(name.to_string(), Var { ty, pointer });
    }

    // ---- functions ----

    /// Parameters as (name, type, by pointer, dropped).
    fn params(&self, f: &Func) -> R<Vec<(String, Ty, bool, bool)>> {
        f.params
            .iter()
            .map(|p| {
                let ty = self.ty(&p.ty, p.array)?;
                let dropped = ty == Ty::Resources;
                let pointer = (p.ty.reference || p.ty.pointer) && p.ty.space.as_deref() != Some("constant") && !p.ty.is_const && !dropped;
                Ok((p.name.clone(), ty, pointer, dropped))
            })
            .collect()
    }

    fn function(&mut self, f: &Func, name: &str) -> R<String> {
        let ret = self.ty(&f.ret, None)?;
        let params = self.params(f)?;
        let sig = FnSig { ret: ret.clone(), params: params.iter().map(|(_, t, p, d)| (t.clone(), *p, *d)).collect(), defaults: f.params.iter().map(|p| p.default.clone()).collect() };
        self.funcs.insert(name.to_string(), sig);
        self.scopes.push(HashMap::new());
        self.ret = ret.clone();
        let mut sig = Vec::new();
        let mut copies = String::new();
        for (n, ty, pointer, dropped) in &params {
            if *dropped {
                continue;
            }
            if *pointer {
                sig.push(format!("{}: ptr<function, {}>", ident(n), ty.wgsl()));
            } else {
                // WGSL parameters are immutable; MSL code assigns to them, so copy.
                sig.push(format!("_p_{}: {}", ident(n), ty.wgsl()));
                writeln!(copies, "    var {} = _p_{};", ident(n), ident(n)).unwrap();
            }
            self.declare(n, ty.clone(), *pointer);
        }
        let mut s = format!("fn {}({})", ident(name), sig.join(", "));
        if ret != Ty::Void {
            write!(s, " -> {}", ret.wgsl()).unwrap();
        }
        s += " {\n";
        s += &copies;
        for st in &f.body {
            s += &self.stmt(st, 1)?;
        }
        s += "}\n\n";
        self.scopes.pop();
        Ok(s)
    }

    fn entry_point(&mut self, f: &Func) -> R<(String, EntryPoint)> {
        let stage = match f.stage.as_deref() {
            Some("vertex") => Stage::Vertex,
            Some("fragment") => Stage::Fragment,
            other => return Err(format!("unsupported entry point stage {other:?}")),
        };
        let group = group_for(stage);
        let mut ep = EntryPoint { name: f.name.clone(), stage, buffers: Vec::new(), textures: Vec::new(), samplers: Vec::new(), inputs: Vec::new(), color_outputs: Vec::new(), framebuffer_inputs: Vec::new() };
        let mut wrapper_params = Vec::new();
        let mut body_params = Vec::new();
        let mut body_args = Vec::new();
        let mut prologue = String::new();
        let mut module = String::new();
        for p in &f.params {
            let attr = p.attrs.first().map(|a| (a.name.as_str(), a.args.first().and_then(|x| x.parse::<u32>().ok())));
            match attr {
                Some(("stage_in", _)) => {
                    let sname = p.ty.name.clone();
                    let fields = self.structs.get(&sname).cloned().ok_or(format!("unknown stage_in struct {sname}"))?;
                    let mut io = format!("struct {}_io {{\n", ident(&sname));
                    let mut ctor = Vec::new();
                    for fl in &fields {
                        let ty = self.ty(&fl.ty, fl.array)?;
                        let loc = io_location(fl, stage, true)?;
                        let interp = if matches!(ty.scalar(), Some(Scalar::I | Scalar::U)) || fl.attrs.iter().any(|a| a.name == "flat") { " @interpolate(flat)" } else { "" };
                        writeln!(io, "    @location({loc}){interp} {}: {},", ident(&fl.name), ty.wgsl()).unwrap();
                        if stage == Stage::Vertex {
                            ep.inputs.push(VertexInput { location: loc, scalar: ty.scalar().unwrap_or(Scalar::F), components: ty.width().unwrap_or(1) });
                        }
                        ctor.push(format!("_io.{}", ident(&fl.name)));
                    }
                    io += "}\n";
                    if fields.is_empty() {
                        ctor.push("0u".into());
                    } else {
                        module += &io;
                        wrapper_params.push(format!("_io: {}_io", ident(&sname)));
                    }
                    body_params.push(format!("_p_{}: {}", ident(&p.name), ident(&sname)));
                    body_args.push(format!("{}({})", ident(&sname), ctor.join(", ")));
                    self.pending_param(&p.name, Ty::Struct(sname));
                }
                Some(("buffer", Some(index))) => {
                    let ty = self.ty(&p.ty, None)?;
                    let size = self.metal_size(&p.ty, p.array)?.0;
                    let words = size.div_ceil(16).max(1);
                    let name = ident(&p.name);
                    writeln!(module, "@group({group}) @binding({index}) var<uniform> _buf{index}: array<vec4<u32>, {words}>;").unwrap();
                    writeln!(module, "var<private> {name}: {};", ty.wgsl()).unwrap();
                    let loader = self.load(&p.ty, p.array, 0, index)?;
                    writeln!(module, "fn _load_{name}() -> {} {{\n    return {loader};\n}}", ty.wgsl()).unwrap();
                    writeln!(prologue, "    {name} = _load_{name}();").unwrap();
                    self.word_helper(index);
                    self.resources.insert(p.name.clone(), ty);
                    ep.buffers.push(Binding { index, group, binding: index, size });
                }
                Some(("texture", Some(index))) => {
                    writeln!(module, "@group({group}) @binding({}) var {}: texture_2d<f32>;", TEXTURE_BASE + index, ident(&p.name)).unwrap();
                    self.resources.insert(p.name.clone(), Ty::Texture);
                    ep.textures.push(Binding { index, group, binding: TEXTURE_BASE + index, size: 0 });
                }
                Some(("sampler", Some(index))) => {
                    writeln!(module, "@group({group}) @binding({}) var {}: sampler;", SAMPLER_BASE + index, ident(&p.name)).unwrap();
                    self.resources.insert(p.name.clone(), Ty::Sampler);
                    ep.samplers.push(Binding { index, group, binding: SAMPLER_BASE + index, size: 0 });
                }
                // Framebuffer fetch: the attachment's current pixel, from a copy at this fragment.
                Some(("color", Some(n))) if stage == Stage::Fragment => {
                    let declared = self.ty(&p.ty, None)?;
                    let binding = FRAMEBUFFER_BASE + n;
                    writeln!(module, "@group({group}) @binding({binding}) var _framebuffer{n}: texture_2d<f32>;").unwrap();
                    if !wrapper_params.iter().any(|w| w.contains("@builtin(position)")) {
                        wrapper_params.push("@builtin(position) _fb_position: vec4<f32>".into());
                    }
                    let pos = wrapper_params.iter().find(|w| w.contains("@builtin(position)")).and_then(|w| w.split_whitespace().nth(1)).unwrap_or("_fb_position").trim_end_matches(':').to_string();
                    let load = format!("textureLoad(_framebuffer{n}, vec2<i32>({pos}.xy), 0)");
                    body_params.push(format!("_p_{}: {}", ident(&p.name), declared.wgsl()));
                    body_args.push(self.convert(load, &Ty::Vec(Scalar::F, 4), &declared, None));
                    self.pending_param(&p.name, declared);
                    ep.framebuffer_inputs.push(n);
                }
                Some((builtin_name, _)) => {
                    let (wgsl_builtin, ty) = match builtin_name {
                        "vertex_id" => ("vertex_index", Ty::Scalar(Scalar::U)),
                        "instance_id" => ("instance_index", Ty::Scalar(Scalar::U)),
                        "position" => ("position", Ty::Vec(Scalar::F, 4)),
                        "front_facing" => ("front_facing", Ty::Scalar(Scalar::B)),
                        "sample_id" => ("sample_index", Ty::Scalar(Scalar::U)),
                        "sample_mask" => ("sample_mask", Ty::Scalar(Scalar::U)),
                        other => return Err(format!("unsupported entry point attribute [[{other}]]")),
                    };
                    let declared = self.ty(&p.ty, None)?;
                    let n = ident(&p.name);
                    wrapper_params.push(format!("@builtin({wgsl_builtin}) {n}: {}", ty.wgsl()));
                    body_params.push(format!("_p_{n}: {}", declared.wgsl()));
                    body_args.push(self.convert(n.clone(), &ty, &declared, None));
                    self.pending_param(&p.name, declared);
                }
                None => return Err(format!("entry point parameter {} has no attribute", p.name)),
            }
        }
        // The body, as a plain function returning the original output struct.
        let ret = self.ty(&f.ret, None)?;
        let body_name = format!("{}_body", f.name);
        self.ret = ret.clone();
        let mut s = format!("fn {}({}) -> {} {{\n", ident(&body_name), body_params.join(", "), ret.wgsl());
        let pending = std::mem::take(&mut self.pending);
        self.scopes.push(HashMap::new());
        for (n, ty) in pending {
            writeln!(s, "    var {} = _p_{};", ident(&n), ident(&n)).unwrap();
            self.declare(&n, ty, false);
        }
        for st in &f.body {
            s += &self.stmt(st, 1)?;
        }
        s += "}\n\n";
        self.scopes.pop();

        let stage_attr = if stage == Stage::Vertex { "@vertex" } else { "@fragment" };
        // An entry point with no outputs (a fragment that only writes depth/stencil).
        if ret == Ty::Void {
            writeln!(module, "\n{stage_attr}\nfn {}({}) {{", ident(&f.name), wrapper_params.join(", ")).unwrap();
            module += &prologue;
            writeln!(module, "    {}({});\n}}\n", ident(&body_name), body_args.join(", ")).unwrap();
            return Ok((s.replacen(" -> void {", " {", 1) + &module, ep));
        }
        // The output I/O struct.
        let Ty::Struct(rname) = &ret else { return Err("entry points must return a struct".into()) };
        let fields = self.structs[rname].clone();
        let mut io = format!("struct {}_io {{\n", ident(rname));
        let mut ctor = Vec::new();
        for fl in &fields {
            let ty = self.ty(&fl.ty, fl.array)?;
            let attr = fl.attrs.first().map(|a| a.name.as_str());
            let decor = match attr {
                Some("position") => "@builtin(position)".to_string(),
                Some("point_size") => continue,
                Some("depth") => "@builtin(frag_depth)".to_string(),
                Some("color") => {
                    let loc = fl.attrs[0].args.first().and_then(|a| a.parse::<u32>().ok()).unwrap_or(0);
                    ep.color_outputs.push(loc);
                    format!("@location({loc})")
                }
                _ => {
                    let loc = io_location(fl, stage, false)?;
                    let flat = matches!(ty.scalar(), Some(Scalar::I | Scalar::U)) || fl.attrs.iter().any(|a| a.name == "flat");
                    format!("@location({loc}){}", if flat { " @interpolate(flat)" } else { "" })
                }
            };
            writeln!(io, "    {decor} {}: {},", ident(&fl.name), ty.wgsl()).unwrap();
            ctor.push(format!("_r.{}", ident(&fl.name)));
        }
        io += "}\n";
        module += &io;

        writeln!(module, "\n{stage_attr}\nfn {}({}) -> {}_io {{", ident(&f.name), wrapper_params.join(", "), ident(rname)).unwrap();
        module += &prologue;
        writeln!(module, "    let _r = {}({});", ident(&body_name), body_args.join(", ")).unwrap();
        writeln!(module, "    return {}_io({});\n}}\n", ident(rname), ctor.join(", ")).unwrap();
        Ok((s + &module, ep))
    }

    fn pending_param(&mut self, name: &str, ty: Ty) {
        self.pending.push((name.to_string(), ty));
    }

    // ---- uniform loading at Metal offsets ----

    fn word_helper(&mut self, index: u32) {
        // IEEE half → f32 without the shader-f16 capability.
        self.helpers.entry("f16".into()).or_insert_with(|| {
            "fn _f16(h: u32) -> f32 {
    let e = (h >> 10u) & 31u;
    let m = h & 1023u;
    var v: f32;
    if (e == 0u) {
        v = f32(m) * 5.9604644775390625e-8;
    } else if (e == 31u) {
        v = select(bitcast<f32>(0x7f800000u), bitcast<f32>(0x7fc00000u), m != 0u);
    } else {
        v = bitcast<f32>(((e + 112u) << 23u) | (m << 13u));
    }
    return select(v, -v, (h & 0x8000u) != 0u);
}
"
            .into()
        });
        self.helpers
            .entry(format!("w{index:03}"))
            .or_insert_with(|| format!("fn _w{index}(i: u32) -> u32 {{\n    return _buf{index}[i >> 2u][i & 3u];\n}}\n"));
    }

    /// Size and alignment in Metal.
    fn metal_size(&self, tn: &TypeName, array: Option<u32>) -> R<(u32, u32)> {
        let (size, align) = if tn.name == "array" {
            let (Some(TypeArg::Type(el)), Some(TypeArg::Int(n))) = (tn.args.first(), tn.args.get(1)) else { return Err("bad array".into()) };
            let (s, a) = self.metal_size(el, None)?;
            (s.next_multiple_of(a) * *n as u32, a)
        } else if let Some(fields) = self.structs.get(&tn.name) {
            let (mut off, mut align) = (0u32, 1u32);
            for f in fields {
                let (s, a) = self.metal_size(&f.ty, f.array)?;
                off = off.next_multiple_of(a) + s;
                align = align.max(a);
            }
            (off.next_multiple_of(align), align)
        } else {
            let packed = tn.name.starts_with("packed_");
            let name = tn.name.trim_start_matches("packed_");
            let base = name.trim_end_matches(|c: char| c.is_ascii_digit() || c == 'x');
            let es = scalar_size(base);
            match builtin(name) {
                Some(Ty::Scalar(_)) => (es, es),
                Some(Ty::Vec(_, n)) if packed => (es * n as u32, es),
                Some(Ty::Vec(_, n)) => {
                    let s = es * if n == 3 { 4 } else { n as u32 };
                    (s, s)
                }
                Some(Ty::Mat(c, r)) => {
                    let col = es * if r == 3 { 4 } else { r as u32 };
                    (col * c as u32, col)
                }
                _ => return Err(format!("no Metal layout for {}", tn.name)),
            }
        };
        Ok(match array {
            Some(n) => (size.next_multiple_of(align) * n, align),
            None => (size, align),
        })
    }

    /// A WGSL expression reading a `tn` stored at byte `off` of buffer `buf`.
    fn load(&self, tn: &TypeName, array: Option<u32>, off: u32, buf: u32) -> R<String> {
        if let Some(n) = array {
            let (s, a) = self.metal_size(tn, None)?;
            let stride = s.next_multiple_of(a);
            let items: R<Vec<_>> = (0..n).map(|i| self.load(tn, None, off + i * stride, buf)).collect();
            return Ok(format!("{}({})", self.ty(tn, Some(n))?.wgsl(), items?.join(", ")));
        }
        if tn.name == "array" {
            let (Some(TypeArg::Type(el)), Some(TypeArg::Int(n))) = (tn.args.first(), tn.args.get(1)) else { return Err("bad array".into()) };
            let inner = TypeName { name: el.name.clone(), args: el.args.clone(), space: None, is_const: false, reference: false, pointer: false };
            let (s, a) = self.metal_size(&inner, None)?;
            let stride = s.next_multiple_of(a);
            let items: R<Vec<_>> = (0..*n as u32).map(|i| self.load(&inner, None, off + i * stride, buf)).collect();
            return Ok(format!("{}({})", self.ty(tn, None)?.wgsl(), items?.join(", ")));
        }
        if let Some(fields) = self.structs.get(&tn.name) {
            let mut o = off;
            let mut items = Vec::new();
            for f in fields {
                let (s, a) = self.metal_size(&f.ty, f.array)?;
                o = o.next_multiple_of(a);
                items.push(self.load(&f.ty, f.array, o, buf)?);
                o += s;
            }
            if items.is_empty() {
                items.push("0u".into());
            }
            return Ok(format!("{}({})", ident(&tn.name), items.join(", ")));
        }
        let packed = tn.name.starts_with("packed_");
        let name = tn.name.trim_start_matches("packed_");
        let base = name.trim_end_matches(|c: char| c.is_ascii_digit() || c == 'x');
        let es = scalar_size(base);
        let scalar_at = |o: u32| -> String {
            let w = format!("_w{buf}({}u)", o / 4);
            match base {
                "float" => format!("bitcast<f32>({w})"),
                "half" => format!("_f16(extractBits({w}, {}u, 16u))", (o % 4) * 8),
                "int" => format!("bitcast<i32>({w})"),
                "uint" => w,
                "short" => format!("extractBits(bitcast<i32>({w}), {}u, 16u)", (o % 4) * 8),
                "ushort" => format!("extractBits({w}, {}u, 16u)", (o % 4) * 8),
                "char" => format!("extractBits(bitcast<i32>({w}), {}u, 8u)", (o % 4) * 8),
                "uchar" => format!("extractBits({w}, {}u, 8u)", (o % 4) * 8),
                "bool" => format!("(extractBits({w}, {}u, 8u) != 0u)", (o % 4) * 8),
                _ => format!("bitcast<f32>({w})"),
            }
        };
        Ok(match builtin(name) {
            Some(Ty::Scalar(_)) => scalar_at(off),
            Some(t @ Ty::Vec(_, n)) => {
                let items: Vec<_> = (0..n as u32).map(|i| scalar_at(off + i * es)).collect();
                format!("{}({})", t.wgsl(), items.join(", "))
            }
            Some(t @ Ty::Mat(c, r)) => {
                let col = es * if r == 3 && !packed { 4 } else { r as u32 };
                let cols: Vec<_> = (0..c as u32)
                    .map(|ci| {
                        let items: Vec<_> = (0..r as u32).map(|ri| scalar_at(off + ci * col + ri * es)).collect();
                        format!("vec{r}<f32>({})", items.join(", "))
                    })
                    .collect();
                format!("{}({})", t.wgsl(), cols.join(", "))
            }
            _ => return Err(format!("can't load {} from a buffer", tn.name)),
        })
    }

    // ---- statements ----

    fn stmt(&mut self, s: &Stmt, depth: usize) -> R<String> {
        let pad = "    ".repeat(depth);
        Ok(match s {
            Stmt::Empty => String::new(),
            Stmt::Block(b) => {
                self.scopes.push(HashMap::new());
                let mut out = format!("{pad}{{\n");
                for st in b {
                    out += &self.stmt(st, depth + 1)?;
                }
                self.scopes.pop();
                out + &format!("{pad}}}\n")
            }
            Stmt::Decl { ty, name, array, init } => {
                let t = self.ty(ty, *array)?;
                if t == Ty::Resources {
                    self.declare(name, t, false);
                    return Ok(String::new());
                }
                let decl = match init {
                    Some(e) => {
                        let (code, et) = self.expr_expect(e, &t)?;
                        format!("{pad}var {}: {} = {};\n", ident(name), t.wgsl(), self.convert(code, &et, &t, Some(e)))
                    }
                    None => format!("{pad}var {}: {};\n", ident(name), t.wgsl()),
                };
                self.declare(name, t, false);
                decl
            }
            Stmt::Expr(e) => self.expr_stmt(e, &pad)?,
            Stmt::If(c, a, b) => {
                let cond = self.condition(c)?;
                let mut out = format!("{pad}if ({cond}) {{\n");
                out += &self.nested(a, depth + 1)?;
                out += &format!("{pad}}}");
                if let Some(b) = b {
                    out += " else {\n";
                    out += &self.nested(b, depth + 1)?;
                    out += &format!("{pad}}}");
                }
                out + "\n"
            }
            Stmt::For(init, cond, step, body) => {
                self.scopes.push(HashMap::new());
                let init = match init {
                    Some(s) => self.stmt(s, 0)?.trim().trim_end_matches(';').to_string(),
                    None => String::new(),
                };
                let cond = match cond {
                    Some(c) => self.condition(c)?,
                    None => String::new(),
                };
                let step = match step {
                    Some(e) => self.expr_stmt(e, "")?.trim().trim_end_matches(';').to_string(),
                    None => String::new(),
                };
                let mut out = format!("{pad}for ({init}; {cond}; {step}) {{\n");
                out += &self.nested(body, depth + 1)?;
                self.scopes.pop();
                out + &format!("{pad}}}\n")
            }
            Stmt::While(c, body) => {
                let cond = self.condition(c)?;
                let mut out = format!("{pad}while ({cond}) {{\n");
                out += &self.nested(body, depth + 1)?;
                out + &format!("{pad}}}\n")
            }
            Stmt::DoWhile(body, c) => {
                let mut out = format!("{pad}loop {{\n");
                out += &self.nested(body, depth + 1)?;
                let cond = self.condition(c)?;
                out += &format!("{pad}    continuing {{\n{pad}        break if !({cond});\n{pad}    }}\n");
                out + &format!("{pad}}}\n")
            }
            Stmt::Return(None) => format!("{pad}return;\n"),
            Stmt::Return(Some(e)) => {
                let ret = self.ret.clone();
                let (code, et) = self.expr_expect(e, &ret)?;
                format!("{pad}return {};\n", self.convert(code, &et, &ret, Some(e)))
            }
            Stmt::Break => format!("{pad}break;\n"),
            Stmt::Continue => format!("{pad}continue;\n"),
            Stmt::Discard => format!("{pad}discard;\n"),
        })
    }

    /// A statement used as a branch or loop body (braces come from the caller).
    fn nested(&mut self, s: &Stmt, depth: usize) -> R<String> {
        match s {
            Stmt::Block(b) => {
                self.scopes.push(HashMap::new());
                let mut out = String::new();
                for st in b {
                    out += &self.stmt(st, depth)?;
                }
                self.scopes.pop();
                Ok(out)
            }
            other => {
                self.scopes.push(HashMap::new());
                let r = self.stmt(other, depth);
                self.scopes.pop();
                r
            }
        }
    }

    fn condition(&mut self, c: &Expr) -> R<String> {
        let (code, t) = self.expr(c)?;
        Ok(match t {
            Ty::Scalar(Scalar::B) => code,
            Ty::Scalar(Scalar::F) => format!("({code} != 0.0)"),
            Ty::Scalar(_) => format!("({code} != 0)"),
            _ => code,
        })
    }

    fn expr_stmt(&mut self, e: &Expr, pad: &str) -> R<String> {
        Ok(match e {
            Expr::Cast(t, _) if t.name == "void" => String::new(),
            Expr::Assign(op, lhs, rhs) => {
                let (l, lt) = self.expr(lhs)?;
                // Multi-component swizzle targets: WGSL can't assign to them; write each component.
                if let Expr::Member(base, sw) = lhs.as_ref()
                    && sw.len() > 1
                    && is_swizzle(sw)
                {
                    let (b, _) = self.expr(base)?;
                    let (r, rt) = self.expr_expect(rhs, &lt)?;
                    let r = self.convert(r, &rt, &lt, Some(rhs));
                    let mut out = format!("{pad}{{\n{pad}    let _t = {r};\n");
                    for (i, c) in sw.chars().enumerate() {
                        let comp = ['x', 'y', 'z', 'w'][i];
                        if *op == "=" {
                            out += &format!("{pad}    {b}.{c} = _t.{comp};\n");
                        } else {
                            out += &format!("{pad}    {b}.{c} {op} _t.{comp};\n");
                        }
                    }
                    return Ok(out + &format!("{pad}}}\n"));
                }
                let (r, rt) = self.expr_expect(rhs, &lt)?;
                let r = match (*op, &lt, &rt) {
                    // vec *= mat and the like keep the right operand's type.
                    ("*=", Ty::Vec(..), Ty::Mat(..)) | ("*=", Ty::Mat(..), Ty::Mat(..)) => r,
                    ("*=" | "/=" | "+=" | "-=", Ty::Vec(s, _), Ty::Scalar(_)) => self.convert(r, &rt, &Ty::Scalar(*s), Some(rhs)),
                    ("*=" | "/=", Ty::Mat(..), Ty::Scalar(_)) => self.convert(r, &rt, &Ty::Scalar(Scalar::F), Some(rhs)),
                    ("<<=" | ">>=", _, _) => self.convert(r, &rt, &lt.with_scalar(Scalar::U), Some(rhs)),
                    _ => self.convert(r, &rt, &lt, Some(rhs)),
                };
                format!("{pad}{l} {op} {r};\n")
            }
            Expr::Postfix(op, x) | Expr::Unary(op @ ("++" | "--"), x) => {
                let (code, _) = self.expr(x)?;
                format!("{pad}{code}{op};\n")
            }
            Expr::Call(..) => {
                let (code, t) = self.expr(e)?;
                if t == Ty::Void || !self.is_user_call(e) { format!("{pad}{code};\n") } else { format!("{pad}_ = {code};\n") }
            }
            other => {
                let (code, _) = self.expr(other)?;
                format!("{pad}_ = {code};\n")
            }
        })
    }

    fn is_user_call(&self, e: &Expr) -> bool {
        matches!(e, Expr::Call(callee, _) if matches!(callee.as_ref(), Expr::Ident(n) if self.funcs.contains_key(n)))
    }

    // ---- expressions ----

    fn convert(&self, code: String, from: &Ty, to: &Ty, src: Option<&Expr>) -> String {
        if from == to {
            return code;
        }
        match (from, to) {
            (Ty::Scalar(_), Ty::Scalar(s)) => {
                if let Some(lit) = src.and_then(|e| literal(e, *s)) {
                    return lit;
                }
                if *s == Scalar::B {
                    return format!("({code} != {})", zero(from.scalar().unwrap()));
                }
                format!("{}({code})", scalar_name(*s))
            }
            (Ty::Scalar(_), Ty::Vec(s, n)) => {
                let inner = self.convert(code, from, &Ty::Scalar(*s), src);
                format!("vec{n}<{}>({inner})", scalar_name(*s))
            }
            (Ty::Vec(_, n), Ty::Vec(s, m)) if n == m => format!("vec{n}<{}>({code})", scalar_name(*s)),
            _ => code,
        }
    }

    /// Type an expression, with a hint for literals and initializer lists.
    fn expr_expect(&mut self, e: &Expr, want: &Ty) -> R<(String, Ty)> {
        if let Expr::InitList(items) = e {
            return self.construct_list(items, want);
        }
        self.expr(e)
    }

    fn construct_list(&mut self, items: &[Expr], want: &Ty) -> R<(String, Ty)> {
        let field_tys: Vec<Ty> = match want {
            Ty::Array(t, n) => vec![(**t).clone(); *n as usize],
            Ty::Struct(s) => self.structs[s].iter().map(|f| self.ty(&f.ty, f.array)).collect::<R<_>>()?,
            Ty::Vec(s, n) => vec![Ty::Scalar(*s); *n as usize],
            t => vec![t.clone(); items.len()],
        };
        let mut args = Vec::new();
        for (item, ft) in items.iter().zip(&field_tys) {
            let (c, t) = self.expr_expect(item, ft)?;
            args.push(self.convert(c, &t, ft, Some(item)));
        }
        Ok((format!("{}({})", want.wgsl(), args.join(", ")), want.clone()))
    }

    fn expr(&mut self, e: &Expr) -> R<(String, Ty)> {
        Ok(match e {
            Expr::Int(v, u) => {
                if *u {
                    (format!("{v}u"), Ty::Scalar(Scalar::U))
                } else {
                    (format!("{v}i"), Ty::Scalar(Scalar::I))
                }
            }
            Expr::Float(v) => (float_lit(*v), Ty::Scalar(Scalar::F)),
            Expr::Bool(b) => (b.to_string(), Ty::Scalar(Scalar::B)),
            Expr::Ident(n) => {
                if n == "INFINITY" || n == "HUGE_VALF" {
                    return Ok(("bitcast<f32>(0x7f800000u)".into(), Ty::Scalar(Scalar::F)));
                }
                let v = self.lookup(n).ok_or_else(|| format!("unknown name {n}"))?;
                if v.pointer { (format!("(*{})", ident(n)), v.ty) } else { (ident(n), v.ty) }
            }
            Expr::Unary(op, x) => {
                let (c, t) = self.expr(x)?;
                match *op {
                    "&" | "*" => (c, t),
                    "!" if t.scalar() != Some(Scalar::B) => (format!("!({})", self.convert(c, &t, &t.with_scalar(Scalar::B), Some(x))), t.with_scalar(Scalar::B)),
                    "+" => (c, t),
                    op => (format!("({op}{c})"), t),
                }
            }
            Expr::Postfix(op, x) => {
                let (c, t) = self.expr(x)?;
                (format!("{c}{op}"), t)
            }
            Expr::Binary(op, a, b) => self.binary(op, a, b)?,
            Expr::Ternary(c, a, b) => {
                let cond = self.condition(c)?;
                let (ac, at) = self.expr(a)?;
                let (bc, bt) = self.expr(b)?;
                let t = common(&at, &bt);
                let ac = self.convert(ac, &at, &t, Some(a));
                let bc = self.convert(bc, &bt, &t, Some(b));
                if let Ty::Mat(c, r) = at {
                    // select() takes scalars and vectors: choose each column.
                    let cols: Vec<String> = (0..c).map(|i| format!("select(({bc})[{i}], ({ac})[{i}], {cond})")).collect();
                    let _ = r;
                    return Ok((format!("{}({})", at.wgsl(), cols.join(", ")), at));
                }
                (format!("select({bc}, {ac}, {cond})"), t)
            }
            Expr::Assign(..) => return Err("assignment used as a value".into()),
            Expr::Member(base, name) => self.member(base, name)?,
            Expr::Arrow(base, name) => self.member(base, name)?,
            Expr::Index(base, i) => {
                let (b, bt) = self.expr(base)?;
                let (ic, it) = self.expr(i)?;
                let ic = if matches!(it, Ty::Scalar(Scalar::I | Scalar::U)) { ic } else { self.convert(ic, &it, &Ty::Scalar(Scalar::I), Some(i)) };
                let t = match bt {
                    Ty::Array(t, _) => *t,
                    Ty::Vec(s, _) => Ty::Scalar(s),
                    Ty::Mat(_, r) => Ty::Vec(Scalar::F, r),
                    t => return Err(format!("can't index {t:?}")),
                };
                (format!("{b}[{ic}]"), t)
            }
            Expr::Cast(tn, x) => {
                let t = self.ty(tn, None)?;
                let (c, xt) = self.expr(x)?;
                (self.convert(c, &xt, &t, Some(x)), t)
            }
            Expr::InitList(_) => return Err("an initializer list needs a type here".into()),
            Expr::TemplateCall(tn, args) => match tn.name.as_str() {
                "as_type" | "static_cast" => {
                    let Some(TypeArg::Type(target)) = tn.args.first() else { return Err("as_type needs a type".into()) };
                    let t = self.ty(target, None)?;
                    let (c, xt) = self.expr(&args[0])?;
                    if tn.name == "as_type" { (format!("bitcast<{}>({c})", t.wgsl()), t) } else { (self.convert(c, &xt, &t, Some(&args[0])), t) }
                }
                _ => {
                    let t = self.ty(tn, None)?;
                    self.construct_list(args, &t)?
                }
            },
            Expr::Call(callee, args) => self.call(callee, args)?,
        })
    }

    fn member(&mut self, base: &Expr, name: &str) -> R<(String, Ty)> {
        // A field of the dissolved resource struct names a module-scope resource.
        if let Ok((_, Ty::Resources)) = self.expr_peek(base) {
            if let Some((tex, smp)) = self.combined_fields.get(name).cloned() {
                return Ok((String::new(), Ty::Combined(tex, smp)));
            }
            let target = self.resource_fields.get(name).cloned().unwrap_or(name.to_string());
            let t = self.lookup(&target).map(|v| v.ty).ok_or_else(|| format!("unknown resource {name}"))?;
            return Ok((ident(&target), t));
        }
        let (b, bt) = self.expr(base)?;
        match &bt {
            Ty::Struct(s) => {
                let f = self.structs[s].iter().find(|f| f.name == name).cloned().ok_or_else(|| format!("{s} has no field {name}"))?;
                let t = self.ty(&f.ty, f.array)?;
                Ok((format!("{b}.{}", ident(name)), t))
            }
            Ty::Vec(s, _) | Ty::Scalar(s) if is_swizzle(name) => {
                let mapped: String = name
                    .chars()
                    .map(|c| match c {
                        'r' => 'x',
                        'g' => 'y',
                        'b' => 'z',
                        'a' => 'w',
                        c => c,
                    })
                    .collect();
                let t = shaped(*s, name.len() as u8);
                if matches!(bt, Ty::Scalar(_)) {
                    // Swizzling a scalar (`x.xx`) splats it.
                    return Ok((format!("{}({b})", t.wgsl()), t));
                }
                Ok((format!("{b}.{mapped}"), t))
            }
            t => Err(format!("no member {name} on {t:?}")),
        }
    }

    /// The type of an expression without emitting anything that matters.
    fn expr_peek(&mut self, e: &Expr) -> R<(String, Ty)> {
        match e {
            Expr::Ident(n) => self.lookup(n).map(|v| (String::new(), v.ty)).ok_or_else(|| format!("unknown name {n}")),
            _ => Err(String::new()),
        }
    }

    fn binary(&mut self, op: &str, a: &Expr, b: &Expr) -> R<(String, Ty)> {
        let (ac, at) = self.expr(a)?;
        let (bc, bt) = self.expr(b)?;
        match op {
            "&&" | "||" => {
                let b_ = Ty::Scalar(Scalar::B);
                let ac = self.convert(ac, &at, &b_, Some(a));
                let bc = self.convert(bc, &bt, &b_, Some(b));
                Ok((format!("({ac} {op} {bc})"), b_))
            }
            "<<" | ">>" => {
                let bc = self.convert(bc, &bt, &bt.with_scalar(Scalar::U), Some(b));
                Ok((format!("({ac} {op} {bc})"), at))
            }
            "*" | "/" | "+" | "-" | "%" if matches!(at, Ty::Mat(..)) || matches!(bt, Ty::Mat(..)) => {
                let t = match (&at, &bt) {
                    (Ty::Mat(_, r), Ty::Vec(..)) if op == "*" => Ty::Vec(Scalar::F, *r),
                    (Ty::Vec(..), Ty::Mat(c, _)) if op == "*" => Ty::Vec(Scalar::F, *c),
                    (Ty::Mat(_, r), Ty::Mat(c2, _)) if op == "*" => Ty::Mat(*c2, *r),
                    (Ty::Mat(..), _) => at.clone(),
                    _ => bt.clone(),
                };
                let ac = if matches!(at, Ty::Scalar(_)) { self.convert(ac, &at, &Ty::Scalar(Scalar::F), Some(a)) } else { ac };
                let bc = if matches!(bt, Ty::Scalar(_)) { self.convert(bc, &bt, &Ty::Scalar(Scalar::F), Some(b)) } else { bc };
                Ok((format!("({ac} {op} {bc})"), t))
            }
            _ => {
                let s = common_scalar(at.scalar().unwrap_or(Scalar::F), bt.scalar().unwrap_or(Scalar::F));
                let comparison = matches!(op, "==" | "!=" | "<" | ">" | "<=" | ">=");
                let n = at.width().unwrap_or(1).max(bt.width().unwrap_or(1));
                // Arithmetic mixes vectors and scalars; comparisons and bitwise ops need matching shapes.
                let same_shape = comparison || matches!(op, "&" | "|" | "^");
                let ta = if same_shape { shaped(s, n) } else { at.with_scalar(s) };
                let tb = if same_shape { shaped(s, n) } else { bt.with_scalar(s) };
                let ac = self.convert(ac, &at, &ta, Some(a));
                let bc = self.convert(bc, &bt, &tb, Some(b));
                let t = if comparison { shaped(Scalar::B, n) } else { shaped(s, n) };
                Ok((format!("({ac} {op} {bc})"), t))
            }
        }
    }

    fn call(&mut self, callee: &Expr, args: &[Expr]) -> R<(String, Ty)> {
        // Methods on textures.
        if let Expr::Member(obj, method) = callee {
            let (o, ot) = self.expr(obj)?;
            if ot == Ty::Texture {
                return self.texture_method(&o, method, args);
            }
            return Err(format!("unsupported method {method} on {ot:?}"));
        }
        let Expr::Ident(name) = callee else { return Err("unsupported callee".into()) };
        if matches!(name.as_str(), "sample" | "sampleLod" | "sampleGrad")
            && let Some(first) = args.first()
            && let (_, Ty::Combined(tex, smp)) = self.expr(first)?
        {
            return self.combined_sample(name, &ident(&tex), &ident(&smp), &args[1..]);
        }
        // Constructors of built-in and user types.
        if let Some(t) = builtin(name) {
            return self.constructor(&t, args);
        }
        if self.structs.contains_key(name) && !self.resource_structs.contains(name) {
            return self.construct_list(args, &Ty::Struct(name.clone()));
        }
        // Templates are inlined: the body with the arguments substituted (parenthesized).
        if let Some((params, body)) = self.templates.get(name).cloned() {
            let map: HashMap<String, Expr> = params.into_iter().zip(args.iter().cloned()).collect();
            return self.expr(&substitute(&body, &map));
        }
        if let Some(sig) = self.funcs.get(name).cloned() {
            let mut out = Vec::new();
            // Omitted trailing arguments take the parameters' defaults.
            let filled: Vec<Expr> = (0..sig.params.len()).filter_map(|i| args.get(i).cloned().or_else(|| sig.defaults[i].clone())).collect();
            for (a, (pt, pointer, dropped)) in filled.iter().zip(&sig.params) {
                if *dropped {
                    continue;
                }
                if *pointer {
                    let (c, _) = self.expr(a)?;
                    let c = c.strip_prefix("(*").and_then(|c| c.strip_suffix(')')).map(str::to_string).unwrap_or_else(|| format!("&{c}"));
                    out.push(c);
                } else {
                    let (c, t) = self.expr_expect(a, pt)?;
                    out.push(self.convert(c, &t, pt, Some(a)));
                }
            }
            return Ok((format!("{}({})", ident(name), out.join(", ")), sig.ret));
        }
        self.builtin_call(name, args)
    }

    fn constructor(&mut self, t: &Ty, args: &[Expr]) -> R<(String, Ty)> {
        let typed: Vec<(String, Ty)> = args.iter().map(|a| self.expr(a)).collect::<R<_>>()?;
        if let [(c, at)] = typed.as_slice()
            && matches!(t, Ty::Scalar(_))
        {
            return Ok((self.convert(c.clone(), at, t, Some(&args[0])), t.clone()));
        }
        if let Ty::Mat(c, r) = t {
            if let [(code, Ty::Mat(c2, r2))] = typed.as_slice() {
                if (c2, r2) == (c, r) {
                    return Ok((code.clone(), t.clone()));
                }
                // Resizing a matrix: take the overlapping part, identity elsewhere.
                let cols: Vec<String> = (0..*c)
                    .map(|ci| {
                        let comps: Vec<String> = (0..*r)
                            .map(|ri| if ci < *c2 && ri < *r2 { format!("{code}[{ci}][{ri}]") } else if ci == ri { "1.0".into() } else { "0.0".into() })
                            .collect();
                        format!("vec{r}<f32>({})", comps.join(", "))
                    })
                    .collect();
                return Ok((format!("{}({})", t.wgsl(), cols.join(", ")), t.clone()));
            }
            if let [(code, Ty::Scalar(_))] = typed.as_slice() {
                // A scalar fills the diagonal.
                let cols: Vec<String> = (0..*c)
                    .map(|ci| {
                        let comps: Vec<String> = (0..*r).map(|ri| if ci == ri { format!("f32({code})") } else { "0.0".into() }).collect();
                        format!("vec{r}<f32>({})", comps.join(", "))
                    })
                    .collect();
                return Ok((format!("{}({})", t.wgsl(), cols.join(", ")), t.clone()));
            }
        }
        let s = t.scalar().unwrap_or(Scalar::F);
        let items: Vec<String> = typed.iter().zip(args).map(|((c, at), a)| self.convert(c.clone(), at, &at.with_scalar(s), Some(a))).collect();
        Ok((format!("{}({})", t.wgsl(), items.join(", ")), t.clone()))
    }

    fn texture_method(&mut self, tex: &str, method: &str, args: &[Expr]) -> R<(String, Ty)> {
        let v4 = Ty::Vec(Scalar::F, 4);
        match method {
            "sample" => {
                let (s, _) = self.expr(&args[0])?;
                let (c, ct) = self.expr(&args[1])?;
                let c = self.convert(c, &ct, &Ty::Vec(Scalar::F, 2), Some(&args[1]));
                if let Some(Expr::Call(opt, oargs)) = args.get(2)
                    && let Expr::Ident(kind) = opt.as_ref()
                {
                    let (v, vt) = self.expr(&oargs[0])?;
                    let v = self.convert(v, &vt, &Ty::Scalar(Scalar::F), Some(&oargs[0]));
                    return Ok(match kind.as_str() {
                        "bias" => (format!("textureSampleBias({tex}, {s}, {c}, {v})"), v4),
                        "level" => (format!("textureSampleLevel({tex}, {s}, {c}, {v})"), v4),
                        k => return Err(format!("unsupported sample option {k}")),
                    });
                }
                Ok((format!("textureSample({tex}, {s}, {c})"), v4))
            }
            "read" => {
                let (c, ct) = self.expr(&args[0])?;
                let c = self.convert(c, &ct, &Ty::Vec(Scalar::I, 2), Some(&args[0]));
                Ok((format!("textureLoad({tex}, {c}, 0)"), v4))
            }
            "get_width" => Ok((format!("textureDimensions({tex}).x"), Ty::Scalar(Scalar::U))),
            "get_height" => Ok((format!("textureDimensions({tex}).y"), Ty::Scalar(Scalar::U))),
            m => Err(format!("unsupported texture method {m}")),
        }
    }

    /// SkSL's `sample(sampler2D, p[, bias])`, `sampleLod(…, lod)`, `sampleGrad(…, dx, dy)`;
    /// a float3 coordinate is projective. Skia's default bias is -0.475.
    fn combined_sample(&mut self, name: &str, tex: &str, smp: &str, args: &[Expr]) -> R<(String, Ty)> {
        let (p, pt) = self.expr(&args[0])?;
        let p = match pt {
            Ty::Vec(_, 3) => format!("({p}.xy / {p}.z)"),
            t => self.convert(p, &t, &Ty::Vec(Scalar::F, 2), Some(&args[0])),
        };
        let mut rest = Vec::new();
        for a in &args[1..] {
            let (c, t) = self.expr(a)?;
            let want = if matches!(t, Ty::Vec(..)) { Ty::Vec(Scalar::F, 2) } else { Ty::Scalar(Scalar::F) };
            rest.push(self.convert(c, &t, &want, Some(a)));
        }
        let v4 = Ty::Vec(Scalar::F, 4);
        Ok(match name {
            "sample" => {
                let bias = rest.first().cloned().unwrap_or("-0.475".into());
                (format!("textureSampleBias({tex}, {smp}, {p}, {bias})"), v4)
            }
            "sampleLod" => (format!("textureSampleLevel({tex}, {smp}, {p}, {})", rest[0]), v4),
            _ => (format!("textureSampleGrad({tex}, {smp}, {p}, {}, {})", rest[0], rest[1]), v4),
        })
    }

    fn helper(&mut self, key: &str, code: impl FnOnce() -> String) {
        self.helpers.entry(key.to_string()).or_insert_with(code);
    }

    fn builtin_call(&mut self, name: &str, args: &[Expr]) -> R<(String, Ty)> {
        // `fast::` / `precise::` variants are the same functions here; powr is pow for x >= 0.
        let name = name.rsplit("::").next().unwrap_or(name);
        let name = if name == "powr" { "pow" } else { name };
        let typed: Vec<(String, Ty)> = args.iter().map(|a| self.expr(a)).collect::<R<_>>()?;
        let tys: Vec<Ty> = typed.iter().map(|(_, t)| t.clone()).collect();
        let n = tys.iter().filter_map(Ty::width).max().unwrap_or(1);
        let float_t = shaped(Scalar::F, n);
        // All arguments as type `t` (scalars splatted to vectors).
        let all_as = |me: &Self, t: &Ty| -> Vec<String> { typed.iter().zip(args).map(|((c, at), a)| me.convert(c.clone(), at, t, Some(a))).collect() };
        let num_t = {
            let s = tys.iter().filter_map(Ty::scalar).fold(None, |acc: Option<Scalar>, s| Some(acc.map_or(s, |a| common_scalar(a, s)))).unwrap_or(Scalar::F);
            shaped(s, n)
        };
        Ok(match name {
            // Component-wise, every argument the same float type.
            "fma" | "step" | "smoothstep" | "pow" | "atan2" | "fmin" | "fmax" => {
                let wname = match name {
                    "fmin" => "min",
                    "fmax" => "max",
                    n => n,
                };
                (format!("{wname}({})", all_as(self, &float_t).join(", ")), float_t)
            }
            "min" | "max" | "clamp" => (format!("{name}({})", all_as(self, &num_t).join(", ")), num_t),
            "mix" => {
                // mix(a, b, t): t may stay a scalar.
                let ab = shaped(Scalar::F, tys[0].width().unwrap_or(1).max(tys[1].width().unwrap_or(1)));
                let a = self.convert(typed[0].0.clone(), &tys[0], &ab, Some(&args[0]));
                let b = self.convert(typed[1].0.clone(), &tys[1], &ab, Some(&args[1]));
                let tt = if tys[2].width() == Some(1) { Ty::Scalar(Scalar::F) } else { ab.clone() };
                let t = self.convert(typed[2].0.clone(), &tys[2], &tt, Some(&args[2]));
                (format!("mix({a}, {b}, {t})"), ab)
            }
            "abs" | "sign" => (format!("{name}({})", typed[0].0), tys[0].clone()),
            "floor" | "ceil" | "fract" | "trunc" | "round" | "sqrt" | "exp" | "exp2" | "log" | "log2" | "sin" | "cos" | "tan" | "asin" | "acos" | "atan" | "sinh" | "cosh"
            | "tanh" | "normalize" | "saturate" | "degrees" | "radians" | "rsqrt" | "dfdx" | "dfdy" | "fwidth" | "precise::sqrt" | "fast::sqrt" => {
                let w = match name {
                    "rsqrt" => "inverseSqrt",
                    "dfdx" => "dpdx",
                    "dfdy" => "dpdy",
                    "precise::sqrt" | "fast::sqrt" => "sqrt",
                    n => n,
                };
                let t = tys[0].with_scalar(Scalar::F);
                let a = self.convert(typed[0].0.clone(), &tys[0], &t, Some(&args[0]));
                (format!("{w}({a})"), t)
            }
            "length" => {
                let t = tys[0].with_scalar(Scalar::F);
                let a = self.convert(typed[0].0.clone(), &tys[0], &t, Some(&args[0]));
                (format!("length({a})"), Ty::Scalar(Scalar::F))
            }
            "dot" | "distance" => (format!("{name}({})", all_as(self, &float_t).join(", ")), Ty::Scalar(Scalar::F)),
            "cross" | "reflect" | "faceforward" => (format!("{name}({})", all_as(self, &float_t).join(", ")), float_t),
            "fmod" => {
                let v = all_as(self, &float_t);
                (format!("({0} - {1} * trunc({0} / {1}))", v[0], v[1]), float_t)
            }
            "isinf" | "isnan" => {
                let t = tys[0].with_scalar(Scalar::F);
                let n = t.width().unwrap_or(1);
                let key = format!("{name}{n}");
                let fname = format!("_{name}{n}");
                let ut = shaped(Scalar::U, n);
                let bt = shaped(Scalar::B, n);
                let test = if name == "isinf" { format!("(bits & {0}(0x7fffffffu)) == {0}(0x7f800000u)", ut.wgsl()) } else { format!("(bits & {0}(0x7fffffffu)) > {0}(0x7f800000u)", ut.wgsl()) };
                let (tw, uw, bw) = (t.wgsl(), ut.wgsl(), bt.wgsl());
                self.helper(&key, || format!("fn {fname}(x: {tw}) -> {bw} {{\n    let bits = bitcast<{uw}>(x);\n    return {test};\n}}\n"));
                let a = self.convert(typed[0].0.clone(), &tys[0], &t, Some(&args[0]));
                (format!("{fname}({a})"), bt)
            }
            "any" | "all" => (format!("{name}({})", typed[0].0), Ty::Scalar(Scalar::B)),
            "is_function_constant_defined" => {
                let defined = matches!(&args[0], Expr::Ident(n) if self.defined_constants.contains(n));
                (defined.to_string(), Ty::Scalar(Scalar::B))
            }
            "not" => (format!("!({})", typed[0].0), tys[0].clone()),
            "select" => {
                let t = common(&tys[0], &tys[1]);
                let a = self.convert(typed[0].0.clone(), &tys[0], &t, Some(&args[0]));
                let b = self.convert(typed[1].0.clone(), &tys[1], &t, Some(&args[1]));
                (format!("select({a}, {b}, {})", typed[2].0), t)
            }
            "transpose" => {
                let t = match &tys[0] {
                    Ty::Mat(c, r) => Ty::Mat(*r, *c),
                    t => t.clone(),
                };
                (format!("transpose({})", typed[0].0), t)
            }
            "determinant" => (format!("determinant({})", typed[0].0), Ty::Scalar(Scalar::F)),
            "sample" => {
                // `sample(texture, sampler, coords)` (a free-function form some generators use).
                let (t, s) = (typed[0].0.clone(), typed[1].0.clone());
                let c = self.convert(typed[2].0.clone(), &tys[2], &Ty::Vec(Scalar::F, 2), Some(&args[2]));
                (format!("textureSample({t}, {s}, {c})"), Ty::Vec(Scalar::F, 4))
            }
            other => return Err(format!("unsupported function {other}")),
        })
    }
}

/// `e` with identifiers replaced by expressions (template inlining).
fn substitute(e: &Expr, map: &HashMap<String, Expr>) -> Expr {
    let s = |x: &Expr| Box::new(substitute(x, map));
    match e {
        Expr::Ident(n) => map.get(n).cloned().unwrap_or_else(|| e.clone()),
        Expr::Unary(op, x) => Expr::Unary(op, s(x)),
        Expr::Postfix(op, x) => Expr::Postfix(op, s(x)),
        Expr::Binary(op, a, b) => Expr::Binary(op, s(a), s(b)),
        Expr::Assign(op, a, b) => Expr::Assign(op, s(a), s(b)),
        Expr::Ternary(c, a, b) => Expr::Ternary(s(c), s(a), s(b)),
        Expr::Call(c, args) => Expr::Call(s(c), args.iter().map(|a| substitute(a, map)).collect()),
        Expr::TemplateCall(t, args) => Expr::TemplateCall(t.clone(), args.iter().map(|a| substitute(a, map)).collect()),
        Expr::Member(b, f) => Expr::Member(s(b), f.clone()),
        Expr::Arrow(b, f) => Expr::Arrow(s(b), f.clone()),
        Expr::Index(b, i) => Expr::Index(s(b), s(i)),
        Expr::Cast(t, x) => Expr::Cast(t.clone(), s(x)),
        Expr::InitList(items) => Expr::InitList(items.iter().map(|a| substitute(a, map)).collect()),
        other => other.clone(),
    }
}

fn is_swizzle(s: &str) -> bool {
    !s.is_empty() && s.len() <= 4 && (s.chars().all(|c| "xyzw".contains(c)) || s.chars().all(|c| "rgba".contains(c)))
}

fn common_scalar(a: Scalar, b: Scalar) -> Scalar {
    use Scalar::*;
    match (a, b) {
        (F, _) | (_, F) => F,
        (U, _) | (_, U) => U,
        (I, _) | (_, I) => I,
        _ => B,
    }
}

fn common(a: &Ty, b: &Ty) -> Ty {
    match (a.scalar(), b.scalar(), a.width(), b.width()) {
        (Some(x), Some(y), Some(n), Some(m)) => shaped(common_scalar(x, y), n.max(m)),
        _ => a.clone(),
    }
}

fn zero(s: Scalar) -> &'static str {
    match s {
        Scalar::F => "0.0",
        Scalar::I => "0i",
        Scalar::U => "0u",
        Scalar::B => "false",
    }
}

fn float_lit(v: f64) -> String {
    let f = v as f32;
    if f.is_infinite() {
        return if f > 0.0 { "bitcast<f32>(0x7f800000u)".into() } else { "bitcast<f32>(0xff800000u)".into() };
    }
    let s = format!("{f:?}");
    if s.contains('.') || s.contains('e') { s } else { format!("{s}.0") }
}

/// A literal written directly in another scalar type (`2` as `2.0`).
fn literal(e: &Expr, to: Scalar) -> Option<String> {
    let neg = |s: String| format!("-{s}");
    match e {
        Expr::Int(v, _) => Some(match to {
            Scalar::F => float_lit(*v as f64),
            Scalar::I => format!("{v}i"),
            Scalar::U => format!("{v}u"),
            Scalar::B => (*v != 0).to_string(),
        }),
        Expr::Float(v) => match to {
            Scalar::F => Some(float_lit(*v)),
            _ => None,
        },
        Expr::Unary("-", inner) => literal(inner, to).filter(|_| to != Scalar::U && to != Scalar::B).map(neg),
        _ => None,
    }
}

/// `[[attribute(n)]]` / `[[user(locnN)]]` → a WGSL location.
fn io_location(f: &Field, stage: Stage, input: bool) -> R<u32> {
    for a in &f.attrs {
        match a.name.as_str() {
            "attribute" if stage == Stage::Vertex && input => return a.args.first().and_then(|x| x.parse().ok()).ok_or("bad attribute index".into()),
            "user" => {
                let arg = a.args.first().ok_or("user() needs a name")?;
                let digits: String = arg.chars().rev().take_while(char::is_ascii_digit).collect::<Vec<_>>().into_iter().rev().collect();
                return digits.parse().map_err(|_| format!("can't number user({arg})"));
            }
            _ => {}
        }
    }
    Err(format!("no location for {}", f.name))
}
