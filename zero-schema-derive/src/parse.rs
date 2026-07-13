use crate::ir::*;
use proc_macro2::Span;
#[cfg(test)]
use quote::ToTokens;
use quote::format_ident;
use std::collections::{BTreeMap, BTreeSet};
use syn::{
    Attribute, Data, DeriveInput, Error, Expr, Fields, GenericParam, LitInt, LitStr, Meta, Path,
    PathArguments, Token, Type, Visibility, WherePredicate, parse::Parser, punctuated::Punctuated,
    spanned::Spanned as _, visit::Visit, visit_mut::VisitMut,
};

fn combine(dst: &mut Option<Error>, error: Error) {
    if let Some(e) = dst {
        e.combine(error)
    } else {
        *dst = Some(error)
    }
}
fn logical(id: &syn::Ident) -> String {
    id.to_string().trim_start_matches("r#").to_owned()
}
fn quoted_policy(meta: &syn::meta::ParseNestedMeta<'_>) -> syn::Result<(String, Span)> {
    let literal: LitStr = meta
        .value()?
        .parse()
        .map_err(|_| meta.error("policy value must be a quoted string literal"))?;
    Ok((literal.value(), literal.span()))
}
fn endian(meta: &syn::meta::ParseNestedMeta<'_>) -> syn::Result<(Endian, Span)> {
    let (s, sp) = quoted_policy(meta)?;
    Ok((
        match s.as_str() {
            "native" => Endian::Native,
            "little" => Endian::Little,
            "big" => Endian::Big,
            _ => {
                return Err(Error::new(
                    sp,
                    "endian must be \"native\", \"little\", or \"big\"",
                ));
            }
        },
        sp,
    ))
}
fn tail(meta: &syn::meta::ParseNestedMeta<'_>) -> syn::Result<(Tail, Span)> {
    let (s, sp) = quoted_policy(meta)?;
    Ok((
        match s.as_str() {
            "ignore" => Tail::Ignore,
            "zero" => Tail::Zero,
            _ => return Err(Error::new(sp, "tail must be \"ignore\" or \"zero\"")),
        },
        sp,
    ))
}
fn padding(meta: &syn::meta::ParseNestedMeta<'_>) -> syn::Result<(Padding, Span)> {
    let (s, sp) = quoted_policy(meta)?;
    Ok((
        match s.as_str() {
            "ignore" => Padding::Ignore,
            "zero" => Padding::Zero,
            _ => return Err(Error::new(sp, "padding must be \"ignore\" or \"zero\"")),
        },
        sp,
    ))
}
fn unsigned(
    meta: &syn::meta::ParseNestedMeta<'_>,
    what: &str,
    max: u128,
) -> syn::Result<(u32, Span)> {
    let lit: LitInt = meta.value()?.parse()?;
    if !lit.suffix().is_empty() {
        return Err(Error::new(
            lit.span(),
            format!("{what} must be an unsuffixed integer literal"),
        ));
    }
    let text = lit.to_string().replace('_', "");
    let (radix, digits) = if let Some(x) = text.strip_prefix("0x") {
        (16, x)
    } else if let Some(x) = text.strip_prefix("0o") {
        (8, x)
    } else if let Some(x) = text.strip_prefix("0b") {
        (2, x)
    } else {
        (10, text.as_str())
    };
    let n = u128::from_str_radix(digits, radix)
        .map_err(|_| Error::new(lit.span(), format!("{what} is out of range")))?;
    if n > max {
        return Err(Error::new(lit.span(), format!("{what} is out of range")));
    }
    Ok((n as u32, lit.span()))
}
fn duplicate(seen: &mut BTreeMap<String, Span>, key: &str, span: Span) -> syn::Result<()> {
    if seen.insert(key.into(), span).is_some() {
        Err(Error::new(span, format!("duplicate zero option `{key}`")))
    } else {
        Ok(())
    }
}

fn parse_container(attrs: &[Attribute]) -> (ContainerOptions, Option<Error>) {
    let mut o = ContainerOptions::default();
    let mut seen = BTreeMap::new();
    let mut errors = None;
    for a in attrs.iter().filter(|a| a.path().is_ident("zero")) {
        if let Err(e) = a.parse_nested_meta(|m| {
            let result: syn::Result<()> = (|| {
                let key = m
                    .path
                    .get_ident()
                    .ok_or_else(|| m.error("expected option name"))?
                    .to_string();
                duplicate(&mut seen, &key, m.path.span())?;
                match key.as_str() {
                    "crate" => {
                        o.spans.runtime = Some(m.path.span());
                        o.runtime = Some(m.value()?.parse()?)
                    }
                    "endian" => {
                        let (v, s) = endian(&m)?;
                        o.endian = v;
                        o.spans.endian = Some(s)
                    }
                    "align" => {
                        let (v, s) = unsigned(&m, "alignment", 1 << 29)?;
                        if v == 0 || !v.is_power_of_two() {
                            return Err(Error::new(
                                s,
                                "alignment must be a power of two no greater than 2^29",
                            ));
                        }
                        o.align = Some(v);
                        o.spans.align = Some(s)
                    }
                    "padding" => {
                        let (v, s) = padding(&m)?;
                        o.padding = v;
                        o.spans.padding = Some(s)
                    }
                    "tail" => {
                        let (v, s) = tail(&m)?;
                        o.tail = v;
                        o.spans.tail = Some(s)
                    }
                    "tag" => {
                        o.spans.tag = Some(m.path.span());
                        o.tag = Some(m.value()?.parse()?)
                    }
                    "borrow" => {
                        o.spans.borrow = Some(m.path.span());
                        o.borrow = Some(m.value()?.parse()?)
                    }
                    "validate_with" => {
                        o.spans.validate_with = Some(m.path.span());
                        o.validate_with = Some(m.value()?.parse()?)
                    }
                    _ => return Err(m.error(format!("unknown zero option `{key}`"))),
                };
                Ok(())
            })();
            if let Err(error) = result {
                combine(&mut errors, error);
            }
            Ok(())
        }) {
            combine(&mut errors, e)
        }
    }
    (o, errors)
}
fn parse_field(attrs: &[Attribute]) -> (FieldOptions, Option<Error>) {
    let mut o = FieldOptions::default();
    let mut seen = BTreeMap::new();
    let mut errors = None;
    for a in attrs.iter().filter(|a| a.path().is_ident("zero")) {
        if let Err(e) = a.parse_nested_meta(|m| {
            let result: syn::Result<()> = (|| {
                let key = m
                    .path
                    .get_ident()
                    .ok_or_else(|| m.error("expected option name"))?
                    .to_string();
                duplicate(&mut seen, &key, m.path.span())?;
                match key.as_str() {
                    "endian" => {
                        let (v, s) = endian(&m)?;
                        o.endian = Some(v);
                        o.spans.endian = Some(s)
                    }
                    "align" => {
                        let (v, s) = unsigned(&m, "alignment", 1 << 29)?;
                        if v == 0 || !v.is_power_of_two() {
                            return Err(Error::new(
                                s,
                                "alignment must be a power of two no greater than 2^29",
                            ));
                        }
                        o.align = Some(v);
                        o.spans.align = Some(s)
                    }
                    "capacity" => {
                        let (v, s) = unsigned(&m, "capacity", u32::MAX as u128)?;
                        o.capacity = Some(v);
                        o.spans.capacity = Some(s)
                    }
                    "len_type" => {
                        let i: syn::Ident = m.value()?.parse()?;
                        if !matches!(i.to_string().as_str(), "u8" | "u16" | "u32") {
                            return Err(Error::new(i.span(), "len_type must be u8, u16, or u32"));
                        }
                        o.spans.len_type = Some(i.span());
                        o.len_type = Some(i)
                    }
                    "tail" => {
                        let (v, s) = tail(&m)?;
                        o.tail = Some(v);
                        o.spans.tail = Some(s)
                    }
                    "tag_field" => {
                        let i: syn::Ident = m.value()?.parse()?;
                        o.spans.tag_field = Some(i.span());
                        o.tag_field = Some(i)
                    }
                    "validate_with" => {
                        o.spans.validate_with = Some(m.path.span());
                        o.validate_with = Some(m.value()?.parse()?)
                    }
                    "range" => {
                        let e: Expr = m.value()?.parse()?;
                        let Expr::Range(r) = e else {
                            return Err(Error::new(
                                e.span(),
                                "range must be a bounded range expression",
                            ));
                        };
                        o.spans.range = Some(r.span());
                        o.range = Some(r)
                    }
                    "must_equal" => {
                        let e: Expr = m.value()?.parse()?;
                        o.spans.must_equal = Some(e.span());
                        o.must_equal = Some(e)
                    }
                    _ => return Err(m.error(format!("unknown zero option `{key}`"))),
                };
                Ok(())
            })();
            if let Err(error) = result {
                combine(&mut errors, error);
            }
            Ok(())
        }) {
            combine(&mut errors, e)
        }
    }
    (o, errors)
}

struct Interior<'a> {
    root: &'a [Attribute],
    errors: Option<Error>,
    allow_macros: bool,
}
impl<'ast> Visit<'ast> for Interior<'_> {
    fn visit_attribute(&mut self, a: &'ast Attribute) {
        if a.path().is_ident("zero") && !self.root.iter().any(|x| std::ptr::eq(x, a)) {
            combine(
                &mut self.errors,
                Error::new(a.span(), "#[zero] is not allowed in this nested syntax"),
            )
        }
        syn::visit::visit_attribute(self, a)
    }
    fn visit_macro(&mut self, m: &'ast syn::Macro) {
        if !self.allow_macros {
            combine(
                &mut self.errors,
                Error::new(
                    m.span(),
                    "macros are not supported in syntax moved into generated code",
                ),
            )
        }
    }
}
fn unsupported_owned_container(path: &syn::TypePath) -> Option<&'static str> {
    if path.qself.is_some() {
        return None;
    }
    let names: Vec<_> = path
        .path
        .segments
        .iter()
        .map(|s| s.ident.to_string())
        .collect();
    let canonical = match names.as_slice() {
        [name] => matches!(name.as_str(), "String" | "Vec" | "Box" | "Option"),
        [root, module, name] => matches!(
            (root.as_str(), module.as_str(), name.as_str()),
            ("core", "option", "Option")
                | ("alloc", "string", "String")
                | ("alloc", "vec", "Vec")
                | ("alloc", "boxed", "Box")
                | ("std", "string", "String")
                | ("std", "vec", "Vec")
                | ("std", "boxed", "Box")
                | ("std", "option", "Option")
        ),
        _ => false,
    };
    canonical.then(|| {
        if names.last().is_some_and(|name| name == "Vec") {
            "dynamic-layout container type is not supported"
        } else {
            "owned container type is not supported"
        }
    })
}

