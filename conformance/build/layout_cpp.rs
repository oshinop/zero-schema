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
    pub tail_zero: bool,
}
#[derive(Clone, Debug)]
pub enum PathKind {
    Scalar,
    Bool,
    String(StringKind),
    FixedBytes,
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
    fn lower_field(&mut self, schema: &str, index: usize, f: &Field) -> (String, String, PathMeta) {
        let member = format!("m{index}");
        let (mut ty, kind, unit, data, lenoff, lenw): (
            String,
            PathKind,
            u32,
            Option<String>,
            Option<String>,
            Option<IntWidth>,
        ) = match &f.ty {
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
                    SchemaKind::TaggedEnum { variants, .. } if f.tag_field.is_some() => {
                        let nonzero: Vec<_> =
                            variants.iter().filter_map(|v| v.payload.as_ref()).collect();
                        if nonzero.is_empty() {
                            ("".into(), PathKind::Payload, 1, None, None, None)
                        } else {
                            let u = self.fresh();
                            writeln!(self.decl, "union {u} {{").unwrap();
                            for (j, p) in nonzero.iter().enumerate() {
                                writeln!(self.decl, "  {} m{j};", self.typename(p)).unwrap();
                            }
                            writeln!(self.decl, "}};").unwrap();
                            (u, PathKind::Payload, 1, None, None, None)
                        }
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
        };
        if let Some(a) = f.align {
            if !ty.is_empty() {
                let w = self.fresh();
                let al = max_expr(&a.to_string(), &format!("alignof({ty})"));
                writeln!(self.decl, "struct alignas({al}) {w} {{ {ty} m0; }};").unwrap();
                writeln!(self.asserts,"static_assert(alignof({w})=={al} && sizeof({w})%alignof({w})==0 && offsetof({w},m0)==0);").unwrap();
                ty = w;
            }
        }
        let base = format!("offsetof({}, {member})", self.typename(schema));
        let meta = PathMeta {
            schema: schema.into(),
            path: vec![f.name.clone()],
            offset_expr: base.clone(),
            width_expr: if ty.is_empty() {
                "0".into()
            } else {
                format!("sizeof({ty})")
            },
            endian: f.endian,
            kind,
            data_offset_expr: data.map(|x| format!("({base})+({x})")),
            len_offset_expr: lenoff.map(|x| format!("({base})+({x})")),
            len_width: lenw,
            len_endian: f.len.map(|x| x.1),
            capacity: f.capacity,
            unit_width: unit,
            tail_zero: f.tail_zero,
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
                for (i, f) in fields.iter().enumerate() {
                    lowered.push(self.lower_field(n, i, f));
                }
                let trailing = lowered
                    .last()
                    .and_then(|x| {
                        if x.1.is_empty() {
                            fields.last().and_then(|f| f.align)
                        } else {
                            None
                        }
                    })
                    .unwrap_or(1);
                let root_align = s.align.unwrap_or(1).max(trailing);
                let al = if root_align > 1 {
                    format!(" alignas({root_align})")
                } else {
                    String::new()
                };
                writeln!(self.decl, "struct{al} {cpp} {{").unwrap();
                let mut pending = 1;
                for (i, (member, ty, _)) in lowered.iter().enumerate() {
                    if ty.is_empty() {
                        pending = pending.max(fields[i].align.unwrap_or(1));
                    } else {
                        if pending > 1 {
                            writeln!(self.decl, "  alignas({pending}) {ty} {member};").unwrap();
                        } else {
                            writeln!(self.decl, "  {ty} {member};").unwrap();
                        }
                        pending = 1;
                    }
                }
                writeln!(self.decl, "}};").unwrap();
                let mut paths = Vec::new();
                for (i, (member, ty, mut meta)) in lowered.into_iter().enumerate() {
                    if !ty.is_empty() {
                        writeln!(
                            self.asserts,
                            "static_assert(offsetof({cpp},{member}) < sizeof({cpp}));"
                        )
                        .unwrap();
                    } else {
                        let next=(i+1..fields.len()).find(|j|!matches!(&fields[*j].ty,Type::Schema(q) if matches!(&self.m.schema(q).kind,SchemaKind::TaggedEnum{variants,..} if fields[*j].tag_field.is_some()&&variants.iter().all(|v|v.payload.is_none()))));
                        meta.offset_expr = next
                            .map(|j| format!("offsetof({cpp},m{j})"))
                            .unwrap_or_else(|| format!("sizeof({cpp})"));
                    }
                    paths.push(meta)
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
            SchemaKind::TaggedEnum { tag, variants } => {
                let tagty = self.typename(&tag);
                let u = self.fresh();
                let payloads: Vec<_> = variants.iter().filter_map(|v| v.payload.as_ref()).collect();
                if !payloads.is_empty() {
                    writeln!(self.decl, "union {u} {{").unwrap();
                    for (j, p) in payloads.iter().enumerate() {
                        writeln!(self.decl, "  {} m{j};", self.typename(p)).unwrap();
                    }
                    writeln!(self.decl, "}};").unwrap();
                }
                let al = s
                    .align
                    .map(|x| format!(" alignas({x})"))
                    .unwrap_or_default();
                writeln!(
                    self.decl,
                    "struct{al} {cpp} {{ {tagty} m0; {} }};",
                    if payloads.is_empty() {
                        String::new()
                    } else {
                        format!("{u} m1;")
                    }
                )
                .unwrap();
                writeln!(
                    self.asserts,
                    "static_assert(offsetof({cpp},m0)==0 && std::is_standard_layout<{cpp}>::value);"
                )
                .unwrap();
                let mut paths = vec![PathMeta {
                    schema: n.into(),
                    path: vec![],
                    offset_expr: format!("offsetof({cpp},m0)"),
                    width_expr: format!("sizeof({tagty})"),
                    endian: Endian::Native,
                    kind: PathKind::Tag,
                    data_offset_expr: None,
                    len_offset_expr: None,
                    len_width: None,
                    capacity: None,
                    unit_width: 1,
                    tail_zero: false,
                    len_endian: None,
                }];
                if !payloads.is_empty() {
                    paths.push(PathMeta {
                        schema: n.into(),
                        path: vec!["$payload".into()],
                        offset_expr: format!("offsetof({cpp},m1)"),
                        width_expr: format!("sizeof({u})"),
                        endian: Endian::Native,
                        kind: PathKind::Payload,
                        data_offset_expr: None,
                        len_offset_expr: None,
                        len_width: None,
                        len_endian: None,
                        capacity: None,
                        unit_width: 1,
                        tail_zero: s.tail_zero,
                    });
                }
                self.metas.push(TypeMeta {
                    schema: n.into(),
                    cpp: cpp.clone(),
                    size_expr: format!("sizeof({cpp})"),
                    align_expr: format!("alignof({cpp})"),
                    paths,
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
    // The ABI-only graph deliberately omits zero-size members and carries their alignment to the next real member.
    for (i, a) in model.abi_structs.iter().enumerate() {
        let cpp = format!("a{i}");
        if a.fields.is_empty() {
            continue;
        }
        let natural = a
            .fields
            .iter()
            .filter_map(|f| {
                model
                    .abi_structs
                    .iter()
                    .find(|x| x.name == f.ty && x.fields.is_empty())
                    .and_then(|z| z.align)
            })
            .max()
            .unwrap_or(1);
        let al = a.align.unwrap_or(1).max(natural);
        writeln!(g.decl, "struct alignas({al}) {cpp} {{").unwrap();
        let mut pending = 1u32;
        for (j, f) in a.fields.iter().enumerate() {
            if let Some(z) = model
                .abi_structs
                .iter()
                .find(|x| x.name == f.ty && x.fields.is_empty())
            {
                pending = pending.max(z.align.unwrap_or(1));
                continue;
            }
            let ty = match f.ty.as_str() {
                "u8" => "std::uint8_t",
                "u32" => "std::uint32_t",
                _ => panic!("unsupported ABI field type"),
            };
            if pending > 1 {
                writeln!(g.decl, "  alignas({pending}) {ty} m{j};").unwrap()
            } else {
                writeln!(g.decl, "  {ty} m{j};").unwrap()
            };
            pending = 1;
        }
        writeln!(g.decl, "}};").unwrap();
        writeln!(
            g.asserts,
            "static_assert(std::is_standard_layout<{cpp}>::value && alignof({cpp})=={al});"
        )
        .unwrap();
        let mut paths = Vec::new();
        for (j, f) in a.fields.iter().enumerate() {
            let z = model
                .abi_structs
                .iter()
                .find(|x| x.name == f.ty && x.fields.is_empty());
            let (off, wid, unit) = if let Some(z) = z {
                let next = (j + 1..a.fields.len()).find(|k| {
                    !model
                        .abi_structs
                        .iter()
                        .any(|x| x.name == a.fields[*k].ty && x.fields.is_empty())
                });
                (
                    next.map(|k| format!("offsetof({cpp},m{k})"))
                        .unwrap_or_else(|| format!("sizeof({cpp})")),
                    "0".into(),
                    z.align.unwrap_or(1),
                )
            } else {
                (
                    format!("offsetof({cpp},m{j})"),
                    match f.ty.as_str() {
                        "u8" => "1",
                        "u32" => "4",
                        _ => unreachable!(),
                    }
                    .into(),
                    1,
                )
            };
            paths.push(PathMeta {
                schema: a.name.clone(),
                path: vec![f.name.clone()],
                offset_expr: off,
                width_expr: wid,
                endian: Endian::Native,
                kind: if z.is_some() {
                    PathKind::Nested
                } else {
                    PathKind::Scalar
                },
                data_offset_expr: None,
                len_offset_expr: None,
                len_width: None,
                len_endian: None,
                capacity: None,
                unit_width: unit,
                tail_zero: false,
            });
        }
        g.metas.push(TypeMeta {
            schema: a.name.clone(),
            cpp: cpp.clone(),
            size_expr: format!("sizeof({cpp})"),
            align_expr: format!("alignof({cpp})"),
            paths,
        });
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
            let Type::Schema(child) = &f.ty else { continue };
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
                tm.paths.push(p);
            }
        }
    }
    let metadata = LayoutMetadata { types: g.metas };
    let mut arms = String::new();
    for c in &model.cases {
        let root = match &c.root {
            Root::Schema(n) | Root::Abi(n) => metadata.ty(n),
        };
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
            if model_has_schema(m, &root.schema) {
                let f = field(m, &root.schema, n);
                if let Some(a) = f.align {
                    a.to_string()
                } else {
                    format!(
                        "alignof(decltype({}::m{}))",
                        root.cpp,
                        field_index(m, &root.schema, n)
                    )
                }
            } else {
                let p = md.path(&root.schema, std::slice::from_ref(n));
                if p.width_expr == "0" {
                    p.unit_width.to_string()
                } else {
                    format!(
                        "alignof(decltype({}::m{}))",
                        root.cpp,
                        field_index_abi(m, &root.schema, n)
                    )
                }
            }
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
        Metric::StringTail(n) => u64::from(field(m, &root.schema, n).tail_zero).to_string(),
        Metric::TagOffset => {
            let (ts, tag_field) = tagged_context(m, &root.schema);
            if let Some(n) = tag_field {
                md.path(&root.schema, &[n]).offset_expr.clone()
            } else {
                md.ty(&ts)
                    .paths
                    .iter()
                    .find(|p| matches!(p.kind, PathKind::Tag))
                    .unwrap()
                    .offset_expr
                    .clone()
            }
        }
        Metric::PayloadOffset => root
            .paths
            .iter()
            .find(|p| matches!(p.kind, PathKind::Payload))
            .map(|p| p.offset_expr.clone())
            .unwrap_or_else(|| "0".into()),
        Metric::PayloadSize => root
            .paths
            .iter()
            .find(|p| matches!(p.kind, PathKind::Payload))
            .map(|p| p.width_expr.clone())
            .unwrap_or_else(|| "0".into()),
        Metric::PayloadAlign => payload_align_expr(m, root),
        Metric::TagSize => {
            let (ts, tf) = tagged_context(m, &root.schema);
            if let Some(n) = tf {
                md.path(&root.schema, &[n]).width_expr.clone()
            } else {
                md.ty(&ts)
                    .paths
                    .iter()
                    .find(|p| matches!(p.kind, PathKind::Tag))
                    .unwrap()
                    .width_expr
                    .clone()
            }
        }
        Metric::TagAlign => {
            let (_, tf) = tagged_context(m, &root.schema);
            if let Some(n) = tf {
                format!(
                    "alignof(decltype({}::m{}))",
                    root.cpp,
                    field_index(m, &root.schema, &n)
                )
            } else {
                format!("alignof(decltype({}::m0))", root.cpp)
            }
        }
        Metric::TagEndian => "0".into(),
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
fn model_has_schema(m: &Model, s: &str) -> bool {
    m.schemas.iter().any(|x| x.name == s)
}
fn field_index_abi(m: &Model, s: &str, n: &str) -> usize {
    m.abi(s).fields.iter().position(|x| x.name == n).unwrap()
}
fn field_index(m: &Model, s: &str, n: &str) -> usize {
    let SchemaKind::Struct { fields } = &m.schema(s).kind else {
        panic!()
    };
    fields.iter().position(|x| x.name == n).unwrap()
}
fn enum_field<'a>(m: &'a Model, s: &str, n: &str) -> &'a str {
    let Type::Schema(e) = &field(m, s, n).ty else {
        panic!("enum metric requires schema field")
    };
    e
}
fn tagged_context(m: &Model, s: &str) -> (String, Option<String>) {
    match &m.schema(s).kind {
        SchemaKind::TaggedEnum { .. } => (s.to_owned(), None),
        SchemaKind::Struct { fields } => {
            let p = fields
                .iter()
                .find(|f| f.tag_field.is_some())
                .unwrap_or_else(|| panic!("tag metric on untagged struct"));
            let Type::Schema(t) = &p.ty else { panic!() };
            (t.clone(), p.tag_field.clone())
        }
        _ => panic!("tag metric on unsupported root"),
    }
}
fn payload_align_expr(m: &Model, root: &TypeMeta) -> String {
    let p = root
        .paths
        .iter()
        .find(|p| matches!(p.kind, PathKind::Payload))
        .unwrap();
    match &m.schema(&root.schema).kind {
        SchemaKind::TaggedEnum { .. } => {
            if p.width_expr == "0" {
                "1".into()
            } else {
                format!("alignof(decltype({}::m1))", root.cpp)
            }
        }
        SchemaKind::Struct { .. } => field(m, &root.schema, &p.path[0])
            .align
            .map(|x| x.to_string())
            .unwrap_or_else(|| {
                if p.width_expr == "0" {
                    "1".into()
                } else {
                    format!(
                        "alignof(decltype({}::m{}))",
                        root.cpp,
                        field_index(m, &root.schema, &p.path[0])
                    )
                }
            }),
        _ => panic!(),
    }
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
        None => if align { "1" } else { "0" }.into(),
        Some(p) => {
            if align {
                md.ty(p).align_expr.clone()
            } else {
                md.ty(p).size_expr.clone()
            }
        }
    }
}
