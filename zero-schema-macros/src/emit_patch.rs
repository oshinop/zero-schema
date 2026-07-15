use proc_macro2::TokenStream;
use quote::{format_ident, quote};
use syn::{GenericParam, Generics, Lifetime, Path, Type, WherePredicate, parse_quote};

use crate::{
    analyze::{self, FieldCategory, FieldIr, SchemaIr, VariantShape},
    emit_access, emit_mutation, emit_wire,
};

/// A projected child patch needs a source lifetime even if the logical parent
/// has no declared source lifetime of its own.
pub(crate) fn needs_generated_patch_lifetime(ir: &SchemaIr) -> bool {
    ir.generics.original.lifetimes().next().is_none()
        && (ir
            .fields
            .iter()
            .any(|field| matches!(field.category, FieldCategory::Path { .. }))
            || ir
                .variants
                .iter()
                .any(|variant| matches!(variant.shape, VariantShape::Newtype(_))))
}

fn generated_patch_source_lifetime(ir: &SchemaIr) -> Lifetime {
    analyze::fresh_generated_lifetime(ir, "__zero_schema_patch_source")
}

pub(crate) fn patch_generics(ir: &SchemaIr) -> Generics {
    let mut generics = emit_wire::source_independent_generics(&ir.generics.original);
    if needs_generated_patch_lifetime(ir) {
        let source = generated_patch_source_lifetime(ir);
        generics.params.insert(0, parse_quote!(#source));
    }
    generics
}

fn patch_from_generics(ir: &SchemaIr) -> Generics {
    let mut generics = ir.generics.original.clone();
    if needs_generated_patch_lifetime(ir) {
        let source = generated_patch_source_lifetime(ir);
        generics.params.insert(0, parse_quote!(#source));
    }
    generics
}

pub(crate) fn patch_source_lifetime(ir: &SchemaIr) -> TokenStream {
    ir.generics
        .original
        .lifetimes()
        .next()
        .map(|parameter| {
            let lifetime = &parameter.lifetime;
            quote!(#lifetime)
        })
        .or_else(|| {
            needs_generated_patch_lifetime(ir).then(|| {
                let source = generated_patch_source_lifetime(ir);
                quote!(#source)
            })
        })
        .unwrap_or_else(|| quote!('static))
}

pub(crate) fn patch_projection_arguments(ir: &SchemaIr, source: TokenStream) -> TokenStream {
    let mut arguments = Vec::new();
    if needs_generated_patch_lifetime(ir) {
        arguments.push(source.clone());
    }
    arguments.extend(
        ir.generics
            .original
            .params
            .iter()
            .map(|parameter| match parameter {
                GenericParam::Lifetime(_) => source.clone(),
                GenericParam::Type(parameter) => {
                    let ident = &parameter.ident;
                    quote!(#ident)
                }
                GenericParam::Const(parameter) => {
                    let ident = &parameter.ident;
                    quote!(#ident)
                }
            }),
    );
    if arguments.is_empty() {
        TokenStream::new()
    } else {
        quote!(<#(#arguments),*>)
    }
}
/// Emits record patch storage plus its two-pass `SchemaPatch` implementation.
pub(crate) fn emit_record_patch(
    ir: &SchemaIr,
    runtime: &Path,
    support_runtime: &Path,
) -> syn::Result<TokenStream> {
    let patch = &ir.names.patch;
    let support = format_ident!("{}Support", ir.logical_name);
    let root_wire = root_wire(ir, support_runtime);
    let storage_generics = record_patch_generics(ir, runtime);
    let mut patch_impl_generics = storage_generics.clone();
    add_predicates(
        &mut patch_impl_generics,
        emit_access::record_support_predicates(ir, runtime),
    );
    let (impl_generics, patch_args, where_clause) = storage_generics.split_for_impl();
    let (patch_impl_generics, _, patch_impl_where_clause) = patch_impl_generics.split_for_impl();
    let plain_args = emit_wire::wire_arguments(&ir.generics.original);
    let fields = ir
        .fields
        .iter()
        .map(|field| {
            let name = &field.ident;
            let ty = patch_field_type(ir, field, runtime)?;
            Ok(quote!(pub #name: Option<#ty>))
        })
        .collect::<syn::Result<Vec<_>>>()?;
    let defaults = ir.fields.iter().map(|field| {
        let name = &field.ident;
        quote!(#name: None)
    });
    let moved = ir.fields.iter().map(|field| {
        let name = &field.ident;
        match &field.category {
            FieldCategory::Path { tagged: false, .. }
                if !emit_mutation::is_external_tag_sibling(ir, field.declaration_index) =>
            {
                quote!(#name: Some(value.#name.into()))
            }
            FieldCategory::Path { tagged: true, .. } => quote!(#name: Some(value.#name.into())),
            FieldCategory::Optional { inner, .. } => match inner.as_ref() {
                FieldCategory::Path { .. } => {
                    quote!(#name: Some(value.#name.map(::core::convert::Into::into)))
                }
                FieldCategory::Array { .. } => quote!(#name: Some(value.#name)),
                _ => unreachable!("optional analysis accepts only path values or path arrays"),
            },
            _ => quote!(#name: Some(value.#name)),
        }
    });
    let complete = ir.fields.iter().map(|field| {
        let name = &field.ident;
        match &field.category {
            FieldCategory::Path { tagged: false, .. } if !emit_mutation::is_external_tag_sibling(ir, field.declaration_index) => {
                let child = &field.support_ty;
                let child_support = quote!(<#child as #runtime::__private::WireTypeSupport>::Support);
                let patch = schema_patch_type(ir, child, runtime);
                quote!(self.#name.as_ref().is_some_and(|patch| <#patch as #runtime::__private::SchemaPatch<#child_support>>::is_complete(patch)))
            }
            FieldCategory::Path { tagged: true, .. } => {
                let payload = &field.support_ty;
                let payload_support = quote!(<#payload as #runtime::__private::TaggedPayloadTypeSupport>::Support);
                let patch = tagged_patch_type(ir, payload, runtime);
                quote!(self.#name.as_ref().is_some_and(|patch| <#patch as #runtime::__private::TaggedPayloadPatch<#payload_support>>::is_complete(patch)))
            }
            FieldCategory::Optional { inner, inner_support_ty, .. } => match inner.as_ref() {
                FieldCategory::Path { .. } => {
                    let child_support = quote!(<#inner_support_ty as #runtime::__private::WireTypeSupport>::Support);
                    let patch = schema_patch_type(ir, inner_support_ty, runtime);
                    quote!(self.#name.as_ref().is_some_and(|value| value.as_ref().is_none_or(|patch| <#patch as #runtime::__private::SchemaPatch<#child_support>>::is_complete(patch))))
                }
                FieldCategory::Array { .. } => quote!(self.#name.is_some()),
                _ => unreachable!("optional analysis accepts only path values or path arrays"),
            },
            _ => quote!(self.#name.is_some()),
        }
    });
    let preflight = record_preflight(ir, runtime, support_runtime, false)?;
    let preflight_init = record_preflight(ir, runtime, support_runtime, true)?;
    let commit = record_commit(ir, runtime, support_runtime, false)?;
    let commit_init = record_commit(ir, runtime, support_runtime, true)?;
    let logical = logical_type(ir);
    let from_generics = record_from_generics(ir, runtime);
    let (from_impl_generics, _, from_where_clause) = from_generics.split_for_impl();
    Ok(quote!(
        pub struct #patch #impl_generics #where_clause { #(#fields,)* }
        impl #impl_generics Default for #patch #patch_args #where_clause { fn default() -> Self { Self { #(#defaults,)* } } }
        impl #from_impl_generics From<#logical> for #patch #patch_args #from_where_clause { fn from(value: #logical) -> Self { Self { #(#moved,)* } } }
        impl #impl_generics #patch #patch_args #where_clause {
            pub(crate) fn __zero_schema_is_complete(&self) -> bool { true #(&& #complete)* }
        }
        impl #patch_impl_generics #runtime::__private::SchemaPatch<#support #plain_args> for #patch #patch_args #patch_impl_where_clause {
            fn is_complete(&self) -> bool { self.__zero_schema_is_complete() }
            fn preflight<'wire>(&self, input: #runtime::__private::SharedInput<'wire, #root_wire>) -> ::core::result::Result<(), <<#support #plain_args as #runtime::__private::SchemaSupport>::Owner as #runtime::__private::OwnerAdapter>::MutationError> {
                #preflight
                Ok(())
            }
            fn commit<'wire>(&self, input: #runtime::__private::ExclusiveInput<'wire, #root_wire>, token: <#support #plain_args as #runtime::__private::InputAccess>::Token) {
                #commit
            }
            fn preflight_init<'wire>(&self, input: #runtime::__private::SharedInput<'wire, #root_wire>) -> ::core::result::Result<(), <<#support #plain_args as #runtime::__private::SchemaSupport>::Owner as #runtime::__private::OwnerAdapter>::MutationError> {
                #preflight_init
                Ok(())
            }
            fn commit_init<'wire>(&self, input: #runtime::__private::ExclusiveInput<'wire, #root_wire>, token: <#support #plain_args as #runtime::__private::InputAccess>::Token) {
                #commit_init
            }
        }
    ))
}

/// Emits tagged payload patches.  These patches intentionally own payload-only
/// work; external tag storage stays with the containing record patch.
pub(crate) fn emit_tagged_patch(
    ir: &SchemaIr,
    runtime: &Path,
    _support_runtime: &Path,
) -> syn::Result<TokenStream> {
    let patch = &ir.names.patch;
    let support = format_ident!("{}Support", ir.logical_name);
    let wire = &ir.names.wire;
    let storage_generics = tagged_patch_generics(ir, runtime);
    let (patch_impl_generics, _, patch_impl_where_clause) = storage_generics.split_for_impl();
    let (impl_generics, patch_args, where_clause) = storage_generics.split_for_impl();
    let plain_args = emit_wire::wire_arguments(&ir.generics.original);
    let variants = ir
        .variants
        .iter()
        .map(|variant| {
            let name = &variant.ident;
            match &variant.shape {
                VariantShape::Unit => Ok(quote!(#name)),
                VariantShape::Newtype(ty) => {
                    let support_ty = analyze::support_type(ty);
                    let patch_ty = schema_patch_type(ir, &support_ty, runtime);
                    Ok(quote!(#name(#patch_ty)))
                }
            }
        })
        .collect::<syn::Result<Vec<_>>>()?;
    let ident = &ir.ident;
    let from_arms = ir
        .variants
        .iter()
        .map(|variant| {
            let name = &variant.ident;
            match variant.shape {
                VariantShape::Unit => quote!(super::#ident::#name => Self::#name),
                VariantShape::Newtype(_) => {
                    quote!(super::#ident::#name(value) => Self::#name(value.into()))
                }
            }
        })
        .collect::<Vec<_>>();
    let tags = ir.variants.iter().map(|variant| {
        let name = &variant.ident;
        let tag = &variant.tag;
        quote!(Self::#name { .. } => #tag)
    });
    // Tuple patterns need distinct arms; unit/newtype handling is generated below.
    let tag_arms = ir.variants.iter().map(|variant| {
        let name = &variant.ident;
        let tag = &variant.tag;
        match variant.shape {
            VariantShape::Unit => quote!(Self::#name => #tag),
            VariantShape::Newtype(_) => quote!(Self::#name(_) => #tag),
        }
    });
    let complete_arms = ir.variants.iter().map(|variant| {
        let name = &variant.ident;
        match &variant.shape {
            VariantShape::Unit => quote!(Self::#name => true),
            VariantShape::Newtype(ty) => {
                let support_ty = analyze::support_type(ty);
                let child_support = quote!(<#support_ty as #runtime::__private::WireTypeSupport>::Support);
                let patch_ty = schema_patch_type(ir, &support_ty, runtime);
                quote!(Self::#name(patch) => <#patch_ty as #runtime::__private::SchemaPatch<#child_support>>::is_complete(patch))
            }
        }
    });
    let preflight_arms = ir.variants.iter().map(|variant| {
        let name = &variant.ident;
        match &variant.shape {
            VariantShape::Unit => Ok(quote!(Self::#name => Ok(()))),
            VariantShape::Newtype(ty) => {
                let support_ty = analyze::support_type(ty);
                let patch_ty = schema_patch_type(ir, &support_ty, runtime);
                let child_support = quote!(<#support_ty as #runtime::__private::WireTypeSupport>::Support);
                let child_wire = quote!(<#support_ty as #runtime::__private::WireType>::Wire);
                Ok(quote!(Self::#name(patch) => {
                    let child = payload.subrange::<#child_wire>(0).map_err(<<#support #plain_args as #runtime::__private::TaggedPayloadSupport>::Owner as #runtime::__private::OwnerAdapter>::mutation_layout)?;
                    <#patch_ty as #runtime::__private::SchemaPatch<#child_support>>::preflight(patch, child)
                        .map_err(|_| <<#support #plain_args as #runtime::__private::TaggedPayloadSupport>::Owner as #runtime::__private::OwnerAdapter>::mutation_layout(#runtime::LayoutError::OffsetOverflow))
                }))
            }
        }
    }).collect::<syn::Result<Vec<_>>>()?;
    let commit_arms = ir.variants.iter().map(|variant| {
        let name = &variant.ident;
        match &variant.shape {
            VariantShape::Unit => Ok(quote!(Self::#name => ())),
            VariantShape::Newtype(ty) => {
                let support_ty = analyze::support_type(ty);
                let patch_ty = schema_patch_type(ir, &support_ty, runtime);
                let child_support = quote!(<#support_ty as #runtime::__private::WireTypeSupport>::Support);
                let child_wire = quote!(<#support_ty as #runtime::__private::WireType>::Wire);
                Ok(quote!(Self::#name(patch) => {
                    let child = match payload.subrange_mut::<#child_wire>(0) { Ok(child) => child, Err(_) => unreachable!("preflighted variant payload remains selectable") };
                    let child_token = <#child_support as #runtime::__private::SchemaSupport>::input_token(&child);
                    <#patch_ty as #runtime::__private::SchemaPatch<#child_support>>::commit(patch, child, child_token);
                }))
            }
        }
    }).collect::<syn::Result<Vec<_>>>()?;
    let init_preflight_arms = ir.variants.iter().map(|variant| {
        let name = &variant.ident;
        match &variant.shape {
            VariantShape::Unit => Ok(quote!(Self::#name => Ok(()))),
            VariantShape::Newtype(ty) => {
                let support_ty = analyze::support_type(ty);
                let patch_ty = schema_patch_type(ir, &support_ty, runtime);
                let child_support = quote!(<#support_ty as #runtime::__private::WireTypeSupport>::Support);
                let child_wire = quote!(<#support_ty as #runtime::__private::WireType>::Wire);
                Ok(quote!(Self::#name(patch) => {
                    let child = payload.subrange::<#child_wire>(0).map_err(<<#support #plain_args as #runtime::__private::TaggedPayloadSupport>::Owner as #runtime::__private::OwnerAdapter>::mutation_layout)?;
                    <#patch_ty as #runtime::__private::SchemaPatch<#child_support>>::preflight_init(patch, child)
                        .map_err(|_| <<#support #plain_args as #runtime::__private::TaggedPayloadSupport>::Owner as #runtime::__private::OwnerAdapter>::mutation_layout(#runtime::LayoutError::OffsetOverflow))
                }))
            }
        }
    }).collect::<syn::Result<Vec<_>>>()?;
    let init_commit_arms = ir.variants.iter().map(|variant| {
        let name = &variant.ident;
        match &variant.shape {
            VariantShape::Unit => Ok(quote!(Self::#name => ())),
            VariantShape::Newtype(ty) => {
                let support_ty = analyze::support_type(ty);
                let patch_ty = schema_patch_type(ir, &support_ty, runtime);
                let child_support = quote!(<#support_ty as #runtime::__private::WireTypeSupport>::Support);
                let child_wire = quote!(<#support_ty as #runtime::__private::WireType>::Wire);
                Ok(quote!(Self::#name(patch) => {
                    let child = match payload.subrange_mut::<#child_wire>(0) { Ok(child) => child, Err(_) => unreachable!("preflighted initialized variant payload remains selectable") };
                    let child_token = <#child_support as #runtime::__private::SchemaSupport>::input_token(&child);
                    <#patch_ty as #runtime::__private::SchemaPatch<#child_support>>::commit_init(patch, child, child_token);
                }))
            }
        }
    }).collect::<syn::Result<Vec<_>>>()?;
    let _ = tags;
    let logical = logical_type(ir);
    let from_generics = tagged_from_generics(ir, runtime);
    let (from_impl_generics, _, from_where_clause) = from_generics.split_for_impl();
    Ok(quote!(
        pub enum #patch #impl_generics #where_clause { #(#variants,)* }
        impl #from_impl_generics From<#logical> for #patch #patch_args #from_where_clause { fn from(value: #logical) -> Self { match value { #(#from_arms,)* } } }
        impl #impl_generics #patch #patch_args #where_clause { pub(crate) fn __zero_schema_is_complete(&self) -> bool { match self { #(#complete_arms,)* } } }
        impl #patch_impl_generics #runtime::__private::TaggedPayloadPatch<#support #plain_args> for #patch #patch_args #patch_impl_where_clause {
            fn tag(&self) -> <#support #plain_args as #runtime::__private::TaggedPayloadSupport>::Tag { match self { #(#tag_arms,)* } }
            fn is_complete(&self) -> bool { self.__zero_schema_is_complete() }
            fn preflight<'wire>(&self, _: <#support #plain_args as #runtime::__private::TaggedPayloadSupport>::Tag, payload: #runtime::__private::SharedInput<'wire, #wire #plain_args>) -> ::core::result::Result<(), <<#support #plain_args as #runtime::__private::TaggedPayloadSupport>::Owner as #runtime::__private::OwnerAdapter>::MutationError> { match self { #(#preflight_arms,)* } }
            fn commit<'wire>(&self, mut payload: #runtime::__private::ExclusiveInput<'wire, #wire #plain_args>, token: <#support #plain_args as #runtime::__private::InputAccess>::Token) { let _ = token; match self { #(#commit_arms,)* } }
            fn preflight_init<'wire>(&self, payload: #runtime::__private::SharedInput<'wire, #wire #plain_args>) -> ::core::result::Result<(), <<#support #plain_args as #runtime::__private::TaggedPayloadSupport>::Owner as #runtime::__private::OwnerAdapter>::MutationError> { match self { #(#init_preflight_arms,)* } }
            fn commit_init<'wire>(&self, mut payload: #runtime::__private::ExclusiveInput<'wire, #wire #plain_args>, token: <#support #plain_args as #runtime::__private::InputAccess>::Token) { let _ = token; match self { #(#init_commit_arms,)* } }
        }
    ))
}

fn record_preflight(
    ir: &SchemaIr,
    runtime: &Path,
    support_runtime: &Path,
    initializing: bool,
) -> syn::Result<TokenStream> {
    let mutation = &ir.names.mutation_error;
    let access_error = &ir.names.access_error;
    let owner = emit_access::owner_name(ir);
    let owner_args = emit_wire::wire_arguments(&ir.generics.original);
    let nested_preflight = if initializing {
        quote!(preflight_init)
    } else {
        quote!(preflight)
    };
    let array_preflight = if initializing {
        quote!(preflight_init)
    } else {
        quote!(preflight)
    };
    let mut statements = Vec::new();
    for field in &ir.fields {
        if emit_mutation::is_external_tag_sibling(ir, field.declaration_index) {
            continue;
        }
        let name = &field.ident;
        let field_name = &field.logical_name;
        let offset = emit_access::field_value_offset(ir, field, support_runtime)?;
        match &field.category {
            FieldCategory::Primitive(_) | FieldCategory::Bool => {}
            FieldCategory::BorrowedStr { .. }
            | FieldCategory::BorrowedCStr { .. }
            | FieldCategory::BorrowedU16Str { .. }
            | FieldCategory::BorrowedU16CStr { .. } => {
                let adapter = format_ident!("{}Adapter", pascal(field_name));
                statements.push(quote!(if let Some(value) = self.#name.as_ref() { let selected = input.subrange(#offset).map_err(<#owner #owner_args as #runtime::__private::OwnerAdapter>::mutation_layout)?; <#adapter #owner_args as #runtime::__private::StringMutationAdapter>::preflight(selected, value)?; }));
            }
            FieldCategory::FixedBytes { .. } => {
                let adapter = format_ident!("{}Adapter", pascal(field_name));
                statements.push(quote!(if let Some(value) = self.#name.as_ref() { <#adapter #owner_args as #runtime::__private::FixedBytesMutationAdapter>::preflight(*value)?; }));
            }
            FieldCategory::Array { element, .. } => {
                let adapter = format_ident!("{}ArrayAdapter", pascal(field_name));
                let Type::Array(array) = &field.ty else {
                    return Err(syn::Error::new_spanned(&field.ty, "array type mismatch"));
                };
                let element_wire = emit_wire::wire_type(
                    element,
                    &analyze::support_type(&array.elem),
                    support_runtime,
                    field.wire_endian,
                )?;
                let length = emit_wire::array_length(&field.support_ty)?;
                statements.push(quote!(if let Some(values) = self.#name.as_ref() {
                    let selected = input.subrange::<[#element_wire; #length]>(#offset).map_err(<#owner #owner_args as #runtime::__private::OwnerAdapter>::mutation_layout)?;
                    for (index, value) in values.iter().enumerate() {
                        let element_offset = #runtime::__private::checked_element_offset(index, <#adapter #owner_args as #runtime::__private::ArrayElementAdapter>::STRIDE).map_err(<#owner #owner_args as #runtime::__private::OwnerAdapter>::mutation_layout)?;
                        let element = selected.subrange::<#element_wire>(element_offset).map_err(<#owner #owner_args as #runtime::__private::OwnerAdapter>::mutation_layout)?;
                        <#adapter #owner_args as #runtime::__private::ArrayElementAdapter>::#array_preflight(index, element, value)?;
                    }
                }));
            }
            FieldCategory::Path { tagged: false, .. } => {
                let child_ty = &field.support_ty;
                let patch_ty = schema_patch_type(ir, child_ty, runtime);
                let child_support =
                    quote!(<#child_ty as #runtime::__private::WireTypeSupport>::Support);
                let child_wire = emit_wire::wire_type(
                    &field.category,
                    &field.support_ty,
                    support_runtime,
                    field.wire_endian,
                )?;
                statements.push(quote!(if let Some(patch) = self.#name.as_ref() { let child = input.subrange::<#child_wire>(#offset).map_err(<#owner #owner_args as #runtime::__private::OwnerAdapter>::mutation_layout)?; if <#patch_ty as #runtime::__private::SchemaPatch<#child_support>>::#nested_preflight(patch, child).is_err() { return Err(#mutation::field_kind(#field_name, #runtime::ErrorKind::CapacityExceeded)); } }));
            }
            FieldCategory::Path {
                tagged: true,
                tag_field: Some(tag_index),
            } => {
                let tag_field = &ir.fields[*tag_index];
                let tag_name = &tag_field.ident;
                let tag_ty = &tag_field.support_ty;
                let tag_wire = emit_wire::wire_type(
                    &tag_field.category,
                    tag_ty,
                    support_runtime,
                    tag_field.wire_endian,
                )?;
                let tag_offset = emit_access::field_value_offset(ir, tag_field, support_runtime)?;
                let tag_variant = emit_access::error_child_variant(tag_field);
                let payload_ty = &field.support_ty;
                let payload_support =
                    quote!(<#payload_ty as #runtime::__private::TaggedPayloadTypeSupport>::Support);
                let payload_wire = emit_wire::wire_type(
                    &field.category,
                    &field.support_ty,
                    support_runtime,
                    field.wire_endian,
                )?;
                statements.push(quote!(match (self.#tag_name.as_ref(), self.#name.as_ref()) {
                    (None, None) => (),
                    (Some(_), None) => return Err(#mutation::field_kind(#field_name, #runtime::ErrorKind::TagOnlyPatch)),
                    (tag, Some(patch)) => {
                        if let Some(tag) = tag { if *tag != <#payload_support as #runtime::__private::TaggedPayloadSupport>::patch_tag(patch) { return Err(#mutation::field_kind(#field_name, #runtime::ErrorKind::TagMismatch)); } }
                        let payload = input.subrange::<#payload_wire>(#offset).map_err(<#owner #owner_args as #runtime::__private::OwnerAdapter>::mutation_layout)?;
                        if #initializing {
                            <#payload_support as #runtime::__private::TaggedPayloadSupport>::preflight_patch_init(payload, patch)
                                .map_err(|_| #mutation::field_kind(#field_name, #runtime::ErrorKind::IncompleteUnionSwitch))?;
                        } else {
                            let tag_input = input.subrange::<#tag_wire>(#tag_offset).map_err(<#owner #owner_args as #runtime::__private::OwnerAdapter>::mutation_layout)?;
                            let tag_proof = <#tag_ty as #runtime::__private::WireTypeSupport>::Support::prove(tag_input)
                                .map_err(|source| #mutation::from(#access_error::#tag_variant(source)))?;
                            let current = <#tag_ty as #runtime::__private::WireTypeSupport>::Support::make_ref(tag_proof);
                            if current == <#payload_support as #runtime::__private::TaggedPayloadSupport>::patch_tag(patch) {
                                <#payload_support as #runtime::__private::TaggedPayloadSupport>::preflight_patch(current, payload, patch)
                            } else {
                                <#payload_support as #runtime::__private::TaggedPayloadSupport>::preflight_patch_init(payload, patch)
                            }.map_err(|_| #mutation::field_kind(#field_name, #runtime::ErrorKind::IncompleteUnionSwitch))?;
                        }
                    }
                }));
            }
            FieldCategory::Optional {
                inner,
                inner_support_ty,
                ..
            } => {
                let adapter = option_adapter_name(field);
                let storage_wire = emit_wire::wire_field_type(field, support_runtime)?;
                let storage_offset = emit_access::field_storage_offset(ir, field, support_runtime);
                match inner.as_ref() {
                    FieldCategory::Path { .. } => {
                        let patch_ty = schema_patch_type(ir, inner_support_ty, runtime);
                        let child_support = quote!(<#inner_support_ty as #runtime::__private::WireTypeSupport>::Support);
                        statements.push(quote!(if let Some(update) = self.#name.as_ref() {
                            let storage = input.subrange::<#storage_wire>(#storage_offset).map_err(<#owner #owner_args as #runtime::__private::OwnerAdapter>::mutation_layout)?;
                            if let Some(patch) = update.as_ref() {
                                let child = storage.subrange::<<#adapter #owner_args as #runtime::__private::OptionFieldAdapter>::ValueWire>(<#adapter #owner_args as #runtime::__private::OptionFieldAdapter>::VALUE_OFFSET).map_err(<#owner #owner_args as #runtime::__private::OwnerAdapter>::mutation_layout)?;
                                if #initializing {
                                    if !<#patch_ty as #runtime::__private::SchemaPatch<#child_support>>::is_complete(patch) {
                                        return Err(#mutation::field_kind(#field_name, #runtime::ErrorKind::IncompleteOptionalInitialization));
                                    }
                                    <#patch_ty as #runtime::__private::SchemaPatch<#child_support>>::preflight_init(patch, child).map_err(|_| #mutation::field_kind(#field_name, #runtime::ErrorKind::CapacityExceeded))?;
                                } else if storage.is_all_zero() {
                                    if !<#patch_ty as #runtime::__private::SchemaPatch<#child_support>>::is_complete(patch) {
                                        return Err(#mutation::field_kind(#field_name, #runtime::ErrorKind::IncompleteOptionalInitialization));
                                    }
                                    <#patch_ty as #runtime::__private::SchemaPatch<#child_support>>::preflight_init(patch, child).map_err(|_| #mutation::field_kind(#field_name, #runtime::ErrorKind::CapacityExceeded))?;
                                } else {
                                    <#patch_ty as #runtime::__private::SchemaPatch<#child_support>>::preflight(patch, child).map_err(|_| #mutation::field_kind(#field_name, #runtime::ErrorKind::CapacityExceeded))?;
                                }
                            }
                        }));
                    }
                    FieldCategory::Array { element, .. } => {
                        let Type::Array(array) = inner_support_ty.as_ref() else {
                            return Err(syn::Error::new_spanned(
                                inner_support_ty,
                                "optional array support type mismatch",
                            ));
                        };
                        let element_wire = emit_wire::wire_type(
                            element,
                            &array.elem,
                            support_runtime,
                            field.wire_endian,
                        )?;
                        let array_adapter = array_adapter_name(field);
                        statements.push(quote!(if let Some(update) = self.#name.as_ref() {
                            let storage = input.subrange::<#storage_wire>(#storage_offset).map_err(<#owner #owner_args as #runtime::__private::OwnerAdapter>::mutation_layout)?;
                            if let Some(values) = update.as_ref() {
                                let selected = storage.subrange::<<#adapter #owner_args as #runtime::__private::OptionFieldAdapter>::ValueWire>(<#adapter #owner_args as #runtime::__private::OptionFieldAdapter>::VALUE_OFFSET).map_err(<#owner #owner_args as #runtime::__private::OwnerAdapter>::mutation_layout)?;
                                let absent = !#initializing && storage.is_all_zero();
                                for (index, value) in values.iter().enumerate() {
                                    let element_offset = #runtime::__private::checked_element_offset(index, <#array_adapter #owner_args as #runtime::__private::ArrayElementAdapter>::STRIDE).map_err(<#owner #owner_args as #runtime::__private::OwnerAdapter>::mutation_layout)?;
                                    let element_input = selected.subrange::<#element_wire>(element_offset).map_err(<#owner #owner_args as #runtime::__private::OwnerAdapter>::mutation_layout)?;
                                    if #initializing || absent { <#array_adapter #owner_args as #runtime::__private::ArrayElementAdapter>::preflight_init(index, element_input, value)?; } else { <#array_adapter #owner_args as #runtime::__private::ArrayElementAdapter>::preflight(index, element_input, value)?; }
                                }
                            }
                        }));
                    }
                    _ => unreachable!("optional analysis accepts only path values or path arrays"),
                }
            }
            FieldCategory::Path { tagged: true, .. } => {}
        }
    }
    Ok(quote!(#(#statements)*))
}

fn record_commit(
    ir: &SchemaIr,
    runtime: &Path,
    support_runtime: &Path,
    initializing: bool,
) -> syn::Result<TokenStream> {
    let mut statements = Vec::new();
    let root_wire = root_wire(ir, support_runtime);
    let owner_args = emit_wire::wire_arguments(&ir.generics.original);
    let nested_commit = if initializing {
        quote!(commit_init)
    } else {
        quote!(commit)
    };
    let array_commit = if initializing {
        quote!(commit_init)
    } else {
        quote!(commit)
    };
    for field in &ir.fields {
        if emit_mutation::is_external_tag_sibling(ir, field.declaration_index) {
            continue;
        }
        let name = &field.ident;
        let offset = emit_access::field_value_offset(ir, field, support_runtime)?;
        match &field.category {
            FieldCategory::Primitive(_) | FieldCategory::Bool => {
                let adapter = format_ident!("{}Adapter", pascal(&field.logical_name));
                let wire = emit_wire::wire_type(
                    &field.category,
                    &field.support_ty,
                    support_runtime,
                    field.wire_endian,
                )?;
                statements.push(quote!(if let Some(value) = self.#name {
                    let selected = match input.subrange_mut::<#wire>(#offset) { Ok(selected) => selected, Err(_) => unreachable!("preflighted scalar field remains selectable") };
                    <#adapter #owner_args as #runtime::__private::ScalarMutationAdapter>::commit(selected, value, token);
                }));
            }
            FieldCategory::BorrowedStr { .. }
            | FieldCategory::BorrowedCStr { .. }
            | FieldCategory::BorrowedU16Str { .. }
            | FieldCategory::BorrowedU16CStr { .. } => {
                let adapter = format_ident!("{}Adapter", pascal(&field.logical_name));
                let wire = emit_wire::wire_type(
                    &field.category,
                    &field.support_ty,
                    support_runtime,
                    field.wire_endian,
                )?;
                statements.push(quote!(if let Some(value) = self.#name.as_ref() {
                    let selected = match input.subrange_mut::<#wire>(#offset) { Ok(selected) => selected, Err(_) => unreachable!("preflighted string field remains selectable") };
                    <#adapter #owner_args as #runtime::__private::StringMutationAdapter>::commit(selected, value, token);
                }));
            }
            FieldCategory::FixedBytes { .. } => {
                let adapter = format_ident!("{}Adapter", pascal(&field.logical_name));
                let wire = emit_wire::wire_type(
                    &field.category,
                    &field.support_ty,
                    support_runtime,
                    field.wire_endian,
                )?;
                statements.push(quote!(if let Some(value) = self.#name.as_ref() {
                    let selected = match input.subrange_mut::<#wire>(#offset) { Ok(selected) => selected, Err(_) => unreachable!("preflighted byte field remains selectable") };
                    <#adapter #owner_args as #runtime::__private::FixedBytesMutationAdapter>::commit(selected, *value, token);
                }));
            }
            FieldCategory::Array { element, .. } => {
                let adapter = format_ident!("{}ArrayAdapter", pascal(&field.logical_name));
                let Type::Array(array) = &field.ty else {
                    return Err(syn::Error::new_spanned(&field.ty, "array type mismatch"));
                };
                let element_wire = emit_wire::wire_type(
                    element,
                    &analyze::support_type(&array.elem),
                    support_runtime,
                    field.wire_endian,
                )?;
                let length = emit_wire::array_length(&field.support_ty)?;
                statements.push(quote!(if let Some(values) = self.#name.as_ref() {
                    let mut selected = match input.subrange_mut::<[#element_wire; #length]>(#offset) { Ok(selected) => selected, Err(_) => unreachable!("preflighted array remains selectable") };
                    for (index, value) in values.iter().enumerate() {
                        let element_offset = match #runtime::__private::checked_element_offset(index, <#adapter #owner_args as #runtime::__private::ArrayElementAdapter>::STRIDE) { Ok(offset) => offset, Err(_) => unreachable!("preflighted array index remains representable") };
                        let element = match selected.subrange_mut::<#element_wire>(element_offset) { Ok(element) => element, Err(_) => unreachable!("preflighted array element remains selectable") };
                        <#adapter #owner_args as #runtime::__private::ArrayElementAdapter>::#array_commit(index, element, value, token);
                    }
                }));
            }
            FieldCategory::Path { tagged: false, .. } => {
                let child_ty = &field.support_ty;
                let patch_ty = schema_patch_type(ir, child_ty, runtime);
                let child_support =
                    quote!(<#child_ty as #runtime::__private::WireTypeSupport>::Support);
                let child_wire = emit_wire::wire_type(
                    &field.category,
                    &field.support_ty,
                    support_runtime,
                    field.wire_endian,
                )?;
                statements.push(quote!(if let Some(patch) = self.#name.as_ref() {
                    let child = match input.subrange_mut::<#child_wire>(#offset) { Ok(child) => child, Err(_) => unreachable!("preflighted nested field remains selectable") };
                    let child_token = <#child_support as #runtime::__private::SchemaSupport>::input_token(&child);
                    <#patch_ty as #runtime::__private::SchemaPatch<#child_support>>::#nested_commit(patch, child, child_token);
                }));
            }
            FieldCategory::Optional {
                inner,
                inner_support_ty,
                ..
            } => {
                let adapter = option_adapter_name(field);
                let storage_wire = emit_wire::wire_field_type(field, support_runtime)?;
                let storage_offset = emit_access::field_storage_offset(ir, field, support_runtime);
                match inner.as_ref() {
                    FieldCategory::Path { .. } => {
                        let patch_ty = schema_patch_type(ir, inner_support_ty, runtime);
                        let child_support = quote!(<#inner_support_ty as #runtime::__private::WireTypeSupport>::Support);
                        statements.push(quote!(if let Some(update) = self.#name.as_ref() {
                            let mut storage = match input.subrange_mut::<#storage_wire>(#storage_offset) { Ok(value) => value, Err(_) => unreachable!("preflighted optional storage remains selectable") };
                            match update.as_ref() {
                                None => <#adapter #owner_args as #runtime::__private::OptionFieldAdapter>::clear(storage, token),
                                Some(patch) => {
                                    let absent = !#initializing && storage.shared().is_all_zero();
                                    let child = match storage.subrange_mut::<<#adapter #owner_args as #runtime::__private::OptionFieldAdapter>::ValueWire>(<#adapter #owner_args as #runtime::__private::OptionFieldAdapter>::VALUE_OFFSET) { Ok(value) => value, Err(_) => unreachable!("preflighted optional value remains selectable") };
                                    let child_token = <#child_support as #runtime::__private::SchemaSupport>::input_token(&child);
                                    if #initializing || absent { <#patch_ty as #runtime::__private::SchemaPatch<#child_support>>::commit_init(patch, child, child_token); } else { <#patch_ty as #runtime::__private::SchemaPatch<#child_support>>::commit(patch, child, child_token); }
                                }
                            }
                        }));
                    }
                    FieldCategory::Array { element, .. } => {
                        let Type::Array(array) = inner_support_ty.as_ref() else {
                            return Err(syn::Error::new_spanned(
                                inner_support_ty,
                                "optional array support type mismatch",
                            ));
                        };
                        let element_wire = emit_wire::wire_type(
                            element,
                            &array.elem,
                            support_runtime,
                            field.wire_endian,
                        )?;
                        let array_adapter = array_adapter_name(field);
                        statements.push(quote!(if let Some(update) = self.#name.as_ref() {
                            let mut storage = match input.subrange_mut::<#storage_wire>(#storage_offset) { Ok(value) => value, Err(_) => unreachable!("preflighted optional storage remains selectable") };
                            match update.as_ref() {
                                None => <#adapter #owner_args as #runtime::__private::OptionFieldAdapter>::clear(storage, token),
                                Some(values) => {
                                    let absent = !#initializing && storage.shared().is_all_zero();
                                    let mut selected = match storage.subrange_mut::<<#adapter #owner_args as #runtime::__private::OptionFieldAdapter>::ValueWire>(<#adapter #owner_args as #runtime::__private::OptionFieldAdapter>::VALUE_OFFSET) { Ok(value) => value, Err(_) => unreachable!("preflighted optional value remains selectable") };
                                    for (index, value) in values.iter().enumerate() {
                                        let element_offset = match #runtime::__private::checked_element_offset(index, <#array_adapter #owner_args as #runtime::__private::ArrayElementAdapter>::STRIDE) { Ok(value) => value, Err(_) => unreachable!("preflighted optional array index remains representable") };
                                        let element = match selected.subrange_mut::<#element_wire>(element_offset) { Ok(value) => value, Err(_) => unreachable!("preflighted optional array element remains selectable") };
                                        if #initializing || absent { <#array_adapter #owner_args as #runtime::__private::ArrayElementAdapter>::commit_init(index, element, value, token); } else { <#array_adapter #owner_args as #runtime::__private::ArrayElementAdapter>::commit(index, element, value, token); }
                                    }
                                }
                            }
                        }));
                    }
                    _ => unreachable!("optional analysis accepts only path values or path arrays"),
                }
            }
            FieldCategory::Path {
                tagged: true,
                tag_field: Some(tag_index),
            } => {
                let tag_field = &ir.fields[*tag_index];
                let tag_ty = &tag_field.support_ty;
                let tag_wire = emit_wire::wire_type(
                    &tag_field.category,
                    tag_ty,
                    support_runtime,
                    tag_field.wire_endian,
                )?;
                let tag_offset = emit_access::field_value_offset(ir, tag_field, support_runtime)?;
                let payload_ty = &field.support_ty;
                let payload_support =
                    quote!(<#payload_ty as #runtime::__private::TaggedPayloadTypeSupport>::Support);
                let payload_wire = emit_wire::wire_type(
                    &field.category,
                    &field.support_ty,
                    support_runtime,
                    field.wire_endian,
                )?;
                statements.push(quote!(if let Some(patch) = self.#name.as_ref() {
                    let target = <#payload_support as #runtime::__private::TaggedPayloadSupport>::patch_tag(patch);
                    if #initializing {
                        match #runtime::__private::commit_payload_before_tag_with::<#root_wire, #payload_support, #tag_wire, _, _>(&mut input, #tag_offset, #offset, |payload| {
                            let payload_token = <#payload_support as #runtime::__private::TaggedPayloadSupport>::input_token(&payload);
                            <#payload_support as #runtime::__private::TaggedPayloadSupport>::commit_patch_init(payload, patch, payload_token);
                        }, token, |tag_input| {
                            let tag_token = <<#tag_ty as #runtime::__private::WireTypeSupport>::Support as #runtime::__private::SchemaSupport>::input_token(&tag_input);
                            <#tag_ty as #runtime::__private::WireTypeSupport>::Support::commit(tag_input, target, tag_token);
                        }) { Ok(()) => (), Err(_) => unreachable!("preflighted initialized external-tag ranges remain selectable") }
                    } else {
                        let tag_input = match input.subrange::<#tag_wire>(#tag_offset) { Ok(tag_input) => tag_input, Err(_) => unreachable!("preflighted tag field remains selectable") };
                        let current = match <<#tag_ty as #runtime::__private::WireTypeSupport>::Support as #runtime::__private::ScalarEnumSupport>::from_raw(<<#tag_ty as #runtime::__private::WireTypeSupport>::Support as #runtime::__private::ScalarEnumSupport>::raw(tag_input)) { Some(value) => value, None => unreachable!("preflight validated the current scalar tag") };
                        if target == current {
                            let payload = match input.subrange_mut::<#payload_wire>(#offset) { Ok(payload) => payload, Err(_) => unreachable!("preflighted selected payload remains selectable") };
                            let payload_token = <#payload_support as #runtime::__private::TaggedPayloadSupport>::input_token(&payload);
                            <#payload_support as #runtime::__private::TaggedPayloadSupport>::commit_patch(payload, patch, payload_token);
                        } else {
                            match #runtime::__private::commit_payload_before_tag_with::<#root_wire, #payload_support, #tag_wire, _, _>(&mut input, #tag_offset, #offset, |payload| {
                                let payload_token = <#payload_support as #runtime::__private::TaggedPayloadSupport>::input_token(&payload);
                                <#payload_support as #runtime::__private::TaggedPayloadSupport>::commit_patch_init(payload, patch, payload_token);
                            }, token, |tag_input| {
                                let tag_token = <<#tag_ty as #runtime::__private::WireTypeSupport>::Support as #runtime::__private::SchemaSupport>::input_token(&tag_input);
                                <#tag_ty as #runtime::__private::WireTypeSupport>::Support::commit(tag_input, target, tag_token);
                            }) { Ok(()) => (), Err(_) => unreachable!("preflighted external-tag ranges remain selectable") }
                        }
                    }
                }));
            }
            FieldCategory::Path { tagged: true, .. } => {}
        }
    }
    Ok(quote!(let _ = token; let mut input = input; #(#statements)*))
}

fn patch_field_type(ir: &SchemaIr, field: &FieldIr, runtime: &Path) -> syn::Result<TokenStream> {
    if emit_mutation::is_external_tag_sibling(ir, field.declaration_index) {
        let ty = &field.ty;
        return Ok(quote!(#ty));
    }
    match &field.category {
        FieldCategory::Path { tagged: false, .. } => {
            Ok(schema_patch_type(ir, &field.support_ty, runtime))
        }
        FieldCategory::Path { tagged: true, .. } => {
            Ok(tagged_patch_type(ir, &field.support_ty, runtime))
        }
        FieldCategory::Optional {
            inner,
            inner_ty,
            inner_support_ty,
        } => match inner.as_ref() {
            FieldCategory::Path { .. } => {
                let patch = schema_patch_type(ir, inner_support_ty, runtime);
                Ok(quote!(Option<#patch>))
            }
            FieldCategory::Array { .. } => Ok(quote!(Option<#inner_ty>)),
            _ => unreachable!("optional analysis accepts only path values or path arrays"),
        },
        _ => {
            let ty = &field.ty;
            Ok(quote!(#ty))
        }
    }
}
fn schema_patch_type(ir: &SchemaIr, ty: &Type, runtime: &Path) -> TokenStream {
    let lifetime = patch_source_lifetime(ir);
    quote!(<#ty as #runtime::__private::SchemaPatchType>::Patch<#lifetime>)
}

fn tagged_patch_type(ir: &SchemaIr, ty: &Type, runtime: &Path) -> TokenStream {
    let lifetime = patch_source_lifetime(ir);
    quote!(<#ty as #runtime::__private::TaggedPayloadPatchType>::Patch<#lifetime>)
}

fn root_wire(ir: &SchemaIr, runtime: &Path) -> TokenStream {
    let wire = &ir.names.wire;
    let args = emit_wire::wire_arguments(&ir.generics.original);
    let base = quote!(#wire #args);
    emit_wire::aligned_root_wire(ir, base, runtime)
}

fn logical_type(ir: &SchemaIr) -> TokenStream {
    let ident = &ir.ident;
    let args = ir
        .generics
        .original
        .params
        .iter()
        .map(|param| match param {
            syn::GenericParam::Lifetime(value) => {
                let value = &value.lifetime;
                quote!(#value)
            }
            syn::GenericParam::Type(value) => {
                let value = &value.ident;
                quote!(#value)
            }
            syn::GenericParam::Const(value) => {
                let value = &value.ident;
                quote!(#value)
            }
        })
        .collect::<Vec<_>>();
    if args.is_empty() {
        quote!(super::#ident)
    } else {
        quote!(super::#ident<#(#args),*>)
    }
}

fn record_patch_generics(ir: &SchemaIr, runtime: &Path) -> Generics {
    let mut generics = patch_generics(ir);
    emit_wire::add_optional_wire_type_bounds(&mut generics, &ir.fields, runtime);
    add_patch_type_bounds(&mut generics, record_patch_dependencies(ir), runtime);
    add_record_tag_bounds(ir, runtime, &mut generics);
    generics
}

fn add_record_tag_bounds(ir: &SchemaIr, runtime: &Path, generics: &mut Generics) {
    let tag_lifetime = analyze::fresh_generated_lifetime(ir, "__zero_schema_tag_logical");
    let tag_bounds = ir.fields.iter().filter_map(|field| match field.category {
        FieldCategory::Path { tagged: true, tag_field: Some(tag_index) } => {
            let payload = &field.support_ty;
            let tag = &ir.fields[tag_index].support_ty;
            let rebased = analyze::logical_source_type(&field.ty);
            let logical = analyze::rebind_ir_source_lifetimes(ir, &rebased, tag_lifetime.clone());
            Some(parse_quote!(for<#tag_lifetime> #payload: #runtime::__private::TaggedPayloadTypeSupport<Tag = #tag, Logical<#tag_lifetime> = #logical> + 'static))
        }
        _ => None,
    }).collect::<Vec<WherePredicate>>();
    add_predicates(generics, tag_bounds);
}

fn tagged_patch_generics(ir: &SchemaIr, runtime: &Path) -> Generics {
    let mut generics = patch_generics(ir);
    add_patch_type_bounds(&mut generics, tagged_patch_dependencies(ir), runtime);
    generics
}

fn record_from_generics(ir: &SchemaIr, runtime: &Path) -> Generics {
    let mut generics = patch_from_generics(ir);
    add_patch_type_bounds(&mut generics, record_patch_dependencies(ir), runtime);
    emit_wire::add_optional_wire_type_bounds(&mut generics, &ir.fields, runtime);
    add_record_tag_bounds(ir, runtime, &mut generics);
    let predicates = ir
        .fields
        .iter()
        .filter_map(|field| {
            if emit_mutation::is_external_tag_sibling(ir, field.declaration_index) {
                return None;
            }
            match &field.category {
                FieldCategory::Path { tagged: false, .. } => {
                    let support_ty = &field.support_ty;
                    let logical_ty = analyze::logical_source_type(&field.ty);
                    let patch = schema_patch_type(ir, support_ty, runtime);
                    Some(parse_quote!(#patch: From<#logical_ty>))
                }
                FieldCategory::Path { tagged: true, .. } => {
                    let support_ty = &field.support_ty;
                    let logical_ty = analyze::logical_source_type(&field.ty);
                    let patch = tagged_patch_type(ir, support_ty, runtime);
                    Some(parse_quote!(#patch: From<#logical_ty>))
                }
                FieldCategory::Optional {
                    inner,
                    inner_ty,
                    inner_support_ty,
                } if matches!(inner.as_ref(), FieldCategory::Path { .. }) => {
                    let logical_ty = analyze::logical_source_type(inner_ty);
                    let patch = schema_patch_type(ir, inner_support_ty, runtime);
                    Some(parse_quote!(#patch: From<#logical_ty>))
                }
                _ => None,
            }
        })
        .collect::<Vec<WherePredicate>>();
    add_predicates(&mut generics, predicates);
    generics
}

fn tagged_from_generics(ir: &SchemaIr, runtime: &Path) -> Generics {
    let mut generics = patch_from_generics(ir);
    add_patch_type_bounds(&mut generics, tagged_patch_dependencies(ir), runtime);
    let predicates = ir
        .variants
        .iter()
        .filter_map(|variant| match &variant.shape {
            VariantShape::Unit => None,
            VariantShape::Newtype(ty) => {
                let support_ty = analyze::support_type(ty);
                let logical_ty = analyze::logical_source_type(ty);
                let patch = schema_patch_type(ir, &support_ty, runtime);
                Some(parse_quote!(#patch: From<#logical_ty>))
            }
        })
        .collect::<Vec<WherePredicate>>();
    add_predicates(&mut generics, predicates);
    generics
}

fn option_adapter_name(field: &FieldIr) -> proc_macro2::Ident {
    format_ident!("{}OptionAdapter", pascal(&field.logical_name))
}

fn array_adapter_name(field: &FieldIr) -> proc_macro2::Ident {
    format_ident!("{}ArrayAdapter", pascal(&field.logical_name))
}

fn record_patch_dependencies(ir: &SchemaIr) -> Vec<(Type, bool)> {
    ir.fields
        .iter()
        .filter_map(|field| {
            if emit_mutation::is_external_tag_sibling(ir, field.declaration_index) {
                return None;
            }
            match &field.category {
                FieldCategory::Path { tagged, .. } => Some((field.support_ty.clone(), *tagged)),
                FieldCategory::Optional {
                    inner,
                    inner_support_ty,
                    ..
                } if matches!(inner.as_ref(), FieldCategory::Path { .. }) => {
                    Some(((**inner_support_ty).clone(), false))
                }
                _ => None,
            }
        })
        .collect()
}

fn tagged_patch_dependencies(ir: &SchemaIr) -> Vec<(Type, bool)> {
    ir.variants
        .iter()
        .filter_map(|variant| match &variant.shape {
            VariantShape::Unit => None,
            VariantShape::Newtype(ty) => Some((analyze::support_type(ty), false)),
        })
        .collect()
}

fn add_patch_type_bounds(generics: &mut Generics, dependencies: Vec<(Type, bool)>, runtime: &Path) {
    let predicates = dependencies
        .into_iter()
        .map(|(ty, tagged)| {
            if tagged {
                parse_quote!(#ty: #runtime::__private::TaggedPayloadPatchType)
            } else {
                parse_quote!(#ty: #runtime::__private::SchemaPatchType)
            }
        })
        .collect();
    add_predicates(generics, predicates);
}

fn add_predicates(generics: &mut Generics, predicates: Vec<WherePredicate>) {
    if predicates.is_empty() {
        return;
    }
    generics.make_where_clause().predicates.extend(predicates);
}

fn pascal(value: &str) -> String {
    let mut out = String::new();
    let mut upper = true;
    for ch in value.chars() {
        if ch == '_' {
            upper = true;
        } else if upper {
            out.extend(ch.to_uppercase());
            upper = false;
        } else {
            out.push(ch);
        }
    }
    out
}
