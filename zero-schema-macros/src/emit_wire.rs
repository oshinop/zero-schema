use proc_macro2::{Span, TokenStream};
use quote::{format_ident, quote};
use syn::{
    Error, GenericParam, Generics, Ident, Lifetime, Path, Type, TypeArray, TypeParam,
    WherePredicate, parse_quote, spanned::Spanned as _, visit::Visit, visit_mut::VisitMut,
};

use crate::{
    analyze::{
        self, ArrayLength, FieldCategory, FieldIr, ItemKind, ScalarRepr, SchemaIr, VariantShape,
    },
    parse::Endian,
};

pub(crate) fn emit(ir: &SchemaIr) -> syn::Result<TokenStream> {
    let runtime = resolve_runtime(ir)?;
    let support_runtime = crate::analyze::rebase_path(&runtime);
    let zerocopy = resolve_zerocopy(ir.ident.span())?;
    match ir.kind {
        ItemKind::Struct => emit_struct(ir, &runtime, &support_runtime, &zerocopy),
        ItemKind::ScalarEnum { repr } => {
            emit_scalar_enum(ir, repr, &runtime, &support_runtime, &zerocopy)
        }
        ItemKind::TaggedEnum => emit_tagged_enum(ir, &runtime, &support_runtime, &zerocopy),
    }
}

fn resolve_runtime(ir: &SchemaIr) -> syn::Result<Path> {
    if let Some(path) = &ir.options.runtime {
        return Ok(path.value.clone());
    }
    match proc_macro_crate::crate_name("zero-schema") {
        Ok(proc_macro_crate::FoundCrate::Itself) => syn::parse_str("::zero_schema"),
        Ok(proc_macro_crate::FoundCrate::Name(name)) => dependency_root(&name, ir.ident.span()),
        Err(_) => Err(Error::new(
            ir.ident.span(),
            "#[zero] requires the consuming crate to depend directly on zero-schema (or use crate = path)",
        )),
    }
}

fn resolve_zerocopy(span: Span) -> syn::Result<Path> {
    match proc_macro_crate::crate_name("zerocopy") {
        Ok(proc_macro_crate::FoundCrate::Itself) => syn::parse_str("::zerocopy"),
        Ok(proc_macro_crate::FoundCrate::Name(name)) => dependency_root(&name, span),
        Err(_) => Err(Error::new(
            span,
            "#[zero] requires the consuming crate to depend directly on zerocopy",
        )),
    }
}

fn dependency_root(name: &str, span: Span) -> syn::Result<Path> {
    let ident = Ident::new_raw(&name.replace('-', "_"), span);
    syn::parse2(quote!(::#ident))
}

fn emit_struct(
    ir: &SchemaIr,
    runtime: &Path,
    support_runtime: &Path,
    zerocopy: &Path,
) -> syn::Result<TokenStream> {
    let module = &ir.names.support_module;
    let wire = &ir.names.wire;
    let ident = &ir.ident;
    let visibility = &ir.visibility;
    let wire_generics = wire_generics(&ir.generics.original);
    let module_scope_anchor = format_ident!("{}__module_scope", module);
    let wire_arguments = wire_arguments(&ir.generics.original);
    let field_types: Vec<_> = ir
        .fields
        .iter()
        .map(|field| wire_field_type(field, support_runtime))
        .collect::<syn::Result<_>>()?;
    let wire_dependency_types = dependency_types(&ir.fields);
    let impl_dependency_types = parent_dependency_types(&ir.fields);
    let wire_where = wire_where_clause(
        &ir.generics.original,
        &wire_dependency_types,
        support_runtime,
    );
    let field_idents: Vec<_> = ir.fields.iter().map(|field| &field.ident).collect();
    let field_offsets = ir.fields.iter().map(|field| {
        let offset = field_offset_name(&field.ident);
        let field = &field.ident;
        quote!(pub(super) const #offset: ::core::primitive::usize = ::core::mem::offset_of!(Self, #field);)
    }).collect::<Vec<_>>();
    let field_offsets = quote!(
        impl #wire_generics #wire #wire_arguments #wire_where {
            #(#field_offsets)*
        }
    );
    let root_wire = aligned_root_wire(ir, quote!(#module::#wire #wire_arguments), support_runtime);
    let access_invariant = crate::emit_access::root_wire_invariant(ir, &root_wire);
    let nonzero_array_assertions = nonzero_assertions(&ir.fields);
    let capabilities = crate::emit_access::emit_module(ir, runtime, support_runtime)?;
    let root_surface = crate::emit_access::emit_root_surface(ir, runtime, support_runtime)?;
    let tag_identity_assertions =
        tag_identity_assertions(ir, runtime, &wire_generics, &wire_where)?;
    let impl_generics =
        impl_generics_with_dependencies(&ir.generics.original, &impl_dependency_types, runtime);
    let (impl_generics_tokens, _, where_clause) = impl_generics.split_for_impl();
    let field_metadata = struct_field_metadata(ir, runtime, support_runtime, &root_wire)?;
    let padding = struct_padding_ranges(ir, runtime, support_runtime, &root_wire)?;
    let abi_assertions = struct_abi_assertions(ir, support_runtime, &root_wire)?;
    let (_, original_ty_generics, _) = ir.generics.original.split_for_impl();
    let logical_name = &ir.logical_name;
    let padding_const = format_ident!("__ZERO_SCHEMA_PADDING");
    let fields_const = format_ident!("__ZERO_SCHEMA_FIELDS");
    let layout = quote!(
        &#runtime::LayoutDescriptor::__new(
            #logical_name,
            #runtime::TypeKind::Struct,
            ::core::mem::size_of::<#root_wire>(),
            ::core::mem::align_of::<#root_wire>(),
            ::core::mem::size_of::<#root_wire>(),
            Self::#padding_const,
            Self::#fields_const,
            &[],
            &[],
        )
    );
    Ok(quote! {
        #[doc(hidden)]
        #[allow(unused_imports)]
        #visibility mod #module {
            use #runtime::{SchemaError as _};
            use #runtime::__private::{OwnerAdapter as _, ScalarEnumSupport as _, SchemaSupport as _, TaggedPayloadSupport as _};
            use super::*;
            #[allow(unused_imports)]
            use ::core::option::Option::{self, None, Some};
            #[allow(unused_imports)]
            use ::core::primitive::u8;
            #tag_identity_assertions
            #[repr(C)]
            #[derive(#zerocopy::FromBytes, #zerocopy::KnownLayout, #zerocopy::Immutable)]
            pub struct #wire #wire_generics #wire_where {
                #(#field_idents: #field_types,)*
            }
            #field_offsets
            #capabilities
        }

        #[allow(unused_imports)]
        use self::#ident as #module_scope_anchor;

        impl #impl_generics_tokens #ident #original_ty_generics #where_clause {
            const #padding_const: &'static [#runtime::ByteRange] = &[#(#padding),*];
            const #fields_const: &'static [#runtime::FieldDescriptor] = &[#(#field_metadata),*];
            const __ZERO_SCHEMA_NONZERO_ARRAYS: () = {
                #(#nonzero_array_assertions)*
            };
            #visibility const SCHEMA_SIZE: ::core::primitive::usize = {
                let _ = Self::__ZERO_SCHEMA_NONZERO_ARRAYS;
                #access_invariant
                #(#abi_assertions)*
                ::core::mem::size_of::<#root_wire>()
            };
            #visibility const SCHEMA_ALIGN: ::core::primitive::usize = {
                let _ = Self::__ZERO_SCHEMA_NONZERO_ARRAYS;
                ::core::mem::align_of::<#root_wire>()
            };
            #visibility const SCHEMA_STRIDE: ::core::primitive::usize = {
                let _ = Self::__ZERO_SCHEMA_NONZERO_ARRAYS;
                ::core::mem::size_of::<#root_wire>()
            };
            #visibility const LAYOUT: &'static #runtime::LayoutDescriptor = {
                let _ = Self::__ZERO_SCHEMA_NONZERO_ARRAYS;
                let _ = Self::SCHEMA_SIZE;
                #layout
            };
        }

        impl #impl_generics_tokens #runtime::__private::WireType for #ident #original_ty_generics #where_clause {
            type Wire = #root_wire;
            const SIZE: ::core::primitive::usize = Self::SCHEMA_SIZE;
            const ALIGN: ::core::primitive::usize = Self::SCHEMA_ALIGN;
            const STRIDE: ::core::primitive::usize = Self::SCHEMA_STRIDE;
            const LAYOUT: &'static #runtime::LayoutDescriptor = Self::LAYOUT;
        }
        #root_surface
    })
}