fn literal_zero(expr: &Expr) -> bool {
    match expr {
        Expr::Group(group) => literal_zero(&group.expr),
        Expr::Paren(paren) => literal_zero(&paren.expr),
        Expr::Lit(lit) => {
            matches!(&lit.lit, syn::Lit::Int(value) if matches!(value.base10_parse::<u128>(), Ok(0)))
        }
        _ => false,
    }
}
fn classify(ty: &Type) -> syn::Result<FieldKind> {
    match ty {
        Type::Path(path) if path.qself.is_none() => {
            if let Some(message) = unsupported_owned_container(path) {
                return Err(Error::new(ty.span(), message));
            }
            if path.path.leading_colon.is_some() || path.path.segments.len() != 1 {
                return Ok(FieldKind::Schema);
            }
            let segment = path.path.segments.first().expect("one segment was checked");
            if !matches!(segment.arguments, PathArguments::None) {
                return Ok(FieldKind::Schema);
            }
            Ok(match segment.ident.to_string().as_str() {
                "u8" => FieldKind::Primitive(PrimitiveKind::U8),
                "i8" => FieldKind::Primitive(PrimitiveKind::I8),
                "u16" => FieldKind::Primitive(PrimitiveKind::U16),
                "i16" => FieldKind::Primitive(PrimitiveKind::I16),
                "u32" => FieldKind::Primitive(PrimitiveKind::U32),
                "i32" => FieldKind::Primitive(PrimitiveKind::I32),
                "u64" => FieldKind::Primitive(PrimitiveKind::U64),
                "i64" => FieldKind::Primitive(PrimitiveKind::I64),
                "f32" => FieldKind::Primitive(PrimitiveKind::F32),
                "f64" => FieldKind::Primitive(PrimitiveKind::F64),
                "bool" => FieldKind::Bool,
                _ => FieldKind::Schema,
            })
        }
        Type::Reference(reference)
            if reference.mutability.is_none() && reference.lifetime.is_some() =>
        {
            match &*reference.elem {
                Type::Path(path)
                    if path.qself.is_none()
                        && path.path.segments.last().is_some_and(|segment| {
                            matches!(segment.arguments, PathArguments::None)
                        }) =>
                {
                    let segment = path
                        .path
                        .segments
                        .last()
                        .expect("segment existence was checked");
                    Ok(match segment.ident.to_string().as_str() {
                        "str" => FieldKind::Utf8,
                        "CStr" => FieldKind::CStr,
                        "U16Str" => FieldKind::U16Str,
                        "U16CStr" => FieldKind::U16CStr,
                        _ => FieldKind::Schema,
                    })
                }
                Type::Array(array) => match &*array.elem {
                    Type::Path(path) if path.qself.is_none() && path.path.is_ident("u8") => {
                        Ok(FieldKind::FixedBytes(array.len.clone()))
                    }
                    _ => Ok(FieldKind::Schema),
                },
                _ => Ok(FieldKind::Schema),
            }
        }
        Type::Macro(type_macro) => Err(Error::new(
            type_macro.span(),
            "type macros are not supported",
        )),
        _ => Ok(FieldKind::Schema),
    }
}
fn directly_recursive(ty: &Type, name: &syn::Ident) -> bool {
    let Type::Path(path) = ty else { return false };
    path.qself.is_none()
        && path.path.leading_colon.is_none()
        && path.path.segments.len() == 1
        && path
            .path
            .segments
            .first()
            .is_some_and(|segment| segment.ident == "Self" || segment.ident == *name)
}
fn type_allowed(ty: &Type) -> bool {
    struct MacroFinder(bool);
    impl<'ast> Visit<'ast> for MacroFinder {
        fn visit_macro(&mut self, _: &'ast syn::Macro) {
            self.0 = true;
        }
    }
    let mut finder = MacroFinder(false);
    finder.visit_type(ty);
    !finder.0
}
fn path_allowed(path: &Path) -> bool {
    path.segments
        .iter()
        .all(|segment| match &segment.arguments {
            PathArguments::None => true,
            PathArguments::AngleBracketed(arguments) => {
                arguments.args.iter().all(|argument| match argument {
                    syn::GenericArgument::Type(ty) => type_allowed(ty),
                    syn::GenericArgument::Const(expr) => expr_allowed(expr),
                    syn::GenericArgument::AssocType(assoc) => type_allowed(&assoc.ty),
                    syn::GenericArgument::AssocConst(assoc) => expr_allowed(&assoc.value),
                    syn::GenericArgument::Constraint(_) | syn::GenericArgument::Lifetime(_) => true,
                    _ => true,
                })
            }
            PathArguments::Parenthesized(arguments) => {
                arguments.inputs.iter().all(type_allowed)
                    && match &arguments.output {
                        syn::ReturnType::Default => true,
                        syn::ReturnType::Type(_, ty) => type_allowed(ty),
                    }
            }
        })
}
fn expr_allowed(e: &Expr) -> bool {
    match e {
        Expr::Lit(_) => true,
        Expr::Path(x) => {
            !x.path.is_ident("self")
                && x.qself.as_ref().is_none_or(|q| type_allowed(&q.ty))
                && path_allowed(&x.path)
        }
        Expr::Unary(x) => expr_allowed(&x.expr),
        Expr::Binary(x) => expr_allowed(&x.left) && expr_allowed(&x.right),
        Expr::Cast(x) => expr_allowed(&x.expr) && type_allowed(&x.ty),
        Expr::Group(x) => expr_allowed(&x.expr),
        Expr::Paren(x) => expr_allowed(&x.expr),
        Expr::Call(x) => expr_allowed(&x.func) && x.args.iter().all(expr_allowed),
        _ => false,
    }
}
fn validate_path(path: &Path) -> syn::Result<()> {
    if path.segments.is_empty()
        || path
            .segments
            .iter()
            .any(|s| !matches!(s.arguments, PathArguments::None))
    {
        return Err(Error::new(
            path.span(),
            "tag must be an ordinary path without generic arguments",
        ));
    }
    Ok(())
}
struct GenericAttributeCleaner;

impl VisitMut for GenericAttributeCleaner {
    fn visit_lifetime_param_mut(&mut self, parameter: &mut syn::LifetimeParam) {
        parameter.attrs.clear();
        syn::visit_mut::visit_lifetime_param_mut(self, parameter);
    }

    fn visit_type_param_mut(&mut self, parameter: &mut syn::TypeParam) {
        parameter.attrs.clear();
        syn::visit_mut::visit_type_param_mut(self, parameter);
    }

    fn visit_const_param_mut(&mut self, parameter: &mut syn::ConstParam) {
        parameter.attrs.clear();
        syn::visit_mut::visit_const_param_mut(self, parameter);
    }
}

fn cleaned(mut generics: syn::Generics) -> syn::Generics {
    GenericAttributeCleaner.visit_generics_mut(&mut generics);
    generics
}
fn path_strategy(p: &Path) -> PathRebase {
    if p.leading_colon.is_some() || p.segments.first().is_some_and(|s| s.ident == "crate") {
        PathRebase::Preserve
    } else if p
        .segments
        .first()
        .is_some_and(|s| s.ident == "self" || s.ident == "super")
    {
        PathRebase::RebaseOneLevel
    } else {
        PathRebase::Preserve
    }
}
fn hidden_path(path: &Path) -> Path {
    if path.leading_colon.is_some()
        || path
            .segments
            .first()
            .is_some_and(|segment| segment.ident == "crate")
    {
        return path.clone();
    }
    let mut rebased = path.clone();
    if rebased
        .segments
        .first()
        .is_some_and(|segment| segment.ident == "self")
    {
        rebased.segments[0].ident = syn::Ident::new("super", rebased.segments[0].ident.span());
        rebased
    } else if rebased
        .segments
        .first()
        .is_some_and(|segment| segment.ident == "super")
    {
        rebased.segments.insert(0, syn::parse_quote!(super));
        rebased
    } else {
        rebased
    }
}

struct PathCollector {
    paths: Vec<Path>,
}
impl<'ast> Visit<'ast> for PathCollector {
    fn visit_path(&mut self, path: &'ast Path) {
        self.paths.push(path.clone());
        syn::visit::visit_path(self, path);
    }
}

fn moved_paths(
    input: &DeriveInput,
    options: &ContainerOptions,
    fields: &[FieldIr],
    variants: &[VariantIr],
) -> Vec<MovedPath> {
    let mut collector = PathCollector { paths: Vec::new() };
    for parameter in &input.generics.params {
        collector.visit_generic_param(parameter);
    }
    if let Some(clause) = &input.generics.where_clause {
        collector.visit_where_clause(clause);
    }
    for path in options
        .runtime
        .iter()
        .chain(options.validate_with.iter())
        .chain(options.tag.iter())
    {
        collector.visit_path(path);
    }
    for field in fields {
        collector.visit_type(&field.original_type);
        if let Some(path) = &field.options.validate_with {
            collector.visit_path(path);
        }
        if let Some(range) = &field.options.range {
            collector.visit_expr_range(range);
        }
        if let Some(expression) = &field.options.must_equal {
            collector.visit_expr(expression);
        }
    }
    for variant in variants {
        if let VariantShape::Newtype(ty) = &variant.shape {
            collector.visit_type(ty);
        }
        if let Some(path) = &variant.tag {
            collector.visit_path(path);
        }
        // Discriminants stay in the user's enum and are deliberately not moved.
    }
    let mut seen = BTreeSet::new();
    collector
        .paths
        .into_iter()
        .filter_map(|path| {
            let key = quote::ToTokens::to_token_stream(&path).to_string();
            seen.insert(key).then(|| MovedPath {
                strategy: if path
                    .segments
                    .first()
                    .is_some_and(|segment| segment.ident == "Self")
                {
                    PathRebase::RewriteSchemaSelf
                } else {
                    path_strategy(&path)
                },
                span: path.span(),
                path,
            })
        })
        .collect()
}

