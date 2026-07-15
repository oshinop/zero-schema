use std::collections::{BTreeMap, BTreeSet};

use proc_macro2::{Span, TokenStream};
use quote::{ToTokens, format_ident};
use syn::{
    Attribute, Error, Expr, ExprLit, Fields, GenericArgument, GenericParam, Generics, Ident, Item,
    ItemEnum, ItemStruct, Lifetime, Lit, Path, PathArguments, Type, TypeArray, TypePath,
    TypeReference, Visibility, spanned::Spanned as _, visit::Visit, visit_mut::VisitMut,
};

use crate::parse::{self, ContainerOptions, Endian, FieldOptions, SpannedValue};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ItemKind {
    Struct,
    ScalarEnum { repr: ScalarRepr },
    TaggedEnum,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ScalarRepr {
    U8,
    U16,
    U32,
}

impl ScalarRepr {
    pub(crate) fn maximum(self) -> u64 {
        match self {
            Self::U8 => u8::MAX.into(),
            Self::U16 => u16::MAX.into(),
            Self::U32 => u32::MAX.into(),
        }
    }

    pub(crate) fn wire_ident(self, endian: Endian) -> Ident {
        let name = match (self, endian) {
            (Self::U8, _) => "U8",
            (Self::U16, Endian::Native) => "NativeU16",
            (Self::U16, Endian::Little) => "LittleU16",
            (Self::U16, Endian::Big) => "BigU16",
            (Self::U32, Endian::Native) => "NativeU32",
            (Self::U32, Endian::Little) => "LittleU32",
            (Self::U32, Endian::Big) => "BigU32",
        };
        Ident::new(name, Span::call_site())
    }

    pub(crate) fn runtime_variant(self) -> Ident {
        let name = match self {
            Self::U8 => "U8",
            Self::U16 => "U16",
            Self::U32 => "U32",
        };
        Ident::new(name, Span::call_site())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Primitive {
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

impl Primitive {
    pub(crate) fn wire_ident(self, endian: Endian) -> Ident {
        let name = match (self, endian) {
            (Self::U8, _) => "U8",
            (Self::I8, _) => "I8",
            (Self::U16, Endian::Native) => "NativeU16",
            (Self::U16, Endian::Little) => "LittleU16",
            (Self::U16, Endian::Big) => "BigU16",
            (Self::I16, Endian::Native) => "NativeI16",
            (Self::I16, Endian::Little) => "LittleI16",
            (Self::I16, Endian::Big) => "BigI16",
            (Self::U32, Endian::Native) => "NativeU32",
            (Self::U32, Endian::Little) => "LittleU32",
            (Self::U32, Endian::Big) => "BigU32",
            (Self::I32, Endian::Native) => "NativeI32",
            (Self::I32, Endian::Little) => "LittleI32",
            (Self::I32, Endian::Big) => "BigI32",
            (Self::U64, Endian::Native) => "NativeU64",
            (Self::U64, Endian::Little) => "LittleU64",
            (Self::U64, Endian::Big) => "BigU64",
            (Self::I64, Endian::Native) => "NativeI64",
            (Self::I64, Endian::Little) => "LittleI64",
            (Self::I64, Endian::Big) => "BigI64",
            (Self::F32, Endian::Native) => "NativeF32",
            (Self::F32, Endian::Little) => "LittleF32",
            (Self::F32, Endian::Big) => "BigF32",
            (Self::F64, Endian::Native) => "NativeF64",
            (Self::F64, Endian::Little) => "LittleF64",
            (Self::F64, Endian::Big) => "BigF64",
        };
        Ident::new(name, Span::call_site())
    }

    pub(crate) fn runtime_variant(self) -> Ident {
        let name = match self {
            Self::U8 => "U8",
            Self::I8 => "I8",
            Self::U16 => "U16",
            Self::I16 => "I16",
            Self::U32 => "U32",
            Self::I32 => "I32",
            Self::U64 => "U64",
            Self::I64 => "I64",
            Self::F32 => "F32",
            Self::F64 => "F64",
        };
        Ident::new(name, Span::call_site())
    }
}

#[derive(Clone)]
pub(crate) enum ArrayLength {
    Literal(usize),
    Symbolic(Expr),
}

impl ArrayLength {
    pub(crate) fn expression(&self) -> TokenStream {
        match self {
            Self::Literal(value) => quote::quote!(#value),
            Self::Symbolic(expression) => expression.to_token_stream(),
        }
    }
}

#[derive(Clone)]
pub(crate) enum FieldCategory {
    Primitive(Primitive),
    Bool,
    BorrowedStr {
        capacity: usize,
        len_type: Ident,
        endian: Endian,
        lifetime: Lifetime,
    },
    BorrowedCStr {
        capacity: usize,
        lifetime: Lifetime,
    },
    BorrowedU16Str {
        capacity: usize,
        len_type: Ident,
        endian: Endian,
        lifetime: Lifetime,
    },
    BorrowedU16CStr {
        capacity: usize,
        lifetime: Lifetime,
    },
    FixedBytes {
        length: ArrayLength,
        lifetime: Lifetime,
    },
    Path {
        tagged: bool,
        tag_field: Option<usize>,
    },
    Array {
        element: Box<FieldCategory>,
        length: ArrayLength,
    },
    Optional {
        inner_ty: Box<Type>,
        inner_support_ty: Box<Type>,
        inner: Box<FieldCategory>,
    },
}

impl FieldCategory {
    fn direct_lifetime(&self) -> Option<&Lifetime> {
        match self {
            Self::BorrowedStr { lifetime, .. }
            | Self::BorrowedCStr { lifetime, .. }
            | Self::BorrowedU16Str { lifetime, .. }
            | Self::BorrowedU16CStr { lifetime, .. }
            | Self::FixedBytes { lifetime, .. } => Some(lifetime),
            _ => None,
        }
    }
}

#[derive(Clone)]
pub(crate) struct FieldIr {
    pub(crate) ident: Ident,
    pub(crate) logical_name: String,
    pub(crate) declaration_index: usize,
    pub(crate) ty: Type,
    pub(crate) support_ty: Type,
    pub(crate) options: FieldOptions,
    pub(crate) category: FieldCategory,
    pub(crate) wire_endian: Endian,
    pub(crate) span: Span,
}

#[derive(Clone)]
pub(crate) enum VariantShape {
    Unit,
    Newtype(Box<Type>),
}

#[derive(Clone)]
pub(crate) struct VariantIr {
    pub(crate) ident: Ident,
    pub(crate) logical_name: String,
    pub(crate) shape: VariantShape,
    pub(crate) tag: Path,
    pub(crate) tag_type: Path,
    pub(crate) raw_discriminant: Option<u64>,
    pub(crate) span: Span,
}
type AnalyzedItem = (
    ItemKind,
    Ident,
    Visibility,
    Generics,
    Vec<FieldIr>,
    Vec<VariantIr>,
);

#[derive(Clone)]
pub(crate) struct GeneratedNames {
    pub(crate) support_module: Ident,
    pub(crate) wire: Ident,
    pub(crate) reference: Ident,
    pub(crate) mutable: Ident,
    pub(crate) patch: Ident,
    pub(crate) access_error: Ident,
    pub(crate) mutation_error: Ident,
}

#[derive(Clone)]
pub(crate) struct GenericModel {
    pub(crate) original: Generics,
}

#[derive(Clone)]
pub(crate) struct SchemaIr {
    pub(crate) item: Item,
    pub(crate) kind: ItemKind,
    pub(crate) ident: Ident,
    pub(crate) logical_name: String,
    pub(crate) visibility: Visibility,
    pub(crate) options: ContainerOptions,
    pub(crate) generics: GenericModel,
    pub(crate) fields: Vec<FieldIr>,
    pub(crate) variants: Vec<VariantIr>,
    pub(crate) names: GeneratedNames,
}

pub(crate) fn analyze(args: TokenStream, mut item: Item) -> syn::Result<SchemaIr> {
    let attrs = item_attrs(&item);
    let options = parse::container_from_tokens_and_attrs(args, attrs)?;
    reject_misplaced_zero_attributes(&item)?;
    let (kind, ident, visibility, original_generics, fields, variants) = match &item {
        Item::Struct(struct_item) => analyze_struct(struct_item, &options)?,
        Item::Enum(enum_item) => analyze_enum(enum_item, &options)?,
        Item::Union(union_item) => {
            return Err(Error::new(
                union_item.union_token.span,
                "#[zero] does not support unions",
            ));
        }
        unsupported => {
            return Err(Error::new(
                unsupported.span(),
                "#[zero] supports only module-scope named structs and enums",
            ));
        }
    };
    let generic_model = generic_model(&original_generics, &fields, options.borrow.as_ref())?;
    let logical_name = normalized_ident(&ident);
    let names = generated_names(&logical_name, &kind, &options, &fields, &variants);
    validate_generated_names(&kind, &fields, &variants)?;
    strip_zero_attributes(&mut item);
    Ok(SchemaIr {
        item,
        kind,
        ident,
        logical_name,
        visibility,
        options,
        generics: generic_model,
        fields,
        variants,
        names,
    })
}

fn item_attrs(item: &Item) -> &[Attribute] {
    match item {
        Item::Struct(item) => &item.attrs,
        Item::Enum(item) => &item.attrs,
        Item::Union(item) => &item.attrs,
        Item::Const(item) => &item.attrs,
        Item::Fn(item) => &item.attrs,
        Item::Mod(item) => &item.attrs,
        Item::Static(item) => &item.attrs,
        Item::Trait(item) => &item.attrs,
        Item::TraitAlias(item) => &item.attrs,
        Item::Type(item) => &item.attrs,
        Item::Use(item) => &item.attrs,
        Item::Impl(item) => &item.attrs,
        Item::ExternCrate(item) => &item.attrs,
        Item::ForeignMod(item) => &item.attrs,
        Item::Macro(item) => &item.attrs,
        Item::Verbatim(_) => &[],
        _ => &[],
    }
}

fn analyze_struct(item: &ItemStruct, options: &ContainerOptions) -> syn::Result<AnalyzedItem> {
    if !matches!(item.fields, Fields::Named(_)) {
        return Err(Error::new(
            item.fields.span(),
            "#[zero] structs must have named fields",
        ));
    }
    let Fields::Named(named) = &item.fields else {
        unreachable!()
    };
    if named.named.is_empty() {
        return Err(Error::new(
            item.ident.span(),
            "schema structs must contain at least one field",
        ));
    }
    let mut fields = Vec::with_capacity(named.named.len());
    for (index, field) in named.named.iter().enumerate() {
        let Some(ident) = &field.ident else {
            unreachable!()
        };
        let field_options = parse::field_options(&field.attrs)?;
        let wire_endian = field_options
            .endian
            .as_ref()
            .map_or(options.endian, |value| value.value);
        let mut category = classify_field(&field.ty, &field_options, wire_endian)?;
        apply_field_options(
            &mut category,
            &field_options,
            options.endian,
            field.ty.span(),
        )?;
        fields.push(FieldIr {
            ident: ident.clone(),
            logical_name: normalized_ident(ident),
            declaration_index: index,
            ty: field.ty.clone(),
            support_ty: support_type(&field.ty),
            options: field_options,
            category,
            wire_endian,
            span: field.span(),
        });
    }
    link_external_tags(&mut fields)?;
    Ok((
        ItemKind::Struct,
        item.ident.clone(),
        item.vis.clone(),
        item.generics.clone(),
        fields,
        Vec::new(),
    ))
}

fn analyze_enum(item: &ItemEnum, options: &ContainerOptions) -> syn::Result<AnalyzedItem> {
    if item.variants.is_empty() {
        return Err(Error::new(
            item.ident.span(),
            "schema enums must contain at least one variant",
        ));
    }
    let has_variant_tag = item
        .variants
        .iter()
        .any(|variant| variant.attrs.iter().any(parse::is_zero));
    if has_variant_tag {
        if let Some(option) = options.endian_span {
            return Err(Error::new(
                option,
                "endian is not applicable to a tagged payload declaration",
            ));
        }
        if let Some(option) = &options.align {
            return Err(Error::new(
                option.span,
                "align is not applicable to a tagged payload declaration",
            ));
        }
        if let Some(option) = &options.borrow {
            return Err(Error::new(
                option.span,
                "borrow is not applicable to a tagged payload declaration",
            ));
        }
        let variants = analyze_tagged_variants(item)?;
        return Ok((
            ItemKind::TaggedEnum,
            item.ident.clone(),
            item.vis.clone(),
            item.generics.clone(),
            Vec::new(),
            variants,
        ));
    }
    if options.align.is_some() || options.borrow.is_some() {
        let span = options
            .align
            .as_ref()
            .map(|value| value.span)
            .or_else(|| options.borrow.as_ref().map(|value| value.span))
            .unwrap_or(item.ident.span());
        return Err(Error::new(
            span,
            "this option is not applicable to a scalar enum",
        ));
    }
    let repr = scalar_repr(item)?;
    let variants = analyze_scalar_variants(item, repr)?;
    Ok((
        ItemKind::ScalarEnum { repr },
        item.ident.clone(),
        item.vis.clone(),
        item.generics.clone(),
        Vec::new(),
        variants,
    ))
}

fn scalar_repr(item: &ItemEnum) -> syn::Result<ScalarRepr> {
    let mut found = None;
    for attribute in item
        .attrs
        .iter()
        .filter(|attribute| attribute.path().is_ident("repr"))
    {
        attribute.parse_nested_meta(|meta| {
            let Some(ident) = meta.path.get_ident() else {
                return Err(meta.error("scalar enums require repr(u8), repr(u16), or repr(u32)"));
            };
            let repr = match ident.to_string().as_str() {
                "u8" => ScalarRepr::U8,
                "u16" => ScalarRepr::U16,
                "u32" => ScalarRepr::U32,
                _ => {
                    return Err(Error::new(
                        ident.span(),
                        "scalar enums require repr(u8), repr(u16), or repr(u32)",
                    ));
                }
            };
            if found.replace(repr).is_some() {
                return Err(Error::new(
                    ident.span(),
                    "scalar enums require exactly one integer repr",
                ));
            }
            Ok(())
        })?;
    }
    found.ok_or_else(|| {
        Error::new(
            item.ident.span(),
            "fieldless scalar enums require repr(u8), repr(u16), or repr(u32)",
        )
    })
}

fn analyze_scalar_variants(item: &ItemEnum, repr: ScalarRepr) -> syn::Result<Vec<VariantIr>> {
    let mut raw_values = BTreeSet::new();
    let mut variants = Vec::with_capacity(item.variants.len());
    for variant in &item.variants {
        if !matches!(variant.fields, Fields::Unit) {
            return Err(Error::new(
                variant.fields.span(),
                "scalar enum variants must be fieldless",
            ));
        }
        let Some((_, expression)) = &variant.discriminant else {
            return Err(Error::new(
                variant.ident.span(),
                "scalar enum variants require an explicit integer discriminant",
            ));
        };
        let value = discriminant_value(expression, repr)?;
        if !raw_values.insert(value) {
            return Err(Error::new(
                expression.span(),
                "scalar enum discriminants must be unique",
            ));
        }
        variants.push(VariantIr {
            ident: variant.ident.clone(),
            logical_name: normalized_ident(&variant.ident),
            shape: VariantShape::Unit,
            tag: syn::parse_quote!(Self),
            tag_type: syn::parse_quote!(Self),
            raw_discriminant: Some(value),
            span: variant.span(),
        });
    }
    Ok(variants)
}

fn discriminant_value(expression: &Expr, repr: ScalarRepr) -> syn::Result<u64> {
    let Expr::Lit(ExprLit {
        lit: Lit::Int(literal),
        ..
    }) = expression
    else {
        return Err(Error::new(
            expression.span(),
            "scalar enum discriminants must be integer literals",
        ));
    };
    if !literal.suffix().is_empty() {
        return Err(Error::new(
            literal.span(),
            "scalar enum discriminants must be unsuffixed integer literals",
        ));
    }
    let text = literal.to_string().replace('_', "");
    let (radix, digits) = if let Some(value) = text.strip_prefix("0x") {
        (16, value)
    } else if let Some(value) = text.strip_prefix("0o") {
        (8, value)
    } else if let Some(value) = text.strip_prefix("0b") {
        (2, value)
    } else {
        (10, text.as_str())
    };
    let value = u64::from_str_radix(digits, radix)
        .map_err(|_| Error::new(literal.span(), "scalar enum discriminant is out of range"))?;
    if value > repr.maximum() {
        return Err(Error::new(
            literal.span(),
            "scalar enum discriminant does not fit its repr",
        ));
    }
    Ok(value)
}

fn analyze_tagged_variants(item: &ItemEnum) -> syn::Result<Vec<VariantIr>> {
    let mut variants = Vec::with_capacity(item.variants.len());
    let mut tag_type: Option<Path> = None;
    let mut tag_values = BTreeSet::new();
    for variant in &item.variants {
        let options = parse::variant_options(&variant.attrs)?;
        let Some(options) = options else {
            return Err(Error::new(
                variant.ident.span(),
                "tagged enum variants require #[zero(tag = ScalarEnum::Variant)]",
            ));
        };
        let tag_key = options.tag.value.to_token_stream().to_string();
        if !tag_values.insert(tag_key) {
            return Err(Error::new(
                options.tag.span,
                "tagged enum variant tags must be unique",
            ));
        }
        let current_tag_type = tag_owner_path(&options.tag.value)?;
        if let Some(previous) = &tag_type {
            if previous.to_token_stream().to_string()
                != current_tag_type.to_token_stream().to_string()
            {
                return Err(Error::new(
                    options.tag.span,
                    "all tagged enum variants must use values from the same scalar enum",
                ));
            }
        } else {
            tag_type = Some(current_tag_type.clone());
        }
        let shape = match &variant.fields {
            Fields::Unit => VariantShape::Unit,
            Fields::Unnamed(fields) if fields.unnamed.len() == 1 => {
                if path_ends_in_option(&fields.unnamed.first().unwrap().ty) {
                    return Err(Error::new(
                        fields.unnamed.first().unwrap().ty.span(),
                        "Option<T> cannot be used as a tagged payload; an externally tagged payload has no standalone optional sentinel",
                    ));
                }
                VariantShape::Newtype(Box::new(fields.unnamed.first().unwrap().ty.clone()))
            }
            Fields::Unnamed(fields) => {
                return Err(Error::new(
                    fields.span(),
                    "tagged enum variants must be unit or single-field newtype variants",
                ));
            }
            Fields::Named(fields) => {
                return Err(Error::new(
                    fields.span(),
                    "tagged enum variants must be unit or single-field newtype variants",
                ));
            }
        };
        variants.push(VariantIr {
            ident: variant.ident.clone(),
            logical_name: normalized_ident(&variant.ident),
            shape,
            tag: rebase_path(&options.tag.value),
            tag_type: rebase_path(&current_tag_type),
            raw_discriminant: None,
            span: variant.span(),
        });
    }
    Ok(variants)
}

fn tag_owner_path(tag: &Path) -> syn::Result<Path> {
    if tag.segments.len() < 2 {
        return Err(Error::new(
            tag.span(),
            "tag must name a scalar-enum variant such as Kind::Value",
        ));
    }
    let mut result = tag.clone();
    let _ = result.segments.pop();
    if result.segments.trailing_punct() {
        let _ = result.segments.pop_punct();
    }
    Ok(result)
}

fn classify_field(
    ty: &Type,
    options: &FieldOptions,
    _default_endian: Endian,
) -> syn::Result<FieldCategory> {
    if let Some(primitive) = primitive(ty) {
        return Ok(FieldCategory::Primitive(primitive));
    }
    if bool_type(ty) {
        return Ok(FieldCategory::Bool);
    }
    if let Some((lifetime, target)) = borrowed_target(ty) {
        if is_single_ident(target, "str") {
            return Ok(FieldCategory::BorrowedStr {
                capacity: required_capacity(options, ty.span())?,
                len_type: length_type(options),
                endian: Endian::Native,
                lifetime,
            });
        }
        if is_single_ident(target, "CStr") {
            return Ok(FieldCategory::BorrowedCStr {
                capacity: required_capacity(options, ty.span())?,
                lifetime,
            });
        }
        if is_single_ident(target, "U16Str") {
            return Ok(FieldCategory::BorrowedU16Str {
                capacity: required_capacity(options, ty.span())?,
                len_type: length_type(options),
                endian: Endian::Native,
                lifetime,
            });
        }
        if is_single_ident(target, "U16CStr") {
            return Ok(FieldCategory::BorrowedU16CStr {
                capacity: required_capacity(options, ty.span())?,
                lifetime,
            });
        }
        if let Type::Array(array) = target {
            if is_single_ident(&array.elem, "u8") {
                return Ok(FieldCategory::FixedBytes {
                    length: array_length(array)?,
                    lifetime,
                });
            }
        }
        return Err(Error::new(
            target.span(),
            "unsupported borrowed field; use &str, &CStr, &U16Str, &U16CStr, or &[u8; N]",
        ));
    }
    if let Some(result) = classify_option_field(ty)? {
        return Ok(result);
    }
    if let Type::Array(array) = ty {
        let length = array_length(array)?;
        let element = classify_array_element(&array.elem)?;
        return Ok(FieldCategory::Array {
            element: Box::new(element),
            length,
        });
    }
    if matches!(ty, Type::Path(_)) {
        return Ok(FieldCategory::Path {
            tagged: options.tag_field.is_some(),
            tag_field: None,
        });
    }
    Err(Error::new(ty.span(), "unsupported schema field type"))
}

/// Recognizes only the three public Option spellings. A bare path ending in
/// `Option` is diagnosed so aliases and lookalikes never gain sentinel
/// semantics accidentally.
fn classify_option_field(ty: &Type) -> syn::Result<Option<FieldCategory>> {
    let Type::Path(TypePath { qself: None, path }) = ty else {
        return Ok(None);
    };
    let Some(last) = path.segments.last() else {
        return Ok(None);
    };
    if last.ident != "Option" {
        return Ok(None);
    }
    if !is_canonical_option_path(path) {
        return Err(Error::new(
            last.ident.span(),
            "Option must be spelled Option<T>, core::option::Option<T>, or std::option::Option<T>",
        ));
    }
    let PathArguments::AngleBracketed(arguments) = &last.arguments else {
        return Err(Error::new(
            last.span(),
            "zero-sentinel Option fields require exactly one type argument: Option<T>",
        ));
    };
    if arguments.args.len() != 1 {
        return Err(Error::new(
            arguments.span(),
            "zero-sentinel Option fields require exactly one type argument: Option<T>",
        ));
    }
    let Some(GenericArgument::Type(inner_ty)) = arguments.args.first() else {
        return Err(Error::new(
            arguments.span(),
            "zero-sentinel Option fields require exactly one type argument: Option<T>",
        ));
    };
    let inner = classify_optional_inner(inner_ty)?;
    Ok(Some(FieldCategory::Optional {
        inner_ty: Box::new(inner_ty.clone()),
        inner_support_ty: Box::new(support_type(inner_ty)),
        inner: Box::new(inner),
    }))
}

fn is_canonical_option_path(path: &Path) -> bool {
    let names = path
        .segments
        .iter()
        .map(|segment| segment.ident.to_string())
        .collect::<Vec<_>>();
    matches!(names.as_slice(), [option] if option == "Option")
        || matches!(names.as_slice(), [root, option, value] if (root == "core" || root == "std") && option == "option" && value == "Option")
}

fn path_ends_in_option(ty: &Type) -> bool {
    matches!(ty, Type::Path(TypePath { qself: None, path }) if path.segments.last().is_some_and(|segment| segment.ident == "Option"))
}

fn classify_optional_inner(inner_ty: &Type) -> syn::Result<FieldCategory> {
    if path_ends_in_option(inner_ty) {
        return Err(Error::new(
            inner_ty.span(),
            "nested Option<Option<T>> is unsupported; the all-zero sentinel represents one absence state",
        ));
    }
    if primitive(inner_ty).is_some() || bool_type(inner_ty) {
        return Err(Error::new(
            inner_ty.span(),
            "Option<T> cannot use a primitive or bool T because the all-zero wire is valid",
        ));
    }
    if matches!(inner_ty, Type::Reference(_)) {
        return Err(Error::new(
            inner_ty.span(),
            "Option<T> cannot use string or fixed-byte T because the all-zero wire can be valid",
        ));
    }
    if let Type::Array(array) = inner_ty {
        let length = array_length(array)?;
        if path_ends_in_option(&array.elem) {
            return Err(Error::new(
                array.elem.span(),
                "Option<T> is not supported as a fixed-array element; put Option around the complete array",
            ));
        }
        if primitive(&array.elem).is_some() || bool_type(&array.elem) {
            return Err(Error::new(
                array.elem.span(),
                "Option<[T; N]> requires an all-zero-invalid scalar-enum or schema element",
            ));
        }
        if matches!(&*array.elem, Type::Path(TypePath { qself: None, .. })) {
            return Ok(FieldCategory::Array {
                element: Box::new(FieldCategory::Path {
                    tagged: false,
                    tag_field: None,
                }),
                length,
            });
        }
        return Err(Error::new(
            array.elem.span(),
            "Option<[T; N]> requires an all-zero-invalid scalar-enum or schema element",
        ));
    }
    if matches!(inner_ty, Type::Path(TypePath { qself: None, .. })) {
        return Ok(FieldCategory::Path {
            tagged: false,
            tag_field: None,
        });
    }
    Err(Error::new(
        inner_ty.span(),
        "Option<T> cannot use string or fixed-byte T because the all-zero wire can be valid",
    ))
}

fn apply_field_options(
    category: &mut FieldCategory,
    options: &FieldOptions,
    default_endian: Endian,
    span: Span,
) -> syn::Result<()> {
    let field_endian = options
        .endian
        .as_ref()
        .map_or(default_endian, |value| value.value);
    match category {
        FieldCategory::Primitive(_) => {
            if options.capacity.is_some()
                || options.len_type.is_some()
                || options.tag_field.is_some()
            {
                return Err(Error::new(
                    span,
                    "this zero option is not applicable to a primitive field",
                ));
            }
        }
        FieldCategory::Bool | FieldCategory::FixedBytes { .. } | FieldCategory::Array { .. } => {
            if options.capacity.is_some()
                || options.len_type.is_some()
                || options.endian.is_some()
                || options.tag_field.is_some()
            {
                return Err(Error::new(
                    span,
                    "this zero option is not applicable to this field",
                ));
            }
        }
        FieldCategory::BorrowedStr { endian, .. }
        | FieldCategory::BorrowedU16Str { endian, .. } => {
            *endian = field_endian;
            if options.tag_field.is_some() {
                return Err(Error::new(
                    span,
                    "tag_field is only applicable to tagged payload fields",
                ));
            }
        }
        FieldCategory::BorrowedCStr { capacity, .. }
        | FieldCategory::BorrowedU16CStr { capacity, .. } => {
            if options.len_type.is_some() || options.endian.is_some() || options.tag_field.is_some()
            {
                return Err(Error::new(
                    span,
                    "this zero option is not applicable to this string field",
                ));
            }
            if *capacity == 0 {
                return Err(Error::new(
                    span,
                    "nul-terminated string capacity must be nonzero",
                ));
            }
        }
        FieldCategory::Path { tagged, .. } => {
            if options.capacity.is_some() || options.len_type.is_some() || options.endian.is_some()
            {
                return Err(Error::new(
                    span,
                    "this zero option is not applicable to a schema field",
                ));
            }
            *tagged = options.tag_field.is_some();
        }
        FieldCategory::Optional { .. } => {
            if let Some(option) = &options.tag_field {
                return Err(Error::new(
                    option.span,
                    "tag_field cannot be applied to Option<T>; an externally tagged payload has no standalone optional sentinel",
                ));
            }
            if let Some(option) = options
                .capacity
                .as_ref()
                .map(|option| option.span)
                .or_else(|| options.len_type.as_ref().map(|option| option.span))
                .or_else(|| options.endian.as_ref().map(|option| option.span))
            {
                return Err(Error::new(
                    option,
                    "align is the only zero field option accepted by Option<T>",
                ));
            }
        }
    }
    Ok(())
}

fn classify_array_element(ty: &Type) -> syn::Result<FieldCategory> {
    if path_ends_in_option(ty) {
        return Err(Error::new(
            ty.span(),
            "Option<T> is not supported as a fixed-array element; put Option around the complete array",
        ));
    }
    if let Some(primitive) = primitive(ty) {
        return Ok(FieldCategory::Primitive(primitive));
    }
    if bool_type(ty) {
        return Ok(FieldCategory::Bool);
    }
    if matches!(ty, Type::Path(_)) {
        return Ok(FieldCategory::Path {
            tagged: false,
            tag_field: None,
        });
    }
    Err(Error::new(
        ty.span(),
        "array elements must be primitives, bool, scalar enums, or finite nested schemas",
    ))
}

fn primitive(ty: &Type) -> Option<Primitive> {
    let Type::Path(TypePath { qself: None, path }) = ty else {
        return None;
    };
    let mut segments = path.segments.iter();
    let primitive_ident = match (
        segments.next(),
        segments.next(),
        segments.next(),
        segments.next(),
    ) {
        (Some(name), None, None, None) if matches!(name.arguments, PathArguments::None) => {
            &name.ident
        }
        (Some(root), Some(primitive), Some(name), None)
            if (root.ident == "core" || root.ident == "std")
                && primitive.ident == "primitive"
                && matches!(root.arguments, PathArguments::None)
                && matches!(primitive.arguments, PathArguments::None)
                && matches!(name.arguments, PathArguments::None) =>
        {
            &name.ident
        }
        _ => return None,
    };
    match primitive_ident.to_string().as_str() {
        "u8" => Some(Primitive::U8),
        "i8" => Some(Primitive::I8),
        "u16" => Some(Primitive::U16),
        "i16" => Some(Primitive::I16),
        "u32" => Some(Primitive::U32),
        "i32" => Some(Primitive::I32),
        "u64" => Some(Primitive::U64),
        "i64" => Some(Primitive::I64),
        "f32" => Some(Primitive::F32),
        "f64" => Some(Primitive::F64),
        _ => None,
    }
}

fn bool_type(ty: &Type) -> bool {
    let Type::Path(TypePath { qself: None, path }) = ty else {
        return false;
    };
    let mut segments = path.segments.iter();
    matches!(
        (segments.next(), segments.next(), segments.next(), segments.next()),
        (Some(name), None, None, None)
            if name.ident == "bool" && matches!(name.arguments, PathArguments::None)
    ) || matches!(
        (path.segments.first(), path.segments.iter().nth(1), path.segments.iter().nth(2), path.segments.iter().nth(3)),
        (Some(root), Some(primitive), Some(name), None)
            if (root.ident == "core" || root.ident == "std")
                && primitive.ident == "primitive"
                && name.ident == "bool"
                && matches!(root.arguments, PathArguments::None)
                && matches!(primitive.arguments, PathArguments::None)
                && matches!(name.arguments, PathArguments::None)
    )
}

fn is_single_ident(ty: &Type, expected: &str) -> bool {
    let Type::Path(TypePath { qself: None, path }) = ty else {
        return false;
    };
    path.is_ident(expected)
}

fn borrowed_target(ty: &Type) -> Option<(Lifetime, &Type)> {
    let Type::Reference(TypeReference {
        lifetime: Some(lifetime),
        mutability: None,
        elem,
        ..
    }) = ty
    else {
        return None;
    };
    Some((lifetime.clone(), elem))
}

fn required_capacity(options: &FieldOptions, span: Span) -> syn::Result<usize> {
    options
        .capacity
        .as_ref()
        .map(|value| value.value)
        .ok_or_else(|| Error::new(span, "borrowed string fields require #[zero(capacity = N)]"))
}

fn length_type(options: &FieldOptions) -> Ident {
    options
        .len_type
        .as_ref()
        .map(|value| value.value.clone())
        .unwrap_or_else(|| Ident::new("u32", Span::call_site()))
}

fn array_length(array: &TypeArray) -> syn::Result<ArrayLength> {
    match &array.len {
        Expr::Lit(ExprLit {
            lit: Lit::Int(literal),
            ..
        }) => {
            if !literal.suffix().is_empty() {
                return Err(Error::new(
                    literal.span(),
                    "array length must be an unsuffixed integer literal",
                ));
            }
            let value =
                parse::parse_unsigned(literal, usize::MAX as u128, "array length")? as usize;
            if value == 0 {
                return Err(Error::new(
                    literal.span(),
                    "zero-length schema arrays are unsupported",
                ));
            }
            Ok(ArrayLength::Literal(value))
        }
        Expr::Path(path) if path.qself.is_none() && simple_const_path(&path.path) => {
            Ok(ArrayLength::Symbolic(array.len.clone()))
        }
        expression => Err(Error::new(
            expression.span(),
            "array length must be a nonzero integer literal, direct const parameter, or simple const path",
        )),
    }
}

fn simple_const_path(path: &Path) -> bool {
    path.segments
        .iter()
        .all(|segment| matches!(segment.arguments, PathArguments::None))
}

fn link_external_tags(fields: &mut [FieldIr]) -> syn::Result<()> {
    let lookup: BTreeMap<_, _> = fields
        .iter()
        .enumerate()
        .map(|(index, field)| (field.logical_name.clone(), index))
        .collect();
    let mut claimed = BTreeMap::<usize, Span>::new();
    for field_index in 0..fields.len() {
        let tagged = matches!(
            fields[field_index].category,
            FieldCategory::Path { tagged: true, .. }
        );
        if !tagged {
            continue;
        }
        let requested = fields[field_index]
            .options
            .tag_field
            .as_ref()
            .expect("tagged classification requires tag_field");
        let logical = normalized_ident(&requested.value);
        let Some(&tag_index) = lookup.get(&logical) else {
            return Err(Error::new(
                requested.span,
                format!("tag_field `{logical}` does not name a sibling field"),
            ));
        };
        if tag_index == field_index {
            return Err(Error::new(
                requested.span,
                "tag_field cannot name the payload field itself",
            ));
        }
        if !matches!(
            fields[tag_index].category,
            FieldCategory::Path { tagged: false, .. }
        ) {
            return Err(Error::new(
                requested.span,
                "tag_field must name a scalar-enum schema field",
            ));
        }
        if claimed.insert(tag_index, requested.span).is_some() {
            return Err(Error::new(
                requested.span,
                "each external tag field may be used by exactly one tagged payload",
            ));
        }
        let FieldCategory::Path { tag_field, .. } = &mut fields[field_index].category else {
            unreachable!("tagged field classification must be a path");
        };
        *tag_field = Some(tag_index);
    }
    Ok(())
}

fn generic_model(
    generics: &Generics,
    fields: &[FieldIr],
    requested: Option<&SpannedValue<Lifetime>>,
) -> syn::Result<GenericModel> {
    let declared: BTreeSet<_> = generics
        .lifetimes()
        .map(|parameter| parameter.lifetime.ident.to_string())
        .collect();
    let direct: BTreeMap<_, _> = fields
        .iter()
        .filter_map(|field| field.category.direct_lifetime().cloned())
        .map(|lifetime| (lifetime.ident.to_string(), lifetime))
        .collect();
    let _source_lifetime = if let Some(requested) = requested {
        let requested_name = requested.value.ident.to_string();
        if !declared.contains(&requested_name) {
            return Err(Error::new(
                requested.span,
                "borrow must name one of this declaration's lifetime parameters",
            ));
        }
        if !direct.contains_key(&requested_name) {
            return Err(Error::new(
                requested.span,
                "borrow must name a lifetime used by a direct borrowed field",
            ));
        }
        Some(requested.value.clone())
    } else if direct.len() <= 1 {
        direct.into_values().next()
    } else {
        return Err(Error::new(
            fields
                .iter()
                .find_map(|field| field.category.direct_lifetime().map(Lifetime::span))
                .unwrap_or_else(Span::call_site),
            "multiple borrowed source lifetimes require #[zero(borrow = 'lifetime)]",
        ));
    };
    Ok(GenericModel {
        original: generics.clone(),
    })
}

fn generated_names(
    logical_name: &str,
    kind: &ItemKind,
    options: &ContainerOptions,
    fields: &[FieldIr],
    variants: &[VariantIr],
) -> GeneratedNames {
    let suffix = stable_suffix(logical_name, kind, options, fields, variants);
    let module_base = logical_name.to_ascii_lowercase();
    GeneratedNames {
        support_module: format_ident!("__zero_schema_support_{}_{}", module_base, suffix),
        wire: format_ident!("{}Wire", logical_name),
        reference: format_ident!("{}Ref", logical_name),
        mutable: format_ident!("{}Mut", logical_name),
        patch: format_ident!("{}Patch", logical_name),
        access_error: format_ident!("{}AccessError", logical_name),
        mutation_error: format_ident!("{}MutationError", logical_name),
    }
}

fn stable_suffix(
    logical_name: &str,
    kind: &ItemKind,
    options: &ContainerOptions,
    fields: &[FieldIr],
    variants: &[VariantIr],
) -> String {
    let mut feed = format!("{logical_name}|{kind:?}|{:?}|", options.endian);
    if let Some(align) = &options.align {
        feed.push_str(&format!("align={}|", align.value));
    }
    if let Some(runtime) = &options.runtime {
        feed.push_str(&format!("crate={}|", runtime.value.to_token_stream()));
    }
    if let Some(borrow) = &options.borrow {
        feed.push_str(&format!("borrow={}|", borrow.value));
    }
    for field in fields {
        feed.push_str(&format!(
            "field:{}:{}:{:?}:{}|",
            field.logical_name,
            field.ty.to_token_stream(),
            category_key(&field.category),
            field.options.align.as_ref().map_or(0, |align| align.value)
        ));
    }
    for variant in variants {
        feed.push_str(&format!(
            "variant:{}:{}:{}|",
            variant.logical_name,
            variant.tag.to_token_stream(),
            variant.tag_type.to_token_stream()
        ));
    }
    let mut hash = 0xcbf2_9ce4_8422_2325_u64;
    for byte in feed.bytes() {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("{hash:016x}")
}

fn category_key(category: &FieldCategory) -> &'static str {
    match category {
        FieldCategory::Primitive(_) => "primitive",
        FieldCategory::Bool => "bool",
        FieldCategory::BorrowedStr { .. } => "str",
        FieldCategory::BorrowedCStr { .. } => "cstr",
        FieldCategory::BorrowedU16Str { .. } => "u16str",
        FieldCategory::BorrowedU16CStr { .. } => "u16cstr",
        FieldCategory::FixedBytes { .. } => "bytes",
        FieldCategory::Path { tagged: true, .. } => "tagged",
        FieldCategory::Path { .. } => "path",
        FieldCategory::Array { .. } => "array",
        FieldCategory::Optional { .. } => "optional",
    }
}

fn validate_generated_names(
    kind: &ItemKind,
    fields: &[FieldIr],
    variants: &[VariantIr],
) -> syn::Result<()> {
    let mut names = BTreeMap::<String, Span>::new();
    let mut reserve = |name: String, span: Span| -> syn::Result<()> {
        if names.insert(name.clone(), span).is_some() {
            Err(Error::new(
                span,
                format!("generated zero-schema name collision for `{name}`"),
            ))
        } else {
            Ok(())
        }
    };
    match kind {
        ItemKind::Struct => {
            for name in [
                "access",
                "access_mut",
                "SCHEMA_SIZE",
                "SCHEMA_ALIGN",
                "SCHEMA_STRIDE",
                "LAYOUT",
                "copy_into",
                "copy_from",
            ] {
                reserve(name.to_owned(), Span::call_site())?;
            }
            for field in fields {
                reserve(field.logical_name.clone(), field.span)?;
                if !is_tag_sibling(fields, field.declaration_index) {
                    reserve(format!("{}_mut", field.logical_name), field.span)?;
                }
            }
        }
        ItemKind::ScalarEnum { .. } => {
            for name in [
                "access",
                "access_mut",
                "SCHEMA_SIZE",
                "SCHEMA_ALIGN",
                "SCHEMA_STRIDE",
                "LAYOUT",
            ] {
                reserve(name.to_owned(), Span::call_site())?;
            }
            for variant in variants {
                reserve(variant.logical_name.clone(), variant.span)?;
            }
        }
        ItemKind::TaggedEnum => {
            for name in ["tag", "copy_into", "copy_from"] {
                reserve(name.to_owned(), Span::call_site())?;
            }
            for variant in variants {
                let method = snake_case(&variant.logical_name);
                reserve(method.clone(), variant.span)?;
                reserve(format!("{method}_mut"), variant.span)?;
            }
        }
    }
    Ok(())
}

fn is_tag_sibling(fields: &[FieldIr], index: usize) -> bool {
    fields.iter().any(|field| {
        matches!(field.category, FieldCategory::Path { tag_field: Some(tag), .. } if tag == index)
    })
}

fn snake_case(name: &str) -> String {
    let mut result = String::new();
    for (index, character) in name.chars().enumerate() {
        if character.is_uppercase() {
            if index != 0 {
                result.push('_');
            }
            result.extend(character.to_lowercase());
        } else {
            result.push(character);
        }
    }
    result
}

fn normalized_ident(ident: &Ident) -> String {
    ident.to_string().trim_start_matches("r#").to_owned()
}

/// Rebase a field type for generated wire support. Free non-static lifetimes
/// are declaration lifetimes at this point; higher-ranked binders remain in
/// their lexical scope and therefore retain their original lifetimes.
pub(crate) fn support_type(ty: &Type) -> Type {
    let mut result = ty.clone();
    let mut rebase = RebaseSupportPaths::new();
    rebase.visit_type_mut(&mut result);
    result
}

/// Rebases declaration-relative paths for generated support without erasing
/// logical source lifetimes. Patch `From<Logical>` bounds need this exact form.
pub(crate) fn logical_source_type(ty: &Type) -> Type {
    let mut result = ty.clone();
    let mut rebase = RebaseLogicalPaths;
    rebase.visit_type_mut(&mut result);
    result
}

struct RebaseLogicalPaths;

impl VisitMut for RebaseLogicalPaths {
    fn visit_path_mut(&mut self, path: &mut Path) {
        let first = path
            .segments
            .first()
            .map(|segment| (segment.ident.to_string(), segment.ident.span()));
        match first {
            Some((name, span)) if name == "self" => {
                if let Some(segment) = path.segments.first_mut() {
                    segment.ident = Ident::new("super", span);
                }
            }
            Some((name, span)) if name == "super" => {
                path.segments
                    .insert(0, syn::PathSegment::from(Ident::new("super", span)));
            }
            _ => {}
        }
        syn::visit_mut::visit_path_mut(self, path);
    }
}

/// Erases only free declaration lifetimes from a support projection. This is
/// used where the item generics are unavailable; a free lifetime in a field
/// type can only name the enclosing declaration, while HRTB binders are kept.
pub(crate) fn erased_source_type(ty: &Type) -> Type {
    let mut result = ty.clone();
    let mut erase = EraseSourceLifetimes::new();
    erase.visit_type_mut(&mut result);
    result
}

/// Rebinds precisely the lifetime parameters declared by `generics`. This is
/// the logical-side counterpart to source-lifetime erasure: `'static` and
/// higher-ranked lifetime binders are deliberately left unchanged.
pub(crate) fn rebind_declared_source_lifetimes(
    generics: &Generics,
    ty: &Type,
    lifetime: Lifetime,
) -> Type {
    let mut result = ty.clone();
    let mut rebind = DeclaredLifetimeRebinder::new(generics, lifetime);
    rebind.visit_type_mut(&mut result);
    result
}

pub(crate) fn rebind_ir_source_lifetimes(ir: &SchemaIr, ty: &Type, lifetime: Lifetime) -> Type {
    rebind_declared_source_lifetimes(&ir.generics.original, ty, lifetime)
}

/// Builds an implementation generic list for a logical view at `lifetime`.
/// Every declared source lifetime is rebound together, while type and const
/// parameters, explicit `'static`, and HRTB binders retain their source form.
pub(crate) fn logical_view_generics(original: &Generics, lifetime: Lifetime) -> Generics {
    let mut result = original.clone();
    result.params = result
        .params
        .into_iter()
        .filter(|parameter| !matches!(parameter, GenericParam::Lifetime(_)))
        .collect();
    result.params.insert(0, syn::parse_quote!(#lifetime));
    let mut rebind = DeclaredLifetimeRebinder::new(original, lifetime);
    rebind.visit_generics_mut(&mut result);
    result
}

/// Chooses a generated lifetime that cannot shadow any declaration or HRTB
/// binder written by the schema author.
pub(crate) fn fresh_generated_lifetime(ir: &SchemaIr, stem: &str) -> Lifetime {
    struct LifetimeNames(BTreeSet<String>);

    impl<'ast> Visit<'ast> for LifetimeNames {
        fn visit_lifetime(&mut self, lifetime: &'ast Lifetime) {
            self.0.insert(lifetime.ident.to_string());
        }
    }

    let mut names = LifetimeNames(BTreeSet::new());
    names.visit_generics(&ir.generics.original);
    for field in &ir.fields {
        names.visit_type(&field.ty);
    }
    for variant in &ir.variants {
        if let VariantShape::Newtype(ty) = &variant.shape {
            names.visit_type(ty);
        }
    }

    let mut suffix = 0_usize;
    loop {
        let candidate = if suffix == 0 {
            stem.to_owned()
        } else {
            format!("{stem}_{suffix}")
        };
        if !names.0.contains(&candidate) {
            return Lifetime::new(&format!("'{candidate}"), Span::call_site());
        }
        suffix += 1;
    }
}

pub(crate) fn rebase_path(path: &Path) -> Path {
    let mut result = path.clone();
    let mut rebase = RebaseSupportPaths::new();
    rebase.visit_path_mut(&mut result);
    result
}

fn bound_lifetime_names(lifetimes: &syn::BoundLifetimes) -> BTreeSet<String> {
    lifetimes
        .lifetimes
        .iter()
        .filter_map(|parameter| match parameter {
            GenericParam::Lifetime(lifetime) => Some(lifetime.lifetime.ident.to_string()),
            GenericParam::Type(_) | GenericParam::Const(_) => None,
        })
        .collect()
}

struct RebaseSupportPaths {
    bound_lifetimes: Vec<BTreeSet<String>>,
}

impl RebaseSupportPaths {
    fn new() -> Self {
        Self {
            bound_lifetimes: Vec::new(),
        }
    }
}

impl VisitMut for RebaseSupportPaths {
    fn visit_bound_lifetimes_mut(&mut self, lifetimes: &mut syn::BoundLifetimes) {
        syn::visit_mut::visit_bound_lifetimes_mut(self, lifetimes);
    }

    fn visit_trait_bound_mut(&mut self, bound: &mut syn::TraitBound) {
        let names = bound.lifetimes.as_ref().map(bound_lifetime_names);
        if let Some(names) = names {
            self.bound_lifetimes.push(names);
        }
        syn::visit_mut::visit_trait_bound_mut(self, bound);
        if bound.lifetimes.is_some() {
            self.bound_lifetimes.pop();
        }
    }

    fn visit_type_bare_fn_mut(&mut self, bare_fn: &mut syn::TypeBareFn) {
        let names = bare_fn.lifetimes.as_ref().map(bound_lifetime_names);
        if let Some(names) = names {
            self.bound_lifetimes.push(names);
        }
        syn::visit_mut::visit_type_bare_fn_mut(self, bare_fn);
        if bare_fn.lifetimes.is_some() {
            self.bound_lifetimes.pop();
        }
    }

    fn visit_predicate_type_mut(&mut self, predicate: &mut syn::PredicateType) {
        let names = predicate.lifetimes.as_ref().map(bound_lifetime_names);
        if let Some(names) = names {
            self.bound_lifetimes.push(names);
        }
        syn::visit_mut::visit_predicate_type_mut(self, predicate);
        if predicate.lifetimes.is_some() {
            self.bound_lifetimes.pop();
        }
    }

    fn visit_path_mut(&mut self, path: &mut Path) {
        let first = path
            .segments
            .first()
            .map(|segment| (segment.ident.to_string(), segment.ident.span()));
        match first {
            Some((name, span)) if name == "self" => {
                if let Some(segment) = path.segments.first_mut() {
                    segment.ident = Ident::new("super", span);
                }
            }
            Some((name, span)) if name == "super" => {
                path.segments
                    .insert(0, syn::PathSegment::from(Ident::new("super", span)));
            }
            _ => {}
        }
        syn::visit_mut::visit_path_mut(self, path);
    }

    fn visit_lifetime_mut(&mut self, lifetime: &mut Lifetime) {
        let is_bound = self
            .bound_lifetimes
            .iter()
            .any(|bound| bound.contains(&lifetime.ident.to_string()));
        if lifetime.ident != "static" && !is_bound {
            *lifetime = Lifetime::new("'static", lifetime.span());
        }
    }
}

struct EraseSourceLifetimes {
    bound_lifetimes: Vec<BTreeSet<String>>,
}

impl EraseSourceLifetimes {
    fn new() -> Self {
        Self {
            bound_lifetimes: Vec::new(),
        }
    }
}

impl VisitMut for EraseSourceLifetimes {
    fn visit_bound_lifetimes_mut(&mut self, lifetimes: &mut syn::BoundLifetimes) {
        syn::visit_mut::visit_bound_lifetimes_mut(self, lifetimes);
    }

    fn visit_trait_bound_mut(&mut self, bound: &mut syn::TraitBound) {
        let names = bound.lifetimes.as_ref().map(bound_lifetime_names);
        if let Some(names) = names {
            self.bound_lifetimes.push(names);
        }
        syn::visit_mut::visit_trait_bound_mut(self, bound);
        if bound.lifetimes.is_some() {
            self.bound_lifetimes.pop();
        }
    }

    fn visit_type_bare_fn_mut(&mut self, bare_fn: &mut syn::TypeBareFn) {
        let names = bare_fn.lifetimes.as_ref().map(bound_lifetime_names);
        if let Some(names) = names {
            self.bound_lifetimes.push(names);
        }
        syn::visit_mut::visit_type_bare_fn_mut(self, bare_fn);
        if bare_fn.lifetimes.is_some() {
            self.bound_lifetimes.pop();
        }
    }

    fn visit_predicate_type_mut(&mut self, predicate: &mut syn::PredicateType) {
        let names = predicate.lifetimes.as_ref().map(bound_lifetime_names);
        if let Some(names) = names {
            self.bound_lifetimes.push(names);
        }
        syn::visit_mut::visit_predicate_type_mut(self, predicate);
        if predicate.lifetimes.is_some() {
            self.bound_lifetimes.pop();
        }
    }

    fn visit_lifetime_mut(&mut self, lifetime: &mut Lifetime) {
        let is_bound = self
            .bound_lifetimes
            .iter()
            .any(|bound| bound.contains(&lifetime.ident.to_string()));
        if lifetime.ident != "static" && !is_bound {
            *lifetime = Lifetime::new("'static", lifetime.span());
        }
    }
}

struct DeclaredLifetimeRebinder {
    source_lifetimes: BTreeSet<String>,
    bound_lifetimes: Vec<BTreeSet<String>>,
    replacement: Lifetime,
}

impl DeclaredLifetimeRebinder {
    fn new(generics: &Generics, replacement: Lifetime) -> Self {
        Self {
            source_lifetimes: generics
                .lifetimes()
                .map(|parameter| parameter.lifetime.ident.to_string())
                .collect(),
            bound_lifetimes: Vec::new(),
            replacement,
        }
    }
}

impl VisitMut for DeclaredLifetimeRebinder {
    fn visit_bound_lifetimes_mut(&mut self, lifetimes: &mut syn::BoundLifetimes) {
        syn::visit_mut::visit_bound_lifetimes_mut(self, lifetimes);
    }

    fn visit_trait_bound_mut(&mut self, bound: &mut syn::TraitBound) {
        let names = bound.lifetimes.as_ref().map(bound_lifetime_names);
        if let Some(names) = names {
            self.bound_lifetimes.push(names);
        }
        syn::visit_mut::visit_trait_bound_mut(self, bound);
        if bound.lifetimes.is_some() {
            self.bound_lifetimes.pop();
        }
    }

    fn visit_type_bare_fn_mut(&mut self, bare_fn: &mut syn::TypeBareFn) {
        let names = bare_fn.lifetimes.as_ref().map(bound_lifetime_names);
        if let Some(names) = names {
            self.bound_lifetimes.push(names);
        }
        syn::visit_mut::visit_type_bare_fn_mut(self, bare_fn);
        if bare_fn.lifetimes.is_some() {
            self.bound_lifetimes.pop();
        }
    }

    fn visit_predicate_type_mut(&mut self, predicate: &mut syn::PredicateType) {
        let names = predicate.lifetimes.as_ref().map(bound_lifetime_names);
        if let Some(names) = names {
            self.bound_lifetimes.push(names);
        }
        syn::visit_mut::visit_predicate_type_mut(self, predicate);
        if predicate.lifetimes.is_some() {
            self.bound_lifetimes.pop();
        }
    }

    fn visit_lifetime_mut(&mut self, lifetime: &mut Lifetime) {
        let is_bound = self
            .bound_lifetimes
            .iter()
            .any(|bound| bound.contains(&lifetime.ident.to_string()));
        if self.source_lifetimes.contains(&lifetime.ident.to_string()) && !is_bound {
            *lifetime = self.replacement.clone();
        }
    }
}

fn reject_misplaced_zero_attributes(item: &Item) -> syn::Result<()> {
    let mut error: Option<Error> = None;
    let mut add = |span: Span| {
        let next = Error::new(
            span,
            "#[zero] is only allowed on the item, named fields, or tagged-enum variants",
        );
        if let Some(existing) = &mut error {
            existing.combine(next);
        } else {
            error = Some(next);
        }
    };
    let generics = match item {
        Item::Struct(item) => &item.generics,
        Item::Enum(item) => &item.generics,
        Item::Union(item) => &item.generics,
        _ => return Ok(()),
    };
    for parameter in &generics.params {
        let attrs = match parameter {
            GenericParam::Lifetime(parameter) => &parameter.attrs,
            GenericParam::Type(parameter) => &parameter.attrs,
            GenericParam::Const(parameter) => &parameter.attrs,
        };
        for attribute in attrs.iter().filter(|attribute| parse::is_zero(attribute)) {
            add(attribute.span());
        }
    }
    error.map_or(Ok(()), Err)
}

fn strip_zero_attributes(item: &mut Item) {
    let strip =
        |attributes: &mut Vec<Attribute>| attributes.retain(|attribute| !parse::is_zero(attribute));
    match item {
        Item::Struct(item) => {
            strip(&mut item.attrs);
            for field in &mut item.fields {
                strip(&mut field.attrs);
            }
        }
        Item::Enum(item) => {
            strip(&mut item.attrs);
            for variant in &mut item.variants {
                strip(&mut variant.attrs);
                for field in &mut variant.fields {
                    strip(&mut field.attrs);
                }
            }
        }
        Item::Union(item) => {
            strip(&mut item.attrs);
            for field in &mut item.fields.named {
                strip(&mut field.attrs);
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use quote::quote;

    fn analyze_item(source: TokenStream) -> syn::Result<SchemaIr> {
        analyze(TokenStream::new(), syn::parse2(source).unwrap())
    }

    fn error_of(result: syn::Result<SchemaIr>) -> Error {
        match result {
            Ok(_) => panic!("analysis unexpectedly succeeded"),
            Err(error) => error,
        }
    }

    #[test]
    fn retains_item_and_strips_consumed_options() {
        let ir = analyze_item(quote!(
            #[derive(Clone)]
            pub struct Record<'a, T: ?Sized, const N: usize>
            where
                T: 'a,
            {
                #[zero(capacity = 4, len_type = u8)]
                text: &'a str,
                bytes: &'a [u8; N],
            }
        ))
        .unwrap();
        let Item::Struct(item) = ir.item else {
            panic!("expected retained struct")
        };
        assert!(
            item.attrs
                .iter()
                .any(|attribute| attribute.path().is_ident("derive"))
        );
        assert!(matches!(item.vis, Visibility::Public(_)));
        assert_eq!(item.generics.params.len(), 3);
        assert!(item.generics.where_clause.is_some());
        assert!(item.fields.iter().all(|field| {
            field
                .attrs
                .iter()
                .all(|attribute| !attribute.path().is_ident("zero"))
        }));
    }

    #[test]
    fn rejects_arithmetic_array_lengths_and_zero_literals() {
        let arithmetic = analyze_item(quote!(
            struct Bad<const N: usize> {
                values: [u32; N + 1],
            }
        ));
        assert!(error_of(arithmetic).to_string().contains("array length"));
        let zero = analyze_item(quote!(
            struct Bad {
                values: [u32; 0],
            }
        ));
        assert!(error_of(zero).to_string().contains("zero-length"));
        analyze_item(quote!(
            struct Good<const N: usize> {
                values: [u32; N],
            }
        ))
        .unwrap();
        analyze_item(quote!(
            struct Hex {
                values: [u32; 0x2],
            }
        ))
        .unwrap();
        analyze_item(quote!(
            struct Path {
                values: [u32; widths::COUNT],
            }
        ))
        .unwrap();
    }

    #[test]
    fn requires_unique_external_tags() {
        let error = error_of(analyze_item(quote!(
            struct Parent {
                tag: Tag,
                #[zero(tag_field = tag)]
                left: Left,
                #[zero(tag_field = tag)]
                right: Right,
            }
        )));
        assert!(error.to_string().contains("exactly one"));
    }

    #[test]
    fn source_lifetime_is_unambiguous_or_selected() {
        let ambiguous = analyze_item(quote!(
            struct Pair<'a, 'b> {
                #[zero(capacity = 1)]
                left: &'a str,
                #[zero(capacity = 1)]
                right: &'b str,
            }
        ));
        assert!(
            error_of(ambiguous)
                .to_string()
                .contains("multiple borrowed")
        );
        analyze(
            quote!(borrow = 'a),
            syn::parse2(quote!(
                struct Pair<'a, 'b> {
                    #[zero(capacity = 1)]
                    left: &'a str,
                    #[zero(capacity = 1)]
                    right: &'b str,
                }
            ))
            .unwrap(),
        )
        .unwrap();
    }

    #[test]
    fn accepts_external_tagged_payload_with_const_array() {
        let tag = analyze_item(quote!(
            #[repr(u8)]
            pub enum Tag {
                Unit = 1,
                Data = 2,
            }
        ))
        .unwrap();
        assert!(matches!(
            tag.kind,
            ItemKind::ScalarEnum {
                repr: ScalarRepr::U8
            }
        ));

        let child = analyze_item(quote!(
            pub struct Child<'a> {
                value: u32,
                #[zero(capacity = 4)]
                name: &'a CStr,
            }
        ))
        .unwrap();
        assert!(matches!(
            child.fields[1].category,
            FieldCategory::BorrowedCStr { .. }
        ));

        let payload = analyze_item(quote!(
            pub enum Payload<'a> {
                #[zero(tag = Tag::Unit)]
                Unit,
                #[zero(tag = Tag::Data)]
                Data(Child<'a>),
            }
        ))
        .unwrap();
        assert!(matches!(payload.kind, ItemKind::TaggedEnum));
        let Item::Enum(retained_payload) = payload.item else {
            panic!("expected retained enum")
        };
        assert!(retained_payload.variants.iter().all(|variant| {
            variant
                .attrs
                .iter()
                .all(|attribute| !parse::is_zero(attribute))
        }));

        let root = analyze(
            quote!(align = 8),
            syn::parse2(quote!(
                pub struct Root<'a, const N: usize> {
                    tag: Tag,
                    #[zero(tag_field = tag)]
                    payload: Payload<'a>,
                    values: [u32; N],
                }
            ))
            .unwrap(),
        )
        .unwrap();
        assert!(matches!(
            root.fields[1].category,
            FieldCategory::Path {
                tagged: true,
                tag_field: Some(0)
            }
        ));
        assert!(matches!(
            root.fields[2].category,
            FieldCategory::Array { .. }
        ));
    }

    #[test]
    fn rejects_duplicate_or_inapplicable_options() {
        let duplicate = analyze(
            quote!(endian = "little", endian = "big"),
            syn::parse2(quote!(
                struct Record {
                    value: u32,
                }
            ))
            .unwrap(),
        );
        assert!(error_of(duplicate).to_string().contains("duplicate"));
        let inapplicable = analyze_item(quote!(
            struct Record {
                #[zero(tag_field = tag)]
                value: u32,
                tag: Tag,
            }
        ));
        assert!(
            error_of(inapplicable)
                .to_string()
                .contains("not applicable")
        );
    }

    #[test]
    fn rejects_nonparticipating_borrow_and_duplicate_variant_tags() {
        let borrow = analyze(
            quote!(borrow = 'b),
            syn::parse2(quote!(
                struct Record<'a, 'b> {
                    #[zero(capacity = 4)]
                    name: &'a str,
                }
            ))
            .unwrap(),
        );
        assert!(
            error_of(borrow)
                .to_string()
                .contains("direct borrowed field")
        );
        let duplicate_tag = analyze_item(quote!(
            enum Payload {
                #[zero(tag = Tag::Unit)]
                First,
                #[zero(tag = Tag::Unit)]
                Second,
            }
        ));
        assert!(
            error_of(duplicate_tag)
                .to_string()
                .contains("must be unique")
        );
    }

    #[test]
    fn support_module_hash_is_deterministic_and_sensitive() {
        let one = analyze_item(quote!(
            struct Stable {
                value: u8,
            }
        ))
        .unwrap();
        let same = analyze_item(quote!(
            struct Stable {
                value: u8,
            }
        ))
        .unwrap();
        let changed = analyze_item(quote!(
            struct Stable {
                value: u16,
            }
        ))
        .unwrap();
        assert_eq!(one.names.support_module, same.names.support_module);
        assert_ne!(one.names.support_module, changed.names.support_module);
    }
}