fn emit_scalar_enum(
    ir: &SchemaIr,
    repr: ScalarRepr,
    runtime: &Path,
    support_runtime: &Path,
    zerocopy: &Path,
) -> syn::Result<TokenStream> {
    let module = &ir.names.support_module;
    let wire = &ir.names.wire;
    let ident = &ir.ident;
    let visibility = &ir.visibility;
    let wire_inner = repr.wire_ident(ir.options.endian);
    let module_scope_anchor = format_ident!("{}__module_scope", module);
    let root_wire = aligned_root_wire(ir, quote!(#module::#wire), support_runtime);
    let logical_name = &ir.logical_name;
    let repr_variant = repr.runtime_variant();
    let endian_variant = ir.options.endian.runtime_variant();
    let enum_values = ir.variants.iter().map(|variant| {
        let variant_ident = &variant.ident;
        let name = &variant.logical_name;
        quote!(#runtime::EnumValueDescriptor::__new(#name, #ident::#variant_ident as ::core::primitive::u64))
    });
    let layout = quote!(
        &#runtime::LayoutDescriptor::__new(
            #logical_name,
            #runtime::TypeKind::ScalarEnum {
                repr: #runtime::IntegerRepr::#repr_variant,
                endian: #runtime::Endian::#endian_variant,
            },
            ::core::mem::size_of::<#root_wire>(),
            ::core::mem::align_of::<#root_wire>(),
            ::core::mem::size_of::<#root_wire>(),
            &[],
            &[],
            &[#(#enum_values),*],
            &[],
        )
    );
    let capabilities = crate::emit_access::emit_module(ir, runtime, support_runtime)?;
    let root_surface = crate::emit_access::emit_root_surface(ir, runtime, support_runtime)?;
    Ok(quote! {
        #[doc(hidden)]
        #visibility mod #module {
            #[repr(transparent)]
            #[derive(#zerocopy::FromBytes, #zerocopy::KnownLayout, #zerocopy::Immutable)]
            pub struct #wire(#support_runtime::__private::#wire_inner);
            use #runtime::{SchemaError as _};
            use #runtime::__private::{OwnerAdapter as _, ScalarEnumSupport as _, SchemaSupport as _};
            #[allow(unused_imports)]
            use ::core::option::Option::{self, None, Some};
            #capabilities
        }
        #[allow(unused_imports)]
        use self::#ident as #module_scope_anchor;


        impl #ident {
            #visibility const SCHEMA_SIZE: ::core::primitive::usize = ::core::mem::size_of::<#root_wire>();
            #visibility const SCHEMA_ALIGN: ::core::primitive::usize = ::core::mem::align_of::<#root_wire>();
            #visibility const SCHEMA_STRIDE: ::core::primitive::usize = ::core::mem::size_of::<#root_wire>();
            #visibility const LAYOUT: &'static #runtime::LayoutDescriptor = #layout;
        }

        impl #runtime::__private::WireType for #ident {
            type Wire = #root_wire;
            const SIZE: ::core::primitive::usize = Self::SCHEMA_SIZE;
            const ALIGN: ::core::primitive::usize = Self::SCHEMA_ALIGN;
            const STRIDE: ::core::primitive::usize = Self::SCHEMA_STRIDE;
            const LAYOUT: &'static #runtime::LayoutDescriptor = Self::LAYOUT;
        }
        #root_surface
    })
}
fn emit_tagged_enum(
    ir: &SchemaIr,
    runtime: &Path,
    support_runtime: &Path,
    zerocopy: &Path,
) -> syn::Result<TokenStream> {
    let module = &ir.names.support_module;
    let wire = &ir.names.wire;
    let ident = &ir.ident;
    let visibility = &ir.visibility;
    let module_scope_anchor = format_ident!("{}__module_scope", module);
    let wire_generics = wire_generics(&ir.generics.original);
    let wire_arguments = wire_arguments(&ir.generics.original);
    let dependency_types = tagged_dependency_types(ir);
    let wire_where = wire_where_clause(&ir.generics.original, &dependency_types, support_runtime);
    let payload_fields = ir.variants.iter().enumerate().map(|(index, variant)| {
        let field = format_ident!("variant_{index}");
        let type_name = format_ident!("Variant{index}Wire");
        let payload = match &variant.shape {
            VariantShape::Unit => quote!(#type_name),
            VariantShape::Newtype(ty) => {
                let support_ty = analyze::support_type(ty);
                quote!(<#support_ty as #support_runtime::__private::WireType>::Wire)
            }
        };
        quote!(#field: ::core::mem::ManuallyDrop<#payload>)
    });
    let unit_wires = ir
        .variants
        .iter()
        .enumerate()
        .filter(|(_, variant)| matches!(variant.shape, VariantShape::Unit))
        .map(|(index, variant)| {
            let type_name = format_ident!("Variant{index}Wire");
            let layout_name = format_ident!("Variant{index}Layout");
            let variant_name = format!("{}.{}", ir.logical_name, variant.logical_name);
            quote! {
                #[repr(C)]
                #[derive(#zerocopy::FromBytes, #zerocopy::KnownLayout, #zerocopy::Immutable)]
                pub struct #type_name { byte: #support_runtime::__private::U8 }
                pub(super) const #layout_name: #runtime::LayoutDescriptor = #runtime::LayoutDescriptor::__new(
                    #variant_name,
                    #runtime::TypeKind::Struct,
                    ::core::mem::size_of::<#type_name>(),
                    ::core::mem::align_of::<#type_name>(),
                    ::core::mem::size_of::<#type_name>(),
                    &[], &[], &[], &[],
                );
            }
        })
        .collect::<Vec<_>>();
    let variant_descriptors = ir.variants.iter().enumerate().map(|(index, variant)| {
        let name = &variant.logical_name;
        let tag = &variant.tag;
        match &variant.shape {
            VariantShape::Unit => {
                let wire = format_ident!("Variant{index}Wire");
                let layout = format_ident!("Variant{index}Layout");
                quote!(#runtime::VariantDescriptor::__new(
                    #name, #tag as ::core::primitive::u64, &#module::#layout,
                    ::core::mem::size_of::<#module::#wire>(),
                    ::core::mem::align_of::<#module::#wire>(),
                ))
            }
            VariantShape::Newtype(ty) => {
                let outer_ty = analyze::erased_source_type(ty);
                quote!(#runtime::VariantDescriptor::__new(
                    #name, #tag as ::core::primitive::u64,
                    <#outer_ty as #runtime::__private::WireType>::LAYOUT,
                    ::core::mem::size_of::<<#outer_ty as #runtime::__private::WireType>::Wire>(),
                    ::core::mem::align_of::<<#outer_ty as #runtime::__private::WireType>::Wire>(),
                ))
            }
        }
    });
    let capabilities = crate::emit_access::emit_module(ir, runtime, support_runtime)?;
    let root_surface = crate::emit_access::emit_root_surface(ir, runtime, support_runtime)?;
    let (_, tagged_arguments, tagged_where) = ir.generics.original.split_for_impl();
    let (tagged_impl_generics, _, _) = ir.generics.original.split_for_impl();
    let support = format_ident!("{}Support", ir.logical_name);
    let logical_name = &ir.logical_name;
    let tag_type = &ir.variants[0].tag_type;
    let tagged_zero_state = crate::emit_access::tagged_payload_zero_state(ir, runtime)?;
    let tagged_logical_arguments = ir
        .generics
        .original
        .params
        .iter()
        .map(|parameter| match parameter {
            GenericParam::Lifetime(_) => quote!('wire),
            GenericParam::Type(parameter) => {
                let ident = &parameter.ident;
                quote!(#ident)
            }
            GenericParam::Const(parameter) => {
                let ident = &parameter.ident;
                quote!(#ident)
            }
        })
        .collect::<Vec<_>>();
    let tagged_logical = if tagged_logical_arguments.is_empty() {
        quote!(#ident)
    } else {
        quote!(#ident<#(#tagged_logical_arguments),*>)
    };
    let patch = &ir.names.patch;
    let patch_projection_arguments =
        crate::emit_patch::patch_projection_arguments(ir, quote!('source));
    let tagged_patch_type_support = quote!(
        impl #tagged_impl_generics #runtime::__private::TaggedPayloadPatchType for #ident #tagged_arguments #tagged_where {
            type Patch<'source> = #module::#patch #patch_projection_arguments;
        }
    );
    let tagged_layout = quote!(&#runtime::LayoutDescriptor::__new(
        #logical_name,
        #runtime::TypeKind::TaggedUnion,
        ::core::mem::size_of::<#module::#wire #wire_arguments>(),
        ::core::mem::align_of::<#module::#wire #wire_arguments>(),
        ::core::mem::size_of::<#module::#wire #wire_arguments>(),
        &[], &[], &[], &[#(#variant_descriptors),*],
    ));
    Ok(quote! {
        #[doc(hidden)]
        #[allow(unused_imports)]
        #visibility mod #module {
            use #runtime::{SchemaError as _};
            use #runtime::__private::{OwnerAdapter as _, SchemaSupport as _, TaggedPayloadSupport as _};
            use super::*;
            #[allow(unused_imports)]
            use ::core::option::Option::{self, None, Some};
            #[allow(unused_imports)]
            use ::core::primitive::u8;
            #(#unit_wires)*
            #[repr(C)]
            #[derive(#zerocopy::FromBytes, #zerocopy::KnownLayout, #zerocopy::Immutable)]
            pub union #wire #wire_generics #wire_where {
                #(#payload_fields,)*
            }
            #capabilities
        }

        #[allow(unused_imports)]
        use self::#ident as #module_scope_anchor;
        impl #tagged_impl_generics #runtime::__private::TaggedPayloadTypeSupport for #ident #tagged_arguments #tagged_where {
            type Tag = #tag_type;
            type Logical<'wire> = #tagged_logical;
            type Support = #module::#support #wire_arguments;
            type ZeroState = #tagged_zero_state;
            const LAYOUT: &'static #runtime::LayoutDescriptor = #tagged_layout;
        }
        #tagged_patch_type_support
        #root_surface
    })
}

pub(crate) fn wire_field_type(field: &FieldIr, runtime: &Path) -> syn::Result<TokenStream> {
    let base = match field.category {
        FieldCategory::Primitive(primitive) => {
            let wire = primitive.wire_ident(field.wire_endian);
            quote!(#runtime::__private::#wire)
        }
        _ => wire_type(
            &field.category,
            &field.support_ty,
            runtime,
            field.wire_endian,
        )?,
    };
    if let Some(align) = &field.options.align {
        let marker = alignment_marker(align.value, align.span)?;
        Ok(quote!(#runtime::__private::AlignedWire<#base, #runtime::__private::#marker>))
    } else {
        Ok(base)
    }
}

pub(crate) fn wire_type(
    category: &FieldCategory,
    ty: &Type,
    runtime: &Path,
    default_endian: Endian,
) -> syn::Result<TokenStream> {
    Ok(match category {
        FieldCategory::Primitive(primitive) => {
            let wire = primitive.wire_ident(default_endian);
            quote!(#runtime::__private::#wire)
        }
        FieldCategory::Bool => quote!(#runtime::__private::BoolWire),
        FieldCategory::BorrowedStr {
            capacity,
            len_type,
            endian,
            ..
        } => {
            let length = length_wire(len_type, *endian)?;
            quote!(#runtime::__private::StrWire<#runtime::__private::#length, #capacity>)
        }
        FieldCategory::BorrowedCStr { capacity, .. } => {
            quote!(#runtime::__private::CStrWire<#capacity>)
        }
        FieldCategory::BorrowedU16Str {
            capacity,
            len_type,
            endian,
            ..
        } => {
            let length = length_wire(len_type, *endian)?;
            quote!(#runtime::__private::U16StrWire<#runtime::__private::#length, #capacity>)
        }
        FieldCategory::BorrowedU16CStr { capacity, .. } => {
            quote!(#runtime::__private::U16CStrWire<#capacity>)
        }
        FieldCategory::FixedBytes { .. } => {
            let length = array_length(ty)?;
            quote!([::core::primitive::u8; #length])
        }
        FieldCategory::Path { tagged: true, .. } => quote!(
            <<#ty as #runtime::__private::TaggedPayloadTypeSupport>::Support
                as #runtime::__private::TaggedPayloadSupport>::Wire
        ),
        FieldCategory::Path { .. } => quote!(<#ty as #runtime::__private::WireType>::Wire),
        FieldCategory::Array { element, .. } => {
            let Type::Array(TypeArray { elem, .. }) = ty else {
                return Err(Error::new(
                    ty.span(),
                    "internal zero-schema array analysis mismatch",
                ));
            };
            let element_type = wire_type(element, elem, runtime, default_endian)?;
            let length = array_length(ty)?;
            quote!([#element_type; #length])
        }
        FieldCategory::Optional {
            inner,
            inner_support_ty,
            ..
        } => wire_type(inner, inner_support_ty, runtime, default_endian)?,
    })
}

/// Emits the array length carried by the type supplied to the current emission
/// context. Support-module callers pass a rebased type; outer metadata callers
/// pass declaration-scope syntax.
pub(crate) fn array_length(ty: &Type) -> syn::Result<TokenStream> {
    match ty {
        Type::Array(array) => {
            let length = &array.len;
            Ok(quote!(#length))
        }
        Type::Reference(reference) => array_length(&reference.elem),
        _ => Err(Error::new(
            ty.span(),
            "internal zero-schema array analysis mismatch",
        )),
    }
}

pub(crate) fn length_wire(len_type: &Ident, endian: Endian) -> syn::Result<Ident> {
    let name = match (len_type.to_string().as_str(), endian) {
        ("u8", _) => "U8",
        ("u16", Endian::Native) => "NativeU16",
        ("u16", Endian::Little) => "LittleU16",
        ("u16", Endian::Big) => "BigU16",
        ("u32", Endian::Native) => "NativeU32",
        ("u32", Endian::Little) => "LittleU32",
        ("u32", Endian::Big) => "BigU32",
        _ => {
            return Err(Error::new(
                len_type.span(),
                "len_type must be u8, u16, or u32",
            ));
        }
    };
    Ok(Ident::new(name, len_type.span()))
}

fn tag_identity_assertions(
    ir: &SchemaIr,
    runtime: &Path,
    wire_generics: &TokenStream,
    wire_where: &TokenStream,
) -> syn::Result<TokenStream> {
    let mut payload_tag_types = Vec::new();
    let mut scalar_tag_types = Vec::new();
    let mut actual_types = Vec::new();
    for field in &ir.fields {
        let FieldCategory::Path {
            tagged: true,
            tag_field: Some(tag_field),
        } = &field.category
        else {
            continue;
        };
        let actual_type = fields_tag_type(ir, *tag_field)?;
        let payload = &field.support_ty;
        payload_tag_types
            .push(quote!(<#payload as #runtime::__private::TaggedPayloadTypeSupport>::Tag));
        scalar_tag_types.push(quote!(
            <<#actual_type as #runtime::__private::WireTypeSupport>::Support
                as #runtime::__private::ScalarEnumSupport>::Value
        ));
        actual_types.push(actual_type);
    }
    if actual_types.is_empty() {
        return Ok(TokenStream::new());
    }
    let identity = format_ident!("__zero_schema_same_type");
    let assert_tag = format_ident!("__zero_schema_assert_tag_type");
    let assert_all = format_ident!("__zero_schema_assert_all_tag_types");
    Ok(quote! {
        trait #identity<T> {}
        impl<T> #identity<T> for T {}
        fn #assert_tag<T, Expected>() where T: #identity<Expected> {}
        fn #assert_all #wire_generics () #wire_where {
            #(#assert_tag::<#actual_types, #payload_tag_types>();)*
            #(#assert_tag::<#actual_types, #scalar_tag_types>();)*
        }
    })
}

fn fields_tag_type(ir: &SchemaIr, index: usize) -> syn::Result<Type> {
    ir.fields
        .get(index)
        .map(|field| field.support_ty.clone())
        .ok_or_else(|| {
            Error::new(
                ir.ident.span(),
                "internal zero-schema tag-field index mismatch",
            )
        })
}

fn alignment_marker(value: u32, span: Span) -> syn::Result<Ident> {
    if value == 0 || !value.is_power_of_two() || value > (1 << 29) {
        return Err(Error::new(span, "unsupported wire alignment"));
    }
    Ok(format_ident!("Align{value}"))
}

pub(crate) fn aligned_root_wire(ir: &SchemaIr, base: TokenStream, runtime: &Path) -> TokenStream {
    if let Some(align) = &ir.options.align {
        let marker = alignment_marker(align.value, align.span)
            .expect("validated container alignment must have a runtime marker");
        quote!(#runtime::__private::AlignedWire<#base, #runtime::__private::#marker>)
    } else {
        base
    }
}

fn dependency_types(fields: &[FieldIr]) -> Vec<Type> {
    let mut result = Vec::new();
    for field in fields {
        collect_dependencies(&field.category, &field.ty, &field.support_ty, &mut result);
    }
    deduplicate_types(result)
}

pub(crate) fn parent_dependency_types(fields: &[FieldIr]) -> Vec<Type> {
    let mut result = Vec::new();
    for field in fields {
        collect_parent_dependencies(&field.category, &field.ty, &mut result);
    }
    deduplicate_types(result)
}

fn collect_parent_dependencies(category: &FieldCategory, ty: &Type, result: &mut Vec<Type>) {
    match category {
        FieldCategory::Path { tagged: false, .. } => {
            if !has_source_lifetime(ty) {
                result.push(crate::analyze::erased_source_type(ty));
            }
        }
        FieldCategory::Path { tagged: true, .. } => {}
        FieldCategory::Array { element, .. } => {
            if let Type::Array(array) = ty {
                collect_parent_dependencies(element, &array.elem, result);
            }
        }
        FieldCategory::Optional {
            inner, inner_ty, ..
        } => collect_parent_dependencies(inner, inner_ty, result),
        _ => {}
    }
}

pub(crate) fn optional_dependency_types(fields: &[FieldIr]) -> Vec<Type> {
    let mut result = Vec::new();
    for field in fields {
        collect_optional_dependencies(&field.category, &mut result);
    }
    deduplicate_types(result)
}

fn collect_optional_dependencies(category: &FieldCategory, result: &mut Vec<Type>) {
    match category {
        FieldCategory::Optional {
            inner,
            inner_support_ty,
            ..
        } => collect_optional_inner_dependencies(inner, inner_support_ty, result),
        FieldCategory::Array { element, .. } => collect_optional_dependencies(element, result),
        _ => {}
    }
}

fn collect_optional_inner_dependencies(
    category: &FieldCategory,
    support_ty: &Type,
    result: &mut Vec<Type>,
) {
    match category {
        FieldCategory::Path { .. } => result.push(support_ty.clone()),
        FieldCategory::Array { element, .. } => {
            if let Type::Array(array) = support_ty {
                collect_optional_inner_dependencies(element, &array.elem, result);
            }
        }
        _ => unreachable!("optional analysis accepts only path values or path arrays"),
    }
}

pub(crate) fn tagged_dependency_types(ir: &SchemaIr) -> Vec<Type> {
    let mut result = Vec::new();
    for variant in &ir.variants {
        if let VariantShape::Newtype(ty) = &variant.shape {
            if !has_source_lifetime(ty) {
                result.push(analyze::support_type(ty));
            }
        }
    }
    deduplicate_types(result)
}

fn collect_dependencies(
    category: &FieldCategory,
    source_ty: &Type,
    support_ty: &Type,
    result: &mut Vec<Type>,
) {
    match category {
        FieldCategory::Path { tagged: false, .. } if !has_source_lifetime(source_ty) => {
            result.push(support_ty.clone());
        }
        FieldCategory::Path { .. } => {}
        FieldCategory::Array { element, .. } => {
            if let (Type::Array(source), Type::Array(support)) = (source_ty, support_ty) {
                collect_dependencies(element, &source.elem, &support.elem, result);
            }
        }
        FieldCategory::Optional {
            inner,
            inner_ty,
            inner_support_ty,
        } => collect_dependencies(inner, inner_ty, inner_support_ty, result),
        _ => {}
    }
}

fn has_source_lifetime(ty: &Type) -> bool {
    let erased = analyze::erased_source_type(ty);
    quote!(#ty).to_string() != quote!(#erased).to_string()
}

fn deduplicate_types(types: Vec<Type>) -> Vec<Type> {
    let mut seen = std::collections::BTreeSet::new();
    types
        .into_iter()
        .filter(|ty| seen.insert(quote!(#ty).to_string()))
        .collect()
}

pub(crate) fn wire_generic_parameters(original: &Generics) -> Vec<TokenStream> {
    original
        .params
        .iter()
        .filter_map(|parameter| match parameter {
            GenericParam::Type(parameter) => {
                let mut parameter = parameter.clone();
                rebind_type_parameter(&mut parameter, original);
                Some(quote!(#parameter))
            }
            GenericParam::Const(parameter) => {
                let ident = &parameter.ident;
                let ty = &parameter.ty;
                Some(quote!(const #ident: #ty))
            }
            GenericParam::Lifetime(_) => None,
        })
        .collect()
}

pub(crate) fn wire_generics(original: &Generics) -> TokenStream {
    let params = wire_generic_parameters(original);
    if params.is_empty() {
        TokenStream::new()
    } else {
        quote!(<#(#params),*>)
    }
}

pub(crate) fn wire_arguments(original: &Generics) -> TokenStream {
    let arguments: Vec<_> = original
        .params
        .iter()
        .filter_map(|parameter| match parameter {
            GenericParam::Type(parameter) => {
                let ident = &parameter.ident;
                Some(quote!(#ident))
            }
            GenericParam::Const(parameter) => {
                let ident = &parameter.ident;
                Some(quote!(#ident))
            }
            GenericParam::Lifetime(_) => None,
        })
        .collect();
    if arguments.is_empty() {
        TokenStream::new()
    } else {
        quote!(<#(#arguments),*>)
    }
}

fn impl_generics_with_dependencies(
    original: &Generics,
    types: &[Type],
    runtime: &Path,
) -> Generics {
    let mut result = original.clone();
    if !types.is_empty() {
        let where_clause = result.make_where_clause();
        for ty in types {
            let predicate: syn::WherePredicate =
                parse_quote!(#ty: #runtime::__private::WireType + 'static);
            where_clause.predicates.push(predicate);
        }
    }
    result
}

pub(crate) fn wire_where_clause(
    original: &Generics,
    types: &[Type],
    runtime: &Path,
) -> TokenStream {
    let predicates = erased_where_predicates(original);
    if predicates.is_empty() && types.is_empty() {
        TokenStream::new()
    } else {
        quote!(where
            #(#predicates,)*
            #(#types: #runtime::__private::WireType + 'static),*
        )
    }
}

/// Retains the source-lifetime-independent portion of original type predicates
/// for lifetime-erased wire support. A source-dependent bound cannot be made
/// stronger by substituting `'static`; it belongs only on the logical view.
/// Higher-ranked binders and explicit `'static` bounds remain verbatim.
pub(crate) fn erased_where_predicates(original: &Generics) -> Vec<WherePredicate> {
    let Some(where_clause) = &original.where_clause else {
        return Vec::new();
    };
    where_clause
        .predicates
        .iter()
        .filter_map(|predicate| match predicate {
            WherePredicate::Type(predicate) => {
                if predicate_bounded_type_mentions_source(predicate, original) {
                    return None;
                }
                let mut predicate = predicate.clone();
                let predicate_context = predicate.clone();
                predicate.bounds = predicate
                    .bounds
                    .into_iter()
                    .filter(|bound| {
                        !predicate_bound_mentions_source(&predicate_context, bound, original)
                    })
                    .collect();
                (!predicate.bounds.is_empty()).then_some(WherePredicate::Type(predicate))
            }
            WherePredicate::Lifetime(_) => None,
            _ => None,
        })
        .collect()
}

fn bound_lifetime_names(lifetimes: &syn::BoundLifetimes) -> std::collections::BTreeSet<String> {
    lifetimes
        .lifetimes
        .iter()
        .filter_map(|parameter| match parameter {
            GenericParam::Lifetime(lifetime) => Some(lifetime.lifetime.ident.to_string()),
            GenericParam::Type(_) | GenericParam::Const(_) => None,
        })
        .collect()
}

struct DeclaredSourceLifetimeUse {
    source_lifetimes: std::collections::BTreeSet<String>,
    bound_lifetimes: Vec<std::collections::BTreeSet<String>>,
    found: bool,
}

impl DeclaredSourceLifetimeUse {
    fn new(original: &Generics) -> Self {
        Self {
            source_lifetimes: original
                .lifetimes()
                .map(|parameter| parameter.lifetime.ident.to_string())
                .collect(),
            bound_lifetimes: Vec::new(),
            found: false,
        }
    }
}

impl<'ast> Visit<'ast> for DeclaredSourceLifetimeUse {
    fn visit_bound_lifetimes(&mut self, lifetimes: &'ast syn::BoundLifetimes) {
        syn::visit::visit_bound_lifetimes(self, lifetimes);
    }

    fn visit_trait_bound(&mut self, bound: &'ast syn::TraitBound) {
        let names = bound.lifetimes.as_ref().map(bound_lifetime_names);
        if let Some(names) = names {
            self.bound_lifetimes.push(names);
        }
        syn::visit::visit_trait_bound(self, bound);
        if bound.lifetimes.is_some() {
            self.bound_lifetimes.pop();
        }
    }

    fn visit_type_bare_fn(&mut self, bare_fn: &'ast syn::TypeBareFn) {
        let names = bare_fn.lifetimes.as_ref().map(bound_lifetime_names);
        if let Some(names) = names {
            self.bound_lifetimes.push(names);
        }
        syn::visit::visit_type_bare_fn(self, bare_fn);
        if bare_fn.lifetimes.is_some() {
            self.bound_lifetimes.pop();
        }
    }

    fn visit_predicate_type(&mut self, predicate: &'ast syn::PredicateType) {
        let names = predicate.lifetimes.as_ref().map(bound_lifetime_names);
        if let Some(names) = names {
            self.bound_lifetimes.push(names);
        }
        syn::visit::visit_predicate_type(self, predicate);
        if predicate.lifetimes.is_some() {
            self.bound_lifetimes.pop();
        }
    }

    fn visit_lifetime(&mut self, lifetime: &'ast Lifetime) {
        let is_bound = self
            .bound_lifetimes
            .iter()
            .any(|bound| bound.contains(&lifetime.ident.to_string()));
        if self.source_lifetimes.contains(&lifetime.ident.to_string()) && !is_bound {
            self.found = true;
        }
    }
}

fn predicate_bounded_type_mentions_source(
    predicate: &syn::PredicateType,
    original: &Generics,
) -> bool {
    let mut visitor = DeclaredSourceLifetimeUse::new(original);
    let has_lifetimes = predicate.lifetimes.is_some();
    if let Some(lifetimes) = &predicate.lifetimes {
        visitor
            .bound_lifetimes
            .push(bound_lifetime_names(lifetimes));
    }
    visitor.visit_type(&predicate.bounded_ty);
    if has_lifetimes {
        visitor.bound_lifetimes.pop();
    }
    visitor.found
}

fn predicate_bound_mentions_source(
    predicate: &syn::PredicateType,
    bound: &syn::TypeParamBound,
    original: &Generics,
) -> bool {
    let mut visitor = DeclaredSourceLifetimeUse::new(original);
    let has_lifetimes = predicate.lifetimes.is_some();
    if let Some(lifetimes) = &predicate.lifetimes {
        visitor
            .bound_lifetimes
            .push(bound_lifetime_names(lifetimes));
    }
    visitor.visit_type_param_bound(bound);
    if has_lifetimes {
        visitor.bound_lifetimes.pop();
    }
    visitor.found
}

fn bound_mentions_declared_source_lifetime(
    bound: &syn::TypeParamBound,
    original: &Generics,
) -> bool {
    let mut visitor = DeclaredSourceLifetimeUse::new(original);
    visitor.visit_type_param_bound(bound);
    visitor.found
}

struct SourceLifetimeRebinder {
    source_lifetimes: std::collections::BTreeSet<String>,
    bound_lifetimes: Vec<std::collections::BTreeSet<String>>,
}

impl VisitMut for SourceLifetimeRebinder {
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
            *lifetime = Lifetime::new("'static", lifetime.span());
        }
    }
}

fn source_lifetime_rebinder(original: &Generics) -> SourceLifetimeRebinder {
    let source_lifetimes = original
        .params
        .iter()
        .filter_map(|parameter| match parameter {
            GenericParam::Lifetime(lifetime) => Some(lifetime.lifetime.ident.to_string()),
            GenericParam::Type(_) | GenericParam::Const(_) => None,
        })
        .collect();
    SourceLifetimeRebinder {
        source_lifetimes,
        bound_lifetimes: Vec::new(),
    }
}

fn rebind_type_parameter(parameter: &mut TypeParam, original: &Generics) {
    parameter.bounds = parameter
        .bounds
        .clone()
        .into_iter()
        .filter(|bound| !bound_mentions_declared_source_lifetime(bound, original))
        .collect();
    if let Some(default) = &mut parameter.default {
        source_lifetime_rebinder(original).visit_type_mut(default);
    }
}

/// Keeps declaration lifetime parameters while dropping only type bounds that
/// require one of them. Patch storage may be instantiated at a fresh patch
/// lifetime, so it must not inherit source-only logical well-formedness.
pub(crate) fn source_independent_generics(original: &Generics) -> Generics {
    let mut result = original.clone();
    for parameter in &mut result.params {
        if let GenericParam::Type(parameter) = parameter {
            parameter.bounds = parameter
                .bounds
                .clone()
                .into_iter()
                .filter(|bound| !bound_mentions_declared_source_lifetime(bound, original))
                .collect();
        }
    }
    let predicates = erased_where_predicates(original);
    result.where_clause = None;
    if !predicates.is_empty() {
        result.make_where_clause().predicates.extend(predicates);
    }
    result
}

pub(crate) fn impl_generics_with_support_dependencies(
    original: &Generics,
    types: &[Type],
    runtime: &Path,
) -> Generics {
    let mut result = original.clone();
    if !types.is_empty() {
        let where_clause = result.make_where_clause();
        for ty in types {
            let predicate: syn::WherePredicate =
                parse_quote!(#ty: #runtime::__private::WireTypeSupport + 'static);
            where_clause.predicates.push(predicate);
        }
    }
    result
}

pub(crate) fn add_optional_wire_type_bounds(
    generics: &mut Generics,
    fields: &[FieldIr],
    runtime: &Path,
) {
    let dependencies = optional_dependency_types(fields);
    if dependencies.is_empty() {
        return;
    }
    let where_clause = generics.make_where_clause();
    for ty in dependencies {
        let predicate: syn::WherePredicate =
            parse_quote!(#ty: #runtime::__private::OptionalWireType + 'static);
        where_clause.predicates.push(predicate);
    }
}

pub(crate) fn nonzero_assertions(fields: &[FieldIr]) -> Vec<TokenStream> {
    fields
        .iter()
        .flat_map(|field| nonzero_assertions_category(&field.category))
        .collect()
}

fn struct_field_metadata(
    ir: &SchemaIr,
    runtime: &Path,
    support_runtime: &Path,
    root_wire: &TokenStream,
) -> syn::Result<Vec<TokenStream>> {
    ir.fields
        .iter()
        .map(|field| {
            let name = &field.logical_name;
            let declaration_index = field.declaration_index;
            let field_wire = outer_wire_field_type(field, support_runtime)?;
            let offset = compiler_field_offset(ir, field, support_runtime, root_wire);
            let kind = field_metadata_kind(ir, field, runtime, support_runtime, root_wire)?;
            let optional = matches!(field.category, FieldCategory::Optional { .. });
            Ok(quote!(#runtime::FieldDescriptor::__new(
                #name,
                #declaration_index,
                #offset,
                ::core::mem::size_of::<#field_wire>(),
                ::core::mem::align_of::<#field_wire>(),
                #kind,
                #optional,
            )))
        })
        .collect()
}

fn outer_wire_field_type(field: &FieldIr, runtime: &Path) -> syn::Result<TokenStream> {
    let ty = analyze::erased_source_type(&field.ty);
    let base = wire_type(&field.category, &ty, runtime, field.wire_endian)?;
    if let Some(align) = &field.options.align {
        let marker = alignment_marker(align.value, align.span)?;
        Ok(quote!(#runtime::__private::AlignedWire<#base, #runtime::__private::#marker>))
    } else {
        Ok(base)
    }
}

fn field_offset_name(ident: &Ident) -> Ident {
    let normalized = ident.to_string();
    format_ident!(
        "__ZERO_SCHEMA_OFFSET_{}",
        normalized.trim_start_matches("r#").to_uppercase()
    )
}

fn compiler_field_offset(
    ir: &SchemaIr,
    field: &FieldIr,
    _runtime: &Path,
    root_wire: &TokenStream,
) -> TokenStream {
    let module = &ir.names.support_module;
    let wire = &ir.names.wire;
    let arguments = wire_arguments(&ir.generics.original);
    let ident = &field.ident;
    let offset = field_offset_name(ident);
    let base = quote!(<#module::#wire #arguments>::#offset);
    if ir.options.align.is_some() {
        quote!(<#root_wire>::VALUE_OFFSET + #base)
    } else {
        base
    }
}

fn semantic_field_offset(
    ir: &SchemaIr,
    field: &FieldIr,
    runtime: &Path,
    root_wire: &TokenStream,
) -> syn::Result<TokenStream> {
    let offset = compiler_field_offset(ir, field, runtime, root_wire);
    if let Some(align) = &field.options.align {
        let outer_type = analyze::erased_source_type(&field.ty);
        let base = wire_type(&field.category, &outer_type, runtime, field.wire_endian)?;
        let marker = alignment_marker(align.value, align.span)?;
        Ok(
            quote!(#offset + <#runtime::__private::AlignedWire<#base, #runtime::__private::#marker>>::VALUE_OFFSET),
        )
    } else {
        Ok(offset)
    }
}

fn struct_padding_ranges(
    ir: &SchemaIr,
    runtime: &Path,
    support_runtime: &Path,
    root_wire: &TokenStream,
) -> syn::Result<Vec<TokenStream>> {
    let mut ranges = Vec::new();
    let mut previous_end = quote!(0usize);
    for field in &ir.fields {
        let offset = compiler_field_offset(ir, field, support_runtime, root_wire);
        ranges.push(quote!(#runtime::ByteRange::__new(#previous_end, #offset)));
        let field_wire = outer_wire_field_type(field, support_runtime)?;
        if let Some(align) = &field.options.align {
            let outer_type = analyze::erased_source_type(&field.ty);
            let base = wire_type(
                &field.category,
                &outer_type,
                support_runtime,
                field.wire_endian,
            )?;
            let marker = alignment_marker(align.value, align.span)?;
            let value_offset = quote!(<#support_runtime::__private::AlignedWire<#base, #support_runtime::__private::#marker>>::VALUE_OFFSET);
            ranges.push(quote!(#runtime::ByteRange::__new(#offset, #offset + #value_offset)));
            ranges.push(quote!(#runtime::ByteRange::__new(
                #offset + #value_offset + ::core::mem::size_of::<#base>(),
                #offset + ::core::mem::size_of::<#field_wire>(),
            )));
        }
        previous_end = quote!(#offset + ::core::mem::size_of::<#field_wire>());
    }
    ranges.push(
        quote!(#runtime::ByteRange::__new(#previous_end, ::core::mem::size_of::<#root_wire>())),
    );
    Ok(ranges)
}

fn struct_abi_assertions(
    ir: &SchemaIr,
    support_runtime: &Path,
    root_wire: &TokenStream,
) -> syn::Result<Vec<TokenStream>> {
    let mut assertions = Vec::new();
    for field in &ir.fields {
        let offset = compiler_field_offset(ir, field, support_runtime, root_wire);
        let field_wire = outer_wire_field_type(field, support_runtime)?;
        assertions.push(quote!(assert!(
            #offset + ::core::mem::size_of::<#field_wire>() <= ::core::mem::size_of::<#root_wire>(),
            "compiler-derived field range exceeds its root wire"
        );));
        if let FieldCategory::Array { element, length } = &field.category {
            let Type::Array(array) = &field.ty else {
                return Err(Error::new(
                    field.span,
                    "internal zero-schema array analysis mismatch",
                ));
            };
            let element_outer = analyze::erased_source_type(&array.elem);
            let element_wire =
                wire_type(element, &element_outer, support_runtime, field.wire_endian)?;
            let length = length.expression();
            assertions.push(quote!(assert!(
                ::core::mem::size_of::<[#element_wire; #length]>()
                    == ::core::mem::size_of::<#element_wire>() * #length,
                "array wire stride must be compiler-derived"
            );));
        }
    }
    Ok(assertions)
}

fn field_metadata_kind(
    ir: &SchemaIr,
    field: &FieldIr,
    runtime: &Path,
    support_runtime: &Path,
    root_wire: &TokenStream,
) -> syn::Result<TokenStream> {
    let endian = field.wire_endian.runtime_variant();
    Ok(match &field.category {
        FieldCategory::Primitive(primitive) => {
            let primitive = primitive.runtime_variant();
            quote!(#runtime::FieldKind::Primitive { primitive: #runtime::PrimitiveKind::#primitive, endian: #runtime::Endian::#endian })
        }
        FieldCategory::Bool => quote!(#runtime::FieldKind::Bool),
        FieldCategory::BorrowedStr {
            capacity,
            len_type,
            endian,
            ..
        } => {
            let length = length_wire(len_type, *endian)?;
            let repr = length_repr(len_type);
            let endian = endian.runtime_variant();
            quote!(#runtime::FieldKind::String(#runtime::StringDescriptor::__new(
                #runtime::StringEncoding::Utf8,
                #capacity,
                Some(#runtime::LengthDescriptor::__new(#runtime::LengthRepr::#repr, #runtime::Endian::#endian, 0)),
                #support_runtime::__private::StrWire::<#support_runtime::__private::#length, #capacity>::DATA_OFFSET,
            )))
        }
        FieldCategory::BorrowedCStr { capacity, .. } => {
            quote!(#runtime::FieldKind::String(#runtime::StringDescriptor::__new(
                #runtime::StringEncoding::CBytes, #capacity, None,
                #support_runtime::__private::CStrWire::<#capacity>::DATA_OFFSET,
            )))
        }
        FieldCategory::BorrowedU16Str {
            capacity,
            len_type,
            endian,
            ..
        } => {
            let length = length_wire(len_type, *endian)?;
            let repr = length_repr(len_type);
            let endian = endian.runtime_variant();
            quote!(#runtime::FieldKind::String(#runtime::StringDescriptor::__new(
                #runtime::StringEncoding::U16,
                #capacity,
                Some(#runtime::LengthDescriptor::__new(#runtime::LengthRepr::#repr, #runtime::Endian::#endian, 0)),
                #support_runtime::__private::U16StrWire::<#support_runtime::__private::#length, #capacity>::DATA_OFFSET,
            )))
        }
        FieldCategory::BorrowedU16CStr { capacity, .. } => {
            quote!(#runtime::FieldKind::String(#runtime::StringDescriptor::__new(
                #runtime::StringEncoding::U16C, #capacity, None,
                #support_runtime::__private::U16CStrWire::<#capacity>::DATA_OFFSET,
            )))
        }
        FieldCategory::FixedBytes { length, .. } => {
            let length = length.expression();
            quote!(#runtime::FieldKind::FixedBytes { length: #length })
        }
        FieldCategory::Path { tagged: false, .. } => {
            let ty = analyze::erased_source_type(&field.ty);
            quote!(match <#ty as #runtime::__private::WireType>::LAYOUT.kind() {
                #runtime::TypeKind::ScalarEnum { .. } => #runtime::FieldKind::ScalarEnum {
                    layout: <#ty as #runtime::__private::WireType>::LAYOUT,
                },
                _ => #runtime::FieldKind::Schema {
                    layout: <#ty as #runtime::__private::WireType>::LAYOUT,
                },
            })
        }
        FieldCategory::Path {
            tagged: true,
            tag_field: Some(tag_index),
        } => {
            let payload = analyze::erased_source_type(&field.ty);
            let tag = &ir.fields[*tag_index];
            let tag_ty = analyze::erased_source_type(&tag.ty);
            let tag_name = &tag.logical_name;
            let tag_offset = semantic_field_offset(ir, tag, support_runtime, root_wire)?;
            quote!(#runtime::FieldKind::ExternalTaggedUnion {
                payload: <#payload as #runtime::__private::TaggedPayloadTypeSupport>::LAYOUT,
                tag: #runtime::ExternalTagDescriptor::__new(
                    #tag_name,
                    #tag_offset,
                    <#tag_ty as #runtime::__private::WireType>::LAYOUT,
                ),
            })
        }
        FieldCategory::Path { tagged: true, .. } => unreachable!("linked tagged field"),
        FieldCategory::Array { element, length } => {
            let Type::Array(array) = &field.ty else {
                unreachable!("array category carries an array type")
            };
            let element_wire = wire_type(
                element,
                &analyze::erased_source_type(&array.elem),
                support_runtime,
                field.wire_endian,
            )?;
            let element_kind = match element.as_ref() {
                FieldCategory::Primitive(primitive) => {
                    let primitive = primitive.runtime_variant();
                    quote!(#runtime::ArrayElementKind::Primitive { primitive: #runtime::PrimitiveKind::#primitive, endian: #runtime::Endian::#endian })
                }
                FieldCategory::Bool => quote!(#runtime::ArrayElementKind::Bool),
                FieldCategory::Path { .. } => {
                    let ty = analyze::erased_source_type(&array.elem);
                    quote!(match <#ty as #runtime::__private::WireType>::LAYOUT.kind() {
                        #runtime::TypeKind::ScalarEnum { .. } => #runtime::ArrayElementKind::ScalarEnum {
                            layout: <#ty as #runtime::__private::WireType>::LAYOUT,
                        },
                        _ => #runtime::ArrayElementKind::Schema {
                            layout: <#ty as #runtime::__private::WireType>::LAYOUT,
                        },
                    })
                }
                _ => unreachable!("array analysis restricts element categories"),
            };
            let length = length.expression();
            quote!(#runtime::FieldKind::Array(#runtime::ArrayDescriptor::__new(
                #element_kind, #length, ::core::mem::size_of::<#element_wire>(),
            )))
        }
        FieldCategory::Optional {
            inner, inner_ty, ..
        } => {
            let mut inner_field = field.clone();
            inner_field.ty = (**inner_ty).clone();
            inner_field.support_ty = analyze::support_type(inner_ty);
            inner_field.category = (**inner).clone();
            inner_field.options.align = None;
            field_metadata_kind(ir, &inner_field, runtime, support_runtime, root_wire)?
        }
    })
}

fn length_repr(ident: &Ident) -> Ident {
    let value = match ident.to_string().as_str() {
        "u8" => "U8",
        "u16" => "U16",
        "u32" => "U32",
        _ => unreachable!("validated string length representation"),
    };
    Ident::new(value, ident.span())
}

fn nonzero_assertions_category(category: &FieldCategory) -> Vec<TokenStream> {
    match category {
        FieldCategory::FixedBytes {
            length: ArrayLength::Symbolic(length),
            ..
        }
        | FieldCategory::Array {
            length: ArrayLength::Symbolic(length),
            ..
        } => vec![quote!(assert!(#length > 0, "zero-length schema arrays are unsupported");)],
        FieldCategory::Array { element, .. } => nonzero_assertions_category(element),
        FieldCategory::Optional { inner, .. } => nonzero_assertions_category(inner),
        _ => Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analyze;

    #[test]
    fn tagged_payload_emission_is_valid_rust() {
        let item: syn::Item = syn::parse_quote! {
            pub enum Payload<'a> {
                #[zero(tag = Tag::Unit)]
                Unit,
                #[zero(tag = Tag::Data)]
                Data(Child<'a>),
            }
        };
        let ir = analyze::analyze(TokenStream::new(), item).unwrap();
        let emitted = emit(&ir).unwrap();
        syn::parse2::<syn::File>(emitted).unwrap();
    }
}
