use proc_macro2::{Span, TokenStream};
use std::collections::BTreeMap;
use syn::{
    Attribute, Error, Ident, Lifetime, LitInt, LitStr, Path, parse::Parser, spanned::Spanned as _,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Endian {
    Native,
    Little,
    Big,
}

impl Endian {
    pub(crate) fn runtime_variant(self) -> Ident {
        match self {
            Self::Native => Ident::new("Native", Span::call_site()),
            Self::Little => Ident::new("Little", Span::call_site()),
            Self::Big => Ident::new("Big", Span::call_site()),
        }
    }
}

#[derive(Clone)]
pub(crate) struct SpannedValue<T> {
    pub(crate) value: T,
    pub(crate) span: Span,
}

#[derive(Clone)]
pub(crate) struct ContainerOptions {
    pub(crate) endian: Endian,
    pub(crate) endian_span: Option<Span>,
    pub(crate) align: Option<SpannedValue<u32>>,
    pub(crate) runtime: Option<SpannedValue<Path>>,
    pub(crate) borrow: Option<SpannedValue<Lifetime>>,
}

impl Default for ContainerOptions {
    fn default() -> Self {
        Self {
            endian: Endian::Native,
            endian_span: None,
            align: None,
            runtime: None,
            borrow: None,
        }
    }
}

#[derive(Clone, Default)]
pub(crate) struct FieldOptions {
    pub(crate) capacity: Option<SpannedValue<usize>>,
    pub(crate) len_type: Option<SpannedValue<Ident>>,
    pub(crate) endian: Option<SpannedValue<Endian>>,
    pub(crate) align: Option<SpannedValue<u32>>,
    pub(crate) tag_field: Option<SpannedValue<Ident>>,
}

#[derive(Clone)]
pub(crate) struct VariantOptions {
    pub(crate) tag: SpannedValue<Path>,
}

fn combine(into: &mut Option<Error>, error: Error) {
    if let Some(existing) = into {
        existing.combine(error);
    } else {
        *into = Some(error);
    }
}

fn duplicate(seen: &mut BTreeMap<String, Span>, key: &str, span: Span) -> syn::Result<()> {
    if seen.insert(key.to_owned(), span).is_some() {
        Err(Error::new(span, format!("duplicate zero option `{key}`")))
    } else {
        Ok(())
    }
}

fn key(meta: &syn::meta::ParseNestedMeta<'_>) -> syn::Result<String> {
    meta.path
        .get_ident()
        .map(ToString::to_string)
        .ok_or_else(|| meta.error("zero option names must be identifiers"))
}

fn parse_endian(meta: &syn::meta::ParseNestedMeta<'_>) -> syn::Result<SpannedValue<Endian>> {
    let value: LitStr = meta.value()?.parse()?;
    let endian = match value.value().as_str() {
        "native" => Endian::Native,
        "little" => Endian::Little,
        "big" => Endian::Big,
        _ => {
            return Err(Error::new(
                value.span(),
                "endian must be \"native\", \"little\", or \"big\"",
            ));
        }
    };
    Ok(SpannedValue {
        value: endian,
        span: value.span(),
    })
}

fn parse_alignment(meta: &syn::meta::ParseNestedMeta<'_>) -> syn::Result<SpannedValue<u32>> {
    let literal: LitInt = meta.value()?.parse()?;
    if !literal.suffix().is_empty() {
        return Err(Error::new(
            literal.span(),
            "alignment must be an unsuffixed integer literal",
        ));
    }
    let value = parse_unsigned(&literal, u32::MAX.into(), "alignment")? as u32;
    if value == 0 || !value.is_power_of_two() || value > (1 << 29) {
        return Err(Error::new(
            literal.span(),
            "alignment must be a power of two no greater than 2^29",
        ));
    }
    Ok(SpannedValue {
        value,
        span: literal.span(),
    })
}

fn parse_capacity(meta: &syn::meta::ParseNestedMeta<'_>) -> syn::Result<SpannedValue<usize>> {
    let literal: LitInt = meta.value()?.parse()?;
    if !literal.suffix().is_empty() {
        return Err(Error::new(
            literal.span(),
            "capacity must be an unsuffixed integer literal",
        ));
    }
    let value = parse_unsigned(&literal, usize::MAX as u128, "capacity")? as usize;
    Ok(SpannedValue {
        value,
        span: literal.span(),
    })
}

pub(crate) fn parse_unsigned(literal: &LitInt, max: u128, what: &str) -> syn::Result<u128> {
    let text = literal.to_string().replace('_', "");
    let (radix, digits) = if let Some(digits) = text.strip_prefix("0x") {
        (16, digits)
    } else if let Some(digits) = text.strip_prefix("0o") {
        (8, digits)
    } else if let Some(digits) = text.strip_prefix("0b") {
        (2, digits)
    } else {
        (10, text.as_str())
    };
    let value = u128::from_str_radix(digits, radix)
        .map_err(|_| Error::new(literal.span(), format!("{what} is out of range")))?;
    if value > max {
        return Err(Error::new(
            literal.span(),
            format!("{what} is out of range"),
        ));
    }
    Ok(value)
}

fn parse_container_meta(
    meta: syn::meta::ParseNestedMeta<'_>,
    options: &mut ContainerOptions,
    seen: &mut BTreeMap<String, Span>,
) -> syn::Result<()> {
    let option = key(&meta)?;
    duplicate(seen, &option, meta.path.span())?;
    match option.as_str() {
        "endian" => {
            let parsed = parse_endian(&meta)?;
            options.endian = parsed.value;
            options.endian_span = Some(parsed.span);
        }
        "align" => options.align = Some(parse_alignment(&meta)?),
        "crate" => {
            let value: Path = meta.value()?.parse()?;
            options.runtime = Some(SpannedValue {
                span: value.span(),
                value,
            });
        }
        "borrow" => {
            let value: Lifetime = meta.value()?.parse()?;
            options.borrow = Some(SpannedValue {
                span: value.span(),
                value,
            });
        }
        "tag" => {
            return Err(Error::new(
                meta.path.span(),
                "tag is only valid on tagged-enum variants",
            ));
        }
        _ => return Err(meta.error(format!("unknown container zero option `{option}`"))),
    }
    Ok(())
}

fn parse_field_meta(
    meta: syn::meta::ParseNestedMeta<'_>,
    options: &mut FieldOptions,
    seen: &mut BTreeMap<String, Span>,
) -> syn::Result<()> {
    let option = key(&meta)?;
    duplicate(seen, &option, meta.path.span())?;
    match option.as_str() {
        "capacity" => options.capacity = Some(parse_capacity(&meta)?),
        "len_type" => {
            let value: Ident = meta.value()?.parse()?;
            if !matches!(value.to_string().as_str(), "u8" | "u16" | "u32") {
                return Err(Error::new(value.span(), "len_type must be u8, u16, or u32"));
            }
            options.len_type = Some(SpannedValue {
                span: value.span(),
                value,
            });
        }
        "endian" => options.endian = Some(parse_endian(&meta)?),
        "align" => options.align = Some(parse_alignment(&meta)?),
        "tag_field" => {
            let value: Ident = meta.value()?.parse()?;
            options.tag_field = Some(SpannedValue {
                span: value.span(),
                value,
            });
        }
        "crate" | "borrow" | "tag" => {
            return Err(Error::new(
                meta.path.span(),
                format!("`{option}` is not a field zero option"),
            ));
        }
        _ => return Err(meta.error(format!("unknown field zero option `{option}`"))),
    }
    Ok(())
}

fn parse_variant_meta(
    meta: syn::meta::ParseNestedMeta<'_>,
    option: &mut Option<VariantOptions>,
) -> syn::Result<()> {
    let key = key(&meta)?;
    if key != "tag" {
        return Err(meta.error(format!("`{key}` is not a tagged-variant zero option")));
    }
    if option.is_some() {
        return Err(Error::new(meta.path.span(), "duplicate zero option `tag`"));
    }
    let value: Path = meta.value()?.parse()?;
    *option = Some(VariantOptions {
        tag: SpannedValue {
            span: value.span(),
            value,
        },
    });
    Ok(())
}

fn parse_attrs(
    attrs: &[Attribute],
    mut parse: impl FnMut(syn::meta::ParseNestedMeta<'_>) -> syn::Result<()>,
) -> Result<(), Error> {
    let mut errors = None;
    for attr in attrs
        .iter()
        .filter(|attribute| attribute.path().is_ident("zero"))
    {
        let result = attr.parse_nested_meta(&mut parse);
        if let Err(error) = result {
            combine(&mut errors, error);
        }
    }
    errors.map_or(Ok(()), Err)
}

pub(crate) fn container_from_tokens_and_attrs(
    tokens: TokenStream,
    attrs: &[Attribute],
) -> syn::Result<ContainerOptions> {
    let mut options = ContainerOptions::default();
    let mut seen = BTreeMap::new();
    let mut errors = None;
    if let Err(error) =
        syn::meta::parser(|meta| parse_container_meta(meta, &mut options, &mut seen)).parse2(tokens)
    {
        combine(&mut errors, error);
    }
    for attr in attrs
        .iter()
        .filter(|attribute| attribute.path().is_ident("zero"))
    {
        if let Err(error) =
            attr.parse_nested_meta(|meta| parse_container_meta(meta, &mut options, &mut seen))
        {
            combine(&mut errors, error);
        }
    }
    errors.map_or(Ok(options), Err)
}

pub(crate) fn field_options(attrs: &[Attribute]) -> syn::Result<FieldOptions> {
    let mut options = FieldOptions::default();
    let mut seen = BTreeMap::new();
    parse_attrs(attrs, |meta| {
        parse_field_meta(meta, &mut options, &mut seen)
    })?;
    Ok(options)
}

pub(crate) fn variant_options(attrs: &[Attribute]) -> syn::Result<Option<VariantOptions>> {
    let mut option = None;
    parse_attrs(attrs, |meta| parse_variant_meta(meta, &mut option))?;
    Ok(option)
}

pub(crate) fn is_zero(attribute: &Attribute) -> bool {
    attribute.path().is_ident("zero")
}

#[cfg(test)]
mod tests {
    use super::*;
    use quote::quote;

    #[test]
    fn parses_only_the_declared_container_grammar() {
        let parsed = container_from_tokens_and_attrs(
            quote!(endian = "little", align = 8, crate = renamed, borrow = 'a),
            &[],
        )
        .unwrap();
        assert_eq!(parsed.endian, Endian::Little);
        assert_eq!(parsed.align.unwrap().value, 8);
        assert_eq!(parsed.borrow.unwrap().value.ident, "a");
        let unknown = match container_from_tokens_and_attrs(quote!(unknown = "value"), &[]) {
            Ok(_) => panic!("unknown option unexpectedly parsed"),
            Err(error) => error,
        };
        assert!(unknown.to_string().contains("unknown container"));
    }

    #[test]
    fn parses_field_and_variant_options_with_spans() {
        let field: syn::Field =
            syn::parse_quote!(#[zero(capacity = 4, len_type = u16)] value: &'a str);
        let parsed = field_options(&field.attrs).unwrap();
        assert_eq!(parsed.capacity.unwrap().value, 4);
        assert_eq!(parsed.len_type.unwrap().value, "u16");
        let variant: syn::Variant = syn::parse_quote!(
            #[zero(tag = Kind::A)]
            A
        );
        assert_eq!(
            variant_options(&variant.attrs)
                .unwrap()
                .unwrap()
                .tag
                .value
                .segments
                .len(),
            2
        );
    }
}
