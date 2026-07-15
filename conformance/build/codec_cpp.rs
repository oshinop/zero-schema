use std::fmt::Write as _;

use super::frontend::{
    Endian, Field, IntWidth, Model, Observation, Root, SchemaKind, StringKind, Type, Value,
};
use super::layout_cpp::{LayoutOutput, PathKind, PathMeta};

fn endian(e: Endian) -> &'static str {
    match e {
        Endian::Native => "E_NATIVE",
        Endian::Little => "E_LITTLE",
        Endian::Big => "E_BIG",
    }
}
fn iw(w: IntWidth) -> u32 {
    match w {
        IntWidth::U8 => 1,
        IntWidth::U16 => 2,
        IntWidth::U32 => 4,
    }
}
fn meta<'a>(layout: &'a LayoutOutput, schema: &str, path: &[String]) -> &'a PathMeta {
    layout.metadata.path(schema, path) /*
     */
}
fn off(m: &PathMeta) -> String {
    format!("({})", m.offset_expr)
}
fn at(base: &str, m: &PathMeta) -> String {
    format!("checked_add({}, {}, &pos)", base, off(m))
}

fn optional_inner(ty: &Type) -> (&Type, bool) {
    match ty {
        Type::Option(inner) => (inner, true),
        _ => (ty, false),
    }
}