fn lifetime_model(generics: &syn::Generics, borrow: Option<syn::Lifetime>) -> LifetimeModel {
    let declared: BTreeSet<_> = generics
        .lifetimes()
        .map(|parameter| parameter.lifetime.ident.to_string())
        .collect();
    let mut suffix = 0usize;
    let source = loop {
        let candidate = if suffix == 0 {
            "__zero_input".into()
        } else {
            format!("__zero_input_{suffix}")
        };
        if !declared.contains(&candidate) {
            break syn::Lifetime::new(&format!("'{candidate}"), Span::call_site());
        }
        suffix += 1;
    };
    let mut edges: Vec<(syn::Lifetime, syn::Lifetime)> = Vec::new();
    for parameter in generics.lifetimes() {
        edges.extend(
            parameter
                .bounds
                .iter()
                .cloned()
                .map(|bound| (parameter.lifetime.clone(), bound)),
        );
    }
    if let Some(clause) = &generics.where_clause {
        for predicate in &clause.predicates {
            if let WherePredicate::Lifetime(predicate) = predicate {
                edges.extend(
                    predicate
                        .bounds
                        .iter()
                        .cloned()
                        .map(|bound| (predicate.lifetime.clone(), bound)),
                );
            }
        }
    }
    if let Some(borrow) = &borrow {
        edges.push((source.clone(), borrow.clone()));
    }
    loop {
        let mut additions = Vec::new();
        for (left, middle) in &edges {
            for (candidate, right) in &edges {
                if middle.ident == candidate.ident
                    && !edges
                        .iter()
                        .any(|(a, b)| a.ident == left.ident && b.ident == right.ident)
                {
                    additions.push((left.clone(), right.clone()));
                }
            }
        }
        additions.sort_by_key(|(a, b)| (a.ident.to_string(), b.ident.to_string()));
        additions.dedup_by(|a, b| a.0.ident == b.0.ident && a.1.ident == b.1.ident);
        if additions.is_empty() {
            break;
        }
        edges.extend(additions);
    }
    LifetimeModel {
        source,
        outlives: edges,
    }
}

fn hidden_visibility(visibility: &Visibility) -> Visibility {
    let Visibility::Restricted(restricted) = visibility else {
        return match visibility {
            Visibility::Public(_) => syn::parse_quote!(pub),
            Visibility::Inherited => syn::parse_quote!(pub(super)),
            Visibility::Restricted(_) => unreachable!(),
        };
    };
    let mut transformed = restricted.clone();
    transformed.path = Box::new(hidden_path(&restricted.path));
    if transformed.in_token.is_none() && transformed.path.segments.len() > 1 {
        transformed.in_token = ::core::option::Option::Some(Default::default());
    }
    Visibility::Restricted(transformed)
}

fn visibility(v: &syn::Visibility) -> VisibilityPlan {
    VisibilityPlan {
        module: v.clone(),
        support: hidden_visibility(v),
    }
}

fn encode_stable_tokens(tokens: proc_macro2::TokenStream, output: &mut Vec<u8>) {
    use proc_macro2::{Delimiter, Spacing, TokenTree};
    for token in tokens {
        match token {
            TokenTree::Group(group) => {
                output.extend_from_slice(&[
                    b'g',
                    match group.delimiter() {
                        Delimiter::Parenthesis => b'p',
                        Delimiter::Brace => b'b',
                        Delimiter::Bracket => b'k',
                        Delimiter::None => b'n',
                    },
                ]);
                encode_stable_tokens(group.stream(), output);
                output.push(b'e');
            }
            TokenTree::Ident(ident) => {
                output.push(b'i');
                output.extend_from_slice(ident.to_string().as_bytes());
                output.push(0);
            }
            TokenTree::Punct(punct) => {
                output.extend_from_slice(&[
                    b'u',
                    punct.as_char() as u8,
                    match punct.spacing() {
                        Spacing::Alone => b'a',
                        Spacing::Joint => b'j',
                    },
                ]);
            }
            TokenTree::Literal(literal) => {
                output.push(b'l');
                output.extend_from_slice(literal.to_string().as_bytes());
                output.push(0);
            }
        }
    }
}

