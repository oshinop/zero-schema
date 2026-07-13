use std::fmt::Write as _;

use super::frontend::{
    Endian, IntWidth, Model, Observation, Root, SchemaKind, StringKind, Type, Value,
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
fn bits(v: &Value) -> u64 {
    if let Value::Bits(x) = v {
        *x
    } else {
        panic!("expected bits value")
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
                match (&f.ty, v) {
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
                                let selected =
                                    variants.iter().find(|x| x.name == *variant).unwrap();
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
                                emit_tagged(out, model, layout, child, &b, v)
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
    for (i, x) in units.iter().enumerate() {
        writeln!(
            out,
            "if(!put_bits(stage,byte_count,{}+{},2,UINT64_C({}),{}))return INT;",
            d,
            i * 2,
            x,
            endian(m.endian)
        )
        .unwrap();
    }
}
fn emit_tagged(
    out: &mut String,
    model: &Model,
    layout: &LayoutOutput,
    schema: &str,
    base: &str,
    v: &Value,
) {
    let Value::Union {
        variant, fields, ..
    } = v
    else {
        panic!("union value required")
    };
    let SchemaKind::TaggedEnum { tag, variants } = &model.schema(schema).kind else {
        panic!("tagged schema required")
    };
    let selected = variants.iter().find(|x| x.name == *variant).unwrap();
    let SchemaKind::ScalarEnum {
        repr,
        endian: te,
        variants: tags,
    } = &model.schema(tag).kind
    else {
        panic!("scalar tag required")
    };
    let raw = tags
        .iter()
        .find(|x| x.name == selected.tag_variant)
        .unwrap()
        .raw;
    let tm = layout.metadata.ty(schema);
    let tagm = tm
        .paths
        .iter()
        .find(|p| matches!(p.kind, PathKind::Tag))
        .unwrap();
    writeln!(
        out,
        "if(!put_bits(stage,byte_count,checked_add({},({}),&pos),{},UINT64_C({}),{}))return INT;",
        base,
        tagm.offset_expr,
        iw(*repr),
        raw,
        endian(*te)
    )
    .unwrap();
    if let Some(child) = &selected.payload {
        let pm = tm
            .paths
            .iter()
            .find(|p| matches!(p.kind, PathKind::Payload))
            .unwrap();
        let payload = Value::Record(fields.clone());
        let pb = format!("checked_add({},({}),&pos)", base, pm.offset_expr);
        emit_value(out, model, layout, child, &pb, &payload);
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
                if let Some(Type::Schema(child)) =
                    fields.iter().find(|f| f.name == path[0]).map(|f| &f.ty)
                {
                    if let SchemaKind::ScalarEnum { repr, endian, .. } = &model.schema(child).kind {
                        r.endian = *endian;
                        r.width_expr = iw(*repr).to_string();
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
            let Type::Schema(child) = &f.ty else {
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
fn obs_expr(model: &Model, layout: &LayoutOutput, schema: &str, o: &Observation) -> String {
    let (path, index) = match o {
        Observation::Scalar(p) => (p, None),
        Observation::Tag(p) => (p, None),
        Observation::Length(p) => (p, None),
        Observation::Unit(p, i) => (p, Some(*i)),
    };
    let effective = if matches!(o, Observation::Tag(_)) {
        if path.as_slice() == ["root".to_owned()] {
            Vec::new()
        } else {
            let SchemaKind::Struct { fields } = &model.schema(schema).kind else {
                panic!("named tag path requires struct")
            };
            let payload = fields.iter().find(|f| f.name == path[0]).unwrap();
            vec![
                payload
                    .tag_field
                    .clone()
                    .expect("external payload tag field"),
            ]
        }
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
            let d = m.data_offset_expr.as_ref().unwrap_or(&m.offset_expr);
            pos = format!(
                "checked_add(({}),UINT64_C({}),&scratch)",
                d,
                u64::from(index.unwrap()) * u64::from(m.unit_width)
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
"#);
    let sizes: Vec<String> = model
        .cases
        .iter()
        .filter_map(|c| match &c.root {
            Root::Schema(n) => Some(layout.metadata.ty(n).size_expr.clone()),
            Root::Abi(_) => None,
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
        if let Root::Abi(name) = &c.root {
            let ai = model
                .abi_structs
                .iter()
                .position(|a| a.name == *name)
                .unwrap();
            let Value::Record(values) = &c.value else {
                panic!("ABI record value required")
            };
            writeln!(out,"bool write_{}(std::uint8_t*stage,std::size_t byte_count){{std::size_t pos=0;if(byte_count!=sizeof(a{}))return false;",c.id,ai).unwrap();
            for (field, v) in values {
                if matches!(v, Value::Zst) {
                    continue;
                }
                let fi = model
                    .abi(name)
                    .fields
                    .iter()
                    .position(|f| f.name == *field)
                    .unwrap();
                let width = match model.abi(name).fields[fi].ty.as_str() {
                    "u8" => 1,
                    "u32" => 4,
                    _ => panic!("ABI scalar"),
                };
                emit_scalar_write(
                    out,
                    "0",
                    &PathMeta {
                        schema: name.clone(),
                        path: vec![field.clone()],
                        offset_expr: format!("offsetof(a{ai},m{fi})"),
                        width_expr: width.to_string(),
                        endian: Endian::Native,
                        kind: PathKind::Scalar,
                        data_offset_expr: None,
                        len_offset_expr: None,
                        len_width: None,
                        len_endian: None,
                        capacity: None,
                        unit_width: width,
                        tail_zero: false,
                    },
                    bits(v),
                );
            }
            out.push_str("return true;}\n");
            writeln!(out,"bool inspect_{}(const std::uint8_t*in,std::size_t input_len,std::uint64_t*stage){{std::uint64_t value=0;stage[0]=1;stage[1]={};stage[2]={};",c.id,c.id,c.observe.len()).unwrap();
            for (i, o) in c.observe.iter().enumerate() {
                let Observation::Scalar(p) = &o.source else {
                    panic!("ABI scalar observation")
                };
                let fi = model
                    .abi(name)
                    .fields
                    .iter()
                    .position(|f| f.name == p[0])
                    .unwrap();
                let w = match model.abi(name).fields[fi].ty.as_str() {
                    "u8" => 1,
                    "u32" => 4,
                    _ => panic!("ABI scalar"),
                };
                writeln!(out,"stage[{}]=UINT64_C({});if(!get_bits(in,input_len,offsetof(a{},m{}),{},E_NATIVE,&value))return false;stage[{}]=value;",3+2*i,o.key,ai,fi,w,4+2*i).unwrap();
            }
            out.push_str("return true;}\n");
            continue;
        }
        let root = match &c.root {
            Root::Schema(n) => n,
            Root::Abi(_) => unreachable!(),
        };
        let tm = layout
            .metadata
            .types
            .iter()
            .find(|t| t.schema == *root)
            .unwrap();
        writeln!(out,"bool write_{}(std::uint8_t*stage,std::size_t byte_count){{std::size_t pos=0;if(byte_count!=static_cast<std::size_t>({}))return false;",c.id,tm.size_expr).unwrap();
        match &model.schema(root).kind {
            SchemaKind::TaggedEnum { .. } => emit_tagged(out, model, layout, root, "0", &c.value),
            _ => emit_value(out, model, layout, root, "0", &c.value),
        };
        out.push_str("return true;}\n");
        writeln!(out,"bool inspect_{}(const std::uint8_t*in,std::size_t input_len,std::uint64_t*stage){{std::size_t scratch=0;(void)scratch;std::uint64_t value=0;stage[0]=1;stage[1]={};stage[2]={};",c.id,c.id,c.observe.len()).unwrap();
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
                Observation::Tag(p) => {
                    let tag_schema = if p.as_slice() == ["root".to_owned()] {
                        let SchemaKind::TaggedEnum { tag, .. } = &model.schema(root).kind else {
                            panic!("root tag")
                        };
                        tag.clone()
                    } else {
                        let SchemaKind::Struct { fields } = &model.schema(root).kind else {
                            panic!("external tag")
                        };
                        let f = fields.iter().find(|f| f.name == p[0]).unwrap();
                        let Type::Schema(u) = &f.ty else {
                            panic!("tagged field")
                        };
                        let SchemaKind::TaggedEnum { tag, .. } = &model.schema(u).kind else {
                            panic!("tagged field")
                        };
                        tag.clone()
                    };
                    let SchemaKind::ScalarEnum { variants, .. } = &model.schema(&tag_schema).kind
                    else {
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
        match &c.root {
            Root::Schema(n) => {
                let t = layout.metadata.ty(n);
                writeln!(
                    out,
                    "case {}:n=static_cast<std::size_t>({});break;",
                    c.id, t.size_expr
                )
                .unwrap();
            }
            Root::Abi(n) => {
                let ai = model.abi_structs.iter().position(|a| a.name == *n).unwrap();
                writeln!(out, "case {}:n=sizeof(a{});break;", c.id, ai).unwrap();
            }
        }
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
        match &c.root {
            Root::Schema(n) => {
                let t = layout.metadata.ty(n);
                writeln!(
                    out,
                    "case {}:n=static_cast<std::size_t>({});pairs={};break;",
                    c.id,
                    t.size_expr,
                    c.observe.len()
                )
                .unwrap();
            }
            Root::Abi(n) => {
                let ai = model.abi_structs.iter().position(|a| a.name == *n).unwrap();
                writeln!(
                    out,
                    "case {}:n=sizeof(a{});pairs={};break;",
                    c.id,
                    ai,
                    c.observe.len()
                )
                .unwrap();
            }
        }
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