fn emit_scalar_write(out: &mut String, base: &str, m: &PathMeta, value: u64) {
    writeln!(
        out,
        "if(!put_bits(stage, byte_count, {}, {}, UINT64_C({}), {})) return INT;",
        at(base, m),
        m.unit_width,
        value,
        endian(m.endian)
    )
    .unwrap();
}
fn emit_array_element(
    out: &mut String,
    model: &Model,
    layout: &LayoutOutput,
    element: &Type,
    element_meta: &PathMeta,
    base: &str,
    value: &Value,
) {
    match (element, value) {
        (Type::Primitive(_), Value::Bits(bits)) => {
            emit_scalar_write(out, base, element_meta, *bits)
        }
        (Type::Bool, Value::Boolean(value)) => {
            emit_scalar_write(out, base, element_meta, u64::from(*value))
        }
        (Type::Schema(child), Value::Variant { variant, .. }) => {
            let SchemaKind::ScalarEnum {
                repr,
                endian: enum_endian,
                variants,
            } = &model.schema(child).kind
            else {
                panic!("array enum element required")
            };
            let raw = variants
                .iter()
                .find(|candidate| candidate.name == *variant)
                .unwrap_or_else(|| panic!("unknown array enum variant"))
                .raw;
            let position = at(base, element_meta);
            writeln!(
                out,
                "if(!put_bits(stage,byte_count,{position},{},UINT64_C({}),{}))return INT;",
                iw(*repr),
                raw,
                endian(*enum_endian)
            )
            .unwrap();
        }
        (Type::Schema(child), nested) => {
            let position = at(base, element_meta);
            emit_value(out, model, layout, child, &position, nested)
        }
        _ => panic!("array value/type mismatch"),
    }
}
fn emit_value(
    out: &mut String,
    model: &Model,
    layout: &LayoutOutput,
    schema: &str,
    base: &str,
    value: &Value,
) {
    let s = model.schema(schema);
    match (&s.kind, value) {
        (SchemaKind::Struct { fields }, Value::Record(values)) => {
            for f in fields {
                let v = &values
                    .iter()
                    .find(|(n, _)| n == &f.name)
                    .unwrap_or_else(|| panic!("missing value {}.{}", schema, f.name))
                    .1;
                let p = vec![f.name.clone()];
                let m = meta(layout, schema, &p);
                let b = at(base, m);
                let (field_ty, v) = match (&f.ty, v) {
                    (Type::Option(_), Value::None) => {
                        writeln!(
                            out,
                            "if(!clear_span(stage,byte_count,{b},static_cast<std::size_t>({})))return INT;",
                            m.width_expr
                        )
                        .unwrap();
                        continue;
                    }
                    (Type::Option(inner), Value::Some(value)) => (inner.as_ref(), value.as_ref()),
                    (Type::Option(_), _) => {
                        panic!("optional value must be none or some at {schema}.{}", f.name)
                    }
                    (field_ty, value) => (field_ty, value),
                };
                match (field_ty, v) {
                    (Type::Primitive(_), Value::Bits(x)) => emit_scalar_write(out, base, m, *x),
                    (Type::Bool, Value::Boolean(x)) => {
                        emit_scalar_write(out, base, m, u64::from(*x))
                    }
                    (Type::FixedBytes(n), Value::Bytes(xs)) => {
                        assert_eq!(*n as usize, xs.len());
                        for (i, x) in xs.iter().enumerate() {
                            writeln!(out, "stage[{} + {}]=UINT8_C({});", b, i, x).unwrap();
                        }
                    }
                    (Type::Array { element, length }, Value::Array(values)) => {
                        assert_eq!(*length as usize, values.len());
                        let stride = m.array_stride_expr.as_ref().expect("array field metadata");
                        for (index, value) in values.iter().enumerate() {
                            let mut element_meta = m.clone();
                            element_meta.offset_expr =
                                format!("({})+UINT64_C({})*({stride})", m.offset_expr, index);
                            emit_array_element(
                                out,
                                model,
                                layout,
                                element,
                                &element_meta,
                                base,
                                value,
                            );
                        }
                    }
                    (Type::String(k), Value::Bytes(xs)) => emit_string(out, m, &b, *k, xs, &[]),
                    (Type::String(k), Value::Units(xs)) => emit_string(out, m, &b, *k, &[], xs),
                    (Type::Schema(child), _) => {
                        match &model.schema(child).kind {
                            SchemaKind::ScalarEnum {
                                repr,
                                endian: ee,
                                variants,
                            } => {
                                let Value::Variant { variant, .. } = v else {
                                    panic!("enum value required")
                                };
                                let raw = variants.iter().find(|x| x.name == *variant).unwrap().raw;
                                writeln!(out,"if(!put_bits(stage,byte_count,{}, {},UINT64_C({}),{}))return INT;",b,iw(*repr),raw,endian(*ee)).unwrap();
                            }
                            SchemaKind::TaggedEnum { variants, .. } if f.tag_field.is_some() => {
                                let Value::Union {
                                    variant, fields, ..
                                } = v
                                else {
                                    panic!("external union value")
                                };
                                let selected = variants
                                    .iter()
                                    .find(|candidate| candidate.name == *variant)
                                    .unwrap_or_else(|| panic!("unknown tagged variant"));
                                let tag_field = f.tag_field.as_ref().expect("external tag field");
                                let tag_value = values
                                    .iter()
                                    .find(|(name, _)| name == tag_field)
                                    .map(|(_, value)| value)
                                    .expect("external tag value");
                                let Value::Variant {
                                    ty,
                                    variant: tag_variant,
                                } = tag_value
                                else {
                                    panic!("external tag must be a scalar enum variant")
                                };
                                let SchemaKind::TaggedEnum { tag, .. } = &model.schema(child).kind
                                else {
                                    unreachable!()
                                };
                                if ty != tag || tag_variant != &selected.tag_variant {
                                    panic!("external tag and selected payload must agree")
                                }
                                if let Some(payload) = &selected.payload {
                                    emit_value(
                                        out,
                                        model,
                                        layout,
                                        payload,
                                        &b,
                                        &Value::Record(fields.clone()),
                                    );
                                }
                            }
                            SchemaKind::TaggedEnum { .. } => {
                                panic!("tagged payload requires an external scalar tag")
                            }
                            SchemaKind::Struct { .. } => {
                                emit_value(out, model, layout, child, &b, v)
                            }
                        }
                    }
                    _ => panic!("value/type mismatch at {schema}.{}", f.name),
                }
            }
        }
        _ => panic!("root value mismatch for {schema}"),
    }
}
fn emit_string(
    out: &mut String,
    m: &PathMeta,
    base: &str,
    k: StringKind,
    bytes: &[u8],
    units: &[u16],
) {
    let data = m.data_offset_expr.as_ref().unwrap();
    let d = format!(
        "checked_add({},(({})-({})),&pos)",
        base, data, m.offset_expr
    );
    let logical = if matches!(k, StringKind::U16 | StringKind::U16C) {
        units.len()
    } else {
        bytes.len()
    };
    if let Some(w) = m.len_width {
        let lo = m.len_offset_expr.as_ref().unwrap();
        let relative_length_offset = format!("(({lo})-({}))", m.offset_expr);
        writeln!(out,"if(!put_bits(stage,byte_count,checked_add({},({}),&pos),{},UINT64_C({}),{}))return INT;",base,relative_length_offset,iw(w),logical,endian(m.len_endian.unwrap())).unwrap();
    }
    for (i, x) in bytes.iter().enumerate() {
        writeln!(out, "stage[{}+{}]=UINT8_C({});", d, i, x).unwrap();
    }
    for (index, unit) in units.iter().enumerate() {
        writeln!(
            out,
            "if(!put_bits(stage,byte_count,{}+{},2,UINT64_C({}),{}))return INT;",
            d,
            index * 2,
            unit,
            endian(Endian::Native)
        )
        .unwrap();
    }
}

