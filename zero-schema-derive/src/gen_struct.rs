use crate::ir::*;
use proc_macro2::{Span, TokenStream};
use quote::{format_ident, quote, quote_spanned};
use syn::{
    GenericParam, Generics, Lifetime, Path, Type,
    visit::{self, Visit},
    visit_mut::{self, VisitMut},
};

fn erase_lifetimes(ty: &Type) -> Type {
    struct Erase {
        bound: Vec<syn::Ident>,
    }
    impl VisitMut for Erase {
        fn visit_bound_lifetimes_mut(&mut self, lifetimes: &mut syn::BoundLifetimes) {
            let old = self.bound.len();
            self.bound
                .extend(lifetimes.lifetimes.iter().filter_map(|p| match p {
                    GenericParam::Lifetime(p) => Some(p.lifetime.ident.clone()),
                    _ => None,
                }));
            visit_mut::visit_bound_lifetimes_mut(self, lifetimes);
            self.bound.truncate(old);
        }
        fn visit_lifetime_mut(&mut self, lifetime: &mut Lifetime) {
            if lifetime.ident != "static" && !self.bound.iter().any(|name| name == &lifetime.ident)
            {
                *lifetime = Lifetime::new("'static", lifetime.apostrophe);
            }
        }
    }
    let mut erased = ty.clone();
    Erase { bound: Vec::new() }.visit_type_mut(&mut erased);
    erased
}

