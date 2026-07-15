#![allow(dead_code)]
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
    Array { element: Box<Type>, length: u32 },
    Schema(String),
    Option(Box<Type>),
}
#[derive(Clone, Debug)]
pub struct Field {
    pub name: String,
    pub ty: Type,
    pub endian: Endian,
    pub align: Option<u32>,
    pub capacity: Option<u32>,
    pub len: Option<(IntWidth, Endian)>,
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
}
#[derive(Clone, Debug)]
pub struct RootRegistration {
    pub root_id: String,
    pub case_id: u32,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Root {
    Schema(String),
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
    Array(Vec<Value>),
    None,
    Some(Box<Value>),
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Metric {
    RootSize,
    RootAlign,
    RootStride,
    FieldOffset(String),
    FieldSize(String),
    FieldAlign(String),
    ArrayLength(String),
    ArrayStride(String),
    OptionalSpan(String),
    OptionalIsOptional(String),
    OptionalArrayLength(String),
    OptionalArrayStride(String),
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
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Observation {
    Scalar(Vec<String>),
    Tag(Vec<String>),
    Length(Vec<String>),
    Unit(Vec<String>, u32),
    Element(Vec<String>, u32),
    Optional(Vec<String>),
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
    pub native: bool,
    pub value: Value,
    pub layout: Vec<MetricEntry>,
    pub observe: Vec<ObservationEntry>,
}
#[derive(Clone, Debug)]
pub struct Model {
    pub schemas: Vec<Schema>,
    pub cases: Vec<Case>,
}
impl Model {
    pub fn schema(&self, n: &str) -> &Schema {
        self.schemas
            .iter()
            .find(|x| x.name == n)
            .unwrap_or_else(|| die(format!("unknown schema {n}")))
    }
}

#[derive(Default)]
struct Zero {
    endian: Option<Endian>,
    align: Option<u32>,
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
                    "Clone" | "Copy" | "Debug" | "PartialEq" | "Eq"
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
                if matches!(a.meta, Meta::Path(_)) {
                    continue;
                }
                die("zero must be a marker or option list")
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
fn option_inner(p: &syn::TypePath) -> Option<&SynType> {
    if p.qself.is_some() || p.path.leading_colon.is_some() {
        return None;
    }
    let segments: Vec<_> = p.path.segments.iter().collect();
    let canonical = match segments.as_slice() {
        [option] => name(&option.ident) == "Option",
        [namespace, option_module, option]
            if matches!(name(&namespace.ident).as_str(), "core" | "std") =>
        {
            name(&option_module.ident) == "option" && name(&option.ident) == "Option"
        }
        _ => false,
    };
    if !canonical {
        return None;
    }
    if segments[..segments.len() - 1]
        .iter()
        .any(|segment| !matches!(segment.arguments, syn::PathArguments::None))
    {
        die("canonical Option namespace cannot be generic")
    }
    let syn::PathArguments::AngleBracketed(arguments) = &segments.last().unwrap().arguments else {
        die("Option requires exactly one type argument")
    };
    if arguments.args.len() != 1 {
        die("Option requires exactly one type argument")
    }
    match arguments.args.first().unwrap() {
        syn::GenericArgument::Type(inner) => Some(inner),
        _ => die("Option requires a type argument"),
    }
}
fn classify(t: &SynType) -> Type {
    match t {
        SynType::Path(p) if p.qself.is_none() => {
            if let Some(inner) = option_inner(p) {
                return Type::Option(Box::new(classify(inner)));
            }
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
        SynType::Array(array) => {
            let Expr::Lit(length) = &array.len else {
                die("array length literal required")
            };
            let Lit::Int(length) = &length.lit else {
                die("array length integer required")
            };
            let length = u32::try_from(uint(length)).unwrap_or_else(|_| die("array too large"));
            if length == 0 {
                die("array length must be nonzero")
            }
            let element = classify(&array.elem);
            if !matches!(element, Type::Primitive(_) | Type::Bool | Type::Schema(_)) {
                die("unsupported array element type")
            }
            Type::Array {
                element: Box::new(element),
                length,
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
            SynType::Array(array) => {
                if !matches!(&*array.elem, SynType::Path(path) if path.path.is_ident("u8")) {
                    die("only byte fixed arrays accepted")
                };
                let Expr::Lit(length) = &array.len else {
                    die("array length literal required")
                };
                let Lit::Int(length) = &length.lit else {
                    die("array length integer required")
                };
                let length = u32::try_from(uint(length)).unwrap_or_else(|_| die("array too large"));
                if length == 0 {
                    die("fixed byte length must be nonzero")
                }
                Type::FixedBytes(length)
            }
            _ => die("unsupported reference"),
        },
        _ => die("unsupported type"),
    }
}
fn schema_refs<'a>(ty: &'a Type, refs: &mut Vec<&'a str>) {
    match ty {
        Type::Schema(name) => refs.push(name),
        Type::Array { element, .. } | Type::Option(element) => schema_refs(element, refs),
        Type::Primitive(_) | Type::Bool | Type::String(_) | Type::FixedBytes(_) => {}
    }
}
fn type_is_all_zero_invalid(ty: &Type, schemas: &BTreeMap<&str, &Schema>) -> bool {
    match ty {
        Type::Schema(name) => schema_is_all_zero_invalid(name, schemas),
        Type::Array { element, length } => {
            *length != 0 && type_is_all_zero_invalid(element, schemas)
        }
        Type::Option(_)
        | Type::Primitive(_)
        | Type::Bool
        | Type::String(_)
        | Type::FixedBytes(_) => false,
    }
}
fn schema_is_all_zero_invalid(name: &str, schemas: &BTreeMap<&str, &Schema>) -> bool {
    match &schemas[name].kind {
        SchemaKind::ScalarEnum { variants, .. } => {
            !variants.is_empty() && variants.iter().all(|variant| variant.raw != 0)
        }
        SchemaKind::Struct { fields } => fields
            .iter()
            .any(|field| type_is_all_zero_invalid(&field.ty, schemas)),
        SchemaKind::TaggedEnum { .. } => false,
    }
}
fn option_type_is_zero_sentinel_eligible(ty: &Type, schemas: &BTreeMap<&str, &Schema>) -> bool {
    match ty {
        Type::Schema(name) => schema_is_all_zero_invalid(name, schemas),
        Type::Array { element, length } if *length != 0 => {
            matches!(element.as_ref(), Type::Schema(_))
                && type_is_all_zero_invalid(element, schemas)
        }
        _ => false,
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
                    if r.is_some() || o.tag.is_some() {
                        die("invalid field options")
                    };
                    let ty = classify(&f.ty);
                    if matches!(ty, Type::String(_)) != o.capacity.is_some() {
                        die(format!("string capacity mismatch on {id}"))
                    };
                    if matches!(ty, Type::FixedBytes(_))
                        && [o.capacity.is_some(), o.len.is_some(), o.endian.is_some()]
                            .into_iter()
                            .any(|x| x)
                    {
                        die("options on fixed bytes")
                    };
                    if matches!(ty, Type::Array { .. })
                        && [o.capacity.is_some(), o.len.is_some(), o.endian.is_some()]
                            .into_iter()
                            .any(|present| present)
                    {
                        die("array options are not applicable")
                    };
                    if matches!(ty, Type::Option(_))
                        && [
                            o.capacity.is_some(),
                            o.len.is_some(),
                            o.endian.is_some(),
                            o.tag_field.is_some(),
                        ]
                        .into_iter()
                        .any(|present| present)
                    {
                        die("only align is applicable to Option fields")
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
                        tag_field: o.tag_field,
                    })
                }
                schemas.push(Schema {
                    name: name(&s.ident),
                    kind: SchemaKind::Struct { fields },
                    align: z.align,
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
                if repr.is_none() {
                    if z.tag.is_some() || z.endian.is_some() {
                        die("invalid tagged container options")
                    };
                    let mut tag = None;
                    let mut variants = Vec::new();
                    for variant in &e.variants {
                        if variant.discriminant.is_some() {
                            die("tagged discriminants forbidden")
                        };
                        let (options, variant_repr) = attrs(&variant.attrs, false);
                        if variant_repr.is_some()
                            || options.tag.is_none()
                            || [
                                options.endian.is_some(),
                                options.align.is_some(),
                                options.capacity.is_some(),
                                options.len.is_some(),
                                options.tag_field.is_some(),
                            ]
                            .into_iter()
                            .any(|present| present)
                        {
                            die("tagged variant requires only tag")
                        };
                        let path = options.tag.unwrap();
                        let parts = path.split("::").collect::<Vec<_>>();
                        if parts.len() != 2 {
                            die("tagged variant tag must name enum and variant")
                        }
                        match &tag {
                            Some(existing) if existing != parts[0] => {
                                die("tagged variants must share one scalar enum")
                            }
                            Some(_) => {}
                            None => tag = Some(parts[0].to_owned()),
                        }
                        let payload = match &variant.fields {
                            Fields::Unit => None,
                            Fields::Unnamed(fields) if fields.unnamed.len() == 1 => {
                                match classify(&fields.unnamed[0].ty) {
                                    Type::Schema(name) => Some(name),
                                    _ => die("tag payload must be schema"),
                                }
                            }
                            _ => die("tag variant must unit/newtype"),
                        };
                        variants.push(TaggedVariant {
                            name: name(&variant.ident),
                            tag_variant: parts[1].to_owned(),
                            payload,
                        })
                    }
                    schemas.push(Schema {
                        name: name(&e.ident),
                        kind: SchemaKind::TaggedEnum {
                            tag: tag.unwrap_or_else(|| die("tagged enum requires a variant")),
                            variants,
                        },
                        align: z.align,
                    })
                } else {
                    let repr = repr.unwrap_or_else(|| die("scalar enum repr required"));
                    if z.align.is_some() {
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
            "zero_schema::zero".to_owned(),
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
            if rk != "schema" {
                return Err(b.error("conformance root must be a schema"));
            }
            let root = Root::Schema(pn[0].clone());
            comma(&b)?;
            let native = {
                let lookahead = b.fork();
                let next: syn::Ident = lookahead.parse()?;
                if next == "native" {
                    field(&b, "native")?;
                    let enabled: syn::LitBool = b.parse()?;
                    if !enabled.value {
                        return Err(b.error("native marker must be true"));
                    }
                    comma(&b)?;
                    true
                } else {
                    false
                }
            };
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
                native,
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
        "none" => Ok(Value::None),
        "some" => {
            let p;
            syn::parenthesized!(p in i);
            let value = parse_value(&p)?;
            if !p.is_empty() {
                return Err(p.error("some trailing"));
            }
            Ok(Value::Some(Box::new(value)))
        }
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
        "array" => {
            let parenthesized;
            syn::parenthesized!(parenthesized in i);
            let bracketed;
            syn::bracketed!(bracketed in parenthesized);
            let mut values = Vec::new();
            while !bracketed.is_empty() {
                values.push(parse_value(&bracketed)?);
                if bracketed.peek(Token![,]) {
                    comma(&bracketed)?;
                } else if !bracketed.is_empty() {
                    return Err(bracketed.error("array comma required"));
                }
            }
            if !parenthesized.is_empty() {
                return Err(parenthesized.error("array trailing"));
            }
            Ok(Value::Array(values))
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
        "array_length" => one!(ArrayLength),
        "array_stride" => one!(ArrayStride),
        "optional_span" => one!(OptionalSpan),
        "optional_is_optional" => one!(OptionalIsOptional),
        "optional_array_length" => one!(OptionalArrayLength),
        "optional_array_stride" => one!(OptionalArrayStride),
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
        let source = if n == "unit" || n == "element" {
            let path = dotted(&p)?;
            p.parse::<Token![,]>()?;
            let index: syn::LitInt = p.parse()?;
            let index = u32::try_from(uint(&index)).unwrap_or_else(|_| die("array index"));
            if n == "unit" {
                Observation::Unit(path, index)
            } else {
                Observation::Element(path, index)
            }
        } else {
            let path = dotted(&p)?;
            match n.to_string().as_str() {
                "scalar" => Observation::Scalar(path),
                "tag" => Observation::Tag(path),
                "length" => Observation::Length(path),
                "optional" => Observation::Optional(path),
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

fn parse_cases(f: &File) -> Vec<Case> {
    let mut cases = None;
    for item in &f.items {
        let Item::Macro(macro_item) = item else {
            die("only conformance_cases! is permitted")
        };
        if !macro_item.mac.path.is_ident("conformance_cases")
            || cases.is_some()
            || !macro_item.attrs.is_empty()
            || macro_item.ident.is_some()
            || macro_item.semi_token.is_some()
        {
            die("invalid cases macro")
        };
        let Cases(parsed) =
            syn::parse2(macro_item.mac.tokens.clone()).unwrap_or_else(|error| die(error));
        cases = Some(parsed);
    }
    cases.unwrap_or_else(|| die("missing cases macro"))
}

pub fn parse(corpus: &File, cases: &File) -> Model {
    const CASE_IDS: [u32; 15] = [
        1001, 1002, 1003, 1004, 1005, 1006, 1007, 1008, 1010, 1011, 1012, 1013, 1014, 1015, 1016,
    ];

    let (schemas, roots) = parse_corpus(corpus);
    let cases = parse_cases(cases);
    if cases.len() != CASE_IDS.len() || roots.len() != CASE_IDS.len() {
        die("exactly fifteen registered conformance roots required")
    }
    let schemas_by_name: BTreeSet<_> = schemas.iter().map(|schema| schema.name.clone()).collect();
    let schema_by_name: BTreeMap<_, _> = schemas
        .iter()
        .map(|schema| (schema.name.as_str(), schema))
        .collect();
    let registrations: BTreeMap<_, _> = roots
        .iter()
        .map(|root| (root.case_id, root.root_id.as_str()))
        .collect();
    if registrations.len() != roots.len() {
        die("root registrations must have unique case IDs")
    }

    for schema in &schemas {
        match &schema.kind {
            SchemaKind::Struct { fields } => {
                let mut external_tag_fields = BTreeSet::new();
                for field in fields {
                    let mut referenced_schemas = Vec::new();
                    schema_refs(&field.ty, &mut referenced_schemas);
                    for name in referenced_schemas {
                        if !schemas_by_name.contains(name) {
                            die(format!("unknown schema {name}"))
                        }
                    }
                    let Some(tag_field_name) = &field.tag_field else {
                        if let Type::Schema(name) = &field.ty {
                            if matches!(
                                schema_by_name[name.as_str()].kind,
                                SchemaKind::TaggedEnum { .. }
                            ) {
                                die("tagged payload field requires tag_field")
                            }
                        }
                        continue;
                    };
                    if !external_tag_fields.insert(tag_field_name.as_str()) {
                        die("each external union requires a unique sibling tag field")
                    }
                    let Type::Schema(payload_name) = &field.ty else {
                        die("tag_field is only valid on a tagged payload")
                    };
                    let payload_schema = schema_by_name
                        .get(payload_name.as_str())
                        .unwrap_or_else(|| die("tagged payload schema is missing"));
                    let SchemaKind::TaggedEnum { tag, .. } = &payload_schema.kind else {
                        die("tag_field requires a tagged payload")
                    };
                    let sibling = fields
                        .iter()
                        .find(|candidate| candidate.name == *tag_field_name)
                        .unwrap_or_else(|| die("tag_field sibling is missing"));
                    let Type::Schema(sibling_name) = &sibling.ty else {
                        die("tag_field sibling must be a scalar enum")
                    };
                    let sibling_schema = schema_by_name
                        .get(sibling_name.as_str())
                        .unwrap_or_else(|| die("tag_field sibling schema is missing"));
                    if sibling_name != tag
                        || !matches!(&sibling_schema.kind, SchemaKind::ScalarEnum { .. })
                    {
                        die("tag_field sibling must match the tagged payload scalar enum")
                    }
                }
            }
            SchemaKind::TaggedEnum { tag, variants } => {
                if !matches!(
                    schema_by_name.get(tag.as_str()).map(|schema| &schema.kind),
                    Some(SchemaKind::ScalarEnum { .. })
                ) {
                    die("tagged payload tag must be a scalar enum")
                }
                for variant in variants {
                    if let Some(payload) = &variant.payload {
                        if !schemas_by_name.contains(payload) {
                            die("unknown tagged variant payload")
                        }
                    }
                }
            }
            SchemaKind::ScalarEnum { .. } => {}
        }
    }

    let mut edges = BTreeMap::<String, Vec<String>>::new();
    for schema in &schemas {
        let edges_for_schema = match &schema.kind {
            SchemaKind::Struct { fields } => {
                let mut references = Vec::new();
                for field in fields {
                    schema_refs(&field.ty, &mut references);
                }
                references.into_iter().map(str::to_owned).collect()
            }
            SchemaKind::TaggedEnum { variants, .. } => variants
                .iter()
                .filter_map(|variant| variant.payload.clone())
                .collect(),
            SchemaKind::ScalarEnum { .. } => Vec::new(),
        };
        edges.insert(schema.name.clone(), edges_for_schema);
    }
    fn visit(
        name: &str,
        edges: &BTreeMap<String, Vec<String>>,
        temporary: &mut BTreeSet<String>,
        complete: &mut BTreeSet<String>,
    ) {
        if complete.contains(name) {
            return;
        }
        if !temporary.insert(name.to_owned()) {
            die("schema cycle")
        }
        for child in &edges[name] {
            visit(child, edges, temporary, complete)
        }
        temporary.remove(name);
        complete.insert(name.to_owned());
    }
    let (mut temporary, mut complete) = (BTreeSet::new(), BTreeSet::new());
    for name in edges.keys() {
        visit(name, &edges, &mut temporary, &mut complete)
    }
    for schema in &schemas {
        let SchemaKind::Struct { fields } = &schema.kind else {
            continue;
        };
        for field in fields {
            let Type::Option(inner) = &field.ty else {
                continue;
            };
            if !option_type_is_zero_sentinel_eligible(inner, &schema_by_name) {
                die("Option inner type must be a zero-invalid schema or schema array")
            }
        }
    }

    let mut ids = BTreeSet::new();
    let mut keys = BTreeSet::new();
    let mut used_roots = BTreeSet::new();
    for case in &cases {
        if !CASE_IDS.contains(&case.id) || !ids.insert(case.id) {
            die("case IDs must be the frozen explicit set")
        }
        if case.native != (1012..=1016).contains(&case.id) {
            die("only cases 1012 through 1016 may carry the native marker")
        }
        match registrations.get(&case.id) {
            Some(root_id) if *root_id == case.root_id => {}
            _ => die("root registry/case mismatch"),
        }
        let Root::Schema(root_name) = &case.root;
        let root = schema_by_name
            .get(root_name.as_str())
            .unwrap_or_else(|| die("case root unknown"));
        if matches!(&root.kind, SchemaKind::TaggedEnum { .. }) {
            die("tagged payloads cannot be conformance roots")
        }
        used_roots.insert(root_name.clone());
        for keys_for_case in [
            case.layout
                .iter()
                .map(|entry| entry.key)
                .collect::<Vec<_>>(),
            case.observe
                .iter()
                .map(|entry| entry.key)
                .collect::<Vec<_>>(),
        ] {
            if keys_for_case.is_empty() || keys_for_case.windows(2).any(|pair| pair[0] >= pair[1]) {
                die("keys must be nonempty ascending")
            }
            for key in keys_for_case {
                if key == 0 || !keys.insert(key) {
                    die("keys must be globally unique nonzero")
                }
            }
        }
        let namespace = u64::from(case.id) * 1000;
        if case
            .layout
            .iter()
            .any(|entry| entry.key < namespace + 1 || entry.key > namespace + 499)
            || case
                .observe
                .iter()
                .any(|entry| entry.key < namespace + 501 || entry.key > namespace + 999)
        {
            die("key namespace/formula violation")
        }
    }
    if ids.len() != CASE_IDS.len() || !CASE_IDS.iter().all(|id| ids.contains(id)) {
        die("all frozen case IDs must be present")
    }

    let mut reachable = used_roots;
    loop {
        let before = reachable.len();
        for name in reachable.clone() {
            for child in &edges[&name] {
                reachable.insert(child.clone());
            }
        }
        if reachable.len() == before {
            break;
        }
    }
    if schemas
        .iter()
        .any(|schema| !reachable.contains(&schema.name))
    {
        die("unused schema")
    }
    Model { schemas, cases }
}