fn resolved_meta(model: &Model, layout: &LayoutOutput, schema: &str, path: &[String]) -> PathMeta {
    if let Some(m) = layout
        .metadata
        .ty(schema)
        .paths
        .iter()
        .find(|m| m.path == path)
    {
        let mut r = m.clone();
        if path.len() == 1 {
            if let SchemaKind::Struct { fields } = &model.schema(schema).kind {
                if let Some(f) = fields.iter().find(|f| f.name == path[0]) {
                    let (field_ty, _) = optional_inner(&f.ty);
                    if let Type::Schema(child) = field_ty {
                        if let SchemaKind::ScalarEnum { repr, endian, .. } =
                            &model.schema(child).kind
                        {
                            r.endian = *endian;
                            r.width_expr = iw(*repr).to_string();
                        }
                    }
                }
            }
        }
        return r;
    }
    match &model.schema(schema).kind {
        SchemaKind::Struct { fields } => {
            let f = fields.iter().find(|f| f.name == path[0]).unwrap();
            let head = layout.metadata.path(schema, &[path[0].clone()]);
            let (field_ty, _) = optional_inner(&f.ty);
            let Type::Schema(child) = field_ty else {
                panic!("non-schema dotted path")
            };
            let tail = if f.tag_field.is_some() {
                let SchemaKind::TaggedEnum { variants, .. } = &model.schema(child).kind else {
                    panic!("external tag target")
                };
                let selected = variants.iter().find(|v| v.name == path[1]).unwrap();
                resolved_meta(
                    model,
                    layout,
                    selected
                        .payload
                        .as_ref()
                        .expect("unit external payload path"),
                    &path[2..],
                )
            } else {
                resolved_meta(model, layout, child, &path[1..])
            };
            let mut r = tail.clone();
            r.schema = schema.into();
            r.path = path.to_vec();
            r.offset_expr = format!("({})+({})", head.offset_expr, tail.offset_expr);
            if let Some(x) = tail.data_offset_expr {
                r.data_offset_expr = Some(format!("({})+({})", head.offset_expr, x));
            }
            if let Some(x) = tail.len_offset_expr {
                r.len_offset_expr = Some(format!("({})+({})", head.offset_expr, x));
            }
            r
        }
        SchemaKind::TaggedEnum { variants, .. } => {
            let v = variants.iter().find(|v| v.name == path[0]).unwrap();
            let child = v.payload.as_ref().expect("unit variant has no child path");
            let pm = layout
                .metadata
                .ty(schema)
                .paths
                .iter()
                .find(|p| matches!(p.kind, PathKind::Payload))
                .unwrap();
            let tail = resolved_meta(model, layout, child, &path[1..]);
            let mut r = tail.clone();
            r.schema = schema.into();
            r.path = path.to_vec();
            r.offset_expr = format!("({})+({})", pm.offset_expr, tail.offset_expr);
            r
        }
        _ => panic!("dotted path through scalar"),
    }
}
fn emit_validate_enum(out: &mut String, model: &Model, schema: &str, base: &str) {
    let SchemaKind::ScalarEnum {
        repr,
        endian: enum_endian,
        variants,
    } = &model.schema(schema).kind
    else {
        panic!("zero-sentinel validation requires a scalar enum")
    };
    let valid = variants
        .iter()
        .map(|variant| format!("value==UINT64_C({})", variant.raw))
        .collect::<Vec<_>>()
        .join("||");
    writeln!(
        out,
        "if(!get_bits(in,input_len,{base},{}, {},&value))return false;",
        iw(*repr),
        endian(*enum_endian)
    )
    .unwrap();
    writeln!(out, "if(!({valid}))return false;").unwrap();
}