fn hidden_schema_path(ir: &SchemaIr) -> syn::Result<Path> {
    let name = &ir.ident;
    let arguments: Vec<TokenStream> = ir
        .cleaned_generics
        .params
        .iter()
        .map(|parameter| match parameter {
            GenericParam::Lifetime(_) => quote!('static),
            GenericParam::Type(parameter) => {
                let ident = &parameter.ident;
                quote!(#ident)
            }
            GenericParam::Const(parameter) => {
                let ident = &parameter.ident;
                quote!(#ident)
            }
        })
        .collect();
    if arguments.is_empty() {
        syn::parse2(quote!(super::#name))
    } else {
        syn::parse2(quote!(super::#name::<#(#arguments),*>))
    }
}

fn validate_moved_path_inventory(ir: &SchemaIr) -> syn::Result<()> {
    for moved in &ir.moved_paths {
        let first = moved
            .path
            .segments
            .first()
            .map(|segment| segment.ident.to_string());
        let expected = if first.as_deref() == Some("Self") {
            PathRebase::RewriteSchemaSelf
        } else if moved.path.leading_colon.is_none()
            && matches!(first.as_deref(), Some("self") | Some("super"))
        {
            PathRebase::RebaseOneLevel
        } else {
            PathRebase::Preserve
        };
        if moved.strategy != expected {
            return Err(syn::Error::new(
                moved.span,
                "internal moved-path strategy mismatch",
            ));
        }
    }
    Ok(())
}

struct HiddenSyntaxRebaser<'a> {
    schema: &'a Path,
}

impl VisitMut for HiddenSyntaxRebaser<'_> {
    fn visit_path_mut(&mut self, path: &mut Path) {
        visit_mut::visit_path_mut(self, path);
        if path.leading_colon.is_some() {
            return;
        }
        let Some(first) = path.segments.first() else {
            return;
        };
        match first.ident.to_string().as_str() {
            "Self" => {
                let suffix: Vec<_> = path.segments.iter().skip(1).cloned().collect();
                let mut replacement = self.schema.clone();
                replacement.segments.extend(suffix);
                *path = replacement;
            }
            "self" => {
                path.segments[0].ident = syn::Ident::new("super", first.ident.span());
            }
            "super" => {
                path.segments.insert(0, syn::parse_quote!(super));
            }
            _ => {}
        }
    }
}

fn rebase_hidden_type(mut ty: Type, schema: &Path) -> Type {
    HiddenSyntaxRebaser { schema }.visit_type_mut(&mut ty);
    ty
}

fn rebase_hidden_expr(mut expression: syn::Expr, schema: &Path) -> syn::Expr {
    HiddenSyntaxRebaser { schema }.visit_expr_mut(&mut expression);
    expression
}

fn rebase_hidden_generics(generics: &mut Generics, schema: &Path) {
    HiddenSyntaxRebaser { schema }.visit_generics_mut(generics);
}

fn generic_closure(ir: &SchemaIr, types: impl IntoIterator<Item = Type>) -> Generics {
    struct Used<'a> {
        params: &'a [GenericParam],
        names: Vec<syn::Ident>,
    }
    impl Used<'_> {
        fn record(&mut self, ident: &syn::Ident) {
            if self.params.iter().any(|parameter| match parameter {
                GenericParam::Type(parameter) => parameter.ident == *ident,
                GenericParam::Const(parameter) => parameter.ident == *ident,
                GenericParam::Lifetime(_) => false,
            }) && !self.names.iter().any(|name| name == ident)
            {
                self.names.push(ident.clone());
            }
        }
    }
    impl<'ast> Visit<'ast> for Used<'_> {
        fn visit_path_segment(&mut self, segment: &'ast syn::PathSegment) {
            self.record(&segment.ident);
            visit::visit_path_segment(self, segment);
        }
    }
    let params: Vec<_> = ir.cleaned_generics.params.iter().cloned().collect();
    let mut used = Used {
        params: &params,
        names: Vec::new(),
    };
    for ty in types {
        used.visit_type(&ty);
    }
    loop {
        let before = used.names.len();
        for parameter in &params {
            let selected = match parameter {
                GenericParam::Type(parameter) => used.names.contains(&parameter.ident),
                GenericParam::Const(parameter) => used.names.contains(&parameter.ident),
                GenericParam::Lifetime(_) => false,
            };
            if selected {
                used.visit_generic_param(parameter);
            }
        }
        if used.names.len() == before {
            break;
        }
    }
    let mut generics = Generics::default();
    generics.params.extend(
        params
            .iter()
            .filter(|parameter| match parameter {
                GenericParam::Type(parameter) => used.names.contains(&parameter.ident),
                GenericParam::Const(parameter) => used.names.contains(&parameter.ident),
                GenericParam::Lifetime(_) => false,
            })
            .cloned(),
    );
    if let Some(where_clause) = &ir.cleaned_generics.where_clause {
        for predicate in &where_clause.predicates {
            let mut mentioned = Used {
                params: &params,
                names: Vec::new(),
            };
            mentioned.visit_where_predicate(predicate);
            if !mentioned.names.is_empty()
                && mentioned.names.iter().all(|name| used.names.contains(name))
            {
                generics
                    .make_where_clause()
                    .predicates
                    .push(predicate.clone());
            }
        }
    }
    generics
}
fn generic_arguments(generics: &Generics) -> TokenStream {
    let arguments = generics.params.iter().map(|parameter| match parameter {
        GenericParam::Type(parameter) => {
            let ident = &parameter.ident;
            quote!(#ident)
        }
        GenericParam::Const(parameter) => {
            let ident = &parameter.ident;
            quote!(#ident)
        }
        GenericParam::Lifetime(parameter) => {
            let lifetime = &parameter.lifetime;
            quote!(#lifetime)
        }
    });
    if generics.params.is_empty() {
        quote!()
    } else {
        quote!(<#(#arguments),*>)
    }
}
fn wire_name(k: PrimitiveKind, e: Endian) -> syn::Ident {
    let stem = match k {
        PrimitiveKind::U8 => "U8",
        PrimitiveKind::I8 => "I8",
        PrimitiveKind::U16 => "U16",
        PrimitiveKind::I16 => "I16",
        PrimitiveKind::U32 => "U32",
        PrimitiveKind::I32 => "I32",
        PrimitiveKind::U64 => "U64",
        PrimitiveKind::I64 => "I64",
        PrimitiveKind::F32 => "F32",
        PrimitiveKind::F64 => "F64",
    };
    format_ident!(
        "{}{}",
        if matches!(k, PrimitiveKind::U8 | PrimitiveKind::I8) {
            ""
        } else {
            match e {
                Endian::Native => "Native",
                Endian::Little => "Little",
                Endian::Big => "Big",
            }
        },
        stem
    )
}
fn len_wire(f: &FieldIr) -> syn::Ident {
    let r = f
        .resolved
        .length_repr
        .as_ref()
        .map(ToString::to_string)
        .unwrap_or_else(|| "u16".into());
    format_ident!(
        "{}{}",
        if r == "u8" {
            ""
        } else {
            match f.resolved.endian {
                Endian::Native => "Native",
                Endian::Little => "Little",
                Endian::Big => "Big",
            }
        },
        r.to_uppercase()
    )
}
fn endian(rt: &Path, e: Endian) -> TokenStream {
    let x = format_ident!(
        "{}",
        match e {
            Endian::Native => "Native",
            Endian::Little => "Little",
            Endian::Big => "Big",
        }
    );
    quote!(#rt::Endian::#x)
}
fn tail(rt: &Path, t: Tail) -> TokenStream {
    match t {
        Tail::Ignore => quote!(#rt::TailPolicy::Ignore),
        Tail::Zero => quote!(#rt::TailPolicy::Zero),
    }
}
fn primitive(rt: &Path, k: PrimitiveKind) -> TokenStream {
    let s = match k {
        PrimitiveKind::U8 => "U8",
        PrimitiveKind::I8 => "I8",
        PrimitiveKind::U16 => "U16",
        PrimitiveKind::I16 => "I16",
        PrimitiveKind::U32 => "U32",
        PrimitiveKind::I32 => "I32",
        PrimitiveKind::U64 => "U64",
        PrimitiveKind::I64 => "I64",
        PrimitiveKind::F32 => "F32",
        PrimitiveKind::F64 => "F64",
    };
    let x = format_ident!("{}", s);
    quote!(#rt::PrimitiveKind::#x)
}

fn lower_layout_expr(
    expression: &LayoutExpr,
    rt: &Path,
    field_types: &[TokenStream],
    overflow_message: &syn::LitStr,
) -> TokenStream {
    match expression {
        LayoutExpr::Fixed(value) => quote!(#value as ::core::primitive::usize),
        LayoutExpr::TypeSize(ty) => quote!(<#ty as #rt::ZeroSchemaType>::WIRE_SIZE),
        LayoutExpr::TypeAlign(ty) => quote!(<#ty as #rt::ZeroSchemaType>::WIRE_ALIGN),
        LayoutExpr::FieldSize(index) => {
            let ty = field_types
                .get(*index)
                .expect("layout field-size index must resolve");
            quote!(::core::mem::size_of::<#ty>())
        }
        LayoutExpr::FieldAlign(index) => {
            let ty = field_types
                .get(*index)
                .expect("layout field-align index must resolve");
            quote!(::core::mem::align_of::<#ty>())
        }
        LayoutExpr::AlignUp(size, align) => {
            let size = lower_layout_expr(size, rt, field_types, overflow_message);
            let align = lower_layout_expr(align, rt, field_types, overflow_message);
            quote!({
                let size = #size;
                let align = #align;
                match #rt::__private::__checked_wire_stride(size, align) {
                    ::core::option::Option::Some(value) => value,
                    ::core::option::Option::None => ::core::panic!(#overflow_message),
                }
            })
        }
        LayoutExpr::Add(left, right) => {
            let left = lower_layout_expr(left, rt, field_types, overflow_message);
            let right = lower_layout_expr(right, rt, field_types, overflow_message);
            quote!({
                let left = #left;
                let right = #right;
                match left.checked_add(right) {
                    ::core::option::Option::Some(value) => value,
                    ::core::option::Option::None => ::core::panic!(#overflow_message),
                }
            })
        }
        LayoutExpr::Max(expressions) => {
            let expressions: Vec<_> = expressions
                .iter()
                .map(|expression| lower_layout_expr(expression, rt, field_types, overflow_message))
                .collect();
            quote!({
                let mut maximum = 1usize;
                #(
                    let candidate = #expressions;
                    if candidate > maximum { maximum = candidate; }
                )*
                maximum
            })
        }
    }
}

fn error_tokens(
    case: ErrorCase,
    rt: &Path,
) -> (
    TokenStream,
    TokenStream,
    TokenStream,
    TokenStream,
    TokenStream,
    TokenStream,
) {
    match case {
        ErrorCase::Layout => (
            quote!(Layout(#rt::LayoutError)),
            quote!(Self::Layout(_) => #rt::ErrorKind::Layout),
            quote!(Self::Layout(source) => ::core::option::Option::Some(source)),
            quote!(Self::Layout(_) => ::core::option::Option::None),
            quote!(Self::Layout(source) => ::core::fmt::Display::fmt(source, f)),
            quote!(Self::Layout(_) => ::core::option::Option::None),
        ),
        ErrorCase::InvalidBool => (
            quote!(InvalidBool { field: &'static ::core::primitive::str, value: ::core::primitive::u8 }),
            quote!(Self::InvalidBool { .. } => #rt::ErrorKind::InvalidBool),
            quote!(Self::InvalidBool { .. } => ::core::option::Option::None),
            quote!(Self::InvalidBool { field, .. } => ::core::option::Option::Some(#rt::ErrorPathSegment::Field(field))),
            quote!(Self::InvalidBool { value, .. } => ::core::write!(f, "invalid boolean value {}; expected 0 or 1", value)),
            quote!(Self::InvalidBool { .. } => ::core::option::Option::None),
        ),
        ErrorCase::LengthOutOfBounds => (
            quote!(LengthOutOfBounds { field: &'static ::core::primitive::str, length: ::core::primitive::usize, capacity: ::core::primitive::usize }),
            quote!(Self::LengthOutOfBounds { .. } => #rt::ErrorKind::LengthOutOfBounds),
            quote!(Self::LengthOutOfBounds { .. } => ::core::option::Option::None),
            quote!(Self::LengthOutOfBounds { field, .. } => ::core::option::Option::Some(#rt::ErrorPathSegment::Field(field))),
            quote!(Self::LengthOutOfBounds { length, capacity, .. } => ::core::write!(f, "length {} exceeds capacity {}", length, capacity)),
            quote!(Self::LengthOutOfBounds { .. } => ::core::option::Option::None),
        ),
        ErrorCase::InvalidUtf8 => (
            quote!(InvalidUtf8 { field: &'static ::core::primitive::str, source: ::core::str::Utf8Error }),
            quote!(Self::InvalidUtf8 { .. } => #rt::ErrorKind::InvalidUtf8),
            quote!(Self::InvalidUtf8 { source, .. } => ::core::option::Option::Some(source)),
            quote!(Self::InvalidUtf8 { field, .. } => ::core::option::Option::Some(#rt::ErrorPathSegment::Field(field))),
            quote!(Self::InvalidUtf8 { source, .. } => ::core::write!(f, "invalid UTF-8: {}", source)),
            quote!(Self::InvalidUtf8 { .. } => ::core::option::Option::None),
        ),
        ErrorCase::MissingNul => (
            quote!(MissingNul { field: &'static ::core::primitive::str }),
            quote!(Self::MissingNul { .. } => #rt::ErrorKind::MissingNul),
            quote!(Self::MissingNul { .. } => ::core::option::Option::None),
            quote!(Self::MissingNul { field } => ::core::option::Option::Some(#rt::ErrorPathSegment::Field(field))),
            quote!(Self::MissingNul { .. } => f.write_str("missing NUL terminator")),
            quote!(Self::MissingNul { .. } => ::core::option::Option::None),
        ),
        ErrorCase::NonZeroTail => (
            quote!(NonZeroTail { field: &'static ::core::primitive::str, offset: ::core::primitive::usize }),
            quote!(Self::NonZeroTail { .. } => #rt::ErrorKind::NonZeroTail),
            quote!(Self::NonZeroTail { .. } => ::core::option::Option::None),
            quote!(Self::NonZeroTail { field, .. } => ::core::option::Option::Some(#rt::ErrorPathSegment::Field(field))),
            quote!(Self::NonZeroTail { offset, .. } => ::core::write!(f, "nonzero tail at logical offset {}", offset)),
            quote!(Self::NonZeroTail { .. } => ::core::option::Option::None),
        ),
        ErrorCase::NonZeroPadding => (
            quote!(NonZeroPadding {
                offset: ::core::primitive::usize
            }),
            quote!(Self::NonZeroPadding { .. } => #rt::ErrorKind::NonZeroPadding),
            quote!(Self::NonZeroPadding { .. } => ::core::option::Option::None),
            quote!(Self::NonZeroPadding { .. } => ::core::option::Option::None),
            quote!(Self::NonZeroPadding { offset } => ::core::write!(f, "nonzero padding byte at offset {}", offset)),
            quote!(Self::NonZeroPadding { .. } => ::core::option::Option::None),
        ),
        ErrorCase::CapacityExceeded => (
            quote!(CapacityExceeded { field: &'static ::core::primitive::str, length: ::core::primitive::usize, capacity: ::core::primitive::usize }),
            quote!(Self::CapacityExceeded { .. } => #rt::ErrorKind::CapacityExceeded),
            quote!(Self::CapacityExceeded { .. } => ::core::option::Option::None),
            quote!(Self::CapacityExceeded { field, .. } => ::core::option::Option::Some(#rt::ErrorPathSegment::Field(field))),
            quote!(Self::CapacityExceeded { length, capacity, .. } => ::core::write!(f, "length {} exceeds encoding capacity {}", length, capacity)),
            quote!(Self::CapacityExceeded { .. } => ::core::option::Option::None),
        ),
        ErrorCase::TagMismatch => (
            quote!(TagMismatch { field: &'static ::core::primitive::str, tag_field: &'static ::core::primitive::str, declared: ::core::primitive::u64, selected: ::core::primitive::u64 }),
            quote!(Self::TagMismatch { .. } => #rt::ErrorKind::TagMismatch),
            quote!(Self::TagMismatch { .. } => ::core::option::Option::None),
            quote!(Self::TagMismatch { field, .. } => ::core::option::Option::Some(#rt::ErrorPathSegment::Field(field))),
            quote!(Self::TagMismatch { declared, selected, .. } => ::core::write!(f, "external tag {} does not match selected tag {}", declared, selected)),
            quote!(Self::TagMismatch { .. } => ::core::option::Option::None),
        ),
        ErrorCase::RangeViolation => (
            quote!(RangeViolation { field: &'static ::core::primitive::str }),
            quote!(Self::RangeViolation { .. } => #rt::ErrorKind::RangeViolation),
            quote!(Self::RangeViolation { .. } => ::core::option::Option::None),
            quote!(Self::RangeViolation { field } => ::core::option::Option::Some(#rt::ErrorPathSegment::Field(field))),
            quote!(Self::RangeViolation { .. } => f.write_str("value violates configured range")),
            quote!(Self::RangeViolation { .. } => ::core::option::Option::None),
        ),
        ErrorCase::MustEqualViolation => (
            quote!(MustEqualViolation { field: &'static ::core::primitive::str }),
            quote!(Self::MustEqualViolation { .. } => #rt::ErrorKind::MustEqualViolation),
            quote!(Self::MustEqualViolation { .. } => ::core::option::Option::None),
            quote!(Self::MustEqualViolation { field } => ::core::option::Option::Some(#rt::ErrorPathSegment::Field(field))),
            quote!(Self::MustEqualViolation { .. } => f.write_str("value differs from required constant")),
            quote!(Self::MustEqualViolation { .. } => ::core::option::Option::None),
        ),
        ErrorCase::Custom => (
            quote!(Custom { field: ::core::option::Option<&'static ::core::primitive::str>, variant: ::core::option::Option<&'static ::core::primitive::str>, source: #rt::ValidationFailure }),
            quote!(Self::Custom { .. } => #rt::ErrorKind::CustomValidation),
            quote!(Self::Custom { source, .. } => ::core::option::Option::Some(source)),
            quote!(Self::Custom { field: ::core::option::Option::Some(field), .. } => ::core::option::Option::Some(#rt::ErrorPathSegment::Field(field)), Self::Custom { variant: ::core::option::Option::Some(variant), .. } => ::core::option::Option::Some(#rt::ErrorPathSegment::Variant(variant)), Self::Custom { .. } => ::core::option::Option::None),
            quote!(Self::Custom { source, .. } => ::core::fmt::Display::fmt(source, f)),
            quote!(Self::Custom { source, .. } => ::core::option::Option::Some(source.code())),
        ),
        _ => unreachable!("unsupported direct struct error case"),
    }
}

fn support_ident(module: &syn::Ident, stem: &str) -> syn::Ident {
    let suffix = module
        .to_string()
        .trim_start_matches("__zero_schema_")
        .replace("r#", "");
    syn::Ident::new(&format!("{stem}_{suffix}"), Span::mixed_site())
}

pub fn generate(
    ir: &SchemaIr,
    rt: &Path,
    hidden: &Path,
    zerocopy: &Path,
) -> syn::Result<TokenStream> {
    let name = &ir.ident;
    let vis = &ir.visibility;
    let module = &ir.generated_names.module;
    let wire = &ir.generated_names.wire;
    let de = &ir.generated_names.decode_error;
    let ee = &ir.generated_names.encode_error;
    let module_vis = &ir.visibility_plan.module;
    let support_vis = &ir.visibility_plan.support;
    let logical = name.to_string().trim_start_matches("r#").to_owned();
    validate_moved_path_inventory(ir)?;
    let hidden_schema = hidden_schema_path(ir)?;
    let zerocopy_root = zerocopy
        .segments
        .first()
        .expect("resolved zerocopy path has a root");
    let zerocopy_crate =
        syn::LitStr::new(&zerocopy_root.ident.to_string(), zerocopy_root.ident.span());
    let input_local = support_ident(module, "__zero_input");
    let decoded_value_local = support_ident(module, "__zero_decoded_value");
    let sentinel = support_ident(module, "__end");
    let root_attr = ir
        .options
        .align
        .map(|n| {
            let align = syn::LitInt::new(&n.to_string(), name.span());
            quote!(#[repr(C,align(#align))])
        })
        .unwrap_or_else(|| quote!(#[repr(C)]));
    let source_lt = &ir.source_lifetime;
    let decode_lt = quote!(#source_lt);
    let nested_types: Vec<(Type, Type)> = ir
        .fields
        .iter()
        .filter(|field| matches!(field.kind, FieldKind::Schema))
        .map(|field| {
            let live = field.original_type.clone();
            (live.clone(), erase_lifetimes(&live))
        })
        .collect();
    let nested_fields: Vec<&FieldIr> = ir
        .fields
        .iter()
        .filter(|field| matches!(field.kind, FieldKind::Schema))
        .collect();
    if nested_fields.len() != nested_types.len() {
        return Err(syn::Error::new(
            name.span(),
            "internal nested field planning mismatch",
        ));
    }
    let nested_requires_parameter: Vec<bool> = nested_types
        .iter()
        .map(|(live, erased)| {
            let depends_on_parent =
                generic_closure(ir, [erased.clone()])
                    .params
                    .iter()
                    .any(|parameter| {
                        matches!(parameter, GenericParam::Type(_) | GenericParam::Const(_))
                    });
            let lifetime_erased = quote::ToTokens::to_token_stream(live).to_string()
                != quote::ToTokens::to_token_stream(erased).to_string();
            depends_on_parent || lifetime_erased
        })
        .collect();
    let wire_param_by_field: Vec<Option<syn::Ident>> = nested_requires_parameter
        .iter()
        .enumerate()
        .map(|(index, required)| {
            required.then(|| support_ident(module, &format!("__ZeroWire{index}")))
        })
        .collect();
    let error_param_by_field: Vec<Option<syn::Ident>> = nested_requires_parameter
        .iter()
        .enumerate()
        .map(|(index, required)| {
            required.then(|| support_ident(module, &format!("__ZeroError{index}")))
        })
        .collect();
    // Hidden wire storage is parameterized only by nested wire projections and
    // direct-storage const parameters. The logical public types never become fields,
    // so the runtime's required `Wire: 'static` does not leak into the public API.
    let mut wire_generics = generic_closure(
        ir,
        ir.fields
            .iter()
            .filter(|field| !matches!(field.kind, FieldKind::Schema))
            .map(|field| erase_lifetimes(&field.original_type)),
    );
    rebase_hidden_generics(&mut wire_generics, &hidden_schema);
    wire_generics.params = wire_generics
        .params
        .into_iter()
        .filter(|p| matches!(p, GenericParam::Const(_)))
        .collect();
    wire_generics.where_clause = None;
    let wire_params: Vec<syn::Ident> = wire_param_by_field.iter().flatten().cloned().collect();
    for parameter in wire_params.iter().rev() {
        wire_generics.params.insert(0, syn::parse_quote!(#parameter: #zerocopy::FromBytes + #zerocopy::KnownLayout + #zerocopy::Immutable + 'static));
    }
    let direct_const_args: Vec<_> = wire_generics
        .params
        .iter()
        .filter_map(|parameter| match parameter {
            GenericParam::Const(parameter) => Some(parameter.ident.clone()),
            _ => None,
        })
        .collect();
    let projected_wire_args: Vec<_> = nested_types
        .iter()
        .zip(&nested_fields)
        .zip(&wire_param_by_field)
        .filter_map(|(((live, _), field), parameter)| {
            parameter.as_ref().map(|_| {
                if field.external_tag_link.is_some() {
                    quote!(<#live as #rt::TaggedUnion>::PayloadWire)
                } else {
                    quote!(<#live as #rt::ZeroSchemaType>::Wire)
                }
            })
        })
        .collect();
    let encoded_wire_args: Vec<_> = nested_types
        .iter()
        .zip(&nested_fields)
        .zip(&wire_param_by_field)
        .filter_map(|(((_, erased), field), parameter)| {
            parameter.as_ref().map(|_| {
                let hidden_erased = rebase_hidden_type(erased.clone(), &hidden_schema);
                if field.external_tag_link.is_some() {
                    quote!(<#hidden_erased as #rt::TaggedUnion>::PayloadWire)
                } else {
                    quote!(<#hidden_erased as #rt::ZeroSchemaType>::Wire)
                }
            })
        })
        .collect();
    let wire_args = if wire_generics.params.is_empty() {
        quote!()
    } else {
        quote!(<#(#projected_wire_args,)* #(#direct_const_args),*>)
    };
    let encoded_wire_args = if wire_generics.params.is_empty() {
        quote!()
    } else {
        quote!(<#(#encoded_wire_args,)* #(#direct_const_args),*>)
    };
    let encoded_wire_ty = quote!(#wire #encoded_wire_args);
    let wire_ty = quote!(#module::#wire #wire_args);
    let (wire_ig, wire_tg, wire_wc) = wire_generics.split_for_impl();
    let mut wf = Vec::new();
    let mut dec = Vec::new();
    let mut decode_steps = Vec::new();
    let mut pre = Vec::new();
    let mut enc = Vec::new();
    let mut desc = Vec::new();
    let mut wrappers = Vec::new();
    let mut nested_decode_fields: Vec<(syn::Ident, Type, String)> = Vec::new();
    let mut nested_encode_fields: Vec<(syn::Ident, Type, String)> = Vec::new();
    let mut must_equal_consts = Vec::new();
    let mut field_bounds = Vec::new();
    let mut field_internal_padding: Vec<Vec<TokenStream>> = Vec::new();
    let mut layout_assertions: Vec<TokenStream> = Vec::new();
    let mut layout_inner_types: Vec<TokenStream> = Vec::new();
    let mut layout_field_types: Vec<TokenStream> = Vec::new();
    let mut nested_index = 0usize;
    if ir.external_tag_graph.len() != ir.fields.len()
        || ir
            .external_tag_graph
            .iter()
            .zip(&ir.fields)
            .any(|(planned, field)| *planned != field.external_tag_link)
    {
        return Err(syn::Error::new(
            name.span(),
            "internal external-tag graph mismatch",
        ));
    }
    let mut external_tag_indices: Vec<usize> =
        ir.external_tag_graph.iter().flatten().copied().collect();
    external_tag_indices.sort_unstable();
    external_tag_indices.dedup();
    let external_tag_cache_decls: Vec<_> = external_tag_indices
        .iter()
        .map(|tag_index| {
            let cache = support_ident(module, &format!("__zero_external_tag_cache_{tag_index}"));
            let tag_ty = &ir.fields[*tag_index].original_type;
            quote!(let mut #cache: ::core::option::Option<#tag_ty> = ::core::option::Option::None;)
        })
        .collect();
    for (idx, f) in ir.fields.iter().enumerate() {
        let local_id = support_ident(module, &format!("__zero_decoded_field_{idx}"));
        let id = &f.ident;
        let field_logical = id.to_string().trim_start_matches("r#").to_owned();
        let field_name = syn::LitStr::new(&field_logical, id.span());
        let cap = f.options.capacity.unwrap_or(0) as usize;
        let raw_off = quote!(::core::mem::offset_of!(#wire_ty,#id));
        let off = quote!(#raw_off);
        let wrapper = support_ident(module, &format!("__{logical}Field{idx}"));
        let aligned = f.options.align;
        let access = if aligned.is_some() {
            quote!(.value)
        } else {
            quote!()
        };
        let mut helper_padding = Vec::new();
        let (inner_ty, mut d, p, w, kind) = match &f.kind {
            FieldKind::Primitive(k) => {
                let wn = wire_name(*k, f.resolved.endian);
                let bytes = match f.resolved.endian {
                    Endian::Native => quote!(&self.#id.to_ne_bytes()),
                    Endian::Little => quote!(&self.#id.to_le_bytes()),
                    Endian::Big => quote!(&self.#id.to_be_bytes()),
                };
                let pk = primitive(rt, *k);
                let en = endian(rt, f.resolved.endian);
                (
                    quote!(#hidden::__private::#wn),
                    quote!(#input_local.wire().#id #access.get()),
                    quote!(),
                    quote!({let mut x=destination.subrange(#off,::core::mem::size_of::<#hidden::__private::#wn>()).map_err(#ee::Layout)?;x.write(0,#bytes).map_err(#ee::Layout)?;}),
                    quote!(#rt::FieldKind::Primitive{primitive:#pk,endian:#en}),
                )
            }
            FieldKind::Bool => (
                quote!(#hidden::__private::BoolWire),
                quote!(#input_local.wire().#id #access.decode().ok_or(#de::InvalidBool{field:#field_name,value:#input_local.wire().#id #access.raw()})?),
                quote!(),
                quote!({let mut x=destination.subrange(#off,1).map_err(#ee::Layout)?;x.write(0,&[self.#id as ::core::primitive::u8]).map_err(#ee::Layout)?;}),
                quote!(#rt::FieldKind::Bool),
            ),
            FieldKind::FixedBytes(n) => {
                let hidden_length = rebase_hidden_expr(n.clone(), &hidden_schema);
                (
                    quote!([::core::primitive::u8;#hidden_length]),
                    quote!(&#input_local.wire().#id #access),
                    quote!(),
                    quote!({let mut x=destination.subrange(#off,#n).map_err(#ee::Layout)?;#hidden::__private::encode_fixed_bytes(self.#id,&mut x).map_err(#ee::Layout)?;}),
                    quote!(#rt::FieldKind::FixedBytes{length:#n}),
                )
            }
            FieldKind::Utf8 | FieldKind::U16Str => {
                let lw = len_wire(f);
                let lr = format_ident!(
                    "{}",
                    f.resolved
                        .length_repr
                        .as_ref()
                        .map(ToString::to_string)
                        .unwrap_or_else(|| "u16".into())
                        .to_uppercase()
                );
                let en = endian(rt, f.resolved.endian);
                let wide = matches!(f.kind, FieldKind::U16Str);
                let ty = if wide {
                    quote!(#hidden::__private::FixedU16StrWire<#hidden::__private::#lw,#cap>)
                } else {
                    quote!(#hidden::__private::FixedStrWire<#hidden::__private::#lw,#cap>)
                };
                let z = matches!(f.resolved.tail, Tail::Zero);
                helper_padding.push(quote!(#rt::ByteRange::__new(
                    #off + <#ty>::LEN_OFFSET + ::core::mem::size_of::<#hidden::__private::#lw>(),
                    #off + <#ty>::DATA_OFFSET,
                )));
                let unit_size = if wide { 2usize } else { 1usize };
                let unit_align = unit_size;
                let layout_span = f.type_span;
                layout_assertions.push(quote_spanned!(layout_span=> {
                    let prefix_size = ::core::mem::size_of::<#hidden::__private::#lw>();
                    let prefix_align = ::core::mem::align_of::<#hidden::__private::#lw>();
                    let data_offset = match #hidden::__private::__checked_wire_stride(prefix_size, #unit_align) {
                        ::core::option::Option::Some(value) => value,
                        ::core::option::Option::None => ::core::panic!(::core::concat!(#logical, ".", #field_name, " data offset overflow")),
                    };
                    let data_size = match #cap.checked_mul(#unit_size) {
                        ::core::option::Option::Some(value) => value,
                        ::core::option::Option::None => ::core::panic!(::core::concat!(#logical, ".", #field_name, " data size overflow")),
                    };
                    let raw_end = match data_offset.checked_add(data_size) {
                        ::core::option::Option::Some(value) => value,
                        ::core::option::Option::None => ::core::panic!(::core::concat!(#logical, ".", #field_name, " helper size overflow")),
                    };
                    let helper_align = if prefix_align > #unit_align { prefix_align } else { #unit_align };
                    let helper_size = match #hidden::__private::__checked_wire_stride(raw_end, helper_align) {
                        ::core::option::Option::Some(value) => value,
                        ::core::option::Option::None => ::core::panic!(::core::concat!(#logical, ".", #field_name, " trailing layout overflow")),
                    };
                    ::core::assert!(<#ty>::LEN_OFFSET == 0, ::core::concat!(#logical, ".", #field_name, " length offset mismatch"));
                    ::core::assert!(<#ty>::DATA_OFFSET == data_offset, ::core::concat!(#logical, ".", #field_name, " data offset mismatch"));
                    ::core::assert!(::core::mem::align_of::<#ty>() == helper_align, ::core::concat!(#logical, ".", #field_name, " helper alignment mismatch"));
                    ::core::assert!(::core::mem::size_of::<#ty>() == helper_size, ::core::concat!(#logical, ".", #field_name, " helper size mismatch"));
                }));
                helper_padding.push(quote!(#rt::ByteRange::__new(
                    #off + <#ty>::DATA_OFFSET + #cap * #unit_size,
                    #off + ::core::mem::size_of::<#ty>(),
                )));
                let d = if wide {
                    quote!(#hidden::__private::decode_u16_str(#input_local.wire().#id #access.len_wire(),#input_local.wire().#id #access.units(),#z).map_err(|e|#de::__codec(#field_name,e))?)
                } else {
                    quote!(#hidden::__private::decode_str(#input_local.wire().#id #access.len_wire(),#input_local.wire().#id #access.data(),#z).map_err(|e|#de::__codec(#field_name,e))?)
                };
                let p = if wide {
                    quote!(#hidden::__private::validate_u16_str_encode(self.#id,#cap).map_err(|e|#ee::__codec(#field_name,e))?;)
                } else {
                    quote!(#hidden::__private::validate_str_encode(self.#id,#cap).map_err(|e|#ee::__codec(#field_name,e))?;)
                };
                let w = if wide {
                    quote!({let mut l=destination.subrange(#off+<#ty>::LEN_OFFSET,::core::mem::size_of::<#hidden::__private::#lw>()).map_err(#ee::Layout)?;#hidden::__private::encode_length::<#hidden::__private::#lw>(self.#id.len(),&mut l).map_err(#ee::Layout)?;let mut x=destination.subrange(#off+<#ty>::DATA_OFFSET,#cap*2).map_err(#ee::Layout)?;#hidden::__private::encode_u16_str(self.#id,&mut x).map_err(#ee::Layout)?;})
                } else {
                    quote!({let mut l=destination.subrange(#off+<#ty>::LEN_OFFSET,::core::mem::size_of::<#hidden::__private::#lw>()).map_err(#ee::Layout)?;#hidden::__private::encode_length::<#hidden::__private::#lw>(self.#id.len(),&mut l).map_err(#ee::Layout)?;let mut x=destination.subrange(#off+<#ty>::DATA_OFFSET,#cap).map_err(#ee::Layout)?;#hidden::__private::encode_str(self.#id,&mut x).map_err(#ee::Layout)?;})
                };
                let se = if wide {
                    quote!(#rt::StringEncoding::U16)
                } else {
                    quote!(#rt::StringEncoding::Utf8)
                };
                let tp = tail(rt, f.resolved.tail);
                let unit_endian = if wide {
                    quote!(::core::option::Option::Some(#en))
                } else {
                    quote!(::core::option::Option::None)
                };
                (
                    ty.clone(),
                    d,
                    p,
                    w,
                    quote!(#rt::FieldKind::String(#rt::StringDescriptor::__new(#se,#unit_endian,#cap,::core::option::Option::Some(#rt::LengthDescriptor::__new(#rt::LengthRepr::#lr,#en,<#ty>::LEN_OFFSET)),<#ty>::DATA_OFFSET,#tp))),
                )
            }
            FieldKind::CStr | FieldKind::U16CStr => {
                let wide = matches!(f.kind, FieldKind::U16CStr);
                let ty = if wide {
                    quote!([::core::primitive::u16;#cap])
                } else {
                    quote!([::core::primitive::u8;#cap])
                };
                let z = matches!(f.resolved.tail, Tail::Zero);
                let d = if wide {
                    quote!(#hidden::__private::decode_u16_c_str(&#input_local.wire().#id #access,#z).map_err(|e|#de::__codec(#field_name,e))?)
                } else {
                    quote!(#hidden::__private::decode_c_str(&#input_local.wire().#id #access,#z).map_err(|e|#de::__codec(#field_name,e))?)
                };
                let p = if wide {
                    quote!(#hidden::__private::validate_u16_c_str_encode(self.#id,#cap).map_err(|e|#ee::__codec(#field_name,e))?;)
                } else {
                    quote!(#hidden::__private::validate_c_str_encode(self.#id,#cap).map_err(|e|#ee::__codec(#field_name,e))?;)
                };
                let w = if wide {
                    quote!({let mut x=destination.subrange(#off,#cap*2).map_err(#ee::Layout)?;#hidden::__private::encode_u16_c_str(self.#id,&mut x).map_err(#ee::Layout)?;})
                } else {
                    quote!({let mut x=destination.subrange(#off,#cap).map_err(#ee::Layout)?;#hidden::__private::encode_c_str(self.#id,&mut x).map_err(#ee::Layout)?;})
                };
                let se = if wide {
                    quote!(#rt::StringEncoding::U16C)
                } else {
                    quote!(#rt::StringEncoding::CBytes)
                };
                let tp = tail(rt, f.resolved.tail);
                let en = endian(rt, f.resolved.endian);
                let unit_endian = if wide {
                    quote!(::core::option::Option::Some(#en))
                } else {
                    quote!(::core::option::Option::None)
                };
                (
                    ty,
                    d,
                    p,
                    w,
                    quote!(#rt::FieldKind::String(#rt::StringDescriptor::__new(#se,#unit_endian,#cap,::core::option::Option::None,0,#tp))),
                )
            }
            FieldKind::Schema => {
                let live = &f.original_type;
                let erased = erase_lifetimes(live);
                let wire_storage = match &wire_param_by_field[nested_index] {
                    Some(parameter) => quote!(#parameter),
                    None => {
                        let hidden_erased = rebase_hidden_type(erased.clone(), &hidden_schema);
                        if f.external_tag_link.is_some() {
                            quote!(<#hidden_erased as #rt::TaggedUnion>::PayloadWire)
                        } else {
                            quote!(<#hidden_erased as #rt::ZeroSchemaType>::Wire)
                        }
                    }
                };
                nested_index += 1;
                let field_name = field_logical.clone();
                let decode_ctor = format_ident!("__new_{}", id);
                let encode_ctor = format_ident!("__new_{}", id);
                nested_decode_fields.push((id.clone(), erased.clone(), field_name.clone()));
                nested_encode_fields.push((id.clone(), erased.clone(), field_name.clone()));
                if let Some(tag_index) = f.external_tag_link {
                    let tag_field = &ir.fields[tag_index];
                    let tag_id = &tag_field.ident;
                    let tag_live = &tag_field.original_type;
                    let tag_name_text = tag_id.to_string().trim_start_matches("r#").to_owned();
                    let tag_name = syn::LitStr::new(&tag_name_text, tag_id.span());
                    let tag_ctor = format_ident!("__new_{}", tag_id);
                    let tag_cache =
                        support_ident(module, &format!("__zero_external_tag_cache_{tag_index}"));
                    let tag_off = quote!(::core::mem::offset_of!(#wire_ty,#tag_id));
                    (
                        quote!(#wire_storage),
                        quote!({
                            if #tag_cache.is_none() {
                                ::core::assert!(<#tag_live as #rt::ZeroSchemaType>::WIRE_SIZE > 0);
                                let tag_input = #input_local.subrange::<<#tag_live as #rt::ZeroSchemaType>::Wire>(#tag_off).map_err(|error| #de::Nested(#module::NestedDecodeError::#tag_ctor(<#tag_live as #rt::ScalarEnum>::__decode_layout(error))))?;
                                #tag_cache = ::core::option::Option::Some(<#tag_live as #rt::__private::DecodeWire<#decode_lt>>::decode_at(tag_input).map_err(|source| #de::Nested(#module::NestedDecodeError::#tag_ctor(source)))?);
                            }
                            let payload_input = #input_local.subrange::<<#live as #rt::TaggedUnion>::PayloadWire>(#off).map_err(#de::Layout)?;
                            let value = <#live as #rt::__private::DecodeTaggedUnion<#decode_lt>>::decode_payload(#tag_cache.as_ref().unwrap(), payload_input).map_err(|source| #de::Nested(#module::NestedDecodeError::#decode_ctor(source)))?;
                            <#live as #rt::__private::DecodeTaggedUnion<#decode_lt>>::validate_decoded(&value).map_err(|source| #de::Nested(#module::NestedDecodeError::#decode_ctor(source)))?;
                            value
                        }),
                        quote!({
                            let declared: ::core::primitive::u64 = #rt::ScalarEnum::to_raw(&self.#tag_id).into();
                            let selected_tag = <#live as #rt::TaggedUnion>::tag(&self.#id);
                            let selected: ::core::primitive::u64 = #rt::ScalarEnum::to_raw(&selected_tag).into();
                            if declared != selected { return ::core::result::Result::Err(#ee::TagMismatch{field:#field_name,tag_field:#tag_name,declared,selected}); }
                            <#live as #rt::__private::EncodeWire>::validate_encode(&self.#id).map_err(|source| #ee::Nested(#module::NestedEncodeError::#encode_ctor(source)))?;
                        }),
                        quote!({
                            let mut child = destination.subrange(#off, ::core::mem::size_of::<<#live as #rt::TaggedUnion>::PayloadWire>()).map_err(#ee::Layout)?;
                            <#live as #rt::TaggedUnion>::encode_payload_at(&self.#id, &mut child).map_err(|source| #ee::Nested(#module::NestedEncodeError::#encode_ctor(source)))?;
                        }),
                        quote!(#rt::FieldKind::ExternalTaggedUnion { layout: <#live as #rt::ZeroSchemaType>::LAYOUT, tag_field: #tag_name }),
                    )
                } else {
                    (
                        quote!(#wire_storage),
                        quote!({
                            ::core::assert!(<#live as #rt::ZeroSchemaType>::WIRE_SIZE > 0);
                            let child = #input_local.subrange::<<#live as #rt::ZeroSchemaType>::Wire>(#off).map_err(#de::Layout)?;
                            <#live as #rt::__private::DecodeWire<#decode_lt>>::decode_at(child).map_err(|source| #de::Nested(#module::NestedDecodeError::#decode_ctor(source)))?
                        }),
                        quote!({ ::core::assert!(<#live as #rt::ZeroSchemaType>::WIRE_SIZE > 0); <#live as #rt::__private::EncodeWire>::validate_encode(&self.#id).map_err(|source| #ee::Nested(#module::NestedEncodeError::#encode_ctor(source)))?; }),
                        quote!({ ::core::assert!(<#live as #rt::ZeroSchemaType>::WIRE_SIZE > 0); let mut child = destination.subrange(#off, <#live as #rt::ZeroSchemaType>::WIRE_SIZE).map_err(#ee::Layout)?; <#live as #rt::__private::EncodeWire>::encode_at(&self.#id, &mut child).map_err(|source| #ee::Nested(#module::NestedEncodeError::#encode_ctor(source)))?; }),
                        quote!(#rt::FieldKind::Schema { layout: <#live as #rt::ZeroSchemaType>::LAYOUT }),
                    )
                }
            }
        };
        let mut internal_padding = Vec::new();
        internal_padding.extend(helper_padding);
        let outer_inner_ty = match &f.kind {
            FieldKind::Primitive(kind) => {
                let wire_name = wire_name(*kind, f.resolved.endian);
                quote!(#rt::__private::#wire_name)
            }
            FieldKind::Bool => quote!(#rt::__private::BoolWire),
            FieldKind::FixedBytes(length) => quote!([::core::primitive::u8;#length]),
            FieldKind::Utf8 | FieldKind::U16Str => {
                let length_wire = len_wire(f);
                if matches!(f.kind, FieldKind::U16Str) {
                    quote!(#rt::__private::FixedU16StrWire<#rt::__private::#length_wire,#cap>)
                } else {
                    quote!(#rt::__private::FixedStrWire<#rt::__private::#length_wire,#cap>)
                }
            }
            FieldKind::CStr => quote!([::core::primitive::u8;#cap]),
            FieldKind::U16CStr => quote!([::core::primitive::u16;#cap]),
            FieldKind::Schema => {
                let live = &nested_types[nested_index - 1].0;
                if f.external_tag_link.is_some() {
                    quote!(<#live as #rt::TaggedUnion>::PayloadWire)
                } else {
                    quote!(<#live as #rt::ZeroSchemaType>::Wire)
                }
            }
        };
        let ty = if let Some(a) = aligned {
            let align = syn::LitInt::new(&a.to_string(), id.span());
            wrappers.push(quote!(#[repr(C,align(#align))]#[derive(#zerocopy::FromBytes,#zerocopy::KnownLayout,#zerocopy::Immutable)]#[zerocopy(crate = #zerocopy_crate)]#support_vis struct #wrapper<T>{pub(super) value:T,__end:[::core::primitive::u8;0]}));
            quote!(#wrapper<#inner_ty>)
        } else {
            inner_ty.clone()
        };
        layout_inner_types.push(inner_ty.clone());
        layout_field_types.push(ty.clone());
        let outer_ty = if aligned.is_some() {
            quote!(#module::#wrapper<#outer_inner_ty>)
        } else {
            outer_inner_ty.clone()
        };
        wf.push(quote!(pub(super) #id:#ty));
        if external_tag_indices.contains(&idx) {
            let cache = support_ident(module, &format!("__zero_external_tag_cache_{idx}"));
            dec.push(quote!(#id: #cache.take().unwrap()));
        } else {
            dec.push(quote!(#id: #local_id));
        }
        enc.push(w);
        desc.push(quote!(#rt::FieldDescriptor::__new(#field_name,#idx,#off,::core::mem::size_of::<#outer_ty>(),::core::mem::align_of::<#outer_ty>(),#kind)));
        field_bounds.push((
            off.clone(),
            quote!(#off+::core::mem::size_of::<#outer_ty>()),
        ));
        if aligned.is_some() {
            internal_padding.push(quote!(#rt::ByteRange::__new(#off + ::core::mem::size_of::<#outer_inner_ty>(), #off + ::core::mem::size_of::<#outer_ty>())));
        }
        field_internal_padding.push(internal_padding);
        let mut decode_checks = Vec::new();
        let mut encode_checks = Vec::new();
        if let Some(range) = &f.options.range {
            decode_checks.push(quote!(if !(#range).contains(&#local_id){return Err(#de::RangeViolation{field:#field_name});}));
            encode_checks.push(quote!(if !(#range).contains(&self.#id){return Err(#ee::RangeViolation{field:#field_name});}));
        }
        if let Some(eq) = &f.options.must_equal {
            let const_id = support_ident(module, &format!("__ZERO_MUST_EQUAL_FIELD_{idx}"));
            let field_ty = &f.original_type;
            must_equal_consts.push(quote!(const #const_id: #field_ty = #eq;));
            if matches!(f.kind, FieldKind::Schema) {
                if external_tag_indices.contains(&idx) {
                    decode_checks.push(quote!(if #rt::ScalarEnum::to_raw(#local_id) != #rt::ScalarEnum::to_raw(&Self::#const_id){return Err(#de::MustEqualViolation{field:#field_name});}));
                } else {
                    decode_checks.push(quote!(if #rt::ScalarEnum::to_raw(&#local_id) != #rt::ScalarEnum::to_raw(&Self::#const_id){return Err(#de::MustEqualViolation{field:#field_name});}));
                }
                encode_checks.push(quote!(if #rt::ScalarEnum::to_raw(&self.#id) != #rt::ScalarEnum::to_raw(&Self::#const_id){return Err(#ee::MustEqualViolation{field:#field_name});}));
            } else {
                decode_checks.push(quote!(if #local_id != Self::#const_id{return Err(#de::MustEqualViolation{field:#field_name});}));
                encode_checks.push(quote!(if self.#id != Self::#const_id{return Err(#ee::MustEqualViolation{field:#field_name});}));
            }
        }
        if let Some(v) = &f.options.validate_with {
            let field_ty = &f.original_type;
            let borrowed = matches!(
                f.kind,
                FieldKind::Utf8
                    | FieldKind::CStr
                    | FieldKind::U16Str
                    | FieldKind::U16CStr
                    | FieldKind::FixedBytes(_)
            );
            let argument_ty = if borrowed {
                quote!(#field_ty)
            } else {
                quote!(&#field_ty)
            };
            let decode_arg = if borrowed || external_tag_indices.contains(&idx) {
                quote!(#local_id)
            } else {
                quote!(&#local_id)
            };
            let encode_arg = if borrowed {
                quote!(self.#id)
            } else {
                quote!(&self.#id)
            };
            decode_checks.push(quote!({let __zero_field_validator: fn(#argument_ty, &#rt::ValidationContext<'_>) -> #rt::ValidationResult = #v; __zero_field_validator(#decode_arg,&#rt::ValidationContext::__field(<Self as #rt::ZeroSchemaType>::LAYOUT,#field_name,#rt::ValidationOperation::Decode)).map_err(|source|#de::Custom{field: ::core::option::Option::Some(#field_name),variant: ::core::option::Option::None,source})?;}));
            encode_checks.push(quote!({let __zero_field_validator: fn(#argument_ty, &#rt::ValidationContext<'_>) -> #rt::ValidationResult = #v; __zero_field_validator(#encode_arg,&#rt::ValidationContext::__field(<Self as #rt::ZeroSchemaType>::LAYOUT,#field_name,#rt::ValidationOperation::Encode)).map_err(|source|#ee::Custom{field: ::core::option::Option::Some(#field_name),variant: ::core::option::Option::None,source})?;}));
        }
        if external_tag_indices.contains(&idx) {
            let live = &f.original_type;
            let cache = support_ident(module, &format!("__zero_external_tag_cache_{idx}"));
            let decode_ctor = format_ident!("__new_{}", id);
            d = quote!({
                if #cache.is_none() {
                    ::core::assert!(<#live as #rt::ZeroSchemaType>::WIRE_SIZE > 0);
                    let child = #input_local.subrange::<<#live as #rt::ZeroSchemaType>::Wire>(#off).map_err(|error| #de::Nested(#module::NestedDecodeError::#decode_ctor(<#live as #rt::ScalarEnum>::__decode_layout(error))))?;
                    #cache = ::core::option::Option::Some(<#live as #rt::__private::DecodeWire<#decode_lt>>::decode_at(child).map_err(|source| #de::Nested(#module::NestedDecodeError::#decode_ctor(source)))?);
                }
                #cache.as_ref().unwrap()
            });
        }
        decode_steps.push(quote!(let #local_id = #d; #(#decode_checks)*));
        pre.push(quote!(#p #(#encode_checks)*));
    }
    if ir.layout_plan.fields.len() != layout_field_types.len() {
        return Err(syn::Error::new(
            ir.ident.span(),
            "internal layout field count mismatch",
        ));
    }
    let checked_layout_assertions: Vec<_> = ir
        .layout_plan
        .checked
        .iter()
        .map(|check| {
            let valid_operation = matches!(
                (&check.op, &check.expression),
                (CheckedLayoutOp::AlignUp, LayoutExpr::AlignUp(_, _))
                    | (CheckedLayoutOp::Add, LayoutExpr::Add(_, _))
            );
            let span = check.span;
            let message = syn::LitStr::new("zero-schema checked layout operation overflowed", span);
            let expression = lower_layout_expr(&check.expression, rt, &layout_field_types, &message);
            quote_spanned!(span=> {
                ::core::assert!(#valid_operation, "zero-schema internal checked layout operation mismatch");
                let _ = #expression;
            })
        })
        .collect();
    for (position, field) in ir.layout_plan.fields.iter().enumerate() {
        if field.field_index != position {
            return Err(syn::Error::new(
                ir.fields[position].type_span,
                "internal layout field index mismatch",
            ));
        }
        let source = &ir.fields[position];
        let id = &source.ident;
        let field_name_text = id.to_string().trim_start_matches("r#").to_owned();
        let field_name = syn::LitStr::new(&field_name_text, id.span());
        let span = source.type_span;
        let overflow_message = syn::LitStr::new(
            &format!("{logical}.{field_name_text} layout overflow"),
            span,
        );
        let offset = lower_layout_expr(&field.offset, rt, &layout_field_types, &overflow_message);
        let stride = lower_layout_expr(&field.stride, rt, &layout_field_types, &overflow_message);
        let inner_ty = &layout_inner_types[position];
        let field_ty = &layout_field_types[position];
        let symbolic_assertion = match &field.size {
            SymbolicSize::Fixed(size) => quote! {
                ::core::assert!(::core::mem::size_of::<#inner_ty>() == #size as ::core::primitive::usize, ::core::concat!(#logical, ".", #field_name, " symbolic size mismatch"));
            },
            SymbolicSize::Expr(size) => {
                let hidden_size = rebase_hidden_expr(size.clone(), &hidden_schema);
                quote! {
                    ::core::assert!(::core::mem::size_of::<#inner_ty>() == (#hidden_size), ::core::concat!(#logical, ".", #field_name, " symbolic size mismatch"));
                }
            }
            SymbolicSize::Type(logical_ty) => {
                let _ = logical_ty;
                quote!()
            }
            SymbolicSize::String {
                helper,
                capacity,
                unit_size,
                length_size,
            } => {
                let capacity = *capacity;
                let unit_size = *unit_size;
                match helper {
                    StringHelper::Utf8 | StringHelper::U16Str => {
                        let Some(length_size) = *length_size else {
                            return Err(syn::Error::new(
                                span,
                                "internal length-prefixed string layout is missing its prefix",
                            ));
                        };
                        quote! {
                            let data_offset = match #rt::__private::__checked_wire_stride(#length_size as ::core::primitive::usize, #unit_size as ::core::primitive::usize) {
                                ::core::option::Option::Some(value) => value,
                                ::core::option::Option::None => ::core::panic!(#overflow_message),
                            };
                            let data_size = match (#capacity as ::core::primitive::usize).checked_mul(#unit_size as ::core::primitive::usize) {
                                ::core::option::Option::Some(value) => value,
                                ::core::option::Option::None => ::core::panic!(#overflow_message),
                            };
                            let raw_size = match data_offset.checked_add(data_size) {
                                ::core::option::Option::Some(value) => value,
                                ::core::option::Option::None => ::core::panic!(#overflow_message),
                            };
                            let helper_align = if #length_size > #unit_size { #length_size } else { #unit_size };
                            let helper_size = match #rt::__private::__checked_wire_stride(raw_size, helper_align as ::core::primitive::usize) {
                                ::core::option::Option::Some(value) => value,
                                ::core::option::Option::None => ::core::panic!(#overflow_message),
                            };
                            ::core::assert!(::core::mem::align_of::<#inner_ty>() == helper_align as ::core::primitive::usize, ::core::concat!(#logical, ".", #field_name, " string helper alignment mismatch"));
                            ::core::assert!(::core::mem::size_of::<#inner_ty>() == helper_size, ::core::concat!(#logical, ".", #field_name, " string helper size mismatch"));
                        }
                    }
                    StringHelper::CStr | StringHelper::U16CStr => {
                        if length_size.is_some() {
                            return Err(syn::Error::new(
                                span,
                                "internal nul-terminated string layout has a length prefix",
                            ));
                        }
                        quote! {
                            let helper_size = match (#capacity as ::core::primitive::usize).checked_mul(#unit_size as ::core::primitive::usize) {
                                ::core::option::Option::Some(value) => value,
                                ::core::option::Option::None => ::core::panic!(#overflow_message),
                            };
                            ::core::assert!(::core::mem::align_of::<#inner_ty>() == #unit_size as ::core::primitive::usize, ::core::concat!(#logical, ".", #field_name, " string helper alignment mismatch"));
                            ::core::assert!(::core::mem::size_of::<#inner_ty>() == helper_size, ::core::concat!(#logical, ".", #field_name, " string helper size mismatch"));
                        }
                    }
                }
            }
        };
        let wrapper_assertion = if let Some(explicit_align) = field.align {
            quote! {
                let inner_align = ::core::mem::align_of::<#inner_ty>();
                let expected_align = if inner_align > #explicit_align as ::core::primitive::usize { inner_align } else { #explicit_align as ::core::primitive::usize };
                let expected_size = match #rt::__private::__checked_wire_stride(::core::mem::size_of::<#inner_ty>(), expected_align) {
                    ::core::option::Option::Some(value) => value,
                    ::core::option::Option::None => ::core::panic!(#overflow_message),
                };
                ::core::assert!(::core::mem::align_of::<#field_ty>() == expected_align, ::core::concat!(#logical, ".", #field_name, " aligned wrapper alignment mismatch"));
                ::core::assert!(::core::mem::size_of::<#field_ty>() == expected_size, ::core::concat!(#logical, ".", #field_name, " aligned wrapper size mismatch"));
            }
        } else {
            quote! {
                ::core::assert!(::core::mem::align_of::<#field_ty>() == ::core::mem::align_of::<#inner_ty>(), ::core::concat!(#logical, ".", #field_name, " field alignment mismatch"));
                ::core::assert!(::core::mem::size_of::<#field_ty>() == ::core::mem::size_of::<#inner_ty>(), ::core::concat!(#logical, ".", #field_name, " field size mismatch"));
            }
        };
        layout_assertions.push(quote_spanned!(span=> {
            #symbolic_assertion
            #wrapper_assertion
            let expected_offset = #offset;
            let expected_stride = #stride;
            let actual_stride = match expected_offset.checked_add(::core::mem::size_of::<#field_ty>()) {
                ::core::option::Option::Some(value) => value,
                ::core::option::Option::None => ::core::panic!(#overflow_message),
            };
            ::core::assert!(::core::mem::offset_of!(Self, #id) == expected_offset, ::core::concat!(#logical, ".", #field_name, " field offset mismatch"));
            ::core::assert!(expected_stride == actual_stride, ::core::concat!(#logical, ".", #field_name, " field stride mismatch"));
        }));
    }
    let aggregate_message = syn::LitStr::new(
        &format!("{logical} aggregate layout overflow"),
        ir.ident.span(),
    );
    let aggregate_align = ir.layout_plan.aggregate_align.as_ref().ok_or_else(|| {
        syn::Error::new(ir.ident.span(), "internal aggregate alignment is missing")
    })?;
    let aggregate_size =
        ir.layout_plan.aggregate_size.as_ref().ok_or_else(|| {
            syn::Error::new(ir.ident.span(), "internal aggregate size is missing")
        })?;
    let aggregate_stride =
        ir.layout_plan.aggregate_stride.as_ref().ok_or_else(|| {
            syn::Error::new(ir.ident.span(), "internal aggregate stride is missing")
        })?;
    let aggregate_align =
        lower_layout_expr(aggregate_align, rt, &layout_field_types, &aggregate_message);
    let aggregate_size =
        lower_layout_expr(aggregate_size, rt, &layout_field_types, &aggregate_message);
    let aggregate_stride = lower_layout_expr(
        aggregate_stride,
        rt,
        &layout_field_types,
        &aggregate_message,
    );
    let aggregate_cursor = ir
        .layout_plan
        .fields
        .last()
        .map(|field| lower_layout_expr(&field.stride, rt, &layout_field_types, &aggregate_message))
        .unwrap_or_else(|| quote!(0usize));
    if ir.layout_plan.root_align != ir.options.align {
        return Err(syn::Error::new(
            ir.ident.span(),
            "internal root alignment plan mismatch",
        ));
    }
    let nested_wire_assertions: Vec<_> = ir
        .fields
        .iter()
        .filter(|field| matches!(field.kind, FieldKind::Schema))
        .map(|field| {
            let live = &field.original_type;
            let span = field.type_span;
            quote_spanned!(span=> ::core::assert!(<#live as #rt::ZeroSchemaType>::WIRE_SIZE > 0);)
        })
        .collect();
    let wide_target_checks: Vec<_> = ir
        .layout_plan
        .wide_checks
        .iter()
        .map(|(index, endian)| {
            let span = ir.fields[*index].type_span;
            let target = match endian {
                Endian::Little => "little",
                Endian::Big => "big",
                Endian::Native => return quote!(),
            };
            let message =
                format!("wide string wire representation requires a {target}-endian target");
            quote_spanned!(span=> #[cfg(not(target_endian = #target))] compile_error!(#message);)
        })
        .collect();
    let mut layout_generics = ir.cleaned_generics.clone();
    for obligation in &ir.obligations {
        let Some(ty) = &obligation.ty else { continue };
        let span = obligation.span;
        let predicate: Option<syn::WherePredicate> = match &obligation.kind {
            ObligationKind::Schema => None,
            ObligationKind::ScalarEnum | ObligationKind::ScalarWire => {
                Some(syn::parse_quote_spanned!(span=> #ty: #rt::ScalarEnum))
            }
            ObligationKind::TaggedUnion => {
                Some(syn::parse_quote_spanned!(span=> #ty: #rt::TaggedUnion))
            }
            ObligationKind::Decode
            | ObligationKind::Encode
            | ObligationKind::DecodeTaggedUnion
            | ObligationKind::Validator
            | ObligationKind::Layout
            | ObligationKind::Tail
            | ObligationKind::Padding
            | ObligationKind::WholeInput
            | ObligationKind::ExternalTag => None,
            ObligationKind::WideTarget(endian) => {
                let _ = *endian;
                None
            }
        };
        if let Some(predicate) = predicate {
            layout_generics
                .make_where_clause()
                .predicates
                .push(predicate);
        }
    }
    for (index, ((live, _), field)) in nested_types.iter().zip(&nested_fields).enumerate() {
        if wire_param_by_field[index].is_some() && field.external_tag_link.is_none() {
            let span = field.type_span;
            layout_generics
                .make_where_clause()
                .predicates
                .push(syn::parse_quote_spanned!(span=> #live: #rt::ZeroSchemaType));
        }
    }
    for field in ir
        .fields
        .iter()
        .filter(|field| matches!(field.kind, FieldKind::Schema))
    {
        let live = &field.original_type;
        if let Some(tag_index) = field.external_tag_link {
            let tag = &ir.fields[tag_index].original_type;
            layout_generics
                .make_where_clause()
                .predicates
                .push(syn::parse_quote!(#tag: #rt::ScalarEnum));
            layout_generics
                .make_where_clause()
                .predicates
                .push(syn::parse_quote!(#live: #rt::TaggedUnion<Tag = #tag>));
        }
    }
    let mut decode_generics = layout_generics.clone();
    decode_generics
        .params
        .insert(0, GenericParam::Lifetime(syn::parse_quote!(#source_lt)));
    if let Some(borrow_lifetime) = &ir.borrow_lifetime {
        decode_generics
            .make_where_clause()
            .predicates
            .push(syn::parse_quote!(#source_lt: #borrow_lifetime));
    }
    for (index, ((live, _), field)) in nested_types.iter().zip(&nested_fields).enumerate() {
        if wire_param_by_field[index].is_none() {
            continue;
        }
        let span = field.type_span;
        let predicate = if field.external_tag_link.is_some() {
            syn::parse_quote_spanned!(span=> #live: #rt::__private::DecodeTaggedUnion<#source_lt>)
        } else {
            syn::parse_quote_spanned!(span=> #live: #rt::__private::DecodeWire<#source_lt>)
        };
        decode_generics
            .make_where_clause()
            .predicates
            .push(predicate);
    }
    let (decode_ig, _, decode_wc) = decode_generics.split_for_impl();
    let original_args = generic_arguments(&ir.original_generics);
    let decode_impl = quote!(impl #decode_ig #rt::__private::DecodeWire<#source_lt> for #name #original_args #decode_wc);
    let parse_sig = quote!(bytes: &#source_lt [::core::primitive::u8]);
    let mut encode_generics = layout_generics.clone();
    for (index, ((live, _), field)) in nested_types.iter().zip(&nested_fields).enumerate() {
        if wire_param_by_field[index].is_none() {
            continue;
        }
        let span = field.type_span;
        encode_generics
            .make_where_clause()
            .predicates
            .push(syn::parse_quote_spanned!(span=> #live: #rt::__private::EncodeWire));
    }
    let (encode_ig, _, encode_wc) = encode_generics.split_for_impl();
    let (layout_ig, layout_tg, layout_wc) = layout_generics.split_for_impl();
    let generics = &ir.cleaned_generics;
    let (ig, tg, wc) = generics.split_for_impl();
    let mut padding_ranges = Vec::new();
    if let Some((first_start, _)) = field_bounds.first() {
        padding_ranges.push(quote!(#rt::ByteRange::__new(0, #first_start)));
    }
    for index in 0..field_bounds.len() {
        padding_ranges.extend(field_internal_padding[index].iter().cloned());
        let (_, end) = &field_bounds[index];
        let next = field_bounds
            .get(index + 1)
            .map(|(start, _)| start.clone())
            .unwrap_or_else(|| quote!(::core::mem::size_of::<#wire_ty>()));
        padding_ranges.push(quote!(#rt::ByteRange::__new(#end, #next)));
    }
    let decode_padding = if matches!(ir.options.padding, Padding::Zero) {
        quote!(for range in &[#(#padding_ranges),*] { for (offset, byte) in #input_local.bytes()[range.start()..range.end()].iter().enumerate() { if *byte != 0 { return ::core::result::Result::Err(#de::NonZeroPadding { offset: range.start() + offset }); } } })
    } else {
        quote!()
    };
    let pad = match ir.options.padding {
        Padding::Ignore => quote!(#rt::PaddingPolicy::Ignore),
        Padding::Zero => quote!(#rt::PaddingPolicy::Zero),
    };
    let decode_whole = ir.options.validate_with.as_ref().map(|whole_validator| quote!({
        let __zero_whole_validator: fn(&Self, &#rt::ValidationContext<'_>) -> #rt::ValidationResult = #whole_validator;
        __zero_whole_validator(&#decoded_value_local, &#rt::ValidationContext::__whole(<Self as #rt::ZeroSchemaType>::LAYOUT, ::core::option::Option::None, #rt::ValidationOperation::Decode))
            .map_err(|source| #de::Custom { field: ::core::option::Option::None, variant: ::core::option::Option::None, source })?;
    }));
    let encode_whole = ir.options.validate_with.as_ref().map(|whole_validator| quote!({
        let __zero_whole_validator: fn(&Self, &#rt::ValidationContext<'_>) -> #rt::ValidationResult = #whole_validator;
        __zero_whole_validator(self, &#rt::ValidationContext::__whole(<Self as #rt::ZeroSchemaType>::LAYOUT, ::core::option::Option::None, #rt::ValidationOperation::Encode))
            .map_err(|source| #ee::Custom { field: ::core::option::Option::None, variant: ::core::option::Option::None, source })?;
    }));
    let mut decode_cases = ir.error_shape.decode.clone();
    let mut decode_seen = Vec::new();
    decode_cases.retain(|case| {
        if decode_seen.contains(case) {
            false
        } else {
            decode_seen.push(*case);
            true
        }
    });
    let mut encode_cases = ir.error_shape.encode.clone();
    let mut encode_seen = Vec::new();
    encode_cases.retain(|case| {
        if encode_seen.contains(case) {
            false
        } else {
            encode_seen.push(*case);
            true
        }
    });
    let decode_tokens: Vec<_> = decode_cases
        .iter()
        .copied()
        .filter(|case| *case != ErrorCase::Nested)
        .map(|case| error_tokens(case, rt))
        .collect();
    let encode_tokens: Vec<_> = encode_cases
        .iter()
        .copied()
        .filter(|case| *case != ErrorCase::Nested)
        .map(|case| error_tokens(case, rt))
        .collect();
    let decode_variants: Vec<_> = decode_tokens.iter().map(|tokens| &tokens.0).collect();
    let decode_kinds: Vec<_> = decode_tokens.iter().map(|tokens| &tokens.1).collect();
    let decode_sources: Vec<_> = decode_tokens.iter().map(|tokens| &tokens.2).collect();
    let decode_segments: Vec<_> = decode_tokens.iter().map(|tokens| &tokens.3).collect();
    let decode_leaves: Vec<_> = decode_tokens.iter().map(|tokens| &tokens.4).collect();
    let decode_codes: Vec<_> = decode_tokens.iter().map(|tokens| &tokens.5).collect();
    let encode_variants: Vec<_> = encode_tokens.iter().map(|tokens| &tokens.0).collect();
    let encode_kinds: Vec<_> = encode_tokens.iter().map(|tokens| &tokens.1).collect();
    let encode_sources: Vec<_> = encode_tokens.iter().map(|tokens| &tokens.2).collect();
    let encode_segments: Vec<_> = encode_tokens.iter().map(|tokens| &tokens.3).collect();
    let encode_leaves: Vec<_> = encode_tokens.iter().map(|tokens| &tokens.4).collect();
    let encode_codes: Vec<_> = encode_tokens.iter().map(|tokens| &tokens.5).collect();
    let mut decode_codec_arms = Vec::new();
    if decode_cases.contains(&ErrorCase::LengthOutOfBounds) {
        decode_codec_arms.push(quote!(#rt::__private::CodecError::LengthOutOfBounds { length, capacity } => Self::LengthOutOfBounds { field, length, capacity }));
    }
    if decode_cases.contains(&ErrorCase::InvalidUtf8) {
        decode_codec_arms.push(quote!(#rt::__private::CodecError::InvalidUtf8(source) => Self::InvalidUtf8 { field, source }));
    }
    if decode_cases.contains(&ErrorCase::MissingNul) {
        decode_codec_arms
            .push(quote!(#rt::__private::CodecError::MissingNul => Self::MissingNul { field }));
    }
    if decode_cases.contains(&ErrorCase::NonZeroTail) {
        decode_codec_arms.push(quote!(#rt::__private::CodecError::NonZeroTail { offset } => Self::NonZeroTail { field, offset }));
    }
    let encode_codec_arm = encode_cases.contains(&ErrorCase::CapacityExceeded).then(|| quote!(#rt::__private::CodecError::CapacityExceeded { length, capacity } => Self::CapacityExceeded { field, length, capacity },));
    let nested = !nested_decode_fields.is_empty();
    if nested_types.len() != nested_decode_fields.len()
        || nested_types.len() != nested_encode_fields.len()
    {
        return Err(syn::Error::new(
            name.span(),
            "internal nested error field count mismatch",
        ));
    }
    let error_params: Vec<syn::Ident> = error_param_by_field.iter().flatten().cloned().collect();
    let mut decode_error_generics = Generics::default();
    let mut encode_error_generics = Generics::default();
    for parameter in &error_params {
        decode_error_generics
            .params
            .push(syn::parse_quote!(#parameter: #rt::SchemaError));
        encode_error_generics
            .params
            .push(syn::parse_quote!(#parameter: #rt::SchemaError));
    }
    let decode_error_projections: Vec<_> = nested_types
        .iter()
        .zip(&error_param_by_field)
        .filter_map(|((live, _), parameter)| {
            parameter
                .as_ref()
                .map(|_| quote!(<#live as #rt::ZeroSchemaType>::DecodeError))
        })
        .collect();
    let encode_error_projections: Vec<_> = nested_types
        .iter()
        .zip(&error_param_by_field)
        .filter_map(|((live, _), parameter)| {
            parameter
                .as_ref()
                .map(|_| quote!(<#live as #rt::ZeroSchemaType>::EncodeError))
        })
        .collect();
    let decode_storage_types: Vec<_> = nested_types
        .iter()
        .zip(&error_param_by_field)
        .map(|((_, erased), parameter)| match parameter {
            Some(parameter) => quote!(#parameter),
            None => {
                let hidden_ty = rebase_hidden_type(erased.clone(), &hidden_schema);
                quote!(<#hidden_ty as #rt::ZeroSchemaType>::DecodeError)
            }
        })
        .collect();
    let encode_storage_types: Vec<_> = nested_types
        .iter()
        .zip(&error_param_by_field)
        .map(|((_, erased), parameter)| match parameter {
            Some(parameter) => quote!(#parameter),
            None => {
                let hidden_ty = rebase_hidden_type(erased.clone(), &hidden_schema);
                quote!(<#hidden_ty as #rt::ZeroSchemaType>::EncodeError)
            }
        })
        .collect();
    let error_generic_args = generic_arguments(&decode_error_generics);
    let decode_error_ty = quote!(#de #error_generic_args);
    let encode_error_ty = quote!(#ee #error_generic_args);
    let decode_projected_ty = if error_params.is_empty() {
        quote!(#de)
    } else {
        quote!(#de <#(#decode_error_projections),*>)
    };
    let encode_projected_ty = if error_params.is_empty() {
        quote!(#ee)
    } else {
        quote!(#ee <#(#encode_error_projections),*>)
    };
    let nested_decode_ty = quote!(#module::NestedDecodeError #error_generic_args);
    let nested_encode_ty = quote!(#module::NestedEncodeError #error_generic_args);
    let (decode_error_ig, decode_error_tg, decode_error_wc) =
        decode_error_generics.split_for_impl();
    let (encode_error_ig, encode_error_tg, encode_error_wc) =
        encode_error_generics.split_for_impl();
    let decode_storage: Vec<_> = decode_storage_types
        .iter()
        .enumerate()
        .map(|(index, ty)| {
            let variant = format_ident!("Field{index}");
            quote!(#variant(#ty))
        })
        .collect();
    let encode_storage: Vec<_> = encode_storage_types
        .iter()
        .enumerate()
        .map(|(index, ty)| {
            let variant = format_ident!("Field{index}");
            quote!(#variant(#ty))
        })
        .collect();
    let decode_ctors: Vec<_> = nested_decode_fields.iter().enumerate().map(|(index,(id,_,_))| {
        let ty=&decode_storage_types[index];
        let method=format_ident!("__new_{}",id);
        let variant=format_ident!("Field{index}");
        quote!(pub(super) fn #method(source:#ty)->Self{Self(__NestedDecodeStorage::#variant(source))})
    }).collect();
    let encode_ctors: Vec<_> = nested_encode_fields.iter().enumerate().map(|(index,(id,_,_))| {
        let ty=&encode_storage_types[index];
        let method=format_ident!("__new_{}",id);
        let variant=format_ident!("Field{index}");
        quote!(pub(super) fn #method(source:#ty)->Self{Self(__NestedEncodeStorage::#variant(source))})
    }).collect();
    let decode_parts: Vec<_> = nested_decode_fields
        .iter()
        .enumerate()
        .map(|(index, (_, _, field))| {
            let variant = format_ident!("Field{index}");
            quote!(__NestedDecodeStorage::#variant(source)=>(#field,source))
        })
        .collect();
    let encode_parts: Vec<_> = nested_encode_fields
        .iter()
        .enumerate()
        .map(|(index, (_, _, field))| {
            let variant = format_ident!("Field{index}");
            quote!(__NestedEncodeStorage::#variant(source)=>(#field,source))
        })
        .collect();
    let decode_source_parts: Vec<_> = nested_decode_fields
        .iter()
        .enumerate()
        .map(|(index, _)| {
            let variant = format_ident!("Field{index}");
            quote!(__NestedDecodeStorage::#variant(source)=>source)
        })
        .collect();
    let encode_source_parts: Vec<_> = nested_encode_fields
        .iter()
        .enumerate()
        .map(|(index, _)| {
            let variant = format_ident!("Field{index}");
            quote!(__NestedEncodeStorage::#variant(source)=>source)
        })
        .collect();
    let decode_debug_parts: Vec<_> = nested_decode_fields
        .iter()
        .enumerate()
        .map(|(index, _)| {
            let variant = format_ident!("Field{index}");
            quote!(__NestedDecodeStorage::#variant(source)=>::core::fmt::Debug::fmt(source,f))
        })
        .collect();
    let encode_debug_parts: Vec<_> = nested_encode_fields
        .iter()
        .enumerate()
        .map(|(index, _)| {
            let variant = format_ident!("Field{index}");
            quote!(__NestedEncodeStorage::#variant(source)=>::core::fmt::Debug::fmt(source,f))
        })
        .collect();
    let nested_support = nested.then(|| quote!{
        enum __NestedDecodeStorage #decode_error_generics {#(#decode_storage),*}
        #support_vis struct NestedDecodeError #decode_error_generics (__NestedDecodeStorage #decode_error_tg);
        impl #decode_error_ig NestedDecodeError #decode_error_tg #decode_error_wc {#(#decode_ctors)* #support_vis fn __parts(&self)->(&'static ::core::primitive::str,&dyn #rt::SchemaError){match &self.0{#(#decode_parts),*}} #support_vis fn __source(&self)->&(dyn ::core::error::Error+'static){match &self.0{#(#decode_source_parts),*}}}
        impl #decode_error_ig ::core::fmt::Debug for NestedDecodeError #decode_error_tg #decode_error_wc {fn fmt(&self,f:&mut ::core::fmt::Formatter<'_>)->::core::fmt::Result{match &self.0{#(#decode_debug_parts),*}}}
        enum __NestedEncodeStorage #encode_error_generics {#(#encode_storage),*}
        #support_vis struct NestedEncodeError #encode_error_generics (__NestedEncodeStorage #encode_error_tg);
        impl #encode_error_ig NestedEncodeError #encode_error_tg #encode_error_wc {#(#encode_ctors)* #support_vis fn __parts(&self)->(&'static ::core::primitive::str,&dyn #rt::SchemaError){match &self.0{#(#encode_parts),*}} #support_vis fn __source(&self)->&(dyn ::core::error::Error+'static){match &self.0{#(#encode_source_parts),*}}}
        impl #encode_error_ig ::core::fmt::Debug for NestedEncodeError #encode_error_tg #encode_error_wc {fn fmt(&self,f:&mut ::core::fmt::Formatter<'_>)->::core::fmt::Result{match &self.0{#(#encode_debug_parts),*}}}
    });
    let decode_nested_variant = nested.then(|| quote!(Nested(#nested_decode_ty),));
    let encode_nested_variant = nested.then(|| quote!(Nested(#nested_encode_ty),));
    let de_derive = (!nested).then(|| quote!(#[derive(Clone,Copy,Debug,Eq,PartialEq)]));
    let ee_derive = (!nested).then(|| quote!(#[derive(Clone,Copy,Debug,Eq,PartialEq)]));
    let nd_source =
        nested.then(|| quote!(Self::Nested(w)=>::core::option::Option::Some(w.__source()),));
    let ne_source =
        nested.then(|| quote!(Self::Nested(w)=>::core::option::Option::Some(w.__source()),));
    let nd_kind = nested.then(|| quote!(Self::Nested(w)=>w.__parts().1.kind(),));
    let ne_kind = nested.then(|| quote!(Self::Nested(w)=>w.__parts().1.kind(),));
    let nd_segment =
        nested.then(|| quote!(Self::Nested(w)=>::core::option::Option::Some(#rt::ErrorPathSegment::Field(w.__parts().0)),));
    let ne_segment =
        nested.then(|| quote!(Self::Nested(w)=>::core::option::Option::Some(#rt::ErrorPathSegment::Field(w.__parts().0)),));
    let nd_child =
        nested.then(|| quote!(Self::Nested(w)=>::core::option::Option::Some(w.__parts().1),));
    let ne_child =
        nested.then(|| quote!(Self::Nested(w)=>::core::option::Option::Some(w.__parts().1),));
    let nd_code = nested.then(|| quote!(Self::Nested(w)=>w.__parts().1.validation_code(),));
    let ne_code = nested.then(|| quote!(Self::Nested(w)=>w.__parts().1.validation_code(),));
    let nd_leaf = nested.then(|| quote!(Self::Nested(w)=>w.__parts().1.__fmt_leaf(f),));
    let ne_leaf = nested.then(|| quote!(Self::Nested(w)=>w.__parts().1.__fmt_leaf(f),));
    let de_debug=nested.then(||quote!(impl #decode_error_ig ::core::fmt::Debug for #decode_error_ty #decode_error_wc {fn fmt(&self,f:&mut ::core::fmt::Formatter<'_>)->::core::fmt::Result{match self{Self::Nested(w)=>f.debug_tuple("Nested").field(w).finish(),other=>f.debug_struct(#logical).field("kind",&#rt::SchemaError::kind(other)).finish()}}}));
    let ee_debug=nested.then(||quote!(impl #encode_error_ig ::core::fmt::Debug for #encode_error_ty #encode_error_wc {fn fmt(&self,f:&mut ::core::fmt::Formatter<'_>)->::core::fmt::Result{match self{Self::Nested(w)=>f.debug_tuple("Nested").field(w).finish(),other=>f.debug_struct(#logical).field("kind",&#rt::SchemaError::kind(other)).finish()}}}));
    let encoded_storage = (!ir.original_generics.params.iter().any(|parameter| matches!(parameter, GenericParam::Type(_) | GenericParam::Const(_)))).then(|| quote! {
        #[repr(C)] #support_vis struct EncodedAlignment { _align: [#encoded_wire_ty; 0] }
        #support_vis const ENCODED_SIZE: ::core::primitive::usize = ::core::mem::size_of::<#encoded_wire_ty>();
        const _: () = { ::core::assert!(::core::mem::size_of::<EncodedAlignment>() == 0); ::core::assert!(::core::mem::align_of::<EncodedAlignment>() == ::core::mem::align_of::<#encoded_wire_ty>()); };
    });
    let encode_miri_cfg = nested_requires_parameter
        .iter()
        .any(|required| *required)
        .then(|| quote!(#[cfg(not(miri))]));
    let encode_method = (!ir.original_generics.params.iter().any(|parameter| matches!(parameter, GenericParam::Type(_) | GenericParam::Const(_)))).then(|| quote! {
        #encode_miri_cfg
        impl #encode_ig #name #original_args #encode_wc {
            #vis fn encode(&self) -> ::core::result::Result<#rt::AlignedBytes<#module::EncodedAlignment, { #module::ENCODED_SIZE }>, #encode_projected_ty> {
                let mut output = #rt::AlignedBytes::<#module::EncodedAlignment, { #module::ENCODED_SIZE }>::zeroed();
                self.encode_into(output.as_bytes_mut())?;
                ::core::result::Result::Ok(output)
            }
        }
    });
    Ok(quote! {
    #[doc(hidden)]
    #module_vis mod #module {
        use super::*;
        #(#wide_target_checks)*
        #(#wrappers)*
        #nested_support
        #root_attr
        #[derive(#zerocopy::FromBytes,#zerocopy::KnownLayout,#zerocopy::Immutable)]
        #[zerocopy(crate = #zerocopy_crate)]
        #[allow(non_snake_case)]
        #support_vis struct #wire #wire_generics { #(#wf,)* #sentinel:[::core::primitive::u8;0] }
        impl #wire_ig #wire #wire_tg #wire_wc {
            pub(super) const ASSERTED_WIRE_SIZE: ::core::primitive::usize = {
                #(#checked_layout_assertions)*
                #(#layout_assertions)*
                let expected_align = #aggregate_align;
                let expected_size = #aggregate_size;
                let expected_stride = #aggregate_stride;
                let size = ::core::mem::size_of::<Self>();
                ::core::assert!(::core::mem::align_of::<Self>() == expected_align, ::core::concat!(#logical, " wire alignment mismatch"));
                ::core::assert!(size == expected_size, ::core::concat!(#logical, " wire size mismatch"));
                ::core::assert!(expected_size == expected_stride, ::core::concat!(#logical, " aggregate stride mismatch"));
                ::core::assert!(::core::mem::offset_of!(Self,#sentinel) == #aggregate_cursor, ::core::concat!(#logical, " trailing marker offset mismatch"));
                ::core::assert!(size > 0, ::core::concat!(#logical," wire must be nonzero"));
                size
            };
        }
        #encoded_storage
    }
    #de_derive #[non_exhaustive]#vis enum #de #decode_error_generics {#decode_nested_variant #(#decode_variants),*}
    impl #decode_error_ig #decode_error_ty #decode_error_wc {
        fn __codec(field:&'static ::core::primitive::str,e:#rt::__private::CodecError)->Self{match e{#(#decode_codec_arms,)*_=>::core::unreachable!()}}
    }
    #ee_derive #[non_exhaustive]#vis enum #ee #encode_error_generics {#encode_nested_variant #(#encode_variants),*}
    #de_debug
    impl #decode_error_ig ::core::fmt::Display for #decode_error_ty #decode_error_wc {fn fmt(&self,f:&mut ::core::fmt::Formatter<'_>)->::core::fmt::Result{#rt::__private::__fmt_schema_error(self,f)}}
    impl #decode_error_ig ::core::error::Error for #decode_error_ty #decode_error_wc {fn source(&self)->::core::option::Option<&(dyn ::core::error::Error+'static)>{match self{#nd_source #(#decode_sources),*}}}
    impl #decode_error_ig #rt::SchemaError for #decode_error_ty #decode_error_wc {
        fn kind(&self)->#rt::ErrorKind{match self{#nd_kind #(#decode_kinds),*}}
        fn schema(&self)->&'static ::core::primitive::str{#logical}
        fn segment(&self)->::core::option::Option<#rt::ErrorPathSegment>{match self{#nd_segment #(#decode_segments),*}}
        fn child(&self)->::core::option::Option<&dyn #rt::SchemaError>{match self{#nd_child _=>::core::option::Option::None}}
        fn validation_code(&self)->::core::option::Option<::core::primitive::u32>{match self{#nd_code #(#decode_codes),*}}
        fn __fmt_leaf(&self,f:&mut ::core::fmt::Formatter<'_>)->::core::fmt::Result{match self{#nd_leaf #(#decode_leaves),*}}
    }
    impl #encode_error_ig #encode_error_ty #encode_error_wc {
        fn __codec(field:&'static ::core::primitive::str,e:#rt::__private::CodecError)->Self{match e{#encode_codec_arm _=>::core::unreachable!()}}
    }
    #ee_debug
    impl #encode_error_ig ::core::fmt::Display for #encode_error_ty #encode_error_wc {fn fmt(&self,f:&mut ::core::fmt::Formatter<'_>)->::core::fmt::Result{#rt::__private::__fmt_schema_error(self,f)}}
    impl #encode_error_ig ::core::error::Error for #encode_error_ty #encode_error_wc {fn source(&self)->::core::option::Option<&(dyn ::core::error::Error+'static)>{match self{#ne_source #(#encode_sources),*}}}
    impl #encode_error_ig #rt::SchemaError for #encode_error_ty #encode_error_wc {
        fn kind(&self)->#rt::ErrorKind{match self{#ne_kind #(#encode_kinds),*}}
        fn schema(&self)->&'static ::core::primitive::str{#logical}
        fn segment(&self)->::core::option::Option<#rt::ErrorPathSegment>{match self{#ne_segment #(#encode_segments),*}}
        fn child(&self)->::core::option::Option<&dyn #rt::SchemaError>{match self{#ne_child _=>::core::option::Option::None}}
        fn validation_code(&self)->::core::option::Option<::core::primitive::u32>{match self{#ne_code #(#encode_codes),*}}
        fn __fmt_leaf(&self,f:&mut ::core::fmt::Formatter<'_>)->::core::fmt::Result{match self{#ne_leaf #(#encode_leaves),*}}
    }
    impl #layout_ig #rt::ZeroSchemaType for #name #layout_tg #layout_wc{type Wire=#wire_ty;type DecodeError=#decode_projected_ty;type EncodeError=#encode_projected_ty;const WIRE_SIZE: ::core::primitive::usize={#(#nested_wire_assertions)* <#wire_ty>::ASSERTED_WIRE_SIZE};const WIRE_ALIGN: ::core::primitive::usize=::core::mem::align_of::<Self::Wire>();const WIRE_STRIDE: ::core::primitive::usize=match #rt::__private::__checked_wire_stride(Self::WIRE_SIZE,Self::WIRE_ALIGN){::core::option::Option::Some(x)=>x,::core::option::Option::None=>::core::panic!(::core::concat!(#logical," wire stride overflow"))};const LAYOUT:&'static #rt::LayoutDescriptor=&#rt::LayoutDescriptor::__new(#logical,#rt::TypeKind::Struct,Self::WIRE_SIZE,Self::WIRE_ALIGN,Self::WIRE_STRIDE,#pad,&[#(#padding_ranges),*],&[#(#desc),*],&[],&[]);}
    impl #ig #name #tg #wc{#(#must_equal_consts)*}
    #decode_impl{fn decode_at(#input_local:#rt::DecodeInput<#source_lt,Self::Wire>)->::core::result::Result<Self,#decode_projected_ty>{::core::assert!(<#wire_ty>::ASSERTED_WIRE_SIZE > 0);#(#external_tag_cache_decls)*#(#decode_steps)*let #decoded_value_local=Self{#(#dec,)*};#decode_padding #decode_whole ::core::result::Result::Ok(#decoded_value_local)}}
    impl #encode_ig #rt::__private::EncodeWire for #name #original_args #encode_wc{fn validate_encode(&self)->::core::result::Result<(),#encode_projected_ty>{::core::assert!(<#wire_ty>::ASSERTED_WIRE_SIZE > 0);#(#pre)*#encode_whole ::core::result::Result::Ok(())}fn encode_at(&self,destination:&mut #rt::__private::Prezeroed<'_>)->::core::result::Result<(),#encode_projected_ty>{::core::assert!(<#wire_ty>::ASSERTED_WIRE_SIZE > 0);#(#enc)*::core::result::Result::Ok(())}}
    impl #decode_ig #name #original_args #decode_wc{#vis fn parse(#parse_sig)->::core::result::Result<Self,#decode_projected_ty>{::core::assert!(<#wire_ty>::ASSERTED_WIRE_SIZE > 0);let i=#rt::DecodeInput::from_exact(bytes).map_err(#de::Layout)?;<Self as #rt::__private::DecodeWire<#source_lt>>::decode_at(i)}#vis fn parse_prefix(#parse_sig)->::core::result::Result<(Self,&#source_lt [::core::primitive::u8]),#decode_projected_ty>{::core::assert!(<#wire_ty>::ASSERTED_WIRE_SIZE > 0);let i=#rt::DecodeInput::from_prefix(bytes).map_err(#de::Layout)?;::core::result::Result::Ok((<Self as #rt::__private::DecodeWire<#source_lt>>::decode_at(i)?,&bytes[Self::WIRE_SIZE..]))}}
    impl #encode_ig #name #original_args #encode_wc{#vis fn encode_into(&self,d:&mut[::core::primitive::u8])->::core::result::Result<(),#encode_projected_ty>{::core::assert!(<#wire_ty>::ASSERTED_WIRE_SIZE > 0);if d.len()!=Self::WIRE_SIZE{return ::core::result::Result::Err(#ee::Layout(#rt::LayoutError::IncorrectSize{expected:Self::WIRE_SIZE,actual:d.len()}));}let a=d.as_ptr() as ::core::primitive::usize;if a&(Self::WIRE_ALIGN-1)!=0{return ::core::result::Result::Err(#ee::Layout(#rt::LayoutError::Misaligned{required:Self::WIRE_ALIGN,address:a}));}<Self as #rt::__private::EncodeWire>::validate_encode(self)?;let mut p=#rt::__private::Prezeroed::new(d);<Self as #rt::__private::EncodeWire>::encode_at(self,&mut p)}#vis const fn encoded_len(&self)->::core::primitive::usize{Self::WIRE_SIZE}}
    #encode_method
    impl #layout_ig #name #layout_tg #layout_wc{#vis const WIRE_SIZE: ::core::primitive::usize=<Self as #rt::ZeroSchemaType>::WIRE_SIZE;#vis const WIRE_ALIGN: ::core::primitive::usize=<Self as #rt::ZeroSchemaType>::WIRE_ALIGN;#vis const WIRE_STRIDE: ::core::primitive::usize=<Self as #rt::ZeroSchemaType>::WIRE_STRIDE;#vis const LAYOUT:&'static #rt::LayoutDescriptor=<Self as #rt::ZeroSchemaType>::LAYOUT;}
    })
}
