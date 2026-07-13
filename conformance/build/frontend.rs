use std::collections::{BTreeMap, BTreeSet};
use syn::parse::{Parse, ParseStream};
use syn::{Attribute, Expr, Fields, File, GenericParam, Item, Lit, Meta, Token, Type as SynType};

fn die(message: impl std::fmt::Display) -> ! {
    panic!("zero-schema conformance frontend: {message}")
}
fn name(i: &syn::Ident) -> String {
    i.to_string().trim_start_matches("r#").to_owned()
}
fn uint(l: &syn::LitInt) -> u64 {
    if !l.suffix().is_empty() {
        die("integer literals must be unsuffixed")
    };
    l.base10_parse()
        .unwrap_or_else(|_| die("integer literal exceeds u64"))
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Endian {
    Native,
    Little,
    Big,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum IntWidth {
    U8,
    U16,
    U32,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Primitive {
    U8,
    I8,
    U16,
    I16,
    U32,
    I32,
    U64,
    I64,
    F32,
    F64,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StringKind {
    Utf8,
    CBytes,
    U16,
    U16C,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Type {
    Primitive(Primitive),
    Bool,
    String(StringKind),
    FixedBytes(u32),
    Schema(String),
}
#[derive(Clone, Debug)]
pub struct Field {
    pub name: String,
    pub ty: Type,
    pub endian: Endian,
    pub align: Option<u32>,
    pub capacity: Option<u32>,
    pub len: Option<(IntWidth, Endian)>,
    pub tail_zero: bool,
    pub tag_field: Option<String>,
}
#[derive(Clone, Debug)]
pub struct EnumVariant {
    pub name: String,
    pub raw: u64,
}
#[derive(Clone, Debug)]
pub struct TaggedVariant {
    pub name: String,
    pub tag_variant: String,
    pub payload: Option<String>,
}
#[derive(Clone, Debug)]
pub enum SchemaKind {
    Struct {
        fields: Vec<Field>,
    },
    ScalarEnum {
        repr: IntWidth,
        endian: Endian,
        variants: Vec<EnumVariant>,
    },
    TaggedEnum {
        tag: String,
        variants: Vec<TaggedVariant>,
    },
}
#[derive(Clone, Debug)]
pub struct Schema {
    pub name: String,
    pub kind: SchemaKind,
    pub align: Option<u32>,
    pub tail_zero: bool,
}
#[derive(Clone, Debug)]
pub struct AbiField {
    pub name: String,
    pub ty: String,
}
#[derive(Clone, Debug)]
pub struct AbiStruct {
    pub name: String,
    pub align: Option<u32>,
    pub fields: Vec<AbiField>,
}
#[derive(Clone, Debug)]
pub struct RootRegistration {
    pub root_id: String,
    pub case_id: u32,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Root {
    Schema(String),
    Abi(String),
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Value {
    Record(Vec<(String, Value)>),
    Union {
        ty: String,
        variant: String,
        fields: Vec<(String, Value)>,
    },
    Bits(u64),
    Boolean(bool),
    Variant {
        ty: String,
        variant: String,
    },
    Bytes(Vec<u8>),
    Units(Vec<u16>),
    Zst,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Metric {
    RootSize,
    RootAlign,
    RootStride,
    FieldOffset(String),
    FieldSize(String),
    FieldAlign(String),
    TagOffset,
    PayloadOffset,
    PayloadSize,
    PayloadAlign,
    TagSize,
    TagAlign,
    TagEndian,
    VariantRaw(String),
    VariantPayloadSize(String),
    VariantPayloadAlign(String),
    EnumSize(String),
    EnumAlign(String),
    EnumWidth(String),
    EnumEndian(String),
    EnumRaw(String),
    StringEncoding(String),
    StringCapacity(String),
    StringUnitWidth(String),
    StringDataOffset(String),
    StringLengthWidth(String),
    StringLengthEndian(String),
    StringLengthOffset(String),
    StringTail(String),
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Observation {
    Scalar(Vec<String>),
    Tag(Vec<String>),
    Length(Vec<String>),
    Unit(Vec<String>, u32),
}
#[derive(Clone, Debug)]
pub struct MetricEntry {
    pub key: u64,
    pub metric: Metric,
}
#[derive(Clone, Debug)]
pub struct ObservationEntry {
    pub key: u64,
    pub source: Observation,
}
#[derive(Clone, Debug)]
pub struct Case {
    pub id: u32,
    pub root_id: String,
    pub root: Root,
    pub value: Value,
    pub layout: Vec<MetricEntry>,
    pub observe: Vec<ObservationEntry>,
}
#[derive(Clone, Debug)]
pub struct Model {
    pub schemas: Vec<Schema>,
    pub abi_structs: Vec<AbiStruct>,
    pub cases: Vec<Case>,
}
impl Model {
    pub fn schema(&self, n: &str) -> &Schema {
        self.schemas
            .iter()
            .find(|x| x.name == n)
            .unwrap_or_else(|| die(format!("unknown schema {n}")))
    }
    pub fn abi(&self, n: &str) -> &AbiStruct {
        self.abi_structs
            .iter()
            .find(|x| x.name == n)
            .unwrap_or_else(|| die(format!("unknown ABI type {n}")))
    }
}

#[derive(Default)]
struct Zero {
    endian: Option<Endian>,
    align: Option<u32>,
    padding_zero: bool,
    tail_zero: bool,
    capacity: Option<u32>,
    len: Option<IntWidth>,
    tag: Option<String>,
    tag_field: Option<String>,
}
fn attrs(attrs: &[Attribute], allow_naming: bool) -> (Zero, Option<IntWidth>) {
    let mut z = Zero::default();
    let mut repr = None;
    for a in attrs {
        if a.path().is_ident("doc") {
            continue;
        }
        if a.path().is_ident("derive") {
            let Meta::List(l) = &a.meta else {
                die("derive must be list")
            };
            let ps = l
                .parse_args_with(
                    syn::punctuated::Punctuated::<syn::Path, Token![,]>::parse_terminated,
                )
                .unwrap_or_else(|_| die("invalid derive"));
            for p in ps {
                let Some(i) = p.get_ident() else {
                    die("qualified derive forbidden")
                };
                if !matches!(
                    i.to_string().as_str(),
                    "Debug" | "PartialEq" | "Eq" | "ZeroSchema"
                ) {
                    die("unsupported derive")
                }
            }
            continue;
        }
        if a.path().is_ident("allow") {
            if !allow_naming
                || !matches!(&a.meta,Meta::List(l) if l.tokens.to_string()=="non_camel_case_types")
            {
                die("unsupported allow")
            };
            continue;
        }
        if a.path().is_ident("repr") {
            let Meta::List(l) = &a.meta else {
                die("repr must be list")
            };
            let ps = l
                .parse_args_with(
                    syn::punctuated::Punctuated::<syn::Meta, Token![,]>::parse_terminated,
                )
                .unwrap_or_else(|_| die("bad repr"));
            for m in ps {
                match m {
                    Meta::Path(p) if p.is_ident("u8") => repr = Some(IntWidth::U8),
                    Meta::Path(p) if p.is_ident("u16") => repr = Some(IntWidth::U16),
                    Meta::Path(p) if p.is_ident("u32") => repr = Some(IntWidth::U32),
                    Meta::Path(p) if p.is_ident("C") => {}
                    Meta::List(x) if x.path.is_ident("align") => {
                        let l: syn::LitInt =
                            syn::parse2(x.tokens).unwrap_or_else(|_| die("align literal required"));
                        z.align = Some(power(uint(&l)))
                    }
                    _ => die("unsupported repr"),
                }
            }
            continue;
        }
        if a.path().is_ident("zero") {
            let Meta::List(l) = &a.meta else {
                die("zero must be list")
            };
            l.parse_nested_meta(|m| {
                let key = m
                    .path
                    .get_ident()
                    .map(name)
                    .ok_or_else(|| m.error("simple option required"))?;
                match key.as_str() {
                    "endian" => {
                        let s: syn::LitStr = m.value()?.parse()?;
                        z.endian = Some(match s.value().as_str() {
                            "native" => Endian::Native,
                            "little" => Endian::Little,
                            "big" => Endian::Big,
                            _ => return Err(m.error("invalid endian")),
                        })
                    }
                    "padding" | "tail" => {
                        let s: syn::LitStr = m.value()?.parse()?;
                        if s.value() != "zero" {
                            return Err(m.error("only zero policy accepted"));
                        };
                        if key == "padding" {
                            z.padding_zero = true
                        } else {
                            z.tail_zero = true
                        }
                    }
                    "capacity" => {
                        let l: syn::LitInt = m.value()?.parse()?;
                        z.capacity = Some(
                            u32::try_from(uint(&l)).map_err(|_| m.error("capacity exceeds u32"))?,
                        )
                    }
                    "align" => {
                        let l: syn::LitInt = m.value()?.parse()?;
                        z.align = Some(power(uint(&l)))
                    }
                    "len_type" => {
                        let p: syn::Path = m.value()?.parse()?;
                        z.len = Some(width_path(&p))
                    }
                    "tag" => {
                        let p: syn::Path = m.value()?.parse()?;
                        z.tag = Some(simple_path(&p).join("::"))
                    }
                    "tag_field" => {
                        let i: syn::Ident = m.value()?.parse()?;
                        z.tag_field = Some(name(&i))
                    }
                    _ => return Err(m.error("unsupported zero option")),
                };
                Ok(())
            })
            .unwrap_or_else(|e| die(e));
            continue;
        }
        die("unsupported attribute")
    }
    (z, repr)
}
fn power(n: u64) -> u32 {
    let n = u32::try_from(n).unwrap_or_else(|_| die("alignment exceeds u32"));
    if n == 0 || !n.is_power_of_two() {
        die("alignment must be a nonzero power of two")
    };
    n
}
fn simple_path(p: &syn::Path) -> Vec<String> {
    if p.leading_colon.is_some()
        || p.segments.is_empty()
        || p.segments
            .iter()
            .any(|s| !matches!(s.arguments, syn::PathArguments::None))
    {
        die("only simple paths accepted")
    };
    p.segments.iter().map(|s| name(&s.ident)).collect()
}
fn width_path(p: &syn::Path) -> IntWidth {
    match simple_path(p).as_slice() {
        [x] if x == "u8" => IntWidth::U8,
        [x] if x == "u16" => IntWidth::U16,
        [x] if x == "u32" => IntWidth::U32,
        _ => die("width must be u8/u16/u32"),
    }
}
fn classify(t: &SynType) -> Type {
    match t {
        SynType::Path(p) if p.qself.is_none() => {
            let v = simple_path(&p.path);
            if v.len() != 1 {
                die("qualified field type forbidden")
            };
            match v[0].as_str() {
                "u8" => Type::Primitive(Primitive::U8),
                "i8" => Type::Primitive(Primitive::I8),
                "u16" => Type::Primitive(Primitive::U16),
                "i16" => Type::Primitive(Primitive::I16),
                "u32" => Type::Primitive(Primitive::U32),
                "i32" => Type::Primitive(Primitive::I32),
                "u64" => Type::Primitive(Primitive::U64),
                "i64" => Type::Primitive(Primitive::I64),
                "f32" => Type::Primitive(Primitive::F32),
                "f64" => Type::Primitive(Primitive::F64),
                "bool" => Type::Bool,
                x => Type::Schema(x.to_owned()),
            }
        }
        SynType::Reference(r) if r.mutability.is_none() && r.lifetime.is_some() => match &*r.elem {
            SynType::Path(p) if p.qself.is_none() => match simple_path(&p.path).as_slice() {
                [x] if x == "str" => Type::String(StringKind::Utf8),
                [x] if x == "CStr" => Type::String(StringKind::CBytes),
                [x] if x == "U16Str" => Type::String(StringKind::U16),
                [x] if x == "U16CStr" => Type::String(StringKind::U16C),
                _ => die("unsupported reference type"),
            },
            SynType::Array(a) => {
                if !matches!(&*a.elem,SynType::Path(p) if p.path.is_ident("u8")) {
                    die("only byte fixed arrays accepted")
                };
                let Expr::Lit(e) = &a.len else {
                    die("array length literal required")
                };
                let Lit::Int(l) = &e.lit else {
                    die("array length integer required")
                };
                Type::FixedBytes(u32::try_from(uint(l)).unwrap_or_else(|_| die("array too large")))
            }
            _ => die("unsupported reference"),
        },
        _ => die("unsupported type"),
    }
}

fn parse_corpus(f: &File) -> (Vec<Schema>, Vec<RootRegistration>) {
    let mut schemas = Vec::new();
    let mut roots = None;
    let mut imports = BTreeSet::new();
    for item in &f.items {
        match item {
            Item::Use(u) => {
                if !u.attrs.is_empty() {
                    die("attributes on imports forbidden")
                };
                {
                    imports.insert(quote_use(&u.tree));
                }
            }
            Item::Struct(s) => {
                if !matches!(s.vis, syn::Visibility::Public(_)) || s.generics.where_clause.is_some()
                {
                    die("schema structs must be public without where clause")
                };
                if s.generics
                    .params
                    .iter()
                    .any(|p| !matches!(p, GenericParam::Lifetime(_)))
                    || s.generics.params.len() > 1
                {
                    die("only one lifetime generic accepted")
                };
                let (z, r) = attrs(&s.attrs, false);
                if r.is_some() || z.tag.is_some() {
                    die("invalid struct container options")
                };
                let Fields::Named(fs) = &s.fields else {
                    die("only named schema structs")
                };
                let mut fields = Vec::new();
                for f in &fs.named {
                    if !matches!(f.vis, syn::Visibility::Public(_)) {
                        die("schema fields must be public")
                    };
                    let id = f.ident.as_ref().unwrap();
                    let (o, r) = attrs(&f.attrs, false);
                    if r.is_some() || o.padding_zero || o.tag.is_some() {
                        die("invalid field options")
                    };
                    let ty = classify(&f.ty);
                    if matches!(ty, Type::String(_)) != o.capacity.is_some() {
                        die(format!("string capacity mismatch on {id}"))
                    };
                    if matches!(ty, Type::FixedBytes(_))
                        && [
                            o.capacity.is_some(),
                            o.len.is_some(),
                            o.tail_zero,
                            o.endian.is_some(),
                        ]
                        .into_iter()
                        .any(|x| x)
                    {
                        die("options on fixed bytes")
                    };
                    if !matches!(ty, Type::String(StringKind::Utf8 | StringKind::U16))
                        && o.len.is_some()
                    {
                        die("len_type not applicable")
                    };
                    let endian = o.endian.unwrap_or(Endian::Native);
                    fields.push(Field {
                        name: name(id),
                        ty,
                        endian,
                        align: o.align,
                        capacity: o.capacity,
                        len: o.len.map(|w| (w, endian)),
                        tail_zero: o.tail_zero,
                        tag_field: o.tag_field,
                    })
                }
                schemas.push(Schema {
                    name: name(&s.ident),
                    kind: SchemaKind::Struct { fields },
                    align: z.align,
                    tail_zero: false,
                })
            }
            Item::Enum(e) => {
                if !matches!(e.vis, syn::Visibility::Public(_))
                    || !e.generics.params.is_empty()
                    || e.generics.where_clause.is_some()
                {
                    die("invalid enum declaration")
                };
                let (z, repr) = attrs(&e.attrs, true);
                if let Some(tag) = z.tag {
                    if repr.is_some() || z.endian.is_some() || z.padding_zero {
                        die("invalid tagged options")
                    };
                    let mut variants = Vec::new();
                    for v in &e.variants {
                        if v.discriminant.is_some() {
                            die("tagged discriminants forbidden")
                        };
                        let (o, r) = attrs(&v.attrs, false);
                        if r.is_some()
                            || o.tag.is_none()
                            || [
                                o.endian.is_some(),
                                o.align.is_some(),
                                o.capacity.is_some(),
                                o.len.is_some(),
                                o.tag_field.is_some(),
                            ]
                            .into_iter()
                            .any(|x| x)
                        {
                            die("tagged variant requires only tag")
                        };
                        let p = o.tag.unwrap();
                        let ps = p.split("::").collect::<Vec<_>>();
                        if ps.len() != 2 || ps[0] != tag {
                            die("variant tag type mismatch")
                        };
                        let payload = match &v.fields {
                            Fields::Unit => None,
                            Fields::Unnamed(x) if x.unnamed.len() == 1 => {
                                match classify(&x.unnamed[0].ty) {
                                    Type::Schema(n) => Some(n),
                                    _ => die("tag payload must be schema"),
                                }
                            }
                            _ => die("tag variant must unit/newtype"),
                        };
                        variants.push(TaggedVariant {
                            name: name(&v.ident),
                            tag_variant: ps[1].to_owned(),
                            payload,
                        })
                    }
                    schemas.push(Schema {
                        name: name(&e.ident),
                        kind: SchemaKind::TaggedEnum { tag, variants },
                        align: z.align,
                        tail_zero: z.tail_zero,
                    })
                } else {
                    let repr = repr.unwrap_or_else(|| die("scalar enum repr required"));
                    if z.align.is_some() || z.padding_zero || z.tail_zero {
                        die("invalid scalar enum options")
                    };
                    let mut variants = Vec::new();
                    for v in &e.variants {
                        if !v.attrs.is_empty() || !matches!(v.fields, Fields::Unit) {
                            die("scalar variants must be plain units")
                        };
                        let Some((_, Expr::Lit(x))) = &v.discriminant else {
                            die("explicit integer discriminant required")
                        };
                        let Lit::Int(l) = &x.lit else {
                            die("integer discriminant required")
                        };
                        let raw = uint(l);
                        let max = match repr {
                            IntWidth::U8 => u8::MAX as u64,
                            IntWidth::U16 => u16::MAX as u64,
                            IntWidth::U32 => u32::MAX as u64,
                        };
                        if raw > max {
                            die("enum discriminant out of range")
                        };
                        variants.push(EnumVariant {
                            name: name(&v.ident),
                            raw,
                        })
                    }
                    schemas.push(Schema {
                        name: name(&e.ident),
                        kind: SchemaKind::ScalarEnum {
                            repr,
                            endian: z.endian.unwrap_or(Endian::Native),
                            variants,
                        },
                        align: None,
                        tail_zero: false,
                    })
                }
            }
            Item::Const(c) if c.ident == "CONFORMANCE_ROOT_IDS" => {
                if roots.is_some()
                    || !c.attrs.is_empty()
                    || !matches!(c.vis, syn::Visibility::Public(_))
                {
                    die("invalid root registry")
                };
                roots = Some(parse_registry(&c.expr))
            }
            _ => die("unhandled corpus item"),
        }
    }
    if imports
        != BTreeSet::from([
            "core::ffi::CStr".to_owned(),
            "widestring::{U16CStr,U16Str}".to_owned(),
            "zero_schema::ZeroSchema".to_owned(),
        ])
    {
        die("imports differ from frozen set")
    };
    (
        schemas,
        roots.unwrap_or_else(|| die("missing root registry")),
    )
}
fn quote_use(t: &syn::UseTree) -> String {
    match t {
        syn::UseTree::Path(p) => format!("{}::{}", name(&p.ident), quote_use(&p.tree)),
        syn::UseTree::Name(n) => name(&n.ident),
        syn::UseTree::Group(g) => format!(
            "{{{}}}",
            g.items.iter().map(quote_use).collect::<Vec<_>>().join(",")
        ),
        _ => die("unsupported import"),
    }
}
fn parse_registry(e: &Expr) -> Vec<RootRegistration> {
    let Expr::Reference(r) = e else {
        die("registry must borrow slice")
    };
    let Expr::Array(a) = &*r.expr else {
        die("registry must be array")
    };
    a.elems
        .iter()
        .map(|x| {
            let Expr::Tuple(t) = x else {
                die("registry entry tuple required")
            };
            if t.elems.len() != 2 {
                die("registry tuple arity")
            };
            let (Expr::Lit(s), Expr::Lit(i)) = (&t.elems[0], &t.elems[1]) else {
                die("registry literals required")
            };
            let (Lit::Str(s), Lit::Int(i)) = (&s.lit, &i.lit) else {
                die("registry types")
            };
            RootRegistration {
                root_id: s.value(),
                case_id: u32::try_from(uint(i)).unwrap_or_else(|_| die("case id exceeds u32")),
            }
        })
        .collect()
}

struct Cases(Vec<Case>);
impl Parse for Cases {
    fn parse(input: ParseStream<'_>) -> syn::Result<Self> {
        let mut v = Vec::new();
        while !input.is_empty() {
            let k: syn::Ident = input.parse()?;
            if k != "case" {
                return Err(syn::Error::new(k.span(), "expected case"));
            };
            let b;
            syn::braced!(b in input);
            field(&b, "id")?;
            let idl: syn::LitInt = b.parse()?;
            comma(&b)?;
            field(&b, "root_id")?;
            let rid: syn::LitStr = b.parse()?;
            comma(&b)?;
            field(&b, "root")?;
            let rk: syn::Ident = b.parse()?;
            let rp: syn::Path = b.parse()?;
            let pn = simple_path(&rp);
            if pn.len() != 1 {
                return Err(b.error("simple root required"));
            };
            let root = match rk.to_string().as_str() {
                "schema" => Root::Schema(pn[0].clone()),
                "abi" => Root::Abi(pn[0].clone()),
                _ => return Err(b.error("root kind")),
            };
            comma(&b)?;
            field(&b, "value")?;
            let value = parse_value(&b)?;
            comma(&b)?;
            field(&b, "layout")?;
            let layout = parse_entries(&b, true)?;
            comma(&b)?;
            field(&b, "observe")?;
            let observe = parse_obs(&b)?;
            if b.peek(Token![,]) {
                {
                    b.parse::<Token![,]>()?;
                }
            }
            if !b.is_empty() {
                return Err(b.error("trailing case syntax"));
            }
            v.push(Case {
                id: u32::try_from(uint(&idl)).unwrap_or_else(|_| die("id overflow")),
                root_id: rid.value(),
                root,
                value,
                layout,
                observe,
            })
        }
        Ok(Self(v))
    }
}
fn field(i: ParseStream<'_>, s: &str) -> syn::Result<()> {
    let k: syn::Ident = i.parse()?;
    if k != s {
        return Err(syn::Error::new(k.span(), format!("expected {s}")));
    }
    i.parse::<Token![:]>()?;
    Ok(())
}
fn comma(i: ParseStream<'_>) -> syn::Result<()> {
    i.parse::<Token![,]>().map(|_| ())
}
fn parse_value(i: ParseStream<'_>) -> syn::Result<Value> {
    let k: syn::Ident = i.parse()?;
    match k.to_string().as_str() {
        "record" => Ok(Value::Record(named_values(i)?)),
        "union" => {
            let p;
            syn::parenthesized!(p in i);
            let q: syn::Path = p.parse()?;
            if !p.is_empty() {
                return Err(p.error("union path trailing"));
            };
            let x = simple_path(&q);
            if x.len() != 2 {
                return Err(i.error("union path type::variant"));
            };
            Ok(Value::Union {
                ty: x[0].clone(),
                variant: x[1].clone(),
                fields: named_values(i)?,
            })
        }
        "bits" => {
            let p;
            syn::parenthesized!(p in i);
            let l: syn::LitInt = p.parse()?;
            if !p.is_empty() {
                return Err(p.error("bits trailing"));
            };
            Ok(Value::Bits(uint(&l)))
        }
        "boolean" => {
            let p;
            syn::parenthesized!(p in i);
            let b: syn::LitBool = p.parse()?;
            if !p.is_empty() {
                return Err(p.error("boolean trailing"));
            };
            Ok(Value::Boolean(b.value))
        }
        "variant" => {
            let p;
            syn::parenthesized!(p in i);
            let q: syn::Path = p.parse()?;
            let x = simple_path(&q);
            if x.len() != 2 || !p.is_empty() {
                return Err(p.error("variant path"));
            };
            Ok(Value::Variant {
                ty: x[0].clone(),
                variant: x[1].clone(),
            })
        }
        "bytes" | "units" => {
            let p;
            syn::parenthesized!(p in i);
            let a;
            syn::bracketed!(a in p);
            let xs = a.parse_terminated(syn::LitInt::parse, Token![,])?;
            if !p.is_empty() {
                return Err(p.error("array trailing"));
            };
            if k == "bytes" {
                Ok(Value::Bytes(
                    xs.iter()
                        .map(|x| u8::try_from(uint(x)).unwrap_or_else(|_| die("byte overflow")))
                        .collect(),
                ))
            } else {
                Ok(Value::Units(
                    xs.iter()
                        .map(|x| u16::try_from(uint(x)).unwrap_or_else(|_| die("unit overflow")))
                        .collect(),
                ))
            }
        }
        "zst" => Ok(Value::Zst),
        _ => Err(syn::Error::new(k.span(), "unknown value form")),
    }
}
fn named_values(i: ParseStream<'_>) -> syn::Result<Vec<(String, Value)>> {
    let b;
    syn::braced!(b in i);
    let mut v = Vec::new();
    while !b.is_empty() {
        let n: syn::Ident = b.parse()?;
        b.parse::<Token![:]>()?;
        v.push((name(&n), parse_value(&b)?));
        if b.peek(Token![,]) {
            comma(&b)?
        } else if !b.is_empty() {
            return Err(b.error("comma required"));
        }
    }
    Ok(v)
}
fn call(i: ParseStream<'_>) -> syn::Result<(String, Vec<String>)> {
    let n: syn::Ident = i.parse()?;
    let p;
    syn::parenthesized!(p in i);
    let mut a = Vec::new();
    while !p.is_empty() {
        let x: syn::Ident = p.parse()?;
        a.push(name(&x));
        if p.peek(Token![.]) {
            {
                p.parse::<Token![.]>()?;
            }
        } else if !p.is_empty() {
            return Err(p.error("invalid call args"));
        }
    }
    Ok((name(&n), a))
}
fn metric(n: &str, a: Vec<String>) -> Metric {
    macro_rules! one {
        ($v:ident) => {{
            if a.len() != 1 {
                die(concat!(stringify!($v), " arity"))
            };
            Metric::$v(a[0].clone())
        }};
    }
    match n {
        "root_size" if a.is_empty() => Metric::RootSize,
        "root_align" if a.is_empty() => Metric::RootAlign,
        "root_stride" if a.is_empty() => Metric::RootStride,
        "field_offset" => one!(FieldOffset),
        "field_size" => one!(FieldSize),
        "field_align" => one!(FieldAlign),
        "tag_offset" if a.is_empty() => Metric::TagOffset,
        "payload_offset" if a.is_empty() => Metric::PayloadOffset,
        "payload_size" if a.is_empty() => Metric::PayloadSize,
        "payload_align" if a.is_empty() => Metric::PayloadAlign,
        "tag_size" if a.is_empty() => Metric::TagSize,
        "tag_align" if a.is_empty() => Metric::TagAlign,
        "tag_endian" if a.is_empty() => Metric::TagEndian,
        "variant_raw" => one!(VariantRaw),
        "variant_payload_size" => one!(VariantPayloadSize),
        "variant_payload_align" => one!(VariantPayloadAlign),
        "enum_size" => one!(EnumSize),
        "enum_align" => one!(EnumAlign),
        "enum_width" => one!(EnumWidth),
        "enum_endian" => one!(EnumEndian),
        "enum_raw" => one!(EnumRaw),
        "string_encoding" => one!(StringEncoding),
        "string_capacity" => one!(StringCapacity),
        "string_unit_width" => one!(StringUnitWidth),
        "string_data_offset" => one!(StringDataOffset),
        "string_length_width" => one!(StringLengthWidth),
        "string_length_endian" => one!(StringLengthEndian),
        "string_length_offset" => one!(StringLengthOffset),
        "string_tail" => one!(StringTail),
        _ => die(format!("unknown metric {n}")),
    }
}
fn parse_entries(i: ParseStream<'_>, _: bool) -> syn::Result<Vec<MetricEntry>> {
    let b;
    syn::bracketed!(b in i);
    let mut v = Vec::new();
    while !b.is_empty() {
        let e;
        syn::braced!(e in b);
        field(&e, "key")?;
        let k: syn::LitInt = e.parse()?;
        comma(&e)?;
        field(&e, "metric")?;
        let (n, a) = call(&e)?;
        if e.peek(Token![,]) {
            comma(&e)?
        }
        if !e.is_empty() {
            return Err(e.error("metric trailing"));
        }
        v.push(MetricEntry {
            key: uint(&k),
            metric: metric(&n, a),
        });
        if b.peek(Token![,]) {
            comma(&b)?
        }
    }
    Ok(v)
}
fn dotted(i: ParseStream<'_>) -> syn::Result<Vec<String>> {
    let mut out = Vec::new();
    let first: syn::Ident = i.parse()?;
    out.push(name(&first));
    while i.peek(Token![.]) {
        i.parse::<Token![.]>()?;
        let next: syn::Ident = i.parse()?;
        out.push(name(&next));
    }
    Ok(out)
}
fn parse_obs(i: ParseStream<'_>) -> syn::Result<Vec<ObservationEntry>> {
    let b;
    syn::bracketed!(b in i);
    let mut v = Vec::new();
    while !b.is_empty() {
        let e;
        syn::braced!(e in b);
        field(&e, "key")?;
        let k: syn::LitInt = e.parse()?;
        comma(&e)?;
        field(&e, "source")?;
        let n: syn::Ident = e.parse()?;
        let p;
        syn::parenthesized!(p in e);
        let source = if n == "unit" {
            let path = dotted(&p)?;
            p.parse::<Token![,]>()?;
            let index: syn::LitInt = p.parse()?;
            Observation::Unit(
                path,
                u32::try_from(uint(&index)).unwrap_or_else(|_| die("unit index")),
            )
        } else {
            let path = dotted(&p)?;
            match n.to_string().as_str() {
                "scalar" => Observation::Scalar(path),
                "tag" => Observation::Tag(path),
                "length" => Observation::Length(path),
                _ => return Err(e.error("unknown observation")),
            }
        };
        if !p.is_empty() {
            return Err(p.error("observation trailing"));
        }
        if e.peek(Token![,]) {
            comma(&e)?
        }
        if !e.is_empty() {
            return Err(e.error("observation entry trailing"));
        }
        v.push(ObservationEntry {
            key: uint(&k),
            source,
        });
        if b.peek(Token![,]) {
            comma(&b)?
        }
    }
    Ok(v)
}

fn parse_cases(f: &File) -> (Vec<AbiStruct>, Vec<Case>) {
    let mut abi = Vec::new();
    let mut cases = None;
    for x in &f.items {
        match x {
            Item::Struct(s) => {
                if !matches!(s.vis, syn::Visibility::Inherited)
                    || !s.generics.params.is_empty()
                    || s.generics.where_clause.is_some()
                {
                    die("invalid ABI struct")
                };
                let (z, r) = attrs(&s.attrs, false);
                if r.is_some()
                    || z.padding_zero
                    || z.tail_zero
                    || z.endian.is_some()
                    || z.tag.is_some()
                {
                    die("invalid ABI repr")
                };
                let fields = match &s.fields {
                    Fields::Unit => Vec::new(),
                    Fields::Named(n) => n
                        .named
                        .iter()
                        .map(|f| {
                            if !f.attrs.is_empty() || !matches!(f.vis, syn::Visibility::Inherited) {
                                die("invalid ABI field")
                            };
                            let SynType::Path(p) = &f.ty else {
                                die("ABI simple type")
                            };
                            AbiField {
                                name: name(f.ident.as_ref().unwrap()),
                                ty: simple_path(&p.path).join("::"),
                            }
                        })
                        .collect(),
                    _ => die("ABI named/unit only"),
                };
                abi.push(AbiStruct {
                    name: name(&s.ident),
                    align: z.align,
                    fields,
                })
            }
            Item::Macro(m) if m.mac.path.is_ident("conformance_cases") => {
                if cases.is_some()
                    || !m.attrs.is_empty()
                    || m.ident.is_some()
                    || m.semi_token.is_some()
                {
                    die("invalid cases macro")
                };
                let Cases(v) = syn::parse2(m.mac.tokens.clone()).unwrap_or_else(|e| die(e));
                cases = Some(v)
            }
            _ => die("unhandled cases item"),
        }
    }
    if abi.iter().map(|x| x.name.as_str()).collect::<Vec<_>>()
        != [
            "ConformanceZst8",
            "ConformanceZst16",
            "ConformanceZst32",
            "ConformanceZstLayout",
        ]
    {
        die("ABI declarations differ")
    };
    (abi, cases.unwrap_or_else(|| die("missing cases macro")))
}

pub fn parse(corpus: &File, cases: &File) -> Model {
    let (schemas, roots) = parse_corpus(corpus);
    let (abi_structs, cases) = parse_cases(cases);
    if cases.len() != 11 || roots.len() != 11 {
        die("exactly 11 cases and roots required")
    };
    let sn: BTreeSet<_> = schemas.iter().map(|x| x.name.clone()).collect();
    for s in &schemas {
        match &s.kind {
            SchemaKind::Struct { fields } => {
                for f in fields {
                    if let Type::Schema(n) = &f.ty {
                        if !sn.contains(n) {
                            die(format!("unknown schema {n}"))
                        }
                    }
                }
            }
            SchemaKind::TaggedEnum { tag, variants } => {
                if !sn.contains(tag) {
                    die("unknown tag schema")
                };
                for v in variants {
                    if let Some(n) = &v.payload {
                        if !sn.contains(n) {
                            die("unknown payload schema")
                        }
                    }
                }
            }
            _ => {}
        }
    }
    let mut edges: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for s in &schemas {
        let mut e = Vec::new();
        match &s.kind {
            SchemaKind::Struct { fields } => {
                for f in fields {
                    if let Type::Schema(n) = &f.ty {
                        e.push(n.clone())
                    }
                }
            }
            SchemaKind::TaggedEnum { variants, .. } => {
                for v in variants {
                    if let Some(n) = &v.payload {
                        e.push(n.clone())
                    }
                }
            }
            _ => {}
        }
        {
            edges.insert(s.name.clone(), e);
        }
    }
    fn visit(
        n: &str,
        e: &BTreeMap<String, Vec<String>>,
        temp: &mut BTreeSet<String>,
        done: &mut BTreeSet<String>,
    ) {
        if done.contains(n) {
            return;
        }
        if !temp.insert(n.to_owned()) {
            die("schema cycle")
        };
        for x in &e[n] {
            visit(x, e, temp, done)
        }
        temp.remove(n);
        done.insert(n.to_owned());
    }
    let (mut t, mut d) = (BTreeSet::new(), BTreeSet::new());
    for n in edges.keys() {
        visit(n, &edges, &mut t, &mut d)
    }
    let mut ids = BTreeSet::new();
    let mut keys = BTreeSet::new();
    for (idx, c) in cases.iter().enumerate() {
        if c.id != 1001 + idx as u32 || !ids.insert(c.id) {
            die("case ids must be 1001..1011")
        };
        let r = &roots[idx];
        if r.case_id != c.id || r.root_id != c.root_id {
            die("root registry/case mismatch")
        };
        match &c.root {
            Root::Schema(n) if sn.contains(n) => {}
            Root::Abi(n) if abi_structs.iter().any(|x| &x.name == n) => {}
            _ => die("case root unknown"),
        };
        for list in [
            &c.layout.iter().map(|x| x.key).collect::<Vec<_>>(),
            &c.observe.iter().map(|x| x.key).collect::<Vec<_>>(),
        ] {
            if list.is_empty() || list.windows(2).any(|w| w[0] >= w[1]) {
                die("keys must be nonempty ascending")
            };
            for k in list {
                if *k == 0 || !keys.insert(*k) {
                    die("keys must be globally unique nonzero")
                }
            }
        }
        let b = u64::from(c.id) * 1000;
        if c.layout.iter().any(|x| x.key < b + 1 || x.key > b + 499)
            || c.observe.iter().any(|x| x.key < b + 501 || x.key > b + 999)
        {
            die("key namespace/formula violation")
        }
    }
    let used: BTreeSet<_> = cases
        .iter()
        .filter_map(|c| match &c.root {
            Root::Schema(n) => Some(n.clone()),
            _ => None,
        })
        .collect();
    let mut reachable = used.clone();
    loop {
        let old = reachable.len();
        for n in reachable.clone() {
            for x in &edges[&n] {
                reachable.insert(x.clone());
            }
        }
        if reachable.len() == old {
            break;
        }
    }
    if schemas.iter().any(|s| !reachable.contains(&s.name)) {
        die("unused schema")
    };
    Model {
        schemas,
        abi_structs,
        cases,
    }
}