struct ValidationEmitter<'a> {
    out: &'a mut String,
    model: &'a Model,
    layout: &'a LayoutOutput,
    counter: &'a mut usize,
}

impl ValidationEmitter<'_> {
    fn emit_array_element(&mut self, element: &Type, element_endian: Endian, base: &str) {
        let model = self.model;
        match element {
            Type::Primitive(_) | Type::FixedBytes(_) | Type::String(_) => {}
            Type::Bool => {
                writeln!(
                    self.out,
                    "if(!get_bits(in,input_len,{base},1,{},&value))return false;if(value>UINT64_C(1))return false;",
                    endian(element_endian)
                )
                .unwrap();
            }
            Type::Schema(schema) => match &model.schema(schema).kind {
                SchemaKind::ScalarEnum { .. } => {
                    emit_validate_enum(&mut *self.out, model, schema, base)
                }
                SchemaKind::Struct { .. } => self.emit_schema(schema, base),
                SchemaKind::TaggedEnum { .. } => {
                    panic!("zero-sentinel validation does not support tagged enum array elements")
                }
            },
            Type::Array { .. } => panic!("nested fixed arrays are unsupported"),
            Type::Option(_) => panic!("optional fixed-array elements are unsupported"),
        }
    }

    fn emit_storage(
        &mut self,
        ty: &Type,
        field_endian: Endian,
        base: &str,
        field_meta: Option<&PathMeta>,
    ) {
        let model = self.model;
        match ty {
            Type::Primitive(_) | Type::FixedBytes(_) | Type::String(_) => {}
            Type::Bool => {
                writeln!(
                    self.out,
                    "if(!get_bits(in,input_len,{base},1,{},&value))return false;if(value>UINT64_C(1))return false;",
                    endian(field_endian)
                )
                .unwrap();
            }
            Type::Array { element, length } => {
                let stride = field_meta
                    .and_then(|meta| meta.array_stride_expr.as_ref())
                    .expect("array validation requires array metadata");
                for index in 0..*length {
                    let element_base =
                        format!("checked_add(({base}),UINT64_C({index})*({stride}),&scratch)");
                    self.emit_array_element(element, field_endian, &element_base);
                }
            }
            Type::Schema(schema) => match &model.schema(schema).kind {
                SchemaKind::ScalarEnum { .. } => {
                    emit_validate_enum(&mut *self.out, model, schema, base)
                }
                SchemaKind::Struct { .. } => self.emit_schema(schema, base),
                SchemaKind::TaggedEnum { .. } => {
                    panic!("zero-sentinel validation does not support tagged enums")
                }
            },
            Type::Option(_) => panic!("optional storage must be validated as a field"),
        }
    }

    fn emit_field(&mut self, schema: &str, f: &Field, base: &str, presence_name: Option<&str>) {
        let field_meta = meta(self.layout, schema, std::slice::from_ref(&f.name));
        let field_base = format!(
            "checked_add(({base}),({}),&scratch)",
            field_meta.offset_expr
        );
        match &f.ty {
            Type::Option(inner) => {
                let presence = match presence_name {
                    Some(name) => name.to_owned(),
                    None => {
                        let name = format!("optional_value_{}", *self.counter);
                        *self.counter += 1;
                        name
                    }
                };
                writeln!(
                    self.out,
                    "bool {presence}=false;if(!optional_span(in,input_len,{field_base},static_cast<std::size_t>({}),&{presence}))return false;",
                    field_meta.width_expr
                )
                .unwrap();
                writeln!(self.out, "if({presence}){{").unwrap();
                self.emit_storage(inner, f.endian, &field_base, Some(field_meta));
                self.out.push_str("}\n");
            }
            ty => self.emit_storage(ty, f.endian, &field_base, Some(field_meta)),
        }
    }

    fn emit_schema(&mut self, schema: &str, base: &str) {
        let model = self.model;
        let SchemaKind::Struct { fields } = &model.schema(schema).kind else {
            panic!("zero-sentinel validation requires a struct")
        };
        for f in fields {
            self.emit_field(schema, f, base, None);
        }
    }
}

