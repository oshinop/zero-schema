#![allow(dead_code)]

use std::collections::BTreeMap;
use std::fmt::Write as _;

use super::frontend::{
    Endian, Field, IntWidth, Metric, Model, Primitive, Root, SchemaKind, StringKind, Type,
};

#[derive(Clone, Debug)]
pub struct PathMeta {
    pub schema: String,
    pub path: Vec<String>,
    pub offset_expr: String,
    pub width_expr: String,
    pub endian: Endian,
    pub kind: PathKind,
    pub data_offset_expr: Option<String>,
    pub len_offset_expr: Option<String>,
    pub len_width: Option<IntWidth>,
    pub capacity: Option<u32>,
    pub len_endian: Option<Endian>,
    pub unit_width: u32,
    pub array_length: Option<u32>,
    pub array_stride_expr: Option<String>,
    pub optional: bool,
}
#[derive(Clone, Debug)]
pub enum PathKind {
    Scalar,
    Bool,
    String(StringKind),
    FixedBytes,
    Array,
    Nested,
    Tag,
    Payload,
}
#[derive(Clone, Debug)]
pub struct TypeMeta {
    pub schema: String,
    pub cpp: String,
    pub size_expr: String,
    pub align_expr: String,
    pub paths: Vec<PathMeta>,
}
#[derive(Clone, Debug, Default)]
pub struct LayoutMetadata {
    pub types: Vec<TypeMeta>,
}
impl LayoutMetadata {
    pub fn ty(&self, name: &str) -> &TypeMeta {
        self.types
            .iter()
            .find(|x| x.schema == name)
            .unwrap_or_else(|| panic!("missing C++ layout type {name}"))
    }
    pub fn path(&self, schema: &str, path: &[String]) -> &PathMeta {
        self.ty(schema)
            .paths
            .iter()
            .find(|x| x.path == path)
            .unwrap_or_else(|| panic!("missing C++ layout path {schema}.{}", path.join(".")))
    }
}
#[derive(Clone, Debug, Default)]
pub struct LayoutOutput {
    pub declarations: String,
    pub assertions: String,
    pub layout_case_arms: String,
    pub metadata: LayoutMetadata,
}

fn iw(w: IntWidth) -> u32 {
    match w {
        IntWidth::U8 => 1,
        IntWidth::U16 => 2,
        IntWidth::U32 => 4,
    }
}
fn en(e: Endian) -> u64 {
    match e {
        Endian::Native => 0,
        Endian::Little => 1,
        Endian::Big => 2,
    }
}
fn primitive(p: Primitive) -> (&'static str, u32) {
    match p {
        Primitive::U8 => ("std::uint8_t", 1),
        Primitive::I8 => ("std::int8_t", 1),
        Primitive::U16 => ("std::uint16_t", 2),
        Primitive::I16 => ("std::int16_t", 2),
        Primitive::U32 => ("std::uint32_t", 4),
        Primitive::I32 => ("std::int32_t", 4),
        Primitive::U64 => ("std::uint64_t", 8),
        Primitive::I64 => ("std::int64_t", 8),
        Primitive::F32 => ("float", 4),
        Primitive::F64 => ("double", 8),
    }
}
fn raw_primitive(width: u32) -> &'static str {
    match width {
        1 => "std::uint8_t",
        2 => "std::uint16_t",
        4 => "std::uint32_t",
        8 => "std::uint64_t",
        _ => panic!("width"),
    }
}
fn max_expr(a: &str, b: &str) -> String {
    format!("(({a}) > ({b}) ? ({a}) : ({b}))")
}
fn field<'a>(m: &'a Model, s: &str, n: &str) -> &'a Field {
    let SchemaKind::Struct { fields } = &m.schema(s).kind else {
        panic!("field metric on non-struct")
    };
    fields
        .iter()
        .find(|x| x.name == n)
        .unwrap_or_else(|| panic!("missing field {s}.{n}"))
}

fn optional_inner(ty: &Type) -> (&Type, bool) {
    match ty {
        Type::Option(inner) => (inner, true),
        _ => (ty, false),
    }
}