fn stable_suffix(input: &DeriveInput) -> String {
    // FNV-1a 64-bit offset basis and prime over the normalized token-tree byte feed.
    let mut bytes = Vec::new();
    encode_stable_tokens(quote::ToTokens::to_token_stream(input), &mut bytes);
    let mut hash = 0xcbf2_9ce4_8422_2325_u64;
    for byte in bytes {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("{hash:016x}")
}

pub fn build(input: DeriveInput) -> syn::Result<SchemaIr> {
    let (options, mut errors) = parse_container(&input.attrs);
    // Pass 1: recursively inspect all nested syntax; item/struct-field/variant attrs are allowed.
    let mut interior = Interior {
        root: &[],
        errors: None,
        allow_macros: false,
    };
    for p in &input.generics.params {
        match p {
            GenericParam::Lifetime(x) => interior.visit_lifetime_param(x),
            GenericParam::Type(x) => interior.visit_type_param(x),
            GenericParam::Const(x) => interior.visit_const_param(x),
        }
    }
    if let Some(w) = &input.generics.where_clause {
        interior.visit_where_clause(w);
    }
    match &input.data {
        Data::Struct(d) => {
            for f in &d.fields {
                interior.visit_type(&f.ty);
            }
        }
        Data::Enum(d) => {
            for v in &d.variants {
                for f in &v.fields {
                    interior.visit_field(f);
                }
                if let Some((_, discriminant)) = &v.discriminant {
                    interior.allow_macros = true;
                    interior.visit_expr(discriminant);
                    interior.allow_macros = false;
                }
            }
        }
        Data::Union(_) => {}
    }
    if let Some(e) = interior.errors {
        combine(&mut errors, e)
    }
    // Pass 2: classify declaration and repr.
    let mut scalar_repr = None;
    let mut repr_int_count = 0;
    let mut packed_span = None;
    for a in input.attrs.iter().filter(|a| a.path().is_ident("repr")) {
        let Meta::List(l) = &a.meta else {
            combine(&mut errors, Error::new(a.span(), "expected #[repr(...)]"));
            continue;
        };
        let parser = Punctuated::<Meta, Token![,]>::parse_terminated;
        match parser.parse2(l.tokens.clone()) {
            Ok(ms) => {
                for m in ms {
                    match &m {
                        Meta::Path(p)
                            if matches!(
                                p.get_ident().map(ToString::to_string).as_deref(),
                                Some("u8" | "u16" | "u32")
                            ) =>
                        {
                            repr_int_count += 1;
                            scalar_repr = p.get_ident().cloned()
                        }
                        Meta::Path(p) if p.is_ident("packed") => packed_span = Some(p.span()),
                        Meta::List(x) if x.path.is_ident("packed") => packed_span = Some(x.span()),
                        _ => {}
                    }
                }
            }
            Err(e) => combine(&mut errors, e),
        }
    }
    let kind = match &input.data {
        Data::Struct(d) => {
            if let Some(span) = packed_span {
                combine(
                    &mut errors,
                    Error::new(span, "packed schema structs are not supported"),
                )
            }
            if !matches!(d.fields, Fields::Named(_)) {
                combine(
                    &mut errors,
                    Error::new(d.fields.span(), "ZeroSchema structs must have named fields"),
                )
            }
            SchemaKind::Struct
        }
        Data::Enum(_) => {
            if options.tag.is_some() {
                SchemaKind::TaggedEnum
            } else {
                SchemaKind::ScalarEnum
            }
        }
        Data::Union(u) => {
            combine(
                &mut errors,
                Error::new(u.union_token.span(), "unions cannot derive ZeroSchema"),
            );
            SchemaKind::Struct
        }
    };
    let mut fields = Vec::new();
    let mut variants = Vec::new();
    if let Data::Struct(d) = &input.data {
        if let Fields::Named(n) = &d.fields {
            for f in &n.named {
                let (fo, fe) = parse_field(&f.attrs);
                if let Some(e) = fe {
                    combine(&mut errors, e)
                };
                let fk = match classify(&f.ty) {
                    Ok(k) => k,
                    Err(e) => {
                        combine(&mut errors, e);
                        FieldKind::Schema
                    }
                };
                let effective = fo.endian.unwrap_or(options.endian);
                let wide = matches!(fk, FieldKind::U16Str | FieldKind::U16CStr)
                    && effective != Endian::Native;
                fields.push(FieldIr {
                    ident: f.ident.clone().unwrap(),
                    visibility: f.vis.clone(),
                    original_type: f.ty.clone(),
                    type_span: f.ty.span(),
                    kind: fk.clone(),
                    options: fo.clone(),
                    resolved: ResolvedFieldOptions {
                        endian: effective,
                        tail: fo.tail.unwrap_or(Tail::Ignore),
                        length_repr: fo.len_type.clone().or_else(|| {
                            matches!(fk, FieldKind::Utf8 | FieldKind::U16Str)
                                .then(|| format_ident!("u16"))
                        }),
                        target_endian_check: wide.then_some(effective),
                    },
                    external_tag_link: None,
                })
            }
        }
    }
    if let Data::Enum(d) = &input.data {
        for v in &d.variants {
            let mut tag = None;
            let mut tag_span = None;
            let mut seen = false;
            for a in v.attrs.iter().filter(|a| a.path().is_ident("zero")) {
                if let Err(e) = a.parse_nested_meta(|m| {
                    let result: syn::Result<()> = (|| {
                        let key = m
                            .path
                            .get_ident()
                            .ok_or_else(|| m.error("expected option name"))?
                            .to_string();
                        if key != "tag" {
                            return Err(
                                m.error(format!("unknown or inapplicable variant option `{key}`"))
                            );
                        }
                        if seen {
                            return Err(m.error("duplicate zero option `tag`"));
                        }
                        seen = true;
                        let p: Path = m.value()?.parse()?;
                        validate_path(&p)?;
                        tag_span = Some(p.span());
                        tag = Some(p);
                        Ok(())
                    })();
                    if let Err(error) = result {
                        combine(&mut errors, error);
                    }
                    Ok(())
                }) {
                    combine(&mut errors, e)
                }
            }
            let shape = match &v.fields {
                Fields::Unit => VariantShape::Unit,
                Fields::Unnamed(x) if x.unnamed.len() == 1 => {
                    VariantShape::Newtype(Box::new(x.unnamed[0].ty.clone()))
                }
                _ => {
                    combine(
                        &mut errors,
                        Error::new(
                            v.fields.span(),
                            "enum variants must be unit or single-field tuple variants",
                        ),
                    );
                    VariantShape::Unit
                }
            };
            variants.push(VariantIr {
                ident: v.ident.clone(),
                shape,
                discriminant: v.discriminant.as_ref().map(|x| x.1.clone()),
                tag,
                span: v.span(),
                tag_span,
            })
        }
    }
    if kind == SchemaKind::Struct {
        if fields.is_empty() {
            combine(
                &mut errors,
                Error::new(
                    input.ident.span(),
                    "schema structs must contain at least one field",
                ),
            );
        } else if fields.iter().all(
            |field| matches!(&field.kind, FieldKind::FixedBytes(length) if literal_zero(length)),
        ) {
            combine(
                &mut errors,
                Error::new(
                    input.ident.span(),
                    "schema struct has a statically zero-sized wire layout",
                ),
            );
            for field in &fields {
                combine(
                    &mut errors,
                    Error::new(
                        field.type_span,
                        "this fixed-byte field has literal zero length",
                    ),
                );
            }
        }
    }
    // Passes 3-5: applicability, exact limits, shapes, lifetimes, expression grammar.
    let mut poisoned = false;
    for f in &fields {
        let s = &f.options.spans;
        let bad = |sp: Option<Span>, name: &str, errors: &mut Option<Error>| {
            if let Some(x) = sp {
                combine(
                    errors,
                    Error::new(x, format!("`{name}` is not applicable to this field")),
                )
            }
        };
        let string = matches!(
            f.kind,
            FieldKind::Utf8 | FieldKind::CStr | FieldKind::U16Str | FieldKind::U16CStr
        );
        if string && f.options.capacity.is_none() {
            combine(
                &mut errors,
                Error::new(f.type_span, "string fields require `capacity`"),
            )
        }
        if !string {
            bad(s.capacity, "capacity", &mut errors);
            bad(s.len_type, "len_type", &mut errors);
            bad(s.tail, "tail", &mut errors)
        }
        if matches!(f.kind, FieldKind::CStr | FieldKind::U16CStr) {
            bad(s.len_type, "len_type", &mut errors);
            if f.options.capacity == Some(0) {
                combine(
                    &mut errors,
                    Error::new(
                        s.capacity.unwrap(),
                        "C string capacity must include a terminator and cannot be zero",
                    ),
                )
            }
        }
        if matches!(f.kind, FieldKind::Utf8 | FieldKind::U16Str) {
            if let Some(cap) = f.options.capacity {
                let len = f
                    .options
                    .len_type
                    .as_ref()
                    .map_or_else(|| "u16".to_owned(), ToString::to_string);
                let max = match len.as_str() {
                    "u8" => u8::MAX as u32,
                    "u16" => u16::MAX as u32,
                    _ => u32::MAX,
                };
                if cap > max {
                    combine(
                        &mut errors,
                        Error::new(s.capacity.unwrap(), "capacity exceeds len_type maximum"),
                    )
                }
            }
        }
        if s.endian.is_some()
            && !matches!(
                f.kind,
                FieldKind::Primitive(_) | FieldKind::Utf8 | FieldKind::U16Str | FieldKind::U16CStr
            )
        {
            bad(s.endian, "endian", &mut errors)
        }
        if s.range.is_some() && !matches!(f.kind, FieldKind::Primitive(_)) {
            bad(s.range, "range", &mut errors)
        }
        if s.must_equal.is_some()
            && matches!(
                f.kind,
                FieldKind::Utf8
                    | FieldKind::CStr
                    | FieldKind::U16Str
                    | FieldKind::U16CStr
                    | FieldKind::FixedBytes(_)
            )
        {
            bad(s.must_equal, "must_equal", &mut errors)
        }
        if let Some(r) = &f.options.range {
            if r.start.as_deref().is_none_or(|e| !expr_allowed(e))
                || r.end.as_deref().is_none_or(|e| !expr_allowed(e))
            {
                combine(
                    &mut errors,
                    Error::new(
                        r.span(),
                        "range endpoints must be capture-free constant expressions",
                    ),
                )
            }
        }
        if let Some(e) = &f.options.must_equal {
            if !expr_allowed(e) {
                combine(
                    &mut errors,
                    Error::new(
                        e.span(),
                        "must_equal must be a capture-free constant expression",
                    ),
                )
            }
        }
        if directly_recursive(&f.original_type, &input.ident) {
            poisoned = true;
        }
        if matches!(
            f.kind,
            FieldKind::Utf8
                | FieldKind::CStr
                | FieldKind::U16Str
                | FieldKind::U16CStr
                | FieldKind::FixedBytes(_)
        ) {
            if let Type::Reference(reference) = &f.original_type {
                if reference
                    .lifetime
                    .as_ref()
                    .is_some_and(|lifetime| lifetime.ident == "static" || lifetime.ident == "_")
                {
                    combine(
                        &mut errors,
                        Error::new(
                            reference.lifetime.as_ref().unwrap().span(),
                            "borrowed views require a named, non-'static lifetime",
                        ),
                    );
                }
            }
        }
    }
    let inherent_api_names = [
        "parse",
        "parse_prefix",
        "encode_into",
        "encode",
        "encoded_len",
        "WIRE_SIZE",
        "WIRE_ALIGN",
        "WIRE_STRIDE",
        "LAYOUT",
    ];
    let trait_associated_names = ["Wire", "DecodeError", "EncodeError", "Tag", "PayloadWire"];
    let conflicts_with_generated_api = |ident: &syn::Ident| {
        let spelling = ident.to_string();
        let logical = spelling.trim_start_matches("r#");
        inherent_api_names.contains(&logical)
            || (!spelling.starts_with("r#") && trait_associated_names.contains(&logical))
    };
    match kind {
        SchemaKind::ScalarEnum => {
            if repr_int_count != 1 || scalar_repr.is_none() {
                combine(
                    &mut errors,
                    Error::new(
                        input.ident.span(),
                        "scalar enums require exactly one #[repr(u8)], #[repr(u16)], or #[repr(u32)]",
                    ),
                )
            }
            if !input.generics.params.is_empty() {
                combine(
                    &mut errors,
                    Error::new(
                        input.generics.span(),
                        "scalar enums cannot have generic parameters",
                    ),
                )
            }
            for (span, option) in [
                (options.spans.align, "align"),
                (options.spans.padding, "padding"),
                (options.spans.tail, "tail"),
                (options.spans.borrow, "borrow"),
                (options.spans.validate_with, "validate_with"),
                (options.spans.tag, "tag"),
            ] {
                if let Some(span) = span {
                    combine(
                        &mut errors,
                        Error::new(span, format!("scalar enums do not accept `{option}`")),
                    );
                }
            }
            for v in &variants {
                if let Some(span) = v.tag_span {
                    combine(
                        &mut errors,
                        Error::new(span, "scalar enum variants do not accept `tag`"),
                    );
                }
                if !matches!(v.shape, VariantShape::Unit) {
                    combine(
                        &mut errors,
                        Error::new(v.span, "scalar enum variants must be fieldless"),
                    )
                }
                if v.discriminant.is_none() {
                    combine(
                        &mut errors,
                        Error::new(
                            v.span,
                            "scalar enum variants require explicit discriminants",
                        ),
                    )
                }
                if conflicts_with_generated_api(&v.ident) {
                    combine(
                        &mut errors,
                        Error::new(v.ident.span(), "variant name conflicts with generated API"),
                    )
                }
            }
        }
        SchemaKind::TaggedEnum => {
            if let Some(span) = options.spans.endian {
                combine(
                    &mut errors,
                    Error::new(span, "tagged enums do not accept container `endian`"),
                )
            }
            if variants.is_empty() {
                combine(
                    &mut errors,
                    Error::new(
                        options.spans.tag.unwrap_or(input.ident.span()),
                        "tagged enums require at least one variant",
                    ),
                )
            }
            if let Some(p) = &options.tag {
                if let Err(e) = validate_path(p) {
                    combine(&mut errors, e)
                }
            }
            if let Some(prefix) = &options.tag {
                for variant in &variants {
                    if let Some(tag) = &variant.tag {
                        let prefix_len = prefix.segments.len();
                        let exact = tag.leading_colon.is_some() == prefix.leading_colon.is_some()
                            && tag.segments.len() == prefix_len + 1
                            && tag.segments.iter().zip(&prefix.segments).all(|(a, b)| {
                                a.ident == b.ident
                                    && matches!(a.arguments, PathArguments::None)
                                    && matches!(b.arguments, PathArguments::None)
                            })
                            && tag.segments.last().is_some_and(|segment| {
                                matches!(segment.arguments, PathArguments::None)
                            });
                        if !exact {
                            combine(
                                &mut errors,
                                Error::new(
                                    variant.tag_span.unwrap_or(variant.span),
                                    "variant tag must be the container tag path followed by exactly one identifier",
                                ),
                            );
                        }
                    }
                }
            }
            for v in &variants {
                if let VariantShape::Newtype(payload) = &v.shape {
                    if directly_recursive(payload, &input.ident) {
                        poisoned = true;
                    }
                }
                if v.tag.is_none() {
                    combine(
                        &mut errors,
                        Error::new(v.span, "tagged enum variants require `tag`"),
                    )
                }
                if v.discriminant.is_some() {
                    combine(
                        &mut errors,
                        Error::new(v.span, "tagged enum variants cannot have discriminants"),
                    )
                }
                if conflicts_with_generated_api(&v.ident) {
                    combine(
                        &mut errors,
                        Error::new(v.ident.span(), "variant name conflicts with generated API"),
                    )
                }
            }
        }
        SchemaKind::Struct => {
            if let Some(span) = options.spans.tail {
                combine(
                    &mut errors,
                    Error::new(span, "structs do not accept `tail`"),
                );
            }
            if let Some(span) = options.spans.tag {
                combine(&mut errors, Error::new(span, "structs do not accept `tag`"));
            }
        }
    }
    let lifetimes: Vec<_> = input
        .generics
        .lifetimes()
        .map(|x| x.lifetime.clone())
        .collect();
    let borrow = options
        .borrow
        .clone()
        .or_else(|| (lifetimes.len() == 1).then(|| lifetimes[0].clone()));
    if let Some(b) = &borrow {
        if !lifetimes.iter().any(|x| x.ident == b.ident) {
            combine(
                &mut errors,
                Error::new(b.span(), "borrow lifetime must name a declared lifetime"),
            )
        }
    } else if lifetimes.len() > 1 && matches!(kind, SchemaKind::Struct | SchemaKind::TaggedEnum) {
        combine(
            &mut errors,
            Error::new(
                input.generics.span(),
                "multiple lifetimes require `borrow = 'name`",
            ),
        )
    }
    let provisional_lifetimes = lifetime_model(&input.generics, borrow.clone());
    if let Some(selected) = &borrow {
        for field in &fields {
            if matches!(
                field.kind,
                FieldKind::Utf8
                    | FieldKind::CStr
                    | FieldKind::U16Str
                    | FieldKind::U16CStr
                    | FieldKind::FixedBytes(_)
            ) {
                if let Type::Reference(reference) = &field.original_type {
                    if let Some(field_lifetime) = &reference.lifetime {
                        let reachable = field_lifetime.ident == selected.ident
                            || provisional_lifetimes.outlives.iter().any(|(from, to)| {
                                from.ident == selected.ident && to.ident == field_lifetime.ident
                            });
                        if !reachable {
                            combine(
                                &mut errors,
                                Error::new(
                                    field_lifetime.span(),
                                    "borrowed view lifetime must equal the selected borrow lifetime or be reachable through its outlives chain",
                                ),
                            );
                        }
                    }
                }
            }
        }
    }
    // Pass 6: external sibling links and graph.
    let mut graph = vec![None; fields.len()];
    let names: BTreeMap<_, _> = fields
        .iter()
        .enumerate()
        .map(|(i, f)| (logical(&f.ident), i))
        .collect();
    for i in 0..fields.len() {
        if let Some(t) = &fields[i].options.tag_field {
            if !matches!(fields[i].kind, FieldKind::Schema) {
                combine(
                    &mut errors,
                    Error::new(
                        fields[i].type_span,
                        "tag_field is only applicable to Schema payload fields",
                    ),
                );
                continue;
            }
            match names.get(&logical(t)) {
                None => combine(
                    &mut errors,
                    Error::new(t.span(), "tag_field names no sibling field"),
                ),
                Some(&j) if i == j => combine(
                    &mut errors,
                    Error::new(t.span(), "tag_field cannot refer to itself"),
                ),
                Some(&j) if !matches!(fields[j].kind, FieldKind::Schema) => combine(
                    &mut errors,
                    Error::new(t.span(), "tag_field must name a Schema field"),
                ),
                Some(&j) => graph[i] = Some(j),
            }
        }
    }
    for (i, t) in graph.iter().enumerate() {
        if let Some(j) = t {
            if graph[*j].is_some() {
                combine(
                    &mut errors,
                    Error::new(
                        fields[i].options.spans.tag_field.unwrap(),
                        "a tag field cannot also be an externally tagged payload",
                    ),
                )
            }
        }
    }
    for (start, field) in fields.iter().enumerate() {
        let mut seen = BTreeSet::new();
        let mut at = Some(start);
        while let Some(i) = at {
            if !seen.insert(i) {
                combine(
                    &mut errors,
                    Error::new(field.type_span, "external tag dependency cycle"),
                );
                break;
            }
            at = graph[i];
        }
    }
    for (field, target) in fields.iter_mut().zip(graph.iter()) {
        field.external_tag_link = *target;
    }
    let mut obligations = vec![
        Obligation {
            kind: ObligationKind::Decode,
            span: input.ident.span(),
            ty: None,
        },
        Obligation {
            kind: ObligationKind::Encode,
            span: input.ident.span(),
            ty: None,
        },
    ];
    if kind == SchemaKind::ScalarEnum {
        obligations.push(Obligation {
            kind: ObligationKind::ScalarEnum,
            span: input.ident.span(),
            ty: None,
        });
        obligations.push(Obligation {
            kind: ObligationKind::ScalarWire,
            span: input.ident.span(),
            ty: None,
        });
    }
    let mut layout = LayoutPlan {
        root_align: options.align,
        ..Default::default()
    };
    let mut es = ErrorShape::default();
    es.decode.push(ErrorCase::Layout);
    es.encode.push(ErrorCase::Layout);
    if kind == SchemaKind::ScalarEnum {
        es.decode.push(ErrorCase::UnknownScalarValue);
    }
    let mut cursor = LayoutExpr::Fixed(0);
    let mut alignments = Vec::new();
    for (i, f) in fields.iter().enumerate() {
        let natural_align = LayoutExpr::FieldAlign(i);
        let field_align = match f.options.align {
            Some(explicit) => LayoutExpr::Max(vec![natural_align, LayoutExpr::Fixed(explicit)]),
            None => natural_align,
        };
        let offset = LayoutExpr::AlignUp(Box::new(cursor.clone()), Box::new(field_align.clone()));
        layout.checked.push(LayoutCheck {
            op: CheckedLayoutOp::AlignUp,
            expression: offset.clone(),
            span: f.type_span,
        });
        let size =
            match &f.kind {
                FieldKind::Primitive(kind) => SymbolicSize::from(kind.wire_size()),
                FieldKind::Bool => SymbolicSize::from(1),
                FieldKind::FixedBytes(n) => SymbolicSize::from(n.clone()),
                FieldKind::Utf8 | FieldKind::CStr | FieldKind::U16Str | FieldKind::U16CStr => {
                    let helper = match f.kind {
                        FieldKind::Utf8 => StringHelper::Utf8,
                        FieldKind::CStr => StringHelper::CStr,
                        FieldKind::U16Str => StringHelper::U16Str,
                        FieldKind::U16CStr => StringHelper::U16CStr,
                        _ => unreachable!(),
                    };
                    let unit_size = if matches!(f.kind, FieldKind::U16Str | FieldKind::U16CStr) {
                        2
                    } else {
                        1
                    };
                    let length_size = f.resolved.length_repr.as_ref().map(|repr| {
                        match repr.to_string().as_str() {
                            "u8" => 1,
                            "u16" => 2,
                            "u32" => 4,
                            _ => unreachable!(),
                        }
                    });
                    SymbolicSize::String {
                        helper,
                        capacity: f.options.capacity.unwrap_or(0),
                        unit_size,
                        length_size,
                    }
                }
                FieldKind::Schema => SymbolicSize::Type(f.original_type.clone()),
            };
        let size_expr = LayoutExpr::FieldSize(i);
        let end = LayoutExpr::Add(Box::new(offset.clone()), Box::new(size_expr));
        layout.checked.push(LayoutCheck {
            op: CheckedLayoutOp::Add,
            expression: end.clone(),
            span: f.type_span,
        });
        layout.fields.push(LayoutField {
            field_index: i,
            size,
            align: f.options.align,
            offset,
            stride: end.clone(),
        });
        cursor = end;
        alignments.push(field_align);
        if let Some(endian) = f.resolved.target_endian_check {
            layout.wide_checks.push((i, endian));
            obligations.push(Obligation {
                kind: ObligationKind::WideTarget(endian),
                span: f.type_span,
                ty: Some(f.original_type.clone()),
            });
        }
        if f.resolved.tail == Tail::Zero {
            obligations.push(Obligation {
                kind: ObligationKind::Tail,
                span: f.options.spans.tail.unwrap_or(f.type_span),
                ty: Some(f.original_type.clone()),
            });
            es.decode.push(ErrorCase::NonZeroTail);
        }
        match &f.kind {
            FieldKind::Bool => es.decode.push(ErrorCase::InvalidBool),
            FieldKind::Utf8 => {
                es.decode
                    .extend([ErrorCase::LengthOutOfBounds, ErrorCase::InvalidUtf8]);
                es.encode.push(ErrorCase::CapacityExceeded);
            }
            FieldKind::CStr | FieldKind::U16CStr => {
                es.decode.push(ErrorCase::MissingNul);
                es.encode.push(ErrorCase::CapacityExceeded);
            }
            FieldKind::U16Str => {
                es.decode.push(ErrorCase::LengthOutOfBounds);
                es.encode.push(ErrorCase::CapacityExceeded);
            }
            FieldKind::Schema => {
                es.decode.push(ErrorCase::Nested);
                es.encode.push(ErrorCase::Nested);
                for obligation in [
                    ObligationKind::Schema,
                    ObligationKind::Decode,
                    ObligationKind::Encode,
                ] {
                    obligations.push(Obligation {
                        kind: obligation,
                        span: f.type_span,
                        ty: Some(f.original_type.clone()),
                    });
                }
                if f.options.must_equal.is_some() {
                    obligations.push(Obligation {
                        kind: ObligationKind::ScalarEnum,
                        span: f.options.spans.must_equal.unwrap_or(f.type_span),
                        ty: Some(f.original_type.clone()),
                    });
                }
            }
            _ => {}
        }
        if f.options.range.is_some() {
            es.decode.push(ErrorCase::RangeViolation);
            es.encode.push(ErrorCase::RangeViolation);
        }
        if f.options.must_equal.is_some() {
            es.decode.push(ErrorCase::MustEqualViolation);
            es.encode.push(ErrorCase::MustEqualViolation);
        }
        if f.options.validate_with.is_some() {
            obligations.push(Obligation {
                kind: ObligationKind::Validator,
                span: f.options.spans.validate_with.unwrap_or(f.type_span),
                ty: Some(f.original_type.clone()),
            });
            es.decode.push(ErrorCase::Custom);
            es.encode.push(ErrorCase::Custom);
        }
    }
    let natural_root_align = LayoutExpr::Max(alignments);
    let root_align = match options.align {
        Some(explicit) => LayoutExpr::Max(vec![natural_root_align, LayoutExpr::Fixed(explicit)]),
        None => natural_root_align,
    };
    let aggregate_size = LayoutExpr::AlignUp(Box::new(cursor), Box::new(root_align.clone()));
    layout.checked.push(LayoutCheck {
        op: CheckedLayoutOp::AlignUp,
        expression: aggregate_size.clone(),
        span: input.ident.span(),
    });
    layout.aggregate_align = Some(root_align);
    layout.aggregate_size = Some(aggregate_size.clone());
    layout.aggregate_stride = Some(aggregate_size);
    obligations.push(Obligation {
        kind: ObligationKind::Layout,
        span: input.ident.span(),
        ty: None,
    });
    obligations.push(Obligation {
        kind: ObligationKind::WholeInput,
        span: input.ident.span(),
        ty: None,
    });
    if options.tail == Tail::Zero {
        obligations.push(Obligation {
            kind: ObligationKind::Tail,
            span: options.spans.tail.unwrap_or(input.ident.span()),
            ty: None,
        });
        es.decode.push(ErrorCase::NonZeroTail);
    }
    if options.padding == Padding::Zero {
        obligations.push(Obligation {
            kind: ObligationKind::Padding,
            span: options.spans.padding.unwrap_or(input.ident.span()),
            ty: None,
        });
        es.decode.push(ErrorCase::NonZeroPadding);
    }
    if options.validate_with.is_some() {
        obligations.push(Obligation {
            kind: ObligationKind::Validator,
            span: options.spans.validate_with.unwrap_or(input.ident.span()),
            ty: None,
        });
        es.decode.push(ErrorCase::Custom);
        es.encode.push(ErrorCase::Custom);
    }
    if kind == SchemaKind::TaggedEnum {
        es.decode.push(ErrorCase::UnknownUnionTag);
        for variant in &variants {
            if let VariantShape::Newtype(payload) = &variant.shape {
                for obligation in [
                    ObligationKind::Schema,
                    ObligationKind::Decode,
                    ObligationKind::Encode,
                ] {
                    obligations.push(Obligation {
                        kind: obligation,
                        span: variant.span,
                        ty: Some(payload.as_ref().clone()),
                    });
                }
                es.decode.push(ErrorCase::Nested);
                es.encode.push(ErrorCase::Nested);
            }
        }
        let payloads: Vec<&Type> = variants
            .iter()
            .filter_map(|variant| match &variant.shape {
                VariantShape::Newtype(ty) => Some(ty.as_ref()),
                VariantShape::Unit => None,
            })
            .collect();
        if !payloads.is_empty() {
            layout.tagged_payload_size = Some(LayoutExpr::Max(
                payloads
                    .iter()
                    .map(|ty| LayoutExpr::TypeSize((*ty).clone()))
                    .collect(),
            ));
            layout.tagged_payload_align = Some(LayoutExpr::Max(
                payloads
                    .iter()
                    .map(|ty| LayoutExpr::TypeAlign((*ty).clone()))
                    .collect(),
            ));
        }
        if let Some(target) = &options.tag {
            let target: Type = syn::parse_quote!(#target);
            obligations.push(Obligation {
                kind: ObligationKind::ScalarEnum,
                span: options.spans.tag.unwrap_or(input.ident.span()),
                ty: Some(target.clone()),
            });
            obligations.push(Obligation {
                kind: ObligationKind::ScalarWire,
                span: options.spans.tag.unwrap_or(input.ident.span()),
                ty: Some(target),
            });
        }
    }
    for (payload_index, target_index) in graph
        .iter()
        .enumerate()
        .filter_map(|(index, target)| target.map(|target| (index, target)))
    {
        let payload = fields[payload_index].original_type.clone();
        obligations.push(Obligation {
            kind: ObligationKind::ExternalTag,
            span: fields[payload_index].type_span,
            ty: Some(payload.clone()),
        });
        let target = fields[target_index].original_type.clone();
        for obligation in [
            ObligationKind::TaggedUnion,
            ObligationKind::DecodeTaggedUnion,
        ] {
            obligations.push(Obligation {
                kind: obligation,
                span: fields[payload_index].type_span,
                ty: Some(payload.clone()),
            });
        }
        for obligation in [ObligationKind::ScalarEnum, ObligationKind::ScalarWire] {
            obligations.push(Obligation {
                kind: obligation,
                span: fields[target_index].type_span,
                ty: Some(target.clone()),
            });
        }
        es.encode.push(ErrorCase::TagMismatch);
    }
    for path in options
        .runtime
        .iter()
        .chain(options.validate_with.iter())
        .chain(options.tag.iter())
        .chain(
            fields
                .iter()
                .filter_map(|field| field.options.validate_with.as_ref()),
        )
    {
        if !path_allowed(path) {
            combine(
                &mut errors,
                Error::new(
                    path.span(),
                    "macros are not supported in syntax moved into generated code",
                ),
            );
        }
    }
    let moved = moved_paths(&input, &options, &fields, &variants);
    if let Some(e) = errors {
        return Err(e);
    }
    let suffix = stable_suffix(&input);
    let lname = logical(&input.ident);
    Ok(SchemaIr {
        kind,
        ident: input.ident,
        visibility: input.vis.clone(),
        original_generics: input.generics.clone(),
        cleaned_generics: cleaned(input.generics),
        borrow_lifetime: borrow,
        source_lifetime: provisional_lifetimes.source,
        scalar_repr,
        options: options.clone(),
        fields,
        variants,
        generated_names: GeneratedNames {
            module: format_ident!("__zero_schema_{}_{}", lname, suffix),
            wire: format_ident!("Wire"),
            decode_error: format_ident!("{}DecodeError", lname),
            encode_error: format_ident!("{}EncodeError", lname),
        },
        obligations,
        layout_plan: layout,
        error_shape: es,
        path_resolution: PathResolution {
            parent_runtime_path: options.runtime.clone(),
            hidden_runtime_path: options.runtime.as_ref().map(hidden_path),
            runtime_source: if options.runtime.is_some() {
                RuntimePathSource::Explicit
            } else {
                RuntimePathSource::ResolveDirectDependency
            },
        },
        moved_paths: moved,
        visibility_plan: visibility(&input.vis),
        external_tag_graph: graph,
        poisoned,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    fn ok(s: &str) -> SchemaIr {
        build(syn::parse_str(s).unwrap()).unwrap()
    }
    fn errs(s: &str) -> Vec<String> {
        match build(syn::parse_str(s).unwrap()) {
            Ok(_) => panic!("expected error"),
            Err(e) => e.into_iter().map(|e| e.to_string()).collect(),
        }
    }
    #[test]
    fn scalar_compat_and_quoted() {
        assert!(
            ok("#[repr(u16)] #[zero(endian=\"big\",crate=zs)] enum E{A=1}")
                .options
                .endian
                == Endian::Big
        );
        assert!(
            errs("#[repr(u8)] #[zero(endian=native)] enum E{A=0}")
                .iter()
                .any(|error| error.contains("quoted string literal"))
        );
    }
    #[test]
    fn aggregates_independent_errors_in_one_option_list() {
        let errors = errs("struct S { #[zero(align=3, len_type=u64)] value:u8 }");
        assert_eq!(errors.len(), 2, "{errors:?}");
        assert!(
            errors
                .iter()
                .any(|error| error.contains("alignment must be a power of two"))
        );
        assert!(
            errors
                .iter()
                .any(|error| error.contains("len_type must be u8, u16, or u32"))
        );
    }
    #[test]
    fn literals_radix_limits() {
        let x = ok(
            "#[zero(align=0x8)] struct S<'a>{#[zero(capacity=0b1111_1111,len_type=u8)] x:&'a str}",
        );
        assert_eq!(x.fields[0].options.capacity, Some(255));
        assert!(
            errs("struct S<'a>{#[zero(capacity=256,len_type=u8)] x:&'a str}")
                .iter()
                .any(|x| x.contains("maximum"))
        )
    }
    #[test]
    fn strings_zero_and_c_rules() {
        ok("struct S<'a>{#[zero(capacity=0)] x:&'a str}");
        assert!(
            errs("struct S<'a>{#[zero(capacity=0)] x:&'a CStr}")
                .iter()
                .any(|x| x.contains("terminator"))
        )
    }
    #[test]
    fn repr_and_packed() {
        ok("#[repr(C,align(8))] struct S{x:u32}");
        assert!(
            errs("#[repr(C,packed(2))] struct S{x:u32}")
                .iter()
                .any(|x| x.contains("packed"))
        )
    }
    #[test]
    fn tag_graphs() {
        let x = ok("struct S{tag:T,#[zero(tag_field=tag)] a:U,#[zero(tag_field=tag)] b:V}");
        assert_eq!(x.external_tag_graph, vec![None, Some(0), Some(0)]);
        assert!(
            errs("struct S{#[zero(tag_field=b)] a:A,#[zero(tag_field=a)] b:B}")
                .iter()
                .any(|x| x.contains("cycle") || x.contains("also"))
        )
    }
    #[test]
    fn range_grammar() {
        ok("struct S{#[zero(range=(A::MIN+1)..=f(3),must_equal=(1 as u32))] x:u32}");
        assert!(
            errs("struct S{#[zero(range=(||1)..2)] x:u32}")
                .iter()
                .any(|x| x.contains("capture-free"))
        )
    }
    #[test]
    fn lifetimes_and_clean_attrs() {
        assert!(errs("struct S<'a,'b> where 'a:'b{#[zero(capacity=2)] x:&'a str,#[zero(capacity=2)] y:&'b str}").iter().any(|x|x.contains("multiple lifetimes")));
        let x = ok(
            "#[zero(borrow='a)] struct S<'a,'b> where 'a:'b{#[zero(capacity=2)] x:&'a str,#[zero(capacity=2)] y:&'b str}",
        );
        assert_eq!(x.borrow_lifetime.unwrap().ident, "a");
        assert_eq!(
            x.cleaned_generics
                .where_clause
                .as_ref()
                .unwrap()
                .predicates
                .len(),
            1
        );
        assert!(
            errs("#[zero(borrow='b)] struct S<'a,'b> where 'a:'b{#[zero(capacity=2)] x:&'a str}")
                .iter()
                .any(|x| x.contains("outlives chain"))
        )
    }
    #[test]
    fn stray_macro_and_type_macro() {
        assert!(
            errs("struct S<#[zero(x)] T>{x:T}")
                .iter()
                .any(|x| x.contains("nested"))
        );
        assert!(errs("struct S{x:m!()}").iter().any(|x| x.contains("macro")))
    }
    #[test]
    fn tagged_zero_and_tags() {
        assert!(
            errs("#[zero(tag=T)] enum E{}")
                .iter()
                .any(|x| x.contains("at least one"))
        );
        let x = ok("#[zero(tag=T)] enum E{#[zero(tag=T::A)] A,#[zero(tag=T::B)] B(u8)}");
        assert!(x.kind == SchemaKind::TaggedEnum)
    }
    #[test]
    fn raw_names_and_paths() {
        ok("#[repr(u8)] enum E{r#type=1}");
        for name in [
            "Wire",
            "DecodeError",
            "EncodeError",
            "Tag",
            "PayloadWire",
            "parse",
            "parse_prefix",
            "encode_into",
            "encode",
            "encoded_len",
            "WIRE_SIZE",
            "WIRE_ALIGN",
            "WIRE_STRIDE",
            "LAYOUT",
        ] {
            let source = format!("#[repr(u8)] enum E{{{name}=1}}");
            assert!(
                errs(&source).iter().any(|x| x.contains("generated API")),
                "reserved name {name} was accepted"
            );
        }
        for name in ["Wire", "DecodeError", "EncodeError", "Tag", "PayloadWire"] {
            ok(&format!("#[repr(u8)] enum E{{r#{name}=1}}"));
        }
        for name in ["parse", "WIRE_SIZE", "LAYOUT"] {
            let source = format!("#[repr(u8)] enum E{{r#{name}=1}}");
            assert!(errs(&source).iter().any(|x| x.contains("generated API")));
        }
        ok("struct S<Wire>{DecodeError:Wire}");
        let x = ok("#[zero(crate=self::rt)] struct S{x:u8}");
        assert!(x.moved_paths[0].strategy == PathRebase::RebaseOneLevel)
    }
    #[test]
    fn symbolic_fixed_byte_length_is_preserved() {
        let ir = ok("struct S<'a, const N: usize>{bytes: &'a [u8; N + 1]}");
        let FieldKind::FixedBytes(length) = &ir.fields[0].kind else {
            panic!("expected fixed bytes")
        };
        assert_eq!(length.to_token_stream().to_string(), "N + 1");
        let SymbolicSize::Expr(length) = &ir.layout_plan.fields[0].size else {
            panic!("expected symbolic layout expression")
        };
        assert_eq!(length.to_token_stream().to_string(), "N + 1");
    }
    #[test]
    fn runtime_paths_are_frozen_for_both_scopes() {
        for (written, parent, hidden) in [
            ("self::rt", "self :: rt", "super :: rt"),
            ("super::rt", "super :: rt", "super :: super :: rt"),
            ("crate::rt", "crate :: rt", "crate :: rt"),
            ("::rt", ":: rt", ":: rt"),
        ] {
            let ir = ok(&format!("#[zero(crate={written})] struct S{{x:u8}}"));
            assert_eq!(
                ir.path_resolution
                    .parent_runtime_path
                    .unwrap()
                    .to_token_stream()
                    .to_string(),
                parent
            );
            assert_eq!(
                ir.path_resolution
                    .hidden_runtime_path
                    .unwrap()
                    .to_token_stream()
                    .to_string(),
                hidden
            );
        }
    }

    #[test]
    fn moved_syntax_is_recursive_but_discriminants_are_not_moved() {
        let ir = ok("#[repr(u8)] enum E { A = value!() }");
        assert!(ir.variants[0].discriminant.is_some());
        let ir = ok(
            "struct S<T: self::Bound> where <T as super::Trait>::Assoc: crate::Other { #[zero(range=qself::MIN..=Self::MAX,must_equal=crate::VALUE,validate_with=super::validate)] x:u32, y:T }",
        );
        let paths: Vec<_> = ir
            .moved_paths
            .iter()
            .map(|path| (path.path.to_token_stream().to_string(), path.strategy))
            .collect();
        assert!(
            paths.iter().any(|(path, strategy)| path == "self :: Bound"
                && *strategy == PathRebase::RebaseOneLevel)
        );
        assert!(paths.iter().any(|(path, strategy)| path.starts_with("Self")
            && *strategy == PathRebase::RewriteSchemaSelf));
        assert!(paths.iter().any(|(path, _)| path == "super :: validate"));
        assert!(paths.iter().any(|(path, _)| path == "crate :: VALUE"));
    }

    #[test]
    fn lifetime_outlives_closure_has_no_encode_infection() {
        let ir = ok("#[zero(borrow='a)] struct S<'a: 'b, 'b: 'c, 'c>{x:u8}");
        let model = lifetime_model(&ir.original_generics, ir.borrow_lifetime.clone());
        let edges: BTreeSet<_> = model
            .outlives
            .iter()
            .map(|(a, b)| (a.ident.to_string(), b.ident.to_string()))
            .collect();
        assert!(edges.contains(&("__zero_input".into(), "a".into())));
        assert!(edges.contains(&("__zero_input".into(), "c".into())));
        assert!(
            !edges
                .iter()
                .any(|(a, b)| a.contains("encode") || b.contains("encode"))
        );
    }

    #[test]
    fn private_schema_support_reaches_parent_module() {
        let ir = ok("struct Private { value: u8 }");
        assert!(matches!(ir.visibility_plan.module, Visibility::Inherited));
        assert_eq!(
            ir.visibility_plan.support.to_token_stream().to_string(),
            "pub (super)"
        );
    }

    #[test]
    fn parent_visibility_rebases_to_valid_restricted_syntax() {
        let ir = ok("pub(super) struct Restricted { value: u8 }");
        assert_eq!(
            ir.visibility_plan.support.to_token_stream().to_string(),
            "pub (in super :: super)"
        );
    }

    #[test]
    fn restricted_visibility_and_complete_reachability_are_frozen() {
        let ir = ok(
            "#[zero(padding=\"zero\",validate_with=self::whole)] pub(in self::api) struct S{#[zero(validate_with=super::field,range=0..=1,must_equal=1)] x:u8,tag:T,#[zero(tag_field=tag)] payload:U}",
        );
        assert_eq!(
            ir.visibility_plan.module.to_token_stream().to_string(),
            "pub (in self :: api)"
        );
        assert_eq!(
            ir.visibility_plan.support.to_token_stream().to_string(),
            "pub (in super :: api)"
        );
        assert!(
            ir.obligations
                .iter()
                .any(|o| matches!(o.kind, ObligationKind::Padding))
        );
        assert!(
            ir.obligations
                .iter()
                .any(|o| matches!(o.kind, ObligationKind::WholeInput))
        );
        assert!(
            ir.obligations
                .iter()
                .any(|o| matches!(o.kind, ObligationKind::ExternalTag))
        );
        assert!(ir.error_shape.decode.contains(&ErrorCase::NonZeroPadding));
        assert!(ir.error_shape.decode.contains(&ErrorCase::RangeViolation));
        assert!(
            ir.error_shape
                .encode
                .contains(&ErrorCase::MustEqualViolation)
        );
    }

    #[test]
    fn generated_namespace_is_deterministic_and_collision_resistant() {
        let a = ok("struct S{x:u8}");
        let same = ok("struct S{x:u8}");
        let other = ok("struct S{x:u16}");
        assert_eq!(a.generated_names.module, same.generated_names.module);
        assert_ne!(a.generated_names.module, other.generated_names.module);
        assert_eq!(a.generated_names.wire, "Wire");
    }
    #[test]
    fn generated_namespace_has_frozen_normalized_feed() {
        let suffix = stable_suffix(&syn::parse_str("struct S{x:u8}").unwrap());
        assert_eq!(suffix, "811ca6bee2889664");
        assert_eq!(
            suffix,
            stable_suffix(&syn::parse_str("struct S{x:u8}").unwrap())
        );
        assert_ne!(
            suffix,
            stable_suffix(&syn::parse_str("struct r#S{x:u8}").unwrap())
        );
        let _unrelated = stable_suffix(&syn::parse_str("struct Other{y:u32}").unwrap());
        assert_eq!(
            suffix,
            stable_suffix(&syn::parse_str("struct S{x:u8}").unwrap())
        );
    }
    #[test]
    fn zero_structs_are_rejected_without_eagerly_rejecting_symbolic_sizes() {
        assert!(
            errs("struct Empty {}")
                .iter()
                .any(|e| e.contains("at least one field"))
        );
        let errors = errs("struct Zero<'a>{a:&'a [u8;0],b:&'a [u8;((0))]}");
        assert!(errors.iter().any(|e| e.contains("statically zero-sized")));
        assert_eq!(
            errors
                .iter()
                .filter(|e| e.contains("literal zero length"))
                .count(),
            2
        );
        ok("struct Lazy<'a,const N:usize>{a:&'a [u8;N]}");
        ok("struct Mixed<'a>{z:&'a [u8;0],nested:Child}");
    }
    #[test]
    fn canonical_owned_containers_are_targeted_but_aliases_stay_opaque() {
        for spelling in [
            "String",
            "Box<u8>",
            "Option<u8>",
            "alloc::string::String",
            "alloc::boxed::Box<u8>",
            "core::option::Option<u8>",
            "std::string::String",
            "std::boxed::Box<u8>",
            "std::option::Option<u8>",
        ] {
            assert!(
                errs(&format!("struct S{{x:{spelling}}}"))
                    .iter()
                    .any(|e| e.contains("owned container")),
                "{spelling}"
            );
        }
        for spelling in ["Vec<u8>", "alloc::vec::Vec<u8>", "std::vec::Vec<u8>"] {
            assert!(
                errs(&format!("struct S{{x:{spelling}}}"))
                    .iter()
                    .any(|e| e.contains("dynamic-layout")),
                "{spelling}"
            );
        }
        ok("struct S{x:my::Vec}");
        ok("struct T{x:renamed::String}");
    }
    #[test]
    fn applicability_and_wide_plan() {
        let x = ok("#[zero(endian=\"little\")] struct S<'a>{#[zero(capacity=3)] x:&'a U16Str}");
        assert_eq!(x.layout_plan.wide_checks.len(), 1);
        assert!(
            errs("struct S{#[zero(capacity=2)] x:u32}")
                .iter()
                .any(|x| x.contains("not applicable"))
        )
    }
    #[test]
    fn audit_freeze_cases() {
        ok("#[repr(u8)] enum E { A = rustc_owned!() }");
        assert!(
            errs("#[zero(tag=T)] enum E { #[zero(tag=T::A)] A(#[zero(capacity=1)] u8) }")
                .iter()
                .any(|e| e.contains("nested"))
        );
        ok("struct S { wrapped: my::Option<Self> }");
        assert!(ok("struct S<T> { direct: S<T> }").poisoned);
        assert!(
            errs("#[zero(tag=T)] enum E { #[zero(tag=Other::A)] A }")
                .iter()
                .any(|e| e.contains("exactly one identifier"))
        );
        assert!(
            errs("struct S { tag:u8, #[zero(tag_field=tag)] payload:U }")
                .iter()
                .any(|e| e.contains("Schema field"))
        );

        let tagged = ok("#[zero(tag=T)] enum E { #[zero(tag=T::A)] A(P) }");
        for kind in [
            ObligationKind::Schema,
            ObligationKind::Decode,
            ObligationKind::Encode,
        ] {
            assert!(
                tagged
                    .obligations
                    .iter()
                    .any(
                        |o| core::mem::discriminant(&o.kind) == core::mem::discriminant(&kind)
                            && o.ty.is_some()
                    )
            );
        }
        assert!(
            !tagged
                .obligations
                .iter()
                .any(|o| matches!(o.kind, ObligationKind::TaggedUnion) && o.ty.is_some())
        );
        assert!(!tagged.error_shape.encode.contains(&ErrorCase::TagMismatch));

        let external = ok("struct S { tag:T, #[zero(tag_field=tag)] payload:U }");
        assert!(
            !external
                .error_shape
                .decode
                .contains(&ErrorCase::TagMismatch)
        );
        assert!(
            external
                .error_shape
                .encode
                .contains(&ErrorCase::TagMismatch)
        );
        let scalar_constraint = ok("struct S { #[zero(must_equal=V::A)] value:V }");
        assert!(
            scalar_constraint
                .obligations
                .iter()
                .any(|o| matches!(o.kind, ObligationKind::ScalarEnum) && o.ty.is_some())
        );

        let strings = ok(
            "struct S<'a> { #[zero(capacity=7,len_type=u8)] text:&'a str, #[zero(capacity=3)] wide:&'a U16CStr }",
        );
        assert!(matches!(
            strings.layout_plan.fields[0].size,
            SymbolicSize::String {
                helper: StringHelper::Utf8,
                capacity: 7,
                unit_size: 1,
                length_size: Some(1)
            }
        ));
        assert!(matches!(
            strings.layout_plan.fields[1].size,
            SymbolicSize::String {
                helper: StringHelper::U16CStr,
                capacity: 3,
                unit_size: 2,
                length_size: None
            }
        ));
        let zero_tail = ok("struct T<'a> { #[zero(capacity=3,tail=\"zero\")] text:&'a str }");
        assert!(
            zero_tail
                .error_shape
                .decode
                .contains(&ErrorCase::NonZeroTail)
        );
        assert!(
            zero_tail
                .obligations
                .iter()
                .any(|o| matches!(o.kind, ObligationKind::Tail) && o.ty.is_some())
        );
        ok(
            "struct LargeC<'a> { #[zero(capacity=65536)] text:&'a CStr, #[zero(capacity=0xffff_ffff)] wide:&'a U16CStr }",
        );
        assert!(
            errs("struct TooLarge<'a> { #[zero(capacity=65536)] text:&'a str }")
                .iter()
                .any(|e| e.contains("maximum"))
        );
        assert!(
            errs("struct BadPayload { tag:T, #[zero(tag_field=tag)] payload:u32 }")
                .iter()
                .any(|e| e.contains("only applicable to Schema payload"))
        );
        assert!(
            external
                .obligations
                .iter()
                .any(|o| matches!(o.kind, ObligationKind::DecodeTaggedUnion))
        );
        assert!(
            external
                .obligations
                .iter()
                .any(|o| matches!(o.kind, ObligationKind::ScalarWire))
        );
    }
    #[test]
    fn audit_regressions_are_typed_and_poisoned() {
        let qualified = ok("struct S{x: wire::u8, y: model::bool}");
        assert!(
            qualified
                .fields
                .iter()
                .all(|field| matches!(field.kind, FieldKind::Schema))
        );
        let nested = ok("struct S{#[zero(validate_with=check)] x: Child}");
        assert_eq!(
            nested
                .obligations
                .iter()
                .filter(|o| o.ty.is_some()
                    && matches!(
                        o.kind,
                        ObligationKind::Schema
                            | ObligationKind::Decode
                            | ObligationKind::Encode
                            | ObligationKind::Validator
                    ))
                .count(),
            4
        );
        assert!(nested.layout_plan.aggregate_size.is_some());
        assert_eq!(nested.layout_plan.checked.len(), 3);
        let scalar = ok("#[repr(u8)] enum E{A=1}");
        assert!(
            scalar
                .error_shape
                .decode
                .contains(&ErrorCase::UnknownScalarValue)
        );
        assert!(
            errs("#[repr(u8)] enum E{#[zero(tag=T::A)] A=1}")
                .iter()
                .any(|e| e.contains("do not accept `tag`"))
        );
        let tagged = ok("#[zero(tag=T)] enum E{#[zero(tag=T::A)] A(Child)}");
        assert!(
            tagged.error_shape.decode.contains(&ErrorCase::Nested)
                && tagged.error_shape.encode.contains(&ErrorCase::Nested)
        );
        assert!(tagged.layout_plan.tagged_payload_size.is_some());
        assert!(ok("#[zero(tag=T)] enum E{#[zero(tag=T::A)] A(Self)}").poisoned);
    }
    #[test]
    fn moved_expressions_reject_opaque_syntax_and_lifetime_is_fresh() {
        assert!(
            errs("struct S{#[zero(must_equal=self)] x:u8}")
                .iter()
                .any(|e| e.contains("capture-free"))
        );
        assert!(
            errs("struct S{#[zero(must_equal=(1 as m!()))] x:u8}")
                .iter()
                .any(|e| e.contains("capture-free"))
        );
        assert!(
            errs("struct S{#[zero(validate_with=check::<m!()>)] x:u8}")
                .iter()
                .any(|e| e.contains("macros are not supported"))
        );
        let ir = ok("#[zero(borrow='__zero_input)] struct S<'__zero_input>{x:u8}");
        assert_ne!(ir.source_lifetime.ident, "__zero_input");
    }
    #[test]
    fn layout_plan_uses_natural_alignment_and_checked_order() {
        let ir = ok(
            "#[zero(align=2)] struct S<'a>{a:u32,#[zero(align=1)] b:u16,c:bool,d:&'a [u8;3],#[zero(capacity=3,len_type=u32)] s:&'a str,#[zero(capacity=2,len_type=u8)] w:&'a U16Str,n:Child}",
        );
        fn contains_field_alignment(expression: &LayoutExpr, field: usize) -> bool {
            match expression {
                LayoutExpr::FieldAlign(index) => *index == field,
                LayoutExpr::Max(values) => values
                    .iter()
                    .any(|value| contains_field_alignment(value, field)),
                _ => false,
            }
        }
        fn contains_fixed(expression: &LayoutExpr, expected: u32) -> bool {
            match expression {
                LayoutExpr::Fixed(value) => *value == expected,
                LayoutExpr::Max(values) => {
                    values.iter().any(|value| contains_fixed(value, expected))
                }
                _ => false,
            }
        }
        for field in 0..7 {
            assert!(matches!(
                &ir.layout_plan.fields[field].offset,
                LayoutExpr::AlignUp(_, alignment)
                    if contains_field_alignment(alignment, field)
            ));
        }
        assert!(matches!(
            &ir.layout_plan.fields[1].offset,
            LayoutExpr::AlignUp(_, alignment) if contains_fixed(alignment, 1)
        ));
        assert!(matches!(
            &ir.layout_plan.aggregate_align,
            Some(alignment) if contains_fixed(alignment, 2)
        ));
        let expected = [
            CheckedLayoutOp::AlignUp,
            CheckedLayoutOp::Add,
            CheckedLayoutOp::AlignUp,
            CheckedLayoutOp::Add,
            CheckedLayoutOp::AlignUp,
            CheckedLayoutOp::Add,
            CheckedLayoutOp::AlignUp,
            CheckedLayoutOp::Add,
            CheckedLayoutOp::AlignUp,
            CheckedLayoutOp::Add,
            CheckedLayoutOp::AlignUp,
            CheckedLayoutOp::Add,
            CheckedLayoutOp::AlignUp,
            CheckedLayoutOp::Add,
            CheckedLayoutOp::AlignUp,
        ];
        assert!(
            ir.layout_plan
                .checked
                .iter()
                .map(|check| check.op)
                .eq(expected)
        );
    }

    #[test]
    fn wide_checks_zero_literals_and_generic_binders_are_preserved_correctly() {
        let wide = ok(
            "#[zero(endian=\"big\")] struct W<'a>{#[zero(capacity=1)] a:&'a U16Str,#[zero(capacity=1,endian=\"native\")] b:&'a U16CStr}",
        );
        assert!(wide.layout_plan.wide_checks == vec![(0, Endian::Big)]);
        let errors = errs("struct Z<'a>{a:&'a [u8;0x0],b:&'a [u8;0b0_0],c:&'a [u8;00]}");
        assert_eq!(
            errors
                .iter()
                .filter(|error| error.contains("literal zero length"))
                .count(),
            3
        );

        let ir = ok(
            "struct G<#[allow(dead_code)] T> where for<#[allow(dead_code)] 'a> T: Trait<'a> { value:T }",
        );
        assert!(
            matches!(&ir.original_generics.params[0], GenericParam::Type(parameter) if !parameter.attrs.is_empty())
        );
        assert!(
            matches!(&ir.cleaned_generics.params[0], GenericParam::Type(parameter) if parameter.attrs.is_empty())
        );
        let predicate = ir
            .cleaned_generics
            .where_clause
            .as_ref()
            .unwrap()
            .predicates
            .first()
            .unwrap();
        let WherePredicate::Type(predicate) = predicate else {
            panic!("expected type predicate")
        };
        assert!(
            predicate.lifetimes.as_ref().unwrap().lifetimes.iter().all(
                |parameter| match parameter {
                    GenericParam::Lifetime(parameter) => parameter.attrs.is_empty(),
                    _ => false,
                }
            )
        );
        assert!(
            lifetime_model(&ir.original_generics, ir.borrow_lifetime.clone())
                .outlives
                .iter()
                .all(|(from, to)| from.ident != "a" && to.ident != "a")
        );
    }
}