fn emit_optional_prevalidation(out: &mut String, model: &Model, layout: &LayoutOutput, root: &str) {
    let SchemaKind::Struct { fields } = &model.schema(root).kind else {
        panic!("zero-sentinel root must be a struct")
    };
    let mut counter = 0;
    let mut validation = ValidationEmitter {
        out,
        model,
        layout,
        counter: &mut counter,
    };
    for (index, f) in fields.iter().enumerate() {
        if matches!(f.ty, Type::Option(_)) {
            let presence = format!("optional_m{index}");
            validation.emit_field(root, f, "0", Some(&presence));
        }
    }
}

fn optional_presence_var(model: &Model, schema: &str, path: &[String]) -> String {
    if path.len() != 1 {
        panic!("optional observation requires a direct field")
    }
    let SchemaKind::Struct { fields } = &model.schema(schema).kind else {
        panic!("optional observation root must be a struct")
    };
    let index = fields
        .iter()
        .position(|field| field.name == path[0])
        .expect("observed optional field");
    if !matches!(fields[index].ty, Type::Option(_)) {
        panic!("optional observation requires an optional field")
    }
    format!("optional_m{index}")
}

fn obs_expr(model: &Model, layout: &LayoutOutput, schema: &str, o: &Observation) -> String {
    if let Observation::Optional(path) = o {
        let presence = optional_presence_var(model, schema, path);
        return format!("(value={presence}?UINT64_C(1):UINT64_C(0),true)");
    }
    let (path, index) = match o {
        Observation::Scalar(p) => (p, None),
        Observation::Tag(p) => (p, None),
        Observation::Length(p) => (p, None),
        Observation::Unit(p, index) => (p, Some(*index)),
        Observation::Element(p, index) => (p, Some(*index)),
        Observation::Optional(_) => unreachable!("optional observation is handled above"),
    };
    let effective = if matches!(o, Observation::Tag(_)) {
        let SchemaKind::Struct { fields } = &model.schema(schema).kind else {
            panic!("external tag observation requires a record root")
        };
        let payload = fields
            .iter()
            .find(|field| field.name == path[0])
            .expect("observed payload field");
        vec![
            payload
                .tag_field
                .clone()
                .expect("external payload tag field"),
        ]
    } else {
        path.clone()
    };
    let owned = resolved_meta(model, layout, schema, &effective);
    let m = &owned;
    let mut pos = format!("({})", m.offset_expr);
    match o {
        Observation::Length(_) => {
            if let Some(w) = m.len_width {
                pos = format!("({})", m.len_offset_expr.as_ref().expect("length offset"));
                format!(
                    "get_bits(in,input_len,{}, {}, {}, &value)",
                    pos,
                    iw(w),
                    endian(m.len_endian.expect("length endian"))
                )
            } else {
                pos = format!("({})", m.data_offset_expr.as_ref().expect("C data offset"));
                format!(
                    "c_length(in,input_len,{}, {}, {}, {}, &value)",
                    pos,
                    m.capacity.expect("C capacity"),
                    m.unit_width,
                    endian(m.endian)
                )
            }
        }
        Observation::Unit(_, _) => {
            let data = m.data_offset_expr.as_ref().unwrap_or(&m.offset_expr);
            pos = format!(
                "checked_add(({}),UINT64_C({}),&scratch)",
                data,
                u64::from(index.unwrap()) * u64::from(m.unit_width)
            );
            let unit_endian = match m.kind {
                PathKind::String(StringKind::U16 | StringKind::U16C) => Endian::Native,
                _ => m.endian,
            };
            format!(
                "get_bits(in,input_len,{}, {}, {}, &value)",
                pos,
                m.unit_width,
                endian(unit_endian)
            )
        }
        Observation::Element(_, _) => {
            let stride = m.array_stride_expr.as_ref().expect("array element stride");
            pos = format!(
                "checked_add(({}),UINT64_C({})*({stride}),&scratch)",
                m.offset_expr,
                index.unwrap()
            );
            format!(
                "get_bits(in,input_len,{}, {}, {}, &value)",
                pos,
                m.unit_width,
                endian(m.endian)
            )
        }
        _ => {
            format!(
                "get_bits(in,input_len,{}, {}, {}, &value)",
                pos,
                m.unit_width,
                endian(m.endian)
            )
        }
    }
}