struct Gen<'a> {
    m: &'a Model,
    names: BTreeMap<String, String>,
    helpers: usize,
    decl: String,
    asserts: String,
    metas: Vec<TypeMeta>,
}
impl<'a> Gen<'a> {
    fn fresh(&mut self) -> String {
        let n = format!("z{}", self.helpers);
        self.helpers += 1;
        n
    }
    fn typename(&self, n: &str) -> String {
        self.names.get(n).unwrap().clone()
    }
    fn raw(&mut self, width: u32, endian: Endian) -> String {
        let primitive = raw_primitive(width);
        if width == 1 || endian == Endian::Native {
            return primitive.into();
        }
        let n = self.fresh();
        let align = format!("alignof({primitive})");
        writeln!(
            self.decl,
            "struct alignas({align}) {n} {{ std::uint8_t m0[{width}]; }};"
        )
        .unwrap();
        writeln!(
            self.asserts,
            "static_assert(sizeof({n})=={width} && alignof({n})=={align});"
        )
        .unwrap();
        n
    }
    fn array_element_type(&mut self, element: &Type, endian: Endian) -> (String, u32) {
        match element {
            Type::Primitive(value) => {
                let (base, width) = primitive(*value);
                let ty = if width == 1 || endian == Endian::Native {
                    base.into()
                } else {
                    self.raw(width, endian)
                };
                (ty, width)
            }
            Type::Bool => ("std::uint8_t".into(), 1),
            Type::Schema(name) => match &self.m.schema(name).kind {
                SchemaKind::ScalarEnum { repr, endian, .. } => {
                    (self.raw(iw(*repr), *endian), iw(*repr))
                }
                SchemaKind::Struct { .. } => (self.typename(name), 1),
                SchemaKind::TaggedEnum { .. } => panic!("tagged payload array is unsupported"),
            },
            Type::Option(inner) => self.array_element_type(inner, endian),
            _ => panic!("unsupported fixed-array element"),
        }
    }
    fn lower_field(&mut self, schema: &str, index: usize, f: &Field) -> (String, String, PathMeta) {
        let member = format!("m{index}");
        let (field_ty, optional) = optional_inner(&f.ty);
        let (mut ty, kind, unit, data, lenoff, lenw): (
            String,
            PathKind,
            u32,
            Option<String>,
            Option<String>,
            Option<IntWidth>,
        ) = match field_ty {
            Type::Primitive(p) => {
                let (base, w) = primitive(*p);
                let t = if w == 1 || f.endian == Endian::Native {
                    base.into()
                } else {
                    self.raw(w, f.endian)
                };
                (t, PathKind::Scalar, w, None, None, None)
            }
            Type::Bool => ("std::uint8_t".into(), PathKind::Bool, 1, None, None, None),
            Type::FixedBytes(n) => {
                let h = self.fresh();
                writeln!(self.decl, "struct {h} {{ std::uint8_t m0[{n}]; }};").unwrap();
                (h, PathKind::FixedBytes, 1, None, None, None)
            }
            Type::Array { element, length } => {
                let (element_type, width) = self.array_element_type(element, f.endian);
                let h = self.fresh();
                writeln!(self.decl, "struct {h} {{ {element_type} m0[{length}]; }};").unwrap();
                (h, PathKind::Array, width, None, None, None)
            }
            Type::Schema(n) => {
                let child = self.m.schema(n);
                match &child.kind {
                    SchemaKind::ScalarEnum { repr, endian, .. } => (
                        self.raw(iw(*repr), *endian),
                        PathKind::Scalar,
                        iw(*repr),
                        None,
                        None,
                        None,
                    ),
                    SchemaKind::TaggedEnum { .. } if f.tag_field.is_some() => {
                        (self.typename(n), PathKind::Payload, 1, None, None, None)
                    }
                    _ => (self.typename(n), PathKind::Nested, 1, None, None, None),
                }
            }
            Type::String(k) => {
                let cap = f.capacity.unwrap();
                if *k == StringKind::CBytes || *k == StringKind::U16C {
                    let h = self.fresh();
                    let dt = if *k == StringKind::U16C {
                        "std::uint16_t"
                    } else {
                        "std::uint8_t"
                    };
                    writeln!(self.decl, "struct {h} {{ {dt} m0[{cap}]; }};").unwrap();
                    (
                        h,
                        PathKind::String(*k),
                        if *k == StringKind::U16C { 2 } else { 1 },
                        Some("0".into()),
                        None,
                        None,
                    )
                } else {
                    let (lw, le) = f.len.unwrap();
                    let h = self.fresh();
                    let lt = self.raw(iw(lw), le);
                    let dt = if *k == StringKind::U16 {
                        "std::uint16_t"
                    } else {
                        "std::uint8_t"
                    };
                    writeln!(self.decl, "struct {h} {{ {lt} m0; {dt} m1[{cap}]; }};").unwrap();
                    writeln!(
                        self.asserts,
                        "static_assert(offsetof({h},m0)==0 && offsetof({h},m1)>=sizeof({lt}));"
                    )
                    .unwrap();
                    (
                        h.clone(),
                        PathKind::String(*k),
                        if *k == StringKind::U16 { 2 } else { 1 },
                        Some(format!("offsetof({h},m1)")),
                        Some("0".into()),
                        Some(lw),
                    )
                }
            }
            Type::Option(_) => unreachable!("optional type is lowered to its inner storage"),
        };
        let array_metadata = match field_ty {
            Type::Array { length, .. } => Some((*length, format!("sizeof({ty}::m0[0])"))),
            _ => None,
        };
        if let Some(a) = f.align {
            let w = self.fresh();
            let al = max_expr(&a.to_string(), &format!("alignof({ty})"));
            writeln!(self.decl, "struct alignas({al}) {w} {{ {ty} m0; }};").unwrap();
            writeln!(self.asserts,"static_assert(alignof({w})=={al} && sizeof({w})%alignof({w})==0 && offsetof({w},m0)==0);").unwrap();
            ty = w;
        }
        let base = format!("offsetof({}, {member})", self.typename(schema));
        let meta = PathMeta {
            schema: schema.into(),
            path: vec![f.name.clone()],
            offset_expr: base.clone(),
            width_expr: format!("sizeof({ty})"),
            endian: f.endian,
            kind,
            data_offset_expr: data.map(|x| format!("({base})+({x})")),
            len_offset_expr: lenoff.map(|x| format!("({base})+({x})")),
            len_width: lenw,
            len_endian: f.len.map(|x| x.1),
            capacity: f.capacity,
            unit_width: unit,
            array_length: array_metadata.as_ref().map(|(length, _)| *length),
            array_stride_expr: array_metadata.map(|(_, stride)| stride),
            optional,
        };
        (member, ty, meta)
    }
    fn schema(&mut self, n: &str) {
        let s = self.m.schema(n).clone();
        let cpp = self.typename(n);
        match s.kind {
            SchemaKind::ScalarEnum { repr, endian, .. } => {
                let r = self.raw(iw(repr), endian);
                writeln!(self.decl, "using {cpp} = {r};").unwrap();
                self.metas.push(TypeMeta {
                    schema: n.into(),
                    cpp: cpp.clone(),
                    size_expr: format!("sizeof({cpp})"),
                    align_expr: format!("alignof({cpp})"),
                    paths: vec![],
                });
            }
            SchemaKind::Struct { fields } => {
                let mut lowered = Vec::new();
                for (index, field) in fields.iter().enumerate() {
                    lowered.push(self.lower_field(n, index, field));
                }
                let alignment = s.align.unwrap_or(1);
                let alignment_attribute = if alignment > 1 {
                    format!(" alignas({alignment})")
                } else {
                    String::new()
                };
                writeln!(self.decl, "struct{alignment_attribute} {cpp} {{").unwrap();
                for (member, ty, _) in &lowered {
                    writeln!(self.decl, "  {ty} {member};").unwrap();
                }
                writeln!(self.decl, "}};").unwrap();
                let mut paths = Vec::new();
                for (member, ty, meta) in lowered {
                    writeln!(
                        self.asserts,
                        "static_assert(offsetof({cpp},{member}) < sizeof({cpp}));"
                    )
                    .unwrap();
                    if meta.optional {
                        writeln!(
                            self.asserts,
                            "static_assert(std::is_same<decltype({cpp}::{member}),{ty}>::value);"
                        )
                        .unwrap();
                        writeln!(
                            self.asserts,
                            "static_assert(sizeof(decltype({cpp}::{member}))==sizeof({ty}));"
                        )
                        .unwrap();
                        writeln!(
                            self.asserts,
                            "static_assert(alignof(decltype({cpp}::{member}))==alignof({ty}));"
                        )
                        .unwrap();
                    }
                    paths.push(meta);
                }
                writeln!(self.asserts,"static_assert(std::is_standard_layout<{cpp}>::value && std::is_trivially_copyable<{cpp}>::value);").unwrap();
                self.metas.push(TypeMeta {
                    schema: n.into(),
                    cpp: cpp.clone(),
                    size_expr: format!("sizeof({cpp})"),
                    align_expr: format!("alignof({cpp})"),
                    paths,
                });
            }
            SchemaKind::TaggedEnum { variants, .. } => {
                let alignment_attribute = s
                    .align
                    .map(|alignment| format!(" alignas({alignment})"))
                    .unwrap_or_default();
                writeln!(self.decl, "union{alignment_attribute} {cpp} {{").unwrap();
                for (index, variant) in variants.iter().enumerate() {
                    match &variant.payload {
                        Some(payload) => {
                            writeln!(self.decl, "  {} m{index};", self.typename(payload)).unwrap();
                        }
                        None => {
                            let unit = self.fresh();
                            writeln!(
                                self.decl,
                                "  struct {unit} {{ std::uint8_t value; }} m{index};"
                            )
                            .unwrap();
                        }
                    }
                }
                writeln!(self.decl, "}};").unwrap();
                writeln!(self.asserts,"static_assert(sizeof({cpp})>0 && std::is_standard_layout<{cpp}>::value && std::is_trivially_copyable<{cpp}>::value);").unwrap();
                self.metas.push(TypeMeta {
                    schema: n.into(),
                    cpp: cpp.clone(),
                    size_expr: format!("sizeof({cpp})"),
                    align_expr: format!("alignof({cpp})"),
                    paths: vec![PathMeta {
                        schema: n.into(),
                        path: vec!["$payload".into()],
                        offset_expr: "0".into(),
                        width_expr: format!("sizeof({cpp})"),
                        endian: Endian::Native,
                        kind: PathKind::Payload,
                        data_offset_expr: None,
                        len_offset_expr: None,
                        len_width: None,
                        len_endian: None,
                        capacity: None,
                        unit_width: 1,
                        array_length: None,
                        array_stride_expr: None,
                        optional: false,
                    }],
                });
            }
        }
    }
}

pub fn emit(model: &Model) -> LayoutOutput {
    let names = model
        .schemas
        .iter()
        .enumerate()
        .map(|(i, s)| (s.name.clone(), format!("t{i}")))
        .collect();
    let mut g = Gen {
        m: model,
        names,
        helpers: 0,
        decl: String::new(),
        asserts: String::new(),
        metas: Vec::new(),
    };
    for s in &model.schemas {
        g.schema(&s.name);
    }
    let snapshot = g.metas.clone();
    for tm in &mut g.metas {
        let Some(schema) = model.schemas.iter().find(|s| s.name == tm.schema) else {
            continue;
        };
        let SchemaKind::Struct { fields } = &schema.kind else {
            continue;
        };
        let direct = tm.paths.clone();
        for f in fields {
            let (field_ty, _) = optional_inner(&f.ty);
            let Type::Schema(child) = field_ty else {
                continue;
            };
            if f.tag_field.is_some() {
                continue;
            }
            let Some(base) = direct.iter().find(|p| p.path == [f.name.clone()]) else {
                continue;
            };
            let Some(cm) = snapshot.iter().find(|x| x.schema == *child) else {
                continue;
            };
            for cp in &cm.paths {
                if cp.path.is_empty() {
                    continue;
                }
                let mut p = cp.clone();
                p.schema = tm.schema.clone();
                p.path.insert(0, f.name.clone());
                p.offset_expr = format!("({})+({})", base.offset_expr, cp.offset_expr);
                p.data_offset_expr = cp
                    .data_offset_expr
                    .as_ref()
                    .map(|x| format!("({})+({x})", base.offset_expr));
                p.len_offset_expr = cp
                    .len_offset_expr
                    .as_ref()
                    .map(|x| format!("({})+({x})", base.offset_expr));
                p.optional = base.optional || p.optional;
                tm.paths.push(p);
            }
        }
    }
    let metadata = LayoutMetadata { types: g.metas };
    let mut arms = String::new();
    for c in &model.cases {
        let Root::Schema(name) = &c.root;
        let root = metadata.ty(name);
        writeln!(arms, "case {}: {{", c.id).unwrap();
        for e in &c.layout {
            let v = metric_expr(model, &metadata, root, &e.metric);
            writeln!(
                arms,
                "  append({}, static_cast<std::uint64_t>({v}));",
                e.key
            )
            .unwrap();
        }
        writeln!(arms, "  break; }}").unwrap();
    }
    LayoutOutput {
        declarations: g.decl,
        assertions: g.asserts,
        layout_case_arms: arms,
        metadata,
    }
}