pub fn emit(model: &Model, layout: &LayoutOutput, out: &mut String) {
    out.push_str(r#"
namespace zs_codec {
constexpr std::uint8_t OK=0,NW=1,UNK=2,LEN=3,NI=4,NO=5,CAP=6,INT=7;
constexpr unsigned E_NATIVE=0,E_LITTLE=1,E_BIG=2;
bool aligned(const void*p,std::size_t a){return p&&reinterpret_cast<std::uintptr_t>(p)%a==0;}
bool add(std::size_t a,std::size_t b,std::size_t*o){if(a>SIZE_MAX-b)return false;*o=a+b;return true;}
std::size_t checked_add(std::size_t a,std::size_t b,std::size_t*o){return add(a,b,o)?*o:SIZE_MAX;}
bool put_bits(std::uint8_t*d,std::size_t n,std::size_t p,std::size_t w,std::uint64_t v,unsigned e){if(p>n||w>n-p||w>8)return false;if(e==E_NATIVE){switch(w){case 1:d[p]=static_cast<std::uint8_t>(v);return true;case 2:{std::uint16_t x=static_cast<std::uint16_t>(v);std::memcpy(d+p,&x,2);return true;}case 4:{std::uint32_t x=static_cast<std::uint32_t>(v);std::memcpy(d+p,&x,4);return true;}case 8:std::memcpy(d+p,&v,8);return true;default:return false;}}for(std::size_t i=0;i<w;i++){std::size_t j=e==E_LITTLE?i:w-1-i;d[p+i]=static_cast<std::uint8_t>(v>>(8*j));}return true;}
bool get_bits(const std::uint8_t*s,std::size_t n,std::size_t p,std::size_t w,unsigned e,std::uint64_t*v){if(p>n||w>n-p||w>8)return false;*v=0;if(e==E_NATIVE){switch(w){case 1:*v=s[p];return true;case 2:{std::uint16_t x=0;std::memcpy(&x,s+p,2);*v=x;return true;}case 4:{std::uint32_t x=0;std::memcpy(&x,s+p,4);*v=x;return true;}case 8:std::memcpy(v,s+p,8);return true;default:return false;}}for(std::size_t i=0;i<w;i++){std::size_t j=e==E_LITTLE?i:w-1-i;*v|=std::uint64_t(s[p+i])<<(8*j);}return true;}
bool c_length(const std::uint8_t*s,std::size_t n,std::size_t p,std::size_t cap,std::size_t unit,unsigned e,std::uint64_t*v){for(std::size_t i=0;i<cap;i++){std::uint64_t x=0;if(!get_bits(s,n,p+i*unit,unit,e,&x))return false;if(x==0){*v=i+1;return true;}}return false;}
bool clear_span(std::uint8_t*d,std::size_t n,std::size_t p,std::size_t w){if(p>n||w>n-p)return false;std::memset(d+p,0,w);return true;}
bool optional_span(const std::uint8_t*s,std::size_t n,std::size_t p,std::size_t w,bool*present){if(p>n||w>n-p)return false;bool any=false;for(std::size_t i=0;i<w;i++){if(s[p+i]!=UINT8_C(0))any=true;}*present=any;return true;}
"#);
    let sizes: Vec<String> = model
        .cases
        .iter()
        .map(|case| {
            let Root::Schema(root) = &case.root;
            layout.metadata.ty(root).size_expr.clone()
        })
        .collect();
    let max_bytes = sizes
        .into_iter()
        .reduce(|a, b| format!("(({})>({})?({}):({}))", a, b, a, b))
        .unwrap_or_else(|| "1".into());
    let max_obs = model
        .cases
        .iter()
        .flat_map(|c| [3 + 2 * c.observe.len(), 3 + 2 * c.layout.len()])
        .max()
        .unwrap_or(3);
    writeln!(out,"constexpr std::size_t MAX_BYTES={max_bytes}; constexpr std::size_t MAX_REPORT={max_obs}; static_assert(MAX_REPORT>=77 && MAX_BYTES>0);").unwrap();
    for c in &model.cases {
        let Root::Schema(root) = &c.root;
        let tm = layout
            .metadata
            .types
            .iter()
            .find(|t| t.schema == *root)
            .unwrap();
        writeln!(out,"bool write_{}(std::uint8_t*stage,std::size_t byte_count){{std::size_t pos=0;if(byte_count!=static_cast<std::size_t>({}))return false;",c.id,tm.size_expr).unwrap();
        emit_value(out, model, layout, root, "0", &c.value);
        out.push_str("return true;}\n");
        writeln!(out,"bool inspect_{}(const std::uint8_t*in,std::size_t input_len,std::uint64_t*stage){{std::size_t scratch=0;(void)scratch;std::uint64_t value=0;stage[0]=1;stage[1]={};stage[2]={};",c.id,c.id,c.observe.len()).unwrap();
        emit_optional_prevalidation(out, model, layout, root);
        for (i, o) in c.observe.iter().enumerate() {
            writeln!(
                out,
                "stage[{}]=UINT64_C({});if(!{})return false;",
                3 + 2 * i,
                o.key,
                obs_expr(model, layout, root, &o.source)
            )
            .unwrap();
            match &o.source {
                Observation::Scalar(p) => {
                    let m = resolved_meta(model, layout, root, p);
                    if matches!(m.kind, PathKind::Bool) {
                        out.push_str("if(value>1){return false;}\n");
                    }
                }
                Observation::Length(p) => {
                    let m = resolved_meta(model, layout, root, p);
                    if let Some(cap) = m.capacity {
                        writeln!(out, "if(value>UINT64_C({}))return false;", cap).unwrap();
                    }
                }
                Observation::Tag(path) => {
                    let SchemaKind::Struct { fields } = &model.schema(root).kind else {
                        panic!("external union observation requires a record")
                    };
                    let payload = fields
                        .iter()
                        .find(|field| field.name == path[0])
                        .expect("observed payload field");
                    let Type::Schema(tagged) = &payload.ty else {
                        panic!("tagged payload schema required")
                    };
                    let SchemaKind::TaggedEnum { tag, .. } = &model.schema(tagged).kind else {
                        panic!("tagged payload metadata required")
                    };
                    let SchemaKind::ScalarEnum { variants, .. } = &model.schema(tag).kind else {
                        panic!("tag enum")
                    };
                    let cond = variants
                        .iter()
                        .map(|v| format!("value==UINT64_C({})", v.raw))
                        .collect::<Vec<_>>()
                        .join("||");
                    writeln!(out, "if(!({cond}))return false;").unwrap();
                }
                _ => {}
            }
            writeln!(out, "stage[{}]=value;", 4 + 2 * i).unwrap();
        }
        out.push_str("return true;}\n");
    }
    out.push_str("} // namespace zs_codec\n");
    emit_abi(model, layout, out);
}

fn emit_abi(model: &Model, layout: &LayoutOutput, out: &mut String) {
    out.push_str("extern \"C\" std::uint8_t zs_layout_report(std::uint32_t id,std::uint64_t*out,std::size_t cap,std::size_t*w)noexcept{using namespace zs_codec;if(!aligned(w,alignof(std::size_t)))return NW;*w=0;std::size_t pairs=0;switch(id){");
    for c in &model.cases {
        writeln!(out, "case {}:pairs={};break;", c.id, c.layout.len()).unwrap();
    }
    out.push_str("default:return UNK;}if(pairs>(SIZE_MAX-3)/2)return INT;std::size_t n=3+2*pairs;if(cap<n)return CAP;if(!out)return NO;if(!aligned(out,alignof(std::uint64_t)))return INT;std::uint64_t stage[MAX_REPORT]={};stage[0]=1;stage[1]=id;stage[2]=pairs;std::size_t cursor=3;auto append=[&](std::uint64_t k,std::uint64_t v){stage[cursor++]=k;stage[cursor++]=v;};switch(id){");
    out.push_str(&layout.layout_case_arms);
    out.push_str("default:return UNK;}if(cursor!=n)return INT;std::memcpy(out,stage,n*sizeof(*out));*w=n;return OK;}\n");
    out.push_str("extern \"C\" std::uint8_t zs_write_fixture(std::uint32_t id,std::uint8_t*out,std::size_t cap,std::size_t*w)noexcept{using namespace zs_codec;if(!aligned(w,alignof(std::size_t)))return NW;*w=0;std::size_t n=0;switch(id){");
    for c in &model.cases {
        let Root::Schema(root) = &c.root;
        let type_meta = layout.metadata.ty(root);
        writeln!(
            out,
            "case {}:n=static_cast<std::size_t>({});break;",
            c.id, type_meta.size_expr
        )
        .unwrap();
    }
    out.push_str("default:return UNK;}if(cap<n)return CAP;if(!out)return NO;std::uint8_t stage[MAX_BYTES]={};bool ok=false;switch(id){");
    for c in &model.cases {
        writeln!(out, "case {}:ok=write_{}(stage,n);break;", c.id, c.id).unwrap();
    }
    out.push_str(
        "default:return UNK;}if(!ok)return INT;std::memcpy(out,stage,n);*w=n;return OK;}\n",
    );
    out.push_str("extern \"C\" std::uint8_t zs_inspect_fixture(std::uint32_t id,const std::uint8_t*in,std::size_t len,std::uint64_t*out,std::size_t cap,std::size_t*w)noexcept{using namespace zs_codec;if(!aligned(w,alignof(std::size_t)))return NW;*w=0;std::size_t n=0,pairs=0;switch(id){");
    for c in &model.cases {
        let Root::Schema(root) = &c.root;
        let type_meta = layout.metadata.ty(root);
        writeln!(
            out,
            "case {}:n=static_cast<std::size_t>({});pairs={};break;",
            c.id,
            type_meta.size_expr,
            c.observe.len()
        )
        .unwrap();
    }
    out.push_str("default:return UNK;}if(len!=n)return LEN;if(!in)return NI;if(pairs>(SIZE_MAX-3)/2)return INT;std::size_t req=3+2*pairs;if(cap<req)return CAP;if(!out)return NO;if(!aligned(out,alignof(std::uint64_t)))return INT;std::uint64_t stage[MAX_REPORT]={};bool ok=false;switch(id){");
    for c in &model.cases {
        writeln!(
            out,
            "case {}:ok=inspect_{}(in,len,stage);break;",
            c.id, c.id
        )
        .unwrap();
    }
    out.push_str("default:return UNK;}if(!ok)return INT;std::memcpy(out,stage,req*sizeof(*out));*w=req;return OK;}\n");
}