fn metric_expr(m: &Model, md: &LayoutMetadata, root: &TypeMeta, x: &Metric) -> String {
    match x {
        Metric::RootSize | Metric::RootStride => root.size_expr.clone(),
        Metric::RootAlign => root.align_expr.clone(),
        Metric::FieldOffset(n) => md
            .path(&root.schema, std::slice::from_ref(n))
            .offset_expr
            .clone(),
        Metric::FieldSize(n) => md
            .path(&root.schema, std::slice::from_ref(n))
            .width_expr
            .clone(),
        Metric::FieldAlign(n) => {
            let field = field(m, &root.schema, n);
            field
                .align
                .map(|alignment| alignment.to_string())
                .unwrap_or_else(|| {
                    format!(
                        "alignof(decltype({}::m{}))",
                        root.cpp,
                        field_index(m, &root.schema, n)
                    )
                })
        }
        Metric::ArrayLength(n) => {
            let Type::Array { length, .. } = &field(m, &root.schema, n).ty else {
                panic!("array length metric requires an array field")
            };
            length.to_string()
        }
        Metric::ArrayStride(n) => md
            .path(&root.schema, std::slice::from_ref(n))
            .array_stride_expr
            .clone()
            .unwrap_or_else(|| panic!("array stride metric requires an array field")),
        Metric::OptionalSpan(n) => md
            .path(&root.schema, std::slice::from_ref(n))
            .width_expr
            .clone(),
        Metric::OptionalIsOptional(n) => {
            u64::from(md.path(&root.schema, std::slice::from_ref(n)).optional).to_string()
        }
        Metric::OptionalArrayLength(n) => {
            let p = md.path(&root.schema, std::slice::from_ref(n));
            if !p.optional {
                panic!("optional array length metric requires an optional field")
            }
            p.array_length
                .expect("optional array length metric requires an array field")
                .to_string()
        }
        Metric::OptionalArrayStride(n) => {
            let p = md.path(&root.schema, std::slice::from_ref(n));
            if !p.optional {
                panic!("optional array stride metric requires an optional field")
            }
            p.array_stride_expr
                .clone()
                .expect("optional array stride metric requires an array field")
        }
        Metric::EnumSize(n) => {
            let e = enum_field(m, &root.schema, n);
            md.ty(e).size_expr.clone()
        }
        Metric::EnumAlign(n) => {
            let e = enum_field(m, &root.schema, n);
            md.ty(e).align_expr.clone()
        }
        Metric::EnumWidth(n) => {
            let e = enum_field(m, &root.schema, n);
            let SchemaKind::ScalarEnum { repr, .. } = m.schema(e).kind else {
                panic!()
            };
            iw(repr).to_string()
        }
        Metric::EnumEndian(n) => {
            let e = enum_field(m, &root.schema, n);
            let SchemaKind::ScalarEnum { endian, .. } = m.schema(e).kind else {
                panic!()
            };
            en(endian).to_string()
        }
        Metric::EnumRaw(n) => {
            let e = enum_field(m, &root.schema, n);
            let SchemaKind::ScalarEnum { variants, .. } = &m.schema(e).kind else {
                panic!()
            };
            variants[0].raw.to_string()
        }
        Metric::StringEncoding(n) => {
            let Type::String(k) = field(m, &root.schema, n).ty else {
                panic!()
            };
            match k {
                StringKind::Utf8 => 1,
                StringKind::CBytes => 2,
                StringKind::U16 => 3,
                StringKind::U16C => 4,
            }
            .to_string()
        }
        Metric::StringCapacity(n) => field(m, &root.schema, n).capacity.unwrap().to_string(),
        Metric::StringUnitWidth(n) => md
            .path(&root.schema, std::slice::from_ref(n))
            .unit_width
            .to_string(),
        Metric::StringDataOffset(n) => {
            let p = md.path(&root.schema, std::slice::from_ref(n));
            format!(
                "({})-({})",
                p.data_offset_expr.as_ref().unwrap(),
                p.offset_expr
            )
        }
        Metric::StringLengthWidth(n) => field(m, &root.schema, n)
            .len
            .map(|x| iw(x.0))
            .unwrap_or(0)
            .to_string(),
        Metric::StringLengthEndian(n) => field(m, &root.schema, n)
            .len
            .map(|x| en(x.1))
            .unwrap_or(0)
            .to_string(),
        Metric::StringLengthOffset(_) => "0".into(),
        Metric::TagOffset => {
            let (_, tag_field) = tagged_context(m, &root.schema);
            md.path(&root.schema, &[tag_field]).offset_expr.clone()
        }
        Metric::PayloadOffset => root
            .paths
            .iter()
            .find(|path| matches!(path.kind, PathKind::Payload))
            .unwrap()
            .offset_expr
            .clone(),
        Metric::PayloadSize => root
            .paths
            .iter()
            .find(|path| matches!(path.kind, PathKind::Payload))
            .unwrap()
            .width_expr
            .clone(),
        Metric::PayloadAlign => payload_align_expr(m, root),
        Metric::TagSize => {
            let (_, tag_field) = tagged_context(m, &root.schema);
            md.path(&root.schema, &[tag_field]).width_expr.clone()
        }
        Metric::TagAlign => {
            let (_, tag_field) = tagged_context(m, &root.schema);
            format!(
                "alignof(decltype({}::m{}))",
                root.cpp,
                field_index(m, &root.schema, &tag_field)
            )
        }
        Metric::TagEndian => {
            let (tagged, _) = tagged_context(m, &root.schema);
            let SchemaKind::TaggedEnum { tag, .. } = &m.schema(&tagged).kind else {
                panic!("tagged payload metadata required")
            };
            let SchemaKind::ScalarEnum { endian, .. } = &m.schema(tag).kind else {
                panic!("scalar tag metadata required")
            };
            en(*endian).to_string()
        }
        Metric::VariantRaw(v) => {
            let (ts, _) = tagged_context(m, &root.schema);
            let SchemaKind::TaggedEnum { tag, .. } = &m.schema(&ts).kind else {
                panic!()
            };
            let SchemaKind::ScalarEnum { variants, .. } = &m.schema(tag).kind else {
                panic!()
            };
            variants
                .iter()
                .find(|x| x.name == *v)
                .unwrap()
                .raw
                .to_string()
        }
        Metric::VariantPayloadSize(v) => {
            let (ts, _) = tagged_context(m, &root.schema);
            variant_payload(m, md, &ts, v, false)
        }
        Metric::VariantPayloadAlign(v) => {
            let (ts, _) = tagged_context(m, &root.schema);
            variant_payload(m, md, &ts, v, true)
        }
    }
}
fn field_index(m: &Model, s: &str, n: &str) -> usize {
    let SchemaKind::Struct { fields } = &m.schema(s).kind else {
        panic!()
    };
    fields.iter().position(|field| field.name == n).unwrap()
}
fn enum_field<'a>(m: &'a Model, s: &str, n: &str) -> &'a str {
    let Type::Schema(enum_name) = &field(m, s, n).ty else {
        panic!("enum metric requires schema field")
    };
    enum_name
}
fn tagged_context(m: &Model, s: &str) -> (String, String) {
    let SchemaKind::Struct { fields } = &m.schema(s).kind else {
        panic!("tag metric requires an external-union record")
    };
    let payload = fields
        .iter()
        .find(|field| field.tag_field.is_some())
        .unwrap_or_else(|| panic!("tag metric on untagged record"));
    let Type::Schema(tagged) = &payload.ty else {
        panic!("tagged payload schema required")
    };
    (tagged.clone(), payload.tag_field.clone().unwrap())
}
fn payload_align_expr(m: &Model, root: &TypeMeta) -> String {
    let payload = root
        .paths
        .iter()
        .find(|path| matches!(path.kind, PathKind::Payload))
        .unwrap();
    field(m, &root.schema, &payload.path[0])
        .align
        .map(|alignment| alignment.to_string())
        .unwrap_or_else(|| {
            format!(
                "alignof(decltype({}::m{}))",
                root.cpp,
                field_index(m, &root.schema, &payload.path[0])
            )
        })
}
fn variant_payload(m: &Model, md: &LayoutMetadata, s: &str, v: &str, align: bool) -> String {
    let SchemaKind::TaggedEnum { variants, .. } = &m.schema(s).kind else {
        panic!()
    };
    match variants
        .iter()
        .find(|x| x.name == v)
        .unwrap()
        .payload
        .as_ref()
    {
        None => "1".into(),
        Some(p) => {
            if align {
                md.ty(p).align_expr.clone()
            } else {
                md.ty(p).size_expr.clone()
            }
        }
    }
}
