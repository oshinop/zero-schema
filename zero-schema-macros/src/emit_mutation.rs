use proc_macro2::{Ident, TokenStream};
use quote::{format_ident, quote};
use syn::{Lifetime, Path, Type, visit::Visit, visit_mut::VisitMut};

use crate::{
    analyze::{self, FieldCategory, FieldIr, SchemaIr, VariantShape},
    emit_access, emit_wire,
};

/// Emits the field-local mutation capabilities and generated adapter contracts.
///
/// This is intentionally separate from `emit_access`: reads may only construct
/// shared capabilities, while every item here owns an `ExclusiveInput` short
/// reborrow and commits only after its source preflight completed.
pub(crate) fn emit_record_mutation_methods(
    ir: &SchemaIr,
    runtime: &Path,
    support_runtime: &Path,
) -> syn::Result<TokenStream> {
    let methods = ir
        .fields
        .iter()
        .filter(|field| !is_external_tag_sibling(ir, field.declaration_index))
        .map(|field| field_mutator(ir, field, runtime, support_runtime))
        .collect::<syn::Result<Vec<_>>>()?;
    Ok(quote!(#(#methods)*))
}

pub(crate) fn emit_record_adapters(
    ir: &SchemaIr,
    runtime: &Path,
    support_runtime: &Path,
) -> syn::Result<TokenStream> {
    let adapters = ir
        .fields
        .iter()
        .filter(|field| !matches!(field.category, FieldCategory::Array { .. }))
        .filter(|field| scalar_or_string_or_bytes(&field.category))
        .map(|field| field_adapter(ir, field, runtime, support_runtime))
        .collect::<syn::Result<Vec<_>>>()?;
    let option_adapters = ir
        .fields
        .iter()
        .filter(|field| matches!(field.category, FieldCategory::Optional { .. }))
        .map(|field| option_field_adapter(ir, field, runtime, support_runtime))
        .collect::<syn::Result<Vec<_>>>()?;
    let array_adapters = emit_array_adapters(ir, runtime, support_runtime)?;
    let logical_mutation = emit_simple_logical_mutation(ir, runtime, support_runtime)?;
    Ok(quote!(#(#adapters)* #(#option_adapters)* #array_adapters #logical_mutation))
}

pub(crate) fn is_external_tag_sibling(ir: &SchemaIr, index: usize) -> bool {
    ir.fields.iter().any(|field| {
        matches!(field.category, FieldCategory::Path { tag_field: Some(tag), .. } if tag == index)
    })
}

fn scalar_or_string_or_bytes(category: &FieldCategory) -> bool {
    matches!(
        category,
        FieldCategory::Primitive(_)
            | FieldCategory::Bool
            | FieldCategory::BorrowedStr { .. }
            | FieldCategory::BorrowedCStr { .. }
            | FieldCategory::BorrowedU16Str { .. }
            | FieldCategory::BorrowedU16CStr { .. }
            | FieldCategory::FixedBytes { .. }
    )
}

fn field_mutator(
    ir: &SchemaIr,
    field: &FieldIr,
    runtime: &Path,
    support_runtime: &Path,
) -> syn::Result<TokenStream> {
    let method = format_ident!("{}_mut", field.logical_name);
    let offset = emit_access::field_value_offset(ir, field, support_runtime)?;
    match &field.category {
        FieldCategory::Primitive(_) | FieldCategory::Bool => {
            let adapter = field_adapter_name(field);
            let logical = if matches!(field.category, FieldCategory::Bool) {
                quote!(bool)
            } else {
                let ty = &field.ty;
                quote!(#ty)
            };
            let args = emit_wire::wire_arguments(&ir.generics.original);
            Ok(quote!(
                pub fn #method<'view>(&'view mut self) -> #runtime::ScalarMut<'view, #logical, #adapter #args> {
                    let input = match self.input.subrange_mut(#offset) {
                        Ok(input) => input,
                        Err(_) => unreachable!("compiler-asserted field range remains selectable"),
                    };
                    match #runtime::ScalarMut::prove(input, __ZeroSchemaInputAccessToken { _private: () }) {
                        Ok(handle) => handle,
                        Err(_) => unreachable!("a parent capability retains a proved scalar field"),
                    }
                }
            ))
        }
        FieldCategory::BorrowedStr { .. }
        | FieldCategory::BorrowedCStr { .. }
        | FieldCategory::BorrowedU16Str { .. }
        | FieldCategory::BorrowedU16CStr { .. } => {
            let adapter = field_adapter_name(field);
            let args = emit_wire::wire_arguments(&ir.generics.original);
            Ok(quote!(
                pub fn #method<'view>(&'view mut self) -> #runtime::StringMut<'view, #adapter #args> {
                    let input = match self.input.subrange_mut(#offset) {
                        Ok(input) => input,
                        Err(_) => unreachable!("compiler-asserted field range remains selectable"),
                    };
                    match #runtime::StringMut::prove(input, __ZeroSchemaInputAccessToken { _private: () }) {
                        Ok(handle) => handle,
                        Err(_) => unreachable!("a parent capability retains a proved string field"),
                    }
                }
            ))
        }
        FieldCategory::FixedBytes { .. } => {
            let adapter = field_adapter_name(field);
            let args = emit_wire::wire_arguments(&ir.generics.original);
            Ok(quote!(
                pub fn #method<'view>(&'view mut self) -> #runtime::BytesMut<'view, #adapter #args> {
                    let input = match self.input.subrange_mut(#offset) {
                        Ok(input) => input,
                        Err(_) => unreachable!("compiler-asserted field range remains selectable"),
                    };
                    match #runtime::BytesMut::prove(input, __ZeroSchemaInputAccessToken { _private: () }) {
                        Ok(handle) => handle,
                        Err(_) => unreachable!("a parent capability retains a proved byte field"),
                    }
                }
            ))
        }
        FieldCategory::Path { tagged: false, .. } => {
            let ty = &field.support_ty;
            let wire =
                emit_wire::wire_type(&field.category, ty, support_runtime, field.wire_endian)?;
            Ok(quote!(
                pub fn #method<'view>(&'view mut self) -> <<#ty as #runtime::__private::WireTypeSupport>::Support as #runtime::__private::SchemaSupport>::Mut<'view> {
                    let input = match self.input.subrange_mut::<#wire>(#offset) {
                        Ok(input) => input,
                        Err(_) => unreachable!("compiler-asserted field range remains selectable"),
                    };
                    let proof = match <<#ty as #runtime::__private::WireTypeSupport>::Support as #runtime::__private::SchemaSupport>::prove_mut(input) {
                        Ok(proof) => proof,
                        Err(_) => unreachable!("a parent capability retains a proved child field"),
                    };
                    <<#ty as #runtime::__private::WireTypeSupport>::Support as #runtime::__private::SchemaSupport>::make_mut(proof)
                }
            ))
        }
        FieldCategory::Path {
            tagged: true,
            tag_field: Some(tag_index),
        } => {
            let payload = &field.support_ty;
            let payload_support =
                quote!(<#payload as #runtime::__private::TaggedPayloadTypeSupport>::Support);
            let tag_field = &ir.fields[*tag_index];
            let tag_ty = &tag_field.support_ty;
            let tag_wire = emit_wire::wire_type(
                &tag_field.category,
                tag_ty,
                support_runtime,
                tag_field.wire_endian,
            )?;
            let tag_offset = emit_access::field_value_offset(ir, tag_field, support_runtime)?;
            let root_support = emit_access::support_name(ir);
            let root_args = emit_wire::wire_arguments(&ir.generics.original);
            Ok(quote!(
                pub fn #method<'view>(&'view mut self) -> <#payload_support as #runtime::__private::TaggedPayloadSupport>::Mut<'view> {
                    let tag = {
                        let tag_input = match self.input.subrange::<#tag_wire>(#tag_offset) {
                            Ok(input) => input,
                            Err(_) => unreachable!("compiler-asserted tag range remains selectable"),
                        };
                        let proof = match <#tag_ty as #runtime::__private::WireTypeSupport>::Support::prove(tag_input) {
                            Ok(proof) => proof,
                            Err(_) => unreachable!("a parent capability retains a proved tag"),
                        };
                        <#tag_ty as #runtime::__private::WireTypeSupport>::Support::make_ref(proof)
                    };
                    let selection = match #runtime::__private::TaggedMutSelection::<#payload_support>::prove_at::<#root_support #root_args>(&mut self.input, tag, #offset, __ZeroSchemaInputAccessToken { _private: () }) {
                        Ok(selection) => selection,
                        Err(_) => unreachable!("a parent capability retains its proved selected payload"),
                    };
                    selection.make_mut()
                }
            ))
        }
        FieldCategory::Path { tagged: true, .. } => Err(syn::Error::new_spanned(
            &field.ty,
            "tagged field is missing its sibling",
        )),
        FieldCategory::Array { element, .. } => {
            let Type::Array(array) = &field.ty else {
                return Err(syn::Error::new_spanned(&field.ty, "array type mismatch"));
            };
            let Type::Array(support_array) = &field.support_ty else {
                return Err(syn::Error::new_spanned(
                    &field.support_ty,
                    "array support type mismatch",
                ));
            };
            let support_element = &support_array.elem;
            let wire =
                emit_wire::wire_type(element, support_element, support_runtime, field.wire_endian)?;
            let logical = array_logical_type(ir, element, &array.elem, runtime, quote!('view));
            let n = emit_wire::array_length(&field.support_ty)?;
            let adapter = array_adapter_name(field);
            let args = emit_wire::wire_arguments(&ir.generics.original);
            Ok(quote!(
                pub fn #method<'view>(&'view mut self) -> #runtime::ArrayMut<'view, #logical, #n, #adapter #args> {
                    let input = match self.input.subrange_mut::<[#wire; #n]>(#offset) {
                        Ok(input) => input,
                        Err(_) => unreachable!("compiler-asserted field range remains selectable"),
                    };
                    match #runtime::ArrayMut::prove(input, __ZeroSchemaInputAccessToken { _private: () }) {
                        Ok(array) => array,
                        Err(_) => unreachable!("a parent capability retains proved array elements"),
                    }
                }
            ))
        }
        FieldCategory::Optional { inner_ty, .. } => {
            let logical = rebind_source_lifetime(ir, inner_ty, quote!('view));
            let adapter = option_adapter_name(field);
            let args = emit_wire::wire_arguments(&ir.generics.original);
            let storage_wire = emit_wire::wire_field_type(field, support_runtime)?;
            let storage_offset = emit_access::field_storage_offset(ir, field, support_runtime);
            Ok(quote!(
                pub fn #method<'view>(&'view mut self) -> #runtime::OptionMut<'view, #logical, #adapter #args> {
                    let input = match self.input.subrange_mut::<#storage_wire>(#storage_offset) {
                        Ok(input) => input,
                        Err(_) => unreachable!("compiler-asserted optional storage range remains selectable"),
                    };
                    match #runtime::OptionMut::prove(input, __ZeroSchemaInputAccessToken { _private: () }) {
                        Ok(handle) => handle,
                        Err(_) => unreachable!("a parent capability retains proved optional storage"),
                    }
                }
            ))
        }
    }
}

fn option_field_adapter(
    ir: &SchemaIr,
    field: &FieldIr,
    runtime: &Path,
    support_runtime: &Path,
) -> syn::Result<TokenStream> {
    let FieldCategory::Optional {
        inner_ty,
        inner_support_ty,
        inner,
    } = &field.category
    else {
        unreachable!("optional adapter requested for a non-optional field")
    };
    let adapter = option_adapter_name(field);
    let owner = emit_access::owner_name(ir);
    let access_error = &ir.names.access_error;
    let mutation = &ir.names.mutation_error;
    let field_name = &field.logical_name;
    let plain_generics = emit_wire::wire_generics(&ir.generics.original);
    let plain_args = emit_wire::wire_arguments(&ir.generics.original);
    let support_where = emit_access::record_support_where(ir, runtime);
    let storage_wire = emit_wire::wire_field_type(field, support_runtime)?;
    let value_wire =
        emit_wire::wire_type(inner, inner_support_ty, support_runtime, field.wire_endian)?;
    let value_offset = if field.options.align.is_some() {
        quote!(<#storage_wire>::VALUE_OFFSET)
    } else {
        quote!(0usize)
    };
    let source = quote!('source);
    let (read, value, mutable, validate, read_present, make_mut, preflight_init, commit_init) =
        match inner.as_ref() {
            FieldCategory::Path { .. } => {
                let child = inner_support_ty;
                let child_support =
                    quote!(<#child as #runtime::__private::WireTypeSupport>::Support);
                let value = rebind_source_lifetime(ir, inner_ty, quote!(#source));
                let variant = emit_access::error_child_variant(field);
                (
                    quote!(<#child_support as #runtime::__private::SchemaSupport>::Ref<'wire>),
                    value.clone(),
                    quote!(<#child_support as #runtime::__private::SchemaSupport>::Mut<'wire>),
                    quote!(<#child_support as #runtime::__private::SchemaSupport>::prove(input)
                        .map(|_| ())
                        .map_err(|source| #access_error::#variant(source))),
                    quote!(match <#child_support as #runtime::__private::SchemaSupport>::prove(input) {
                        Ok(proof) => Ok(<#child_support as #runtime::__private::SchemaSupport>::make_ref(proof)),
                        Err(source) => Err(#access_error::#variant(source)),
                    }),
                    quote!(match <#child_support as #runtime::__private::SchemaSupport>::prove_mut(input) {
                        Ok(proof) => Ok(<#child_support as #runtime::__private::SchemaSupport>::make_mut(proof)),
                        Err(source) => Err(#access_error::#variant(source)),
                    }),
                    quote!(<#child_support as #runtime::__private::SchemaLogicalMutation<#value>>::preflight_init_logical(input, value)
                        .map_err(|_| #mutation::field_kind(#field_name, #runtime::ErrorKind::CapacityExceeded))),
                    quote!({
                        let child_token = <#child_support as #runtime::__private::SchemaSupport>::input_token(&input);
                        <#child_support as #runtime::__private::SchemaLogicalMutation<#value>>::commit_init_logical(input, value, child_token);
                    }),
                )
            }
            FieldCategory::Array { element, .. } => {
                let Type::Array(array) = inner_ty.as_ref() else {
                    return Err(syn::Error::new_spanned(
                        inner_ty,
                        "optional array type mismatch",
                    ));
                };
                let Type::Array(support_array) = inner_support_ty.as_ref() else {
                    return Err(syn::Error::new_spanned(
                        inner_support_ty,
                        "optional array support type mismatch",
                    ));
                };
                let element_wire = emit_wire::wire_type(
                    element,
                    &support_array.elem,
                    support_runtime,
                    field.wire_endian,
                )?;
                let length = emit_wire::array_length(inner_support_ty)?;
                let array_adapter = array_adapter_name(field);
                let read = array_logical_type(ir, element, &array.elem, runtime, quote!('wire));
                let validation_read =
                    array_logical_type(ir, element, &array.elem, runtime, quote!('_));
                let value = array_value_type(ir, element, &array.elem, quote!(#source));
                let array_value = quote!([#value; #length]);
                let offset = quote!(#runtime::__private::checked_element_offset(index, <#array_adapter #plain_args as #runtime::__private::ArrayElementAdapter>::STRIDE));
                (
                    quote!(#runtime::ArrayRef<'wire, #read, #length, #array_adapter #plain_args>),
                    array_value,
                    quote!(#runtime::ArrayMut<'wire, #read, #length, #array_adapter #plain_args>),
                    quote!(#runtime::ArrayRef::<#validation_read, #length, #array_adapter #plain_args>::prove(input).map(|_| ())),
                    quote!(#runtime::ArrayRef::<#read, #length, #array_adapter #plain_args>::prove(input)),
                    quote!(#runtime::ArrayMut::<#read, #length, #array_adapter #plain_args>::prove(input, token)),
                    quote!({
                        for (index, element_value) in value.iter().enumerate() {
                            let element_offset = #offset
                                .map_err(<Self::Owner as #runtime::__private::OwnerAdapter>::mutation_layout)?;
                            let element_input = input.subrange::<#element_wire>(element_offset)
                                .map_err(<Self::Owner as #runtime::__private::OwnerAdapter>::mutation_layout)?;
                            <#array_adapter #plain_args as #runtime::__private::ArrayElementAdapter>::preflight_init(index, element_input, element_value)?;
                        }
                        Ok(())
                    }),
                    quote!({
                        for (index, element_value) in value.iter().enumerate() {
                            let element_offset = match #offset { Ok(offset) => offset, Err(_) => unreachable!("preflighted optional array index remains representable") };
                            let element_input = match input.subrange_mut::<#element_wire>(element_offset) { Ok(input) => input, Err(_) => unreachable!("preflighted optional array element remains selectable") };
                            <#array_adapter #plain_args as #runtime::__private::ArrayElementAdapter>::commit_init(index, element_input, element_value, token);
                        }
                    }),
                )
            }
            _ => unreachable!("optional analysis accepts only path values or path arrays"),
        };
    Ok(quote!(
        pub struct #adapter #plain_generics(::core::marker::PhantomData<fn() -> (#storage_wire, #owner #plain_args)>) #support_where;
        impl #plain_generics #runtime::__private::InputAccess for #adapter #plain_args #support_where { type Token = __ZeroSchemaInputAccessToken; }
        impl #plain_generics #runtime::__private::OptionFieldAdapter for #adapter #plain_args #support_where {
            type StorageWire = #storage_wire;
            type ValueWire = #value_wire;
            type Owner = #owner #plain_args;
            type Read<'wire> = #read;
            type Value<'source> = #value;
            type Mut<'wire> = #mutable;
            const VALUE_OFFSET: usize = #value_offset;
            fn validate_present(input: #runtime::__private::SharedInput<'_, Self::ValueWire>) -> ::core::result::Result<(), <Self::Owner as #runtime::__private::OwnerAdapter>::AccessError> { #validate }
            fn read_present<'wire>(input: #runtime::__private::SharedInput<'wire, Self::ValueWire>) -> ::core::result::Result<Self::Read<'wire>, <Self::Owner as #runtime::__private::OwnerAdapter>::AccessError> { #read_present }
            fn make_present_mut<'wire>(input: #runtime::__private::ExclusiveInput<'wire, Self::ValueWire>, token: Self::Token) -> ::core::result::Result<Self::Mut<'wire>, <Self::Owner as #runtime::__private::OwnerAdapter>::AccessError> { #make_mut }
            fn preflight_init<'wire, 'source>(input: #runtime::__private::SharedInput<'wire, Self::ValueWire>, value: &Self::Value<'source>) -> ::core::result::Result<(), <Self::Owner as #runtime::__private::OwnerAdapter>::MutationError> { #preflight_init }
            fn commit_init<'wire, 'source>(mut input: #runtime::__private::ExclusiveInput<'wire, Self::ValueWire>, value: &Self::Value<'source>, token: Self::Token) { let _ = value; #commit_init }
        }
    ))
}

fn field_adapter(
    ir: &SchemaIr,
    field: &FieldIr,
    runtime: &Path,
    support_runtime: &Path,
) -> syn::Result<TokenStream> {
    let adapter = field_adapter_name(field);
    let owner = emit_access::owner_name(ir);
    let access_error = &ir.names.access_error;
    let mutation = &ir.names.mutation_error;
    let field_name = &field.logical_name;
    let plain_generics = emit_wire::wire_generics(&ir.generics.original);
    let plain_args = emit_wire::wire_arguments(&ir.generics.original);
    let support_where = emit_access::record_support_where(ir, runtime);
    let wire = emit_wire::wire_type(
        &field.category,
        &field.support_ty,
        support_runtime,
        field.wire_endian,
    )?;
    match &field.category {
        FieldCategory::Primitive(_) => {
            let ty = &field.ty;
            Ok(quote!(
                pub struct #adapter #plain_generics(::core::marker::PhantomData<fn() -> (#wire, #owner #plain_args)>) #support_where;
                impl #plain_generics #runtime::__private::InputAccess for #adapter #plain_args #support_where { type Token = __ZeroSchemaInputAccessToken; }
                impl #plain_generics #runtime::__private::ScalarMutationAdapter for #adapter #plain_args #support_where {
                    type Wire = #wire;
                    type Owner = #owner #plain_args;
                    type Logical = #ty;
                    fn read(input: #runtime::__private::SharedInput<'_, Self::Wire>) -> ::core::result::Result<Self::Logical, <Self::Owner as #runtime::__private::OwnerAdapter>::AccessError> { input.read_copy::<Self::Wire>(0).map(|wire| wire.get()).map_err(<Self::Owner as #runtime::__private::OwnerAdapter>::access_layout) }
                    fn preflight(_: Self::Logical) -> ::core::result::Result<(), <Self::Owner as #runtime::__private::OwnerAdapter>::MutationError> { Ok(()) }
                    fn commit(mut input: #runtime::__private::ExclusiveInput<'_, Self::Wire>, value: Self::Logical, token: Self::Token) {
                        let bytes = match input.subrange_bytes_mut::<Self>(0, ::core::mem::size_of::<Self::Wire>(), token) { Ok(bytes) => bytes, Err(_) => unreachable!("preflighted scalar field remains exact") };
                        Self::Wire::new(value).store_preflighted(bytes);
                    }
                }
            ))
        }
        FieldCategory::Bool => {
            let variant = emit_access::error_bool_variant(field);
            Ok(quote!(
                pub struct #adapter #plain_generics(::core::marker::PhantomData<fn() -> (#wire, #owner #plain_args)>) #support_where;
                impl #plain_generics #runtime::__private::InputAccess for #adapter #plain_args #support_where { type Token = __ZeroSchemaInputAccessToken; }
                impl #plain_generics #runtime::__private::ScalarMutationAdapter for #adapter #plain_args #support_where {
                    type Wire = #wire;
                    type Owner = #owner #plain_args;
                    type Logical = bool;
                    fn read(input: #runtime::__private::SharedInput<'_, Self::Wire>) -> ::core::result::Result<Self::Logical, <Self::Owner as #runtime::__private::OwnerAdapter>::AccessError> { let wire=input.read_copy::<Self::Wire>(0).map_err(<Self::Owner as #runtime::__private::OwnerAdapter>::access_layout)?; wire.decode().ok_or(#access_error::#variant { raw: wire.raw() }) }
                    fn preflight(_: Self::Logical) -> ::core::result::Result<(), <Self::Owner as #runtime::__private::OwnerAdapter>::MutationError> { Ok(()) }
                    fn commit(mut input: #runtime::__private::ExclusiveInput<'_, Self::Wire>, value: Self::Logical, token: Self::Token) {
                        let bytes = match input.subrange_bytes_mut::<Self>(0, ::core::mem::size_of::<Self::Wire>(), token) { Ok(bytes) => bytes, Err(_) => unreachable!("preflighted Boolean field remains exact") };
                        #runtime::__private::BoolWire::store_preflighted(value, bytes);
                    }
                }
            ))
        }
        FieldCategory::BorrowedStr {
            len_type,
            endian,
            capacity,
            ..
        } => {
            let length = emit_wire::length_wire(len_type, *endian)?;
            let variant = emit_access::error_string_variant(field);
            Ok(string_adapter_tokens(
                ir,
                &adapter,
                &wire,
                quote!(str),
                field_name,
                runtime,
                &owner,
                &plain_generics,
                &plain_args,
                &support_where,
                quote!(
                    let prefix = input.subrange_bytes::<Self>(<Self::Wire>::LEN_OFFSET, <#runtime::__private::#length as #runtime::__private::LengthWire>::WIDTH, __ZeroSchemaInputAccessToken { _private: () }).map_err(<Self::Owner as #runtime::__private::OwnerAdapter>::mutation_layout)?;
                    let data = input.subrange_bytes::<Self>(<Self::Wire>::DATA_OFFSET, #capacity, __ZeroSchemaInputAccessToken { _private: () }).map_err(<Self::Owner as #runtime::__private::OwnerAdapter>::mutation_layout)?;
                    #runtime::__private::preflight_str::<#runtime::__private::#length>(prefix, data, value).map_err(|error| #mutation::field(#field_name, error))
                ),
                quote!(
                    let bytes = match input.subrange_bytes_mut::<Self>(0, ::core::mem::size_of::<Self::Wire>(), token) { Ok(bytes) => bytes, Err(_) => unreachable!("preflighted string field remains exact") };
                    let (prefix_storage, data) = bytes.split_at_mut(<Self::Wire>::DATA_OFFSET);
                    let prefix = &mut prefix_storage[<Self::Wire>::LEN_OFFSET..<#runtime::__private::#length as #runtime::__private::LengthWire>::WIDTH];
                    let data = &mut data[..#capacity];
                    #runtime::__private::commit_str::<#runtime::__private::#length>(prefix, data, value);
                ),
                quote!({ let length=input.read_copy::<#runtime::__private::#length>(<Self::Wire>::LEN_OFFSET).map_err(<Self::Owner as #runtime::__private::OwnerAdapter>::access_layout)?; let data=input.subrange_bytes::<Self>(<Self::Wire>::DATA_OFFSET,#capacity,__ZeroSchemaInputAccessToken { _private: () }).map_err(<Self::Owner as #runtime::__private::OwnerAdapter>::access_layout)?; #runtime::__private::prove_str(&length,data).map_err(|error| #access_error::#variant { kind: match error { #runtime::__private::StringProofError::LengthOutOfBounds { .. } => #runtime::ErrorKind::LengthOutOfBounds, #runtime::__private::StringProofError::InvalidUtf8(_) => #runtime::ErrorKind::InvalidUtf8, #runtime::__private::StringProofError::MissingNul => #runtime::ErrorKind::MissingNul } }) }),
            ))
        }
        FieldCategory::BorrowedCStr { capacity, .. } => {
            let variant = emit_access::error_string_variant(field);
            Ok(string_adapter_tokens(
                ir,
                &adapter,
                &wire,
                quote!(::core::ffi::CStr),
                field_name,
                runtime,
                &owner,
                &plain_generics,
                &plain_args,
                &support_where,
                quote!(
                    let data = input.subrange_bytes::<Self>(0, #capacity, __ZeroSchemaInputAccessToken { _private: () }).map_err(<Self::Owner as #runtime::__private::OwnerAdapter>::mutation_layout)?;
                    #runtime::__private::preflight_c_str(data, value).map_err(|error| #mutation::field(#field_name, error))
                ),
                quote!(
                    let data = match input.subrange_bytes_mut::<Self>(0, #capacity, token) { Ok(data) => data, Err(_) => unreachable!("preflighted C string field remains exact") };
                    #runtime::__private::commit_c_str(data, value);
                ),
                quote!(#runtime::__private::prove_c_str(input.subrange_bytes::<Self>(0,#capacity,__ZeroSchemaInputAccessToken { _private: () }).map_err(<Self::Owner as #runtime::__private::OwnerAdapter>::access_layout)?).map_err(|_| #access_error::#variant { kind: #runtime::ErrorKind::MissingNul })),
            ))
        }
        FieldCategory::BorrowedU16Str {
            len_type,
            endian,
            capacity,
            ..
        } => {
            let length = emit_wire::length_wire(len_type, *endian)?;
            let variant = emit_access::error_string_variant(field);
            Ok(string_adapter_tokens(
                ir,
                &adapter,
                &wire,
                quote!(#runtime::__private::U16Str),
                field_name,
                runtime,
                &owner,
                &plain_generics,
                &plain_args,
                &support_where,
                quote!(
                    let prefix = input.subrange_bytes::<Self>(<Self::Wire>::LEN_OFFSET, <#runtime::__private::#length as #runtime::__private::LengthWire>::WIDTH, __ZeroSchemaInputAccessToken { _private: () }).map_err(<Self::Owner as #runtime::__private::OwnerAdapter>::mutation_layout)?;
                    let data = input.subrange_bytes::<Self>(<Self::Wire>::DATA_OFFSET, #capacity * ::core::mem::size_of::<::core::primitive::u16>(), __ZeroSchemaInputAccessToken { _private: () }).map_err(<Self::Owner as #runtime::__private::OwnerAdapter>::mutation_layout)?;
                    #runtime::__private::preflight_u16_str::<#runtime::__private::#length>(prefix, data, value).map_err(|error| #mutation::field(#field_name, error))
                ),
                quote!(
                    let bytes = match input.subrange_bytes_mut::<Self>(0, ::core::mem::size_of::<Self::Wire>(), token) { Ok(bytes) => bytes, Err(_) => unreachable!("preflighted wide string field remains exact") };
                    let (prefix_storage, data) = bytes.split_at_mut(<Self::Wire>::DATA_OFFSET);
                    let prefix = &mut prefix_storage[<Self::Wire>::LEN_OFFSET..<#runtime::__private::#length as #runtime::__private::LengthWire>::WIDTH];
                    let data = &mut data[..#capacity * ::core::mem::size_of::<::core::primitive::u16>()];
                    #runtime::__private::commit_u16_str::<#runtime::__private::#length>(prefix, data, value);
                ),
                quote!({ let length=input.read_copy::<#runtime::__private::#length>(<Self::Wire>::LEN_OFFSET).map_err(<Self::Owner as #runtime::__private::OwnerAdapter>::access_layout)?; let data=input.subrange_bytes::<Self>(<Self::Wire>::DATA_OFFSET,#capacity * ::core::mem::size_of::<::core::primitive::u16>(),__ZeroSchemaInputAccessToken { _private: () }).map_err(<Self::Owner as #runtime::__private::OwnerAdapter>::access_layout)?; #runtime::__private::prove_u16_str_bytes::<#runtime::__private::#length,#capacity>(&length,data).map_err(|error| #access_error::#variant { kind: match error { #runtime::__private::StringProofError::LengthOutOfBounds { .. } => #runtime::ErrorKind::LengthOutOfBounds, #runtime::__private::StringProofError::InvalidUtf8(_) => #runtime::ErrorKind::InvalidUtf8, #runtime::__private::StringProofError::MissingNul => #runtime::ErrorKind::MissingNul } }) }),
            ))
        }
        FieldCategory::BorrowedU16CStr { capacity, .. } => {
            let variant = emit_access::error_string_variant(field);
            Ok(string_adapter_tokens(
                ir,
                &adapter,
                &wire,
                quote!(#runtime::__private::U16CStr),
                field_name,
                runtime,
                &owner,
                &plain_generics,
                &plain_args,
                &support_where,
                quote!(
                    let data = input.subrange_bytes::<Self>(0, #capacity * ::core::mem::size_of::<::core::primitive::u16>(), __ZeroSchemaInputAccessToken { _private: () }).map_err(<Self::Owner as #runtime::__private::OwnerAdapter>::mutation_layout)?;
                    #runtime::__private::preflight_u16_c_str(data, value).map_err(|error| #mutation::field(#field_name, error))
                ),
                quote!(
                    let data = match input.subrange_bytes_mut::<Self>(0, #capacity * ::core::mem::size_of::<::core::primitive::u16>(), token) { Ok(data) => data, Err(_) => unreachable!("preflighted wide C string field remains exact") };
                    #runtime::__private::commit_u16_c_str(data, value);
                ),
                quote!(#runtime::__private::prove_u16_c_str_bytes::<#capacity>(input.subrange_bytes::<Self>(0,#capacity * ::core::mem::size_of::<::core::primitive::u16>(),__ZeroSchemaInputAccessToken { _private: () }).map_err(<Self::Owner as #runtime::__private::OwnerAdapter>::access_layout)?).map_err(|_| #access_error::#variant { kind: #runtime::ErrorKind::MissingNul })),
            ))
        }
        FieldCategory::FixedBytes { .. } => Ok(quote!(
            pub struct #adapter #plain_generics(::core::marker::PhantomData<fn() -> (#wire, #owner #plain_args)>) #support_where;
            impl #plain_generics #runtime::__private::InputAccess for #adapter #plain_args #support_where { type Token = __ZeroSchemaInputAccessToken; }
            impl #plain_generics #runtime::__private::FixedBytesMutationAdapter for #adapter #plain_args #support_where {
                type Wire = #wire;
                type Owner = #owner #plain_args;
                fn read(input: #runtime::__private::SharedInput<'_, Self::Wire>) -> ::core::result::Result<&[::core::primitive::u8], <Self::Owner as #runtime::__private::OwnerAdapter>::AccessError> { input.subrange_bytes::<Self>(0,::core::mem::size_of::<Self::Wire>(),__ZeroSchemaInputAccessToken { _private: () }).map_err(<Self::Owner as #runtime::__private::OwnerAdapter>::access_layout) }
                fn preflight(value: &[::core::primitive::u8]) -> ::core::result::Result<(), <Self::Owner as #runtime::__private::OwnerAdapter>::MutationError> {
                    if value.len() == ::core::mem::size_of::<Self::Wire>() { Ok(()) } else { Err(#mutation::field_kind(#field_name, #runtime::ErrorKind::ArrayLengthMismatch)) }
                }
                fn commit(mut input: #runtime::__private::ExclusiveInput<'_, Self::Wire>, value: &[::core::primitive::u8], token: Self::Token) {
                    let data = match input.subrange_bytes_mut::<Self>(0, ::core::mem::size_of::<Self::Wire>(), token) { Ok(data) => data, Err(_) => unreachable!("preflighted fixed bytes remain exact") };
                    data.copy_from_slice(value);
                }
            }
        )),
        _ => unreachable!("only scalar/string/bytes fields have direct adapters"),
    }
}

#[allow(clippy::too_many_arguments)]
fn string_adapter_tokens(
    _ir: &SchemaIr,
    adapter: &Ident,
    wire: &TokenStream,
    logical: TokenStream,
    _field_name: &str,
    runtime: &Path,
    owner: &Ident,
    plain_generics: &TokenStream,
    plain_args: &TokenStream,
    support_where: &TokenStream,
    preflight: TokenStream,
    commit: TokenStream,
    read: TokenStream,
) -> TokenStream {
    quote!(
        pub struct #adapter #plain_generics(::core::marker::PhantomData<fn() -> (#wire, #owner #plain_args)>) #support_where;
        impl #plain_generics #runtime::__private::InputAccess for #adapter #plain_args #support_where { type Token = __ZeroSchemaInputAccessToken; }
        impl #plain_generics #runtime::__private::StringMutationAdapter for #adapter #plain_args #support_where {
            type Wire = #wire;
            type Owner = #owner #plain_args;
            type Logical = #logical;
            fn read<'wire>(input: #runtime::__private::SharedInput<'wire, Self::Wire>) -> ::core::result::Result<&'wire Self::Logical, <Self::Owner as #runtime::__private::OwnerAdapter>::AccessError> { #read }
            fn preflight(input: #runtime::__private::SharedInput<'_, Self::Wire>, value: &Self::Logical) -> ::core::result::Result<(), <Self::Owner as #runtime::__private::OwnerAdapter>::MutationError> { #preflight }
            fn commit(mut input: #runtime::__private::ExclusiveInput<'_, Self::Wire>, value: &Self::Logical, token: Self::Token) { #commit }
        }
    )
}

pub(crate) fn emit_array_adapters(
    ir: &SchemaIr,
    runtime: &Path,
    support_runtime: &Path,
) -> syn::Result<TokenStream> {
    let mut items = Vec::new();
    for field in &ir.fields {
        match &field.category {
            FieldCategory::Array { .. } => {
                items.push(emit_array_adapter(ir, field, runtime, support_runtime)?);
            }
            FieldCategory::Optional {
                inner,
                inner_ty,
                inner_support_ty,
            } if matches!(inner.as_ref(), FieldCategory::Array { .. }) => {
                let mut array_field = field.clone();
                array_field.ty = (**inner_ty).clone();
                array_field.support_ty = (**inner_support_ty).clone();
                array_field.category = (**inner).clone();
                array_field.options.align = None;
                items.push(emit_array_adapter(
                    ir,
                    &array_field,
                    runtime,
                    support_runtime,
                )?);
            }
            _ => {}
        }
    }
    Ok(quote!(#(#items)*))
}

fn emit_array_adapter(
    ir: &SchemaIr,
    field: &FieldIr,
    runtime: &Path,
    support_runtime: &Path,
) -> syn::Result<TokenStream> {
    let FieldCategory::Array { element, .. } = &field.category else {
        unreachable!()
    };
    let Type::Array(array) = &field.ty else {
        return Err(syn::Error::new_spanned(&field.ty, "array type mismatch"));
    };
    let adapter = array_adapter_name(field);
    let scalar_adapter = array_scalar_adapter_name(field);
    let plain_generics = emit_wire::wire_generics(&ir.generics.original);
    let plain_args = emit_wire::wire_arguments(&ir.generics.original);
    let element_wire = emit_wire::wire_type(
        element,
        &analyze::support_type(&array.elem),
        support_runtime,
        field.wire_endian,
    )?;
    let read = array_logical_type(ir, element, &array.elem, runtime, quote!('wire));
    let value = array_value_type(ir, element, &array.elem, quote!('value));
    let owner = emit_access::owner_name(ir);
    let access_error = &ir.names.access_error;
    let mutation = &ir.names.mutation_error;
    let field_name = &field.logical_name;
    let support_where = array_adapter_support_where(ir, field, runtime);
    let proof = match element.as_ref() {
        FieldCategory::Primitive(_) => quote!(Ok(())),
        FieldCategory::Bool => {
            let variant = emit_access::error_array_bool_variant(field);
            let inner = emit_access::error_array_bool_error_type(field);
            quote!({ let wire=input.read_copy::<Self::Wire>(0).map_err(<Self::Owner as #runtime::__private::OwnerAdapter>::access_layout)?; if wire.decode().is_some() { Ok(()) } else { Err(#access_error::#variant(#inner { index, raw: wire.raw() })) } })
        }
        FieldCategory::Path { .. } => {
            let ty = analyze::support_type(&array.elem);
            let variant = emit_access::error_array_child_variant(field);
            let inner = emit_access::error_array_child_error_type(field);
            quote!(<#ty as #runtime::__private::WireTypeSupport>::Support::prove(input).map(|_| ()).map_err(|source| #access_error::#variant(#inner { index, source })))
        }
        _ => return Err(syn::Error::new_spanned(&field.ty, "invalid array element")),
    };
    let read_expr = match element.as_ref() {
        FieldCategory::Primitive(_) => {
            quote!(input.read_copy::<Self::Wire>(0).map(|wire| wire.get()).map_err(<Self::Owner as #runtime::__private::OwnerAdapter>::access_layout))
        }
        FieldCategory::Bool => {
            let variant = emit_access::error_array_bool_variant(field);
            let inner = emit_access::error_array_bool_error_type(field);
            quote!({ let wire=input.read_copy::<Self::Wire>(0).map_err(<Self::Owner as #runtime::__private::OwnerAdapter>::access_layout)?; wire.decode().ok_or(#access_error::#variant(#inner { index, raw: wire.raw() })) })
        }
        FieldCategory::Path { .. } => {
            let ty = analyze::support_type(&array.elem);
            let variant = emit_access::error_array_child_variant(field);
            let inner = emit_access::error_array_child_error_type(field);
            quote!(match <#ty as #runtime::__private::WireTypeSupport>::Support::prove(input) { Ok(proof) => Ok(<#ty as #runtime::__private::WireTypeSupport>::Support::make_ref(proof)), Err(source) => Err(#access_error::#variant(#inner { index, source })) })
        }
        _ => unreachable!(),
    };
    let (mut_ty, make_mut, preflight, commit, preflight_init, commit_init, scalar_impl) =
        match element.as_ref() {
            FieldCategory::Primitive(_) => {
                let ty = &array.elem;
                (
                    quote!(#runtime::ScalarMut<'wire, #ty, #scalar_adapter #plain_args>),
                    quote!(#runtime::ScalarMut::prove(input, token)),
                    quote!(Ok(())),
                    quote!({
                        let bytes = match input.subrange_bytes_mut::<Self>(
                            0,
                            ::core::mem::size_of::<Self::Wire>(),
                            token,
                        ) {
                            Ok(bytes) => bytes,
                            Err(_) => unreachable!("preflighted array scalar remains exact"),
                        };
                        Self::Wire::new(*value).store_preflighted(bytes);
                    }),
                    quote!(Ok(())),
                    quote!({
                        let bytes = match input.subrange_bytes_mut::<Self>(
                            0,
                            ::core::mem::size_of::<Self::Wire>(),
                            token,
                        ) {
                            Ok(bytes) => bytes,
                            Err(_) => unreachable!("preflighted array scalar remains exact"),
                        };
                        Self::Wire::new(*value).store_preflighted(bytes);
                    }),
                    quote!(
                        pub struct #scalar_adapter #plain_generics(::core::marker::PhantomData<fn() -> (#element_wire, #owner #plain_args)>) #support_where;
                        impl #plain_generics #runtime::__private::InputAccess for #scalar_adapter #plain_args #support_where { type Token = __ZeroSchemaInputAccessToken; }
                        impl #plain_generics #runtime::__private::ScalarMutationAdapter for #scalar_adapter #plain_args #support_where {
                            type Wire = #element_wire; type Owner = #owner #plain_args; type Logical = #ty;
                            fn read(input: #runtime::__private::SharedInput<'_, Self::Wire>) -> ::core::result::Result<Self::Logical, <Self::Owner as #runtime::__private::OwnerAdapter>::AccessError> { input.read_copy::<Self::Wire>(0).map(|wire| wire.get()).map_err(<Self::Owner as #runtime::__private::OwnerAdapter>::access_layout) }
                            fn preflight(_: Self::Logical) -> ::core::result::Result<(), <Self::Owner as #runtime::__private::OwnerAdapter>::MutationError> { Ok(()) }
                            fn commit(mut input: #runtime::__private::ExclusiveInput<'_, Self::Wire>, value: Self::Logical, token: Self::Token) { let bytes = match input.subrange_bytes_mut::<Self>(0, ::core::mem::size_of::<Self::Wire>(), token) { Ok(bytes) => bytes, Err(_) => unreachable!("preflighted array scalar remains exact") }; Self::Wire::new(value).store_preflighted(bytes); }
                        }
                    ),
                )
            }
            FieldCategory::Bool => {
                let variant = emit_access::error_array_bool_variant(field);
                let inner = emit_access::error_array_bool_error_type(field);
                (
                    quote!(#runtime::ScalarMut<'wire, bool, #scalar_adapter #plain_args>),
                    quote!(#runtime::ScalarMut::prove(input, token)),
                    quote!(Ok(())),
                    quote!({ let bytes = match input.subrange_bytes_mut::<Self>(0, ::core::mem::size_of::<Self::Wire>(), token) { Ok(bytes) => bytes, Err(_) => unreachable!("preflighted array Boolean remains exact") }; #runtime::__private::BoolWire::store_preflighted(*value, bytes); }),
                    quote!(Ok(())),
                    quote!({ let bytes = match input.subrange_bytes_mut::<Self>(0, ::core::mem::size_of::<Self::Wire>(), token) { Ok(bytes) => bytes, Err(_) => unreachable!("preflighted array Boolean remains exact") }; #runtime::__private::BoolWire::store_preflighted(*value, bytes); }),
                    quote!(
                        pub struct #scalar_adapter #plain_generics(::core::marker::PhantomData<fn() -> (#element_wire, #owner #plain_args)>) #support_where;
                        impl #plain_generics #runtime::__private::InputAccess for #scalar_adapter #plain_args #support_where { type Token = __ZeroSchemaInputAccessToken; }
                        impl #plain_generics #runtime::__private::ScalarMutationAdapter for #scalar_adapter #plain_args #support_where {
                            type Wire = #element_wire; type Owner = #owner #plain_args; type Logical = bool;
                            fn read(input: #runtime::__private::SharedInput<'_, Self::Wire>) -> ::core::result::Result<Self::Logical, <Self::Owner as #runtime::__private::OwnerAdapter>::AccessError> { let wire=input.read_copy::<Self::Wire>(0).map_err(<Self::Owner as #runtime::__private::OwnerAdapter>::access_layout)?; wire.decode().ok_or(#access_error::#variant(#inner { index: 0, raw: wire.raw() })) }
                            fn preflight(_: Self::Logical) -> ::core::result::Result<(), <Self::Owner as #runtime::__private::OwnerAdapter>::MutationError> { Ok(()) }
                            fn commit(mut input: #runtime::__private::ExclusiveInput<'_, Self::Wire>, value: Self::Logical, token: Self::Token) { let bytes = match input.subrange_bytes_mut::<Self>(0, ::core::mem::size_of::<Self::Wire>(), token) { Ok(bytes) => bytes, Err(_) => unreachable!("preflighted array Boolean remains exact") }; #runtime::__private::BoolWire::store_preflighted(value, bytes); }
                        }
                    ),
                )
            }
            FieldCategory::Path { .. } => {
                let ty = analyze::support_type(&array.elem);
                let child_support = quote!(<#ty as #runtime::__private::WireTypeSupport>::Support);
                let value_ty = array_value_type(ir, element, &array.elem, quote!('value));
                let variant = emit_access::error_array_child_variant(field);
                let inner = emit_access::error_array_child_error_type(field);
                (
                    quote!(<#child_support as #runtime::__private::SchemaSupport>::Mut<'wire>),
                    quote!(match <#child_support as #runtime::__private::SchemaSupport>::prove_mut(input) { Ok(proof) => Ok(<#child_support as #runtime::__private::SchemaSupport>::make_mut(proof)), Err(source) => Err(#access_error::#variant(#inner { index, source })) }),
                    quote!(<#child_support as #runtime::__private::SchemaLogicalMutation<#value_ty>>::preflight_logical(input, value).map_err(|_| #mutation::array(#field_name, index, #runtime::ErrorKind::CapacityExceeded))),
                    quote!({ let child_token = <#child_support as #runtime::__private::SchemaSupport>::input_token(&input); <#child_support as #runtime::__private::SchemaLogicalMutation<#value_ty>>::commit_logical(input, value, child_token) }),
                    quote!(<#child_support as #runtime::__private::SchemaLogicalMutation<#value_ty>>::preflight_init_logical(input, value).map_err(|_| #mutation::array(#field_name, index, #runtime::ErrorKind::CapacityExceeded))),
                    quote!({ let child_token = <#child_support as #runtime::__private::SchemaSupport>::input_token(&input); <#child_support as #runtime::__private::SchemaLogicalMutation<#value_ty>>::commit_init_logical(input, value, child_token) }),
                    TokenStream::new(),
                )
            }
            _ => unreachable!(),
        };
    Ok(quote!(
        #scalar_impl
        pub struct #adapter #plain_generics(::core::marker::PhantomData<fn() -> (#element_wire, #owner #plain_args)>) #support_where;
        impl #plain_generics #runtime::__private::InputAccess for #adapter #plain_args #support_where { type Token = __ZeroSchemaInputAccessToken; }
        impl #plain_generics #runtime::__private::ArrayElementAdapter for #adapter #plain_args #support_where {
            type Wire = #element_wire;
            type ArrayWire<const __ZERO_SCHEMA_N: usize> = [#element_wire; __ZERO_SCHEMA_N];
            type Owner = #owner #plain_args;
            type Read<'wire> = #read;
            type Value<'value> = #value;
            type Mut<'wire> = #mut_ty;
            const STRIDE: usize = ::core::mem::size_of::<#element_wire>();
            fn prove<'wire>(index: usize, input: #runtime::__private::SharedInput<'wire, Self::Wire>) -> ::core::result::Result<(), <Self::Owner as #runtime::__private::OwnerAdapter>::AccessError> { #proof }
            fn read<'wire>(index: usize, input: #runtime::__private::SharedInput<'wire, Self::Wire>) -> ::core::result::Result<Self::Read<'wire>, <Self::Owner as #runtime::__private::OwnerAdapter>::AccessError> { let _ = index; #read_expr }
            fn make_mut<'wire>(index: usize, input: #runtime::__private::ExclusiveInput<'wire, Self::Wire>, token: Self::Token) -> ::core::result::Result<Self::Mut<'wire>, <Self::Owner as #runtime::__private::OwnerAdapter>::AccessError> {
                let _ = token;
                Self::prove(index, input.shared())?;
                #make_mut
            }
            fn preflight<'wire, 'value>(index: usize, input: #runtime::__private::SharedInput<'wire, Self::Wire>, value: &Self::Value<'value>) -> ::core::result::Result<(), <Self::Owner as #runtime::__private::OwnerAdapter>::MutationError> { #preflight }
            fn commit<'wire, 'value>(index: usize, mut input: #runtime::__private::ExclusiveInput<'wire, Self::Wire>, value: &Self::Value<'value>, token: Self::Token) { let _ = index; #commit }
            fn preflight_init<'wire, 'value>(index: usize, input: #runtime::__private::SharedInput<'wire, Self::Wire>, value: &Self::Value<'value>) -> ::core::result::Result<(), <Self::Owner as #runtime::__private::OwnerAdapter>::MutationError> { #preflight_init }
            fn commit_init<'wire, 'value>(index: usize, mut input: #runtime::__private::ExclusiveInput<'wire, Self::Wire>, value: &Self::Value<'value>, token: Self::Token) { let _ = index; #commit_init }
            fn index_error(index: usize, _: usize) -> <Self::Owner as #runtime::__private::OwnerAdapter>::MutationError { #mutation::array(#field_name, index, #runtime::ErrorKind::ArrayIndexOutOfBounds) }
            fn length_error(actual: usize, expected: usize) -> <Self::Owner as #runtime::__private::OwnerAdapter>::MutationError { let _ = actual; let _ = expected; #mutation::field_kind(#field_name, #runtime::ErrorKind::ArrayLengthMismatch) }
        }
    ))
}

fn emit_simple_logical_mutation(
    ir: &SchemaIr,
    runtime: &Path,
    support_runtime: &Path,
) -> syn::Result<TokenStream> {
    let support = format_ident!("{}Support", ir.logical_name);
    let plain_args = emit_wire::wire_arguments(&ir.generics.original);
    let source = logical_source_lifetime(ir);
    let params = logical_generic_parameters(ir, &source);
    let source_generics = if params.is_empty() {
        quote!(<#source>)
    } else {
        quote!(<#source, #(#params),*>)
    };
    let logical_args = ir
        .generics
        .original
        .params
        .iter()
        .map(|parameter| match parameter {
            syn::GenericParam::Lifetime(_) => quote!(#source),
            syn::GenericParam::Type(parameter) => {
                let ident = &parameter.ident;
                quote!(#ident)
            }
            syn::GenericParam::Const(parameter) => {
                let ident = &parameter.ident;
                quote!(#ident)
            }
        })
        .collect::<Vec<_>>();
    let ident = &ir.ident;
    let logical = if logical_args.is_empty() {
        quote!(super::#ident)
    } else {
        quote!(super::#ident<#(#logical_args),*>)
    };
    let support_where = logical_mutation_support_where(ir, runtime, &source);
    let preflight = ir
        .fields
        .iter()
        .map(|field| logical_preflight_field(ir, field, runtime, support_runtime))
        .collect::<syn::Result<Vec<_>>>()?;
    let commit = ir
        .fields
        .iter()
        .map(|field| logical_commit_field(ir, field, runtime, support_runtime))
        .collect::<syn::Result<Vec<_>>>()?;
    let init_preflight = ir
        .fields
        .iter()
        .map(|field| logical_init_preflight_field(ir, field, runtime, support_runtime))
        .collect::<syn::Result<Vec<_>>>()?;
    let init_commit = ir
        .fields
        .iter()
        .map(|field| logical_init_commit_field(ir, field, runtime, support_runtime))
        .collect::<syn::Result<Vec<_>>>()?;
    Ok(quote!(
        impl #source_generics #runtime::__private::SchemaLogicalMutation<#logical> for #support #plain_args #support_where {
            fn preflight_logical<'wire>(input: #runtime::__private::SharedInput<'wire, Self::Wire>, value: &#logical) -> ::core::result::Result<(), <Self::Owner as #runtime::__private::OwnerAdapter>::MutationError> {
                #(#preflight)*
                Ok(())
            }
            fn commit_logical<'wire>(input: #runtime::__private::ExclusiveInput<'wire, Self::Wire>, value: &#logical, token: Self::Token) {
                let _ = token;
                let mut input = input;
                #(#commit)*
            }
            fn preflight_init_logical<'wire>(input: #runtime::__private::SharedInput<'wire, Self::Wire>, value: &#logical) -> ::core::result::Result<(), <Self::Owner as #runtime::__private::OwnerAdapter>::MutationError> {
                #(#init_preflight)*
                Ok(())
            }
            fn commit_init_logical<'wire>(input: #runtime::__private::ExclusiveInput<'wire, Self::Wire>, value: &#logical, token: Self::Token) {
                let mut input = input;
                #(#init_commit)*
            }
        }
    ))
}

pub(crate) fn emit_tagged_logical_mutation(
    ir: &SchemaIr,
    runtime: &Path,
) -> syn::Result<TokenStream> {
    let support = format_ident!("{}Support", ir.logical_name);
    let owner = emit_access::owner_name(ir);
    let plain_args = emit_wire::wire_arguments(&ir.generics.original);
    let source = logical_source_lifetime(ir);
    let params = logical_generic_parameters(ir, &source);
    let source_generics = if params.is_empty() {
        quote!(<#source>)
    } else {
        quote!(<#source, #(#params),*>)
    };
    let logical_args = ir
        .generics
        .original
        .params
        .iter()
        .map(|parameter| match parameter {
            syn::GenericParam::Lifetime(_) => quote!(#source),
            syn::GenericParam::Type(parameter) => {
                let ident = &parameter.ident;
                quote!(#ident)
            }
            syn::GenericParam::Const(parameter) => {
                let ident = &parameter.ident;
                quote!(#ident)
            }
        })
        .collect::<Vec<_>>();
    let ident = &ir.ident;
    let logical = if logical_args.is_empty() {
        quote!(super::#ident)
    } else {
        quote!(super::#ident<#(#logical_args),*>)
    };
    let support_where = tagged_logical_mutation_support_where(ir, runtime, &source);
    let tag_arms = ir.variants.iter().map(|variant| {
        let name = &variant.ident;
        let tag = &variant.tag;
        match variant.shape {
            VariantShape::Unit => quote!(super::#ident::#name => #tag,),
            VariantShape::Newtype(_) => quote!(super::#ident::#name(_) => #tag,),
        }
    });
    let preflight_arms = ir.variants.iter().map(|variant| -> syn::Result<TokenStream> {
        let name = &variant.ident;
        match &variant.shape {
            VariantShape::Unit => Ok(quote!(super::#ident::#name => Ok(()))),
            VariantShape::Newtype(ty) => {
                let child = analyze::support_type(ty);
                let child_support = quote!(<#child as #runtime::__private::WireTypeSupport>::Support);
                let child_wire = quote!(<#child as #runtime::__private::WireType>::Wire);
                let child_logical = rebind_source_lifetime(ir, ty, quote!(#source));
                Ok(quote!(super::#ident::#name(child_value) => {
                    let child_input = payload.subrange::<#child_wire>(0)
                        .map_err(<#owner #plain_args as #runtime::__private::OwnerAdapter>::mutation_layout)?;
                    <#child_support as #runtime::__private::SchemaLogicalMutation<#child_logical>>::preflight_logical(child_input, child_value)
                        .map_err(|_| <#owner #plain_args as #runtime::__private::OwnerAdapter>::mutation_layout(#runtime::LayoutError::OffsetOverflow))
                }))
            }
        }
    }).collect::<syn::Result<Vec<_>>>()?;
    let commit_arms = ir.variants.iter().map(|variant| -> syn::Result<TokenStream> {
        let name = &variant.ident;
        match &variant.shape {
            VariantShape::Unit => Ok(quote!(super::#ident::#name => ())),
            VariantShape::Newtype(ty) => {
                let child = analyze::support_type(ty);
                let child_support = quote!(<#child as #runtime::__private::WireTypeSupport>::Support);
                let child_wire = quote!(<#child as #runtime::__private::WireType>::Wire);
                let child_logical = rebind_source_lifetime(ir, ty, quote!(#source));
                Ok(quote!(super::#ident::#name(child_value) => {
                    let child_input = match payload.subrange_mut::<#child_wire>(0) {
                        Ok(child) => child,
                        Err(_) => unreachable!("preflighted logical tagged payload remains selectable"),
                    };
                    let child_token = <#child_support as #runtime::__private::SchemaSupport>::input_token(&child_input);
                    <#child_support as #runtime::__private::SchemaLogicalMutation<#child_logical>>::commit_logical(child_input, child_value, child_token);
                }))
            }
        }
    }).collect::<syn::Result<Vec<_>>>()?;
    let init_preflight_arms = ir.variants.iter().map(|variant| -> syn::Result<TokenStream> {
        let name = &variant.ident;
        match &variant.shape {
            VariantShape::Unit => Ok(quote!(super::#ident::#name => Ok(()))),
            VariantShape::Newtype(ty) => {
                let child = analyze::support_type(ty);
                let child_support = quote!(<#child as #runtime::__private::WireTypeSupport>::Support);
                let child_wire = quote!(<#child as #runtime::__private::WireType>::Wire);
                let child_logical = rebind_source_lifetime(ir, ty, quote!(#source));
                Ok(quote!(super::#ident::#name(child_value) => {
                    let child_input = payload.subrange::<#child_wire>(0).map_err(<#owner #plain_args as #runtime::__private::OwnerAdapter>::mutation_layout)?;
                    <#child_support as #runtime::__private::SchemaLogicalMutation<#child_logical>>::preflight_init_logical(child_input, child_value)
                        .map_err(|_| <#owner #plain_args as #runtime::__private::OwnerAdapter>::mutation_layout(#runtime::LayoutError::OffsetOverflow))
                }))
            }
        }
    }).collect::<syn::Result<Vec<_>>>()?;
    let init_commit_arms = ir.variants.iter().map(|variant| -> syn::Result<TokenStream> {
        let name = &variant.ident;
        match &variant.shape {
            VariantShape::Unit => Ok(quote!(super::#ident::#name => ())),
            VariantShape::Newtype(ty) => {
                let child = analyze::support_type(ty);
                let child_support = quote!(<#child as #runtime::__private::WireTypeSupport>::Support);
                let child_wire = quote!(<#child as #runtime::__private::WireType>::Wire);
                let child_logical = rebind_source_lifetime(ir, ty, quote!(#source));
                Ok(quote!(super::#ident::#name(child_value) => {
                    let child_input = match payload.subrange_mut::<#child_wire>(0) { Ok(child) => child, Err(_) => unreachable!("preflighted initialized tagged payload remains selectable") };
                    let child_token = <#child_support as #runtime::__private::SchemaSupport>::input_token(&child_input);
                    <#child_support as #runtime::__private::SchemaLogicalMutation<#child_logical>>::commit_init_logical(child_input, child_value, child_token);
                }))
            }
        }
    }).collect::<syn::Result<Vec<_>>>()?;
    Ok(quote!(
        impl #source_generics #runtime::__private::TaggedPayloadLogicalMutation<#logical> for #support #plain_args #support_where {
            fn logical_tag(value: &#logical) -> Self::Tag {
                match value { #(#tag_arms)* }
            }

            fn preflight_logical<'wire>(
                current_tag: Self::Tag,
                payload: #runtime::__private::SharedInput<'wire, Self::Wire>,
                value: &#logical,
            ) -> ::core::result::Result<(), <Self::Owner as #runtime::__private::OwnerAdapter>::MutationError> {
                if current_tag == Self::logical_tag(value) {
                    match value { #(#preflight_arms,)* }
                } else {
                    match value { #(#init_preflight_arms,)* }
                }
            }

            fn commit_logical<'wire>(
                mut payload: #runtime::__private::ExclusiveInput<'wire, Self::Wire>,
                value: &#logical,
                token: Self::Token,
            ) {
                let _ = token;
                match value { #(#commit_arms,)* }
            }
            fn preflight_init_logical<'wire>(
                payload: #runtime::__private::SharedInput<'wire, Self::Wire>,
                value: &#logical,
            ) -> ::core::result::Result<(), <Self::Owner as #runtime::__private::OwnerAdapter>::MutationError> {
                match value { #(#init_preflight_arms,)* }
            }

            fn commit_init_logical<'wire>(
                mut payload: #runtime::__private::ExclusiveInput<'wire, Self::Wire>,
                value: &#logical,
                token: Self::Token,
            ) {
                let _ = token;
                match value { #(#init_commit_arms,)* }
            }
        }
    ))
}

fn tagged_logical_mutation_support_where(
    ir: &SchemaIr,
    runtime: &Path,
    source: &Lifetime,
) -> TokenStream {
    let source_predicates = logical_source_predicates(ir, source);
    let wire_predicates = emit_wire::erased_where_predicates(&ir.generics.original);
    let wire_type_bounds = wire_type_parameter_predicates(ir);
    let dependencies = emit_wire::tagged_dependency_types(ir);
    let logical_bounds = ir
        .variants
        .iter()
        .filter_map(|variant| match &variant.shape {
            VariantShape::Unit => None,
            VariantShape::Newtype(ty) => {
                let child = analyze::support_type(ty);
                let child_support =
                    quote!(<#child as #runtime::__private::WireTypeSupport>::Support);
                let logical = rebind_source_lifetime(ir, ty, quote!(#source));
                Some(quote!(#child_support: #runtime::__private::SchemaLogicalMutation<#logical>))
            }
        })
        .collect::<Vec<_>>();
    if source_predicates.is_empty()
        && wire_predicates.is_empty()
        && wire_type_bounds.is_empty()
        && dependencies.is_empty()
        && logical_bounds.is_empty()
    {
        return TokenStream::new();
    }
    quote!(where
        #(#source_predicates,)*
        #(#wire_predicates,)*
        #(#wire_type_bounds,)*
        #(#dependencies: #runtime::__private::WireTypeSupport + 'static,)*
        #(#logical_bounds,)*
    )
}

fn logical_preflight_field(
    ir: &SchemaIr,
    field: &FieldIr,
    runtime: &Path,
    support_runtime: &Path,
) -> syn::Result<TokenStream> {
    if is_external_tag_sibling(ir, field.declaration_index) {
        return Ok(TokenStream::new());
    }
    let name = &field.ident;
    let field_name = &field.logical_name;
    let offset = emit_access::field_value_offset(ir, field, support_runtime)?;
    let mutation = &ir.names.mutation_error;
    let access_error = &ir.names.access_error;
    let owner = emit_access::owner_name(ir);
    let owner_args = emit_wire::wire_arguments(&ir.generics.original);
    let wire = emit_wire::wire_type(
        &field.category,
        &field.support_ty,
        support_runtime,
        field.wire_endian,
    )?;
    let adapter = field_adapter_name(field);
    let source = logical_source_lifetime(ir);
    Ok(match &field.category {
        FieldCategory::Primitive(_) | FieldCategory::Bool => quote!({
            let _ = input.subrange::<#wire>(#offset).map_err(<#owner #owner_args as #runtime::__private::OwnerAdapter>::mutation_layout)?;
            <#adapter #owner_args as #runtime::__private::ScalarMutationAdapter>::preflight(value.#name)?;
        }),
        FieldCategory::BorrowedStr { .. }
        | FieldCategory::BorrowedCStr { .. }
        | FieldCategory::BorrowedU16Str { .. }
        | FieldCategory::BorrowedU16CStr { .. } => quote!({
            let selected = input.subrange::<#wire>(#offset).map_err(<#owner #owner_args as #runtime::__private::OwnerAdapter>::mutation_layout)?;
            <#adapter #owner_args as #runtime::__private::StringMutationAdapter>::preflight(selected, value.#name)?;
        }),
        FieldCategory::FixedBytes { .. } => {
            quote!(<#adapter #owner_args as #runtime::__private::FixedBytesMutationAdapter>::preflight(value.#name)?;)
        }
        FieldCategory::Array { element, .. } => {
            let Type::Array(array) = &field.ty else {
                return Err(syn::Error::new_spanned(&field.ty, "array type mismatch"));
            };
            let array_adapter = array_adapter_name(field);
            let element_wire = emit_wire::wire_type(
                element,
                &analyze::support_type(&array.elem),
                support_runtime,
                field.wire_endian,
            )?;
            let length = emit_wire::array_length(&field.support_ty)?;
            quote!({
                let selected = input.subrange::<[#element_wire; #length]>(#offset).map_err(<#owner #owner_args as #runtime::__private::OwnerAdapter>::mutation_layout)?;
                for (index, element_value) in value.#name.iter().enumerate() {
                    let element_offset = #runtime::__private::checked_element_offset(index, <#array_adapter #owner_args as #runtime::__private::ArrayElementAdapter>::STRIDE)
                        .map_err(<#owner #owner_args as #runtime::__private::OwnerAdapter>::mutation_layout)?;
                    let element_input = selected.subrange::<#element_wire>(element_offset)
                        .map_err(<#owner #owner_args as #runtime::__private::OwnerAdapter>::mutation_layout)?;
                    <#array_adapter #owner_args as #runtime::__private::ArrayElementAdapter>::preflight(index, element_input, element_value)?;
                }
            })
        }
        FieldCategory::Path { tagged: false, .. } => {
            let child = &field.support_ty;
            let child_support = quote!(<#child as #runtime::__private::WireTypeSupport>::Support);
            let logical = rebind_source_lifetime(ir, &field.ty, quote!(#source));
            quote!({
                let child_input = input.subrange::<#wire>(#offset).map_err(<#owner #owner_args as #runtime::__private::OwnerAdapter>::mutation_layout)?;
                <#child_support as #runtime::__private::SchemaLogicalMutation<#logical>>::preflight_logical(child_input, &value.#name)
                    .map_err(|_| #mutation::field_kind(#field_name, #runtime::ErrorKind::CapacityExceeded))?;
            })
        }
        FieldCategory::Optional { .. } => {
            let adapter = option_adapter_name(field);
            let storage_wire = emit_wire::wire_field_type(field, support_runtime)?;
            let storage_offset = emit_access::field_storage_offset(ir, field, support_runtime);
            quote!({
                let storage = input.subrange::<#storage_wire>(#storage_offset)
                    .map_err(<#owner #owner_args as #runtime::__private::OwnerAdapter>::mutation_layout)?;
                if let Some(optional_value) = value.#name.as_ref() {
                    let selected = storage.subrange::<<#adapter #owner_args as #runtime::__private::OptionFieldAdapter>::ValueWire>(<#adapter #owner_args as #runtime::__private::OptionFieldAdapter>::VALUE_OFFSET)
                        .map_err(<#owner #owner_args as #runtime::__private::OwnerAdapter>::mutation_layout)?;
                    <#adapter #owner_args as #runtime::__private::OptionFieldAdapter>::preflight_init(selected, optional_value)?;
                }
            })
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
            let payload = &field.support_ty;
            let payload_support =
                quote!(<#payload as #runtime::__private::TaggedPayloadTypeSupport>::Support);
            let logical = rebind_source_lifetime(ir, &field.ty, quote!(#source));
            quote!({
                let target = <#payload_support as #runtime::__private::TaggedPayloadLogicalMutation<#logical>>::logical_tag(&value.#name);
                if value.#tag_name != target {
                    return Err(#mutation::field_kind(#field_name, #runtime::ErrorKind::TagMismatch));
                }
                let tag_input = input.subrange::<#tag_wire>(#tag_offset).map_err(<#owner #owner_args as #runtime::__private::OwnerAdapter>::mutation_layout)?;
                let tag_proof = <#tag_ty as #runtime::__private::WireTypeSupport>::Support::prove(tag_input)
                    .map_err(|source| #mutation::from(#access_error::#tag_variant(source)))?;
                let current = <#tag_ty as #runtime::__private::WireTypeSupport>::Support::make_ref(tag_proof);
                let payload_input = input.subrange::<#wire>(#offset).map_err(<#owner #owner_args as #runtime::__private::OwnerAdapter>::mutation_layout)?;
                <#payload_support as #runtime::__private::TaggedPayloadLogicalMutation<#logical>>::preflight_logical(current, payload_input, &value.#name)
                    .map_err(|_| #mutation::field_kind(#field_name, #runtime::ErrorKind::CapacityExceeded))?;
            })
        }
        FieldCategory::Path { tagged: true, .. } => {
            return Err(syn::Error::new_spanned(
                &field.ty,
                "tagged field is missing its sibling",
            ));
        }
    })
}

fn logical_commit_field(
    ir: &SchemaIr,
    field: &FieldIr,
    runtime: &Path,
    support_runtime: &Path,
) -> syn::Result<TokenStream> {
    if is_external_tag_sibling(ir, field.declaration_index) {
        return Ok(TokenStream::new());
    }
    let name = &field.ident;
    let offset = emit_access::field_value_offset(ir, field, support_runtime)?;
    let owner_args = emit_wire::wire_arguments(&ir.generics.original);
    let wire = emit_wire::wire_type(
        &field.category,
        &field.support_ty,
        support_runtime,
        field.wire_endian,
    )?;
    let adapter = field_adapter_name(field);
    let source = logical_source_lifetime(ir);
    Ok(match &field.category {
        FieldCategory::Primitive(_) | FieldCategory::Bool => quote!({
            let selected = match input.subrange_mut::<#wire>(#offset) { Ok(selected) => selected, Err(_) => unreachable!("preflighted logical scalar remains selectable") };
            <#adapter #owner_args as #runtime::__private::ScalarMutationAdapter>::commit(selected, value.#name, token);
        }),
        FieldCategory::BorrowedStr { .. }
        | FieldCategory::BorrowedCStr { .. }
        | FieldCategory::BorrowedU16Str { .. }
        | FieldCategory::BorrowedU16CStr { .. } => quote!({
            let selected = match input.subrange_mut::<#wire>(#offset) { Ok(selected) => selected, Err(_) => unreachable!("preflighted logical string remains selectable") };
            <#adapter #owner_args as #runtime::__private::StringMutationAdapter>::commit(selected, value.#name, token);
        }),
        FieldCategory::FixedBytes { .. } => quote!({
            let selected = match input.subrange_mut::<#wire>(#offset) { Ok(selected) => selected, Err(_) => unreachable!("preflighted logical bytes remain selectable") };
            <#adapter #owner_args as #runtime::__private::FixedBytesMutationAdapter>::commit(selected, value.#name, token);
        }),
        FieldCategory::Array { element, .. } => {
            let Type::Array(array) = &field.ty else {
                return Err(syn::Error::new_spanned(&field.ty, "array type mismatch"));
            };
            let array_adapter = array_adapter_name(field);
            let element_wire = emit_wire::wire_type(
                element,
                &analyze::support_type(&array.elem),
                support_runtime,
                field.wire_endian,
            )?;
            let length = emit_wire::array_length(&field.support_ty)?;
            quote!({
                let mut selected = match input.subrange_mut::<[#element_wire; #length]>(#offset) { Ok(selected) => selected, Err(_) => unreachable!("preflighted logical array remains selectable") };
                for (index, element_value) in value.#name.iter().enumerate() {
                    let element_offset = match #runtime::__private::checked_element_offset(index, <#array_adapter #owner_args as #runtime::__private::ArrayElementAdapter>::STRIDE) { Ok(offset) => offset, Err(_) => unreachable!("preflighted logical array index remains representable") };
                    let element_input = match selected.subrange_mut::<#element_wire>(element_offset) { Ok(element) => element, Err(_) => unreachable!("preflighted logical array element remains selectable") };
                    <#array_adapter #owner_args as #runtime::__private::ArrayElementAdapter>::commit(index, element_input, element_value, token);
                }
            })
        }
        FieldCategory::Path { tagged: false, .. } => {
            let child = &field.support_ty;
            let child_support = quote!(<#child as #runtime::__private::WireTypeSupport>::Support);
            let logical = rebind_source_lifetime(ir, &field.ty, quote!(#source));
            quote!({
                let child_input = match input.subrange_mut::<#wire>(#offset) { Ok(child) => child, Err(_) => unreachable!("preflighted nested logical field remains selectable") };
                let child_token = <#child_support as #runtime::__private::SchemaSupport>::input_token(&child_input);
                <#child_support as #runtime::__private::SchemaLogicalMutation<#logical>>::commit_logical(child_input, &value.#name, child_token);
            })
        }
        FieldCategory::Optional { .. } => {
            let adapter = option_adapter_name(field);
            let storage_wire = emit_wire::wire_field_type(field, support_runtime)?;
            let storage_offset = emit_access::field_storage_offset(ir, field, support_runtime);
            quote!({
                let mut storage = match input.subrange_mut::<#storage_wire>(#storage_offset) {
                    Ok(storage) => storage,
                    Err(_) => unreachable!("preflighted optional storage remains selectable"),
                };
                match value.#name.as_ref() {
                    None => <#adapter #owner_args as #runtime::__private::OptionFieldAdapter>::clear(storage, token),
                    Some(optional_value) => {
                        let selected = match storage.subrange_mut::<<#adapter #owner_args as #runtime::__private::OptionFieldAdapter>::ValueWire>(<#adapter #owner_args as #runtime::__private::OptionFieldAdapter>::VALUE_OFFSET) {
                            Ok(value) => value,
                            Err(_) => unreachable!("preflighted optional value remains selectable"),
                        };
                        <#adapter #owner_args as #runtime::__private::OptionFieldAdapter>::commit_init(selected, optional_value, token);
                    }
                }
            })
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
            let payload = &field.support_ty;
            let payload_support =
                quote!(<#payload as #runtime::__private::TaggedPayloadTypeSupport>::Support);
            let logical = rebind_source_lifetime(ir, &field.ty, quote!(#source));
            let root = &ir.names.wire;
            let root_args = emit_wire::wire_arguments(&ir.generics.original);
            let root_wire =
                emit_wire::aligned_root_wire(ir, quote!(#root #root_args), support_runtime);
            quote!({
                let target = <#payload_support as #runtime::__private::TaggedPayloadLogicalMutation<#logical>>::logical_tag(&value.#name);
                let tag_input = match input.subrange::<#tag_wire>(#tag_offset) { Ok(tag_input) => tag_input, Err(_) => unreachable!("preflighted logical tag remains selectable") };
                let current = match <<#tag_ty as #runtime::__private::WireTypeSupport>::Support as #runtime::__private::ScalarEnumSupport>::from_raw(<<#tag_ty as #runtime::__private::WireTypeSupport>::Support as #runtime::__private::ScalarEnumSupport>::raw(tag_input)) { Some(value) => value, None => unreachable!("logical preflight validated the current scalar tag") };
                if target == current {
                    let payload_input = match input.subrange_mut::<#wire>(#offset) { Ok(payload) => payload, Err(_) => unreachable!("preflighted logical payload remains selectable") };
                    let payload_token = <#payload_support as #runtime::__private::TaggedPayloadSupport>::input_token(&payload_input);
                    <#payload_support as #runtime::__private::TaggedPayloadLogicalMutation<#logical>>::commit_logical(payload_input, &value.#name, payload_token);
                } else {
                    match #runtime::__private::commit_payload_before_tag_with::<#root_wire, #payload_support, #tag_wire, _, _>(&mut input, #tag_offset, #offset, |payload_input| {
                        let payload_token = <#payload_support as #runtime::__private::TaggedPayloadSupport>::input_token(&payload_input);
                        <#payload_support as #runtime::__private::TaggedPayloadLogicalMutation<#logical>>::commit_init_logical(payload_input, &value.#name, payload_token);
                    }, token, |tag_input| {
                        let tag_token = <<#tag_ty as #runtime::__private::WireTypeSupport>::Support as #runtime::__private::SchemaSupport>::input_token(&tag_input);
                        <#tag_ty as #runtime::__private::WireTypeSupport>::Support::commit(tag_input, target, tag_token);
                    }) { Ok(()) => (), Err(_) => unreachable!("preflighted logical tagged ranges remain selectable") }
                }
            })
        }
        FieldCategory::Path { tagged: true, .. } => {
            return Err(syn::Error::new_spanned(
                &field.ty,
                "tagged field is missing its sibling",
            ));
        }
    })
}

fn logical_init_preflight_field(
    ir: &SchemaIr,
    field: &FieldIr,
    runtime: &Path,
    support_runtime: &Path,
) -> syn::Result<TokenStream> {
    if is_external_tag_sibling(ir, field.declaration_index) {
        return Ok(TokenStream::new());
    }
    let name = &field.ident;
    let field_name = &field.logical_name;
    let offset = emit_access::field_value_offset(ir, field, support_runtime)?;
    let mutation = &ir.names.mutation_error;
    let owner = emit_access::owner_name(ir);
    let owner_args = emit_wire::wire_arguments(&ir.generics.original);
    let wire = emit_wire::wire_type(
        &field.category,
        &field.support_ty,
        support_runtime,
        field.wire_endian,
    )?;
    let adapter = field_adapter_name(field);
    let source = logical_source_lifetime(ir);
    Ok(match &field.category {
        FieldCategory::Primitive(_) | FieldCategory::Bool => quote!({
            let _ = input.subrange::<#wire>(#offset).map_err(<#owner #owner_args as #runtime::__private::OwnerAdapter>::mutation_layout)?;
            <#adapter #owner_args as #runtime::__private::ScalarMutationAdapter>::preflight(value.#name)?;
        }),
        FieldCategory::BorrowedStr { .. }
        | FieldCategory::BorrowedCStr { .. }
        | FieldCategory::BorrowedU16Str { .. }
        | FieldCategory::BorrowedU16CStr { .. } => quote!({
            let selected = input.subrange::<#wire>(#offset).map_err(<#owner #owner_args as #runtime::__private::OwnerAdapter>::mutation_layout)?;
            <#adapter #owner_args as #runtime::__private::StringMutationAdapter>::preflight(selected, value.#name)?;
        }),
        FieldCategory::FixedBytes { .. } => {
            quote!(<#adapter #owner_args as #runtime::__private::FixedBytesMutationAdapter>::preflight(value.#name)?;)
        }
        FieldCategory::Array { element, .. } => {
            let Type::Array(array) = &field.ty else {
                return Err(syn::Error::new_spanned(&field.ty, "array type mismatch"));
            };
            let array_adapter = array_adapter_name(field);
            let element_wire = emit_wire::wire_type(
                element,
                &analyze::support_type(&array.elem),
                support_runtime,
                field.wire_endian,
            )?;
            let length = emit_wire::array_length(&field.support_ty)?;
            quote!({
                let selected = input.subrange::<[#element_wire; #length]>(#offset).map_err(<#owner #owner_args as #runtime::__private::OwnerAdapter>::mutation_layout)?;
                for (index, element_value) in value.#name.iter().enumerate() {
                    let element_offset = #runtime::__private::checked_element_offset(index, <#array_adapter #owner_args as #runtime::__private::ArrayElementAdapter>::STRIDE).map_err(<#owner #owner_args as #runtime::__private::OwnerAdapter>::mutation_layout)?;
                    let element_input = selected.subrange::<#element_wire>(element_offset).map_err(<#owner #owner_args as #runtime::__private::OwnerAdapter>::mutation_layout)?;
                    <#array_adapter #owner_args as #runtime::__private::ArrayElementAdapter>::preflight_init(index, element_input, element_value)?;
                }
            })
        }
        FieldCategory::Path { tagged: false, .. } => {
            let child = &field.support_ty;
            let child_support = quote!(<#child as #runtime::__private::WireTypeSupport>::Support);
            let logical = rebind_source_lifetime(ir, &field.ty, quote!(#source));
            quote!({
                let child_input = input.subrange::<#wire>(#offset).map_err(<#owner #owner_args as #runtime::__private::OwnerAdapter>::mutation_layout)?;
                <#child_support as #runtime::__private::SchemaLogicalMutation<#logical>>::preflight_init_logical(child_input, &value.#name)
                    .map_err(|_| #mutation::field_kind(#field_name, #runtime::ErrorKind::CapacityExceeded))?;
            })
        }
        FieldCategory::Optional { .. } => {
            let adapter = option_adapter_name(field);
            let storage_wire = emit_wire::wire_field_type(field, support_runtime)?;
            let storage_offset = emit_access::field_storage_offset(ir, field, support_runtime);
            quote!({
                let storage = input.subrange::<#storage_wire>(#storage_offset).map_err(<#owner #owner_args as #runtime::__private::OwnerAdapter>::mutation_layout)?;
                if let Some(optional_value) = value.#name.as_ref() {
                    let selected = storage.subrange::<<#adapter #owner_args as #runtime::__private::OptionFieldAdapter>::ValueWire>(<#adapter #owner_args as #runtime::__private::OptionFieldAdapter>::VALUE_OFFSET).map_err(<#owner #owner_args as #runtime::__private::OwnerAdapter>::mutation_layout)?;
                    <#adapter #owner_args as #runtime::__private::OptionFieldAdapter>::preflight_init(selected, optional_value)?;
                }
            })
        }
        FieldCategory::Path {
            tagged: true,
            tag_field: Some(tag_index),
        } => {
            let tag_field = &ir.fields[*tag_index];
            let tag_name = &tag_field.ident;
            let payload = &field.support_ty;
            let payload_support =
                quote!(<#payload as #runtime::__private::TaggedPayloadTypeSupport>::Support);
            let logical = rebind_source_lifetime(ir, &field.ty, quote!(#source));
            quote!({
                let target = <#payload_support as #runtime::__private::TaggedPayloadLogicalMutation<#logical>>::logical_tag(&value.#name);
                if value.#tag_name != target { return Err(#mutation::field_kind(#field_name, #runtime::ErrorKind::TagMismatch)); }
                let payload_input = input.subrange::<#wire>(#offset).map_err(<#owner #owner_args as #runtime::__private::OwnerAdapter>::mutation_layout)?;
                <#payload_support as #runtime::__private::TaggedPayloadLogicalMutation<#logical>>::preflight_init_logical(payload_input, &value.#name)
                    .map_err(|_| #mutation::field_kind(#field_name, #runtime::ErrorKind::CapacityExceeded))?;
            })
        }
        FieldCategory::Path { tagged: true, .. } => {
            return Err(syn::Error::new_spanned(
                &field.ty,
                "tagged field is missing its sibling",
            ));
        }
    })
}

fn logical_init_commit_field(
    ir: &SchemaIr,
    field: &FieldIr,
    runtime: &Path,
    support_runtime: &Path,
) -> syn::Result<TokenStream> {
    if is_external_tag_sibling(ir, field.declaration_index) {
        return Ok(TokenStream::new());
    }
    let name = &field.ident;
    let offset = emit_access::field_value_offset(ir, field, support_runtime)?;
    let owner_args = emit_wire::wire_arguments(&ir.generics.original);
    let wire = emit_wire::wire_type(
        &field.category,
        &field.support_ty,
        support_runtime,
        field.wire_endian,
    )?;
    let adapter = field_adapter_name(field);
    let source = logical_source_lifetime(ir);
    Ok(match &field.category {
        FieldCategory::Primitive(_) | FieldCategory::Bool => quote!({
            let selected = match input.subrange_mut::<#wire>(#offset) { Ok(value) => value, Err(_) => unreachable!("preflighted initialized scalar remains selectable") };
            <#adapter #owner_args as #runtime::__private::ScalarMutationAdapter>::commit(selected, value.#name, token);
        }),
        FieldCategory::BorrowedStr { .. }
        | FieldCategory::BorrowedCStr { .. }
        | FieldCategory::BorrowedU16Str { .. }
        | FieldCategory::BorrowedU16CStr { .. } => quote!({
            let selected = match input.subrange_mut::<#wire>(#offset) { Ok(value) => value, Err(_) => unreachable!("preflighted initialized string remains selectable") };
            <#adapter #owner_args as #runtime::__private::StringMutationAdapter>::commit(selected, value.#name, token);
        }),
        FieldCategory::FixedBytes { .. } => quote!({
            let selected = match input.subrange_mut::<#wire>(#offset) { Ok(value) => value, Err(_) => unreachable!("preflighted initialized bytes remain selectable") };
            <#adapter #owner_args as #runtime::__private::FixedBytesMutationAdapter>::commit(selected, value.#name, token);
        }),
        FieldCategory::Array { element, .. } => {
            let Type::Array(array) = &field.ty else {
                return Err(syn::Error::new_spanned(&field.ty, "array type mismatch"));
            };
            let array_adapter = array_adapter_name(field);
            let element_wire = emit_wire::wire_type(
                element,
                &analyze::support_type(&array.elem),
                support_runtime,
                field.wire_endian,
            )?;
            let length = emit_wire::array_length(&field.support_ty)?;
            quote!({
                let mut selected = match input.subrange_mut::<[#element_wire; #length]>(#offset) { Ok(value) => value, Err(_) => unreachable!("preflighted initialized array remains selectable") };
                for (index, element_value) in value.#name.iter().enumerate() {
                    let element_offset = match #runtime::__private::checked_element_offset(index, <#array_adapter #owner_args as #runtime::__private::ArrayElementAdapter>::STRIDE) { Ok(value) => value, Err(_) => unreachable!("preflighted initialized array index remains representable") };
                    let element_input = match selected.subrange_mut::<#element_wire>(element_offset) { Ok(value) => value, Err(_) => unreachable!("preflighted initialized array element remains selectable") };
                    <#array_adapter #owner_args as #runtime::__private::ArrayElementAdapter>::commit_init(index, element_input, element_value, token);
                }
            })
        }
        FieldCategory::Path { tagged: false, .. } => {
            let child = &field.support_ty;
            let child_support = quote!(<#child as #runtime::__private::WireTypeSupport>::Support);
            let logical = rebind_source_lifetime(ir, &field.ty, quote!(#source));
            quote!({
                let child_input = match input.subrange_mut::<#wire>(#offset) { Ok(value) => value, Err(_) => unreachable!("preflighted initialized child remains selectable") };
                let child_token = <#child_support as #runtime::__private::SchemaSupport>::input_token(&child_input);
                <#child_support as #runtime::__private::SchemaLogicalMutation<#logical>>::commit_init_logical(child_input, &value.#name, child_token);
            })
        }
        FieldCategory::Optional { .. } => {
            let adapter = option_adapter_name(field);
            let storage_wire = emit_wire::wire_field_type(field, support_runtime)?;
            let storage_offset = emit_access::field_storage_offset(ir, field, support_runtime);
            quote!({
                let mut storage = match input.subrange_mut::<#storage_wire>(#storage_offset) { Ok(value) => value, Err(_) => unreachable!("preflighted optional storage remains selectable") };
                match value.#name.as_ref() {
                    None => <#adapter #owner_args as #runtime::__private::OptionFieldAdapter>::clear(storage, token),
                    Some(optional_value) => {
                        let selected = match storage.subrange_mut::<<#adapter #owner_args as #runtime::__private::OptionFieldAdapter>::ValueWire>(<#adapter #owner_args as #runtime::__private::OptionFieldAdapter>::VALUE_OFFSET) { Ok(value) => value, Err(_) => unreachable!("preflighted optional value remains selectable") };
                        <#adapter #owner_args as #runtime::__private::OptionFieldAdapter>::commit_init(selected, optional_value, token);
                    }
                }
            })
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
            let payload = &field.support_ty;
            let payload_support =
                quote!(<#payload as #runtime::__private::TaggedPayloadTypeSupport>::Support);
            let logical = rebind_source_lifetime(ir, &field.ty, quote!(#source));
            let root = &ir.names.wire;
            let root_args = emit_wire::wire_arguments(&ir.generics.original);
            let root_wire =
                emit_wire::aligned_root_wire(ir, quote!(#root #root_args), support_runtime);
            quote!({
                let target = <#payload_support as #runtime::__private::TaggedPayloadLogicalMutation<#logical>>::logical_tag(&value.#name);
                match #runtime::__private::commit_payload_before_tag_with::<#root_wire, #payload_support, #tag_wire, _, _>(&mut input, #tag_offset, #offset, |payload_input| {
                    let payload_token = <#payload_support as #runtime::__private::TaggedPayloadSupport>::input_token(&payload_input);
                    <#payload_support as #runtime::__private::TaggedPayloadLogicalMutation<#logical>>::commit_init_logical(payload_input, &value.#name, payload_token);
                }, token, |tag_input| {
                    let tag_token = <<#tag_ty as #runtime::__private::WireTypeSupport>::Support as #runtime::__private::SchemaSupport>::input_token(&tag_input);
                    <#tag_ty as #runtime::__private::WireTypeSupport>::Support::commit(tag_input, target, tag_token);
                }) { Ok(()) => (), Err(_) => unreachable!("preflighted initialized tagged ranges remain selectable") }
            })
        }
        FieldCategory::Path { tagged: true, .. } => {
            return Err(syn::Error::new_spanned(
                &field.ty,
                "tagged field is missing its sibling",
            ));
        }
    })
}

fn logical_mutation_support_where(ir: &SchemaIr, runtime: &Path, source: &Lifetime) -> TokenStream {
    let source_predicates = logical_source_predicates(ir, source);
    let wire_predicates = emit_wire::erased_where_predicates(&ir.generics.original);
    let wire_type_bounds = wire_type_parameter_predicates(ir);
    let dependencies = emit_wire::parent_dependency_types(&ir.fields);
    let optional_dependencies = emit_wire::optional_dependency_types(&ir.fields);
    let mut child_support_bounds = Vec::new();
    let mut nested_bounds = Vec::new();
    let mut tagged_type_bounds = Vec::new();
    let mut tagged_logical_bounds = Vec::new();

    for field in &ir.fields {
        if is_external_tag_sibling(ir, field.declaration_index) {
            continue;
        }
        match &field.category {
            FieldCategory::Path { tagged: false, .. } => {
                let child = &field.support_ty;
                let child_support =
                    quote!(<#child as #runtime::__private::WireTypeSupport>::Support);
                let logical = rebind_source_lifetime(ir, &field.ty, quote!(#source));
                child_support_bounds
                    .push(quote!(#child: #runtime::__private::WireTypeSupport + 'static));
                nested_bounds.push(
                    quote!(#child_support: #runtime::__private::SchemaLogicalMutation<#logical>),
                );
            }
            FieldCategory::Path {
                tagged: true,
                tag_field: Some(tag_index),
            } => {
                let payload = &field.support_ty;
                let payload_support =
                    quote!(<#payload as #runtime::__private::TaggedPayloadTypeSupport>::Support);
                let tag = &ir.fields[*tag_index].support_ty;
                let logical = rebind_source_lifetime(ir, &field.ty, quote!(#source));
                let payload_lifetime =
                    fresh_generated_lifetime(ir, "__zero_schema_payload_logical");
                let payload_logical =
                    rebind_source_lifetime(ir, &field.ty, quote!(#payload_lifetime));
                tagged_type_bounds.push(quote!(for<#payload_lifetime> #payload: #runtime::__private::TaggedPayloadTypeSupport<Tag = #tag, Logical<#payload_lifetime> = #payload_logical> + 'static));
                tagged_logical_bounds.push(quote!(#payload_support: #runtime::__private::TaggedPayloadLogicalMutation<#logical>));
            }
            FieldCategory::Array { element, .. }
                if matches!(element.as_ref(), FieldCategory::Path { .. }) =>
            {
                let Type::Array(array) = &field.ty else {
                    unreachable!("array category has an array type")
                };
                let child = analyze::support_type(&array.elem);
                let child_support =
                    quote!(<#child as #runtime::__private::WireTypeSupport>::Support);
                let logical = array_value_type(ir, element, &array.elem, quote!(#source));
                child_support_bounds
                    .push(quote!(#child: #runtime::__private::WireTypeSupport + 'static));
                nested_bounds.push(
                    quote!(#child_support: #runtime::__private::SchemaLogicalMutation<#logical>),
                );
            }
            FieldCategory::Optional {
                inner,
                inner_ty,
                inner_support_ty,
            } => match inner.as_ref() {
                FieldCategory::Path { .. } => {
                    let child = inner_support_ty;
                    let child_support =
                        quote!(<#child as #runtime::__private::WireTypeSupport>::Support);
                    let logical = rebind_source_lifetime(ir, inner_ty, quote!(#source));
                    child_support_bounds
                        .push(quote!(#child: #runtime::__private::OptionalWireType + 'static));
                    nested_bounds.push(quote!(#child_support: #runtime::__private::SchemaLogicalMutation<#logical>));
                }
                FieldCategory::Array { element, .. } => {
                    let Type::Array(array) = inner_ty.as_ref() else {
                        unreachable!("optional array type")
                    };
                    let Type::Array(support_array) = inner_support_ty.as_ref() else {
                        unreachable!("optional array support type")
                    };
                    let child = &support_array.elem;
                    let child_support =
                        quote!(<#child as #runtime::__private::WireTypeSupport>::Support);
                    let logical = array_value_type(ir, element, &array.elem, quote!(#source));
                    child_support_bounds
                        .push(quote!(#child: #runtime::__private::OptionalWireType + 'static));
                    nested_bounds.push(quote!(#child_support: #runtime::__private::SchemaLogicalMutation<#logical>));
                }
                _ => unreachable!("optional analysis accepts only path values or path arrays"),
            },
            _ => {}
        }
    }

    if source_predicates.is_empty()
        && wire_predicates.is_empty()
        && wire_type_bounds.is_empty()
        && dependencies.is_empty()
        && optional_dependencies.is_empty()
        && child_support_bounds.is_empty()
        && nested_bounds.is_empty()
        && tagged_type_bounds.is_empty()
        && tagged_logical_bounds.is_empty()
    {
        return TokenStream::new();
    }
    quote!(where
        #(#source_predicates,)*
        #(#wire_predicates,)*
        #(#wire_type_bounds,)*
        #(#dependencies: #runtime::__private::WireTypeSupport + 'static,)*
        #(#optional_dependencies: #runtime::__private::OptionalWireType + 'static,)*
        #(#child_support_bounds,)*
        #(#nested_bounds,)*
        #(#tagged_type_bounds,)*
        #(#tagged_logical_bounds,)*
    )
}

fn array_adapter_support_where(ir: &SchemaIr, field: &FieldIr, runtime: &Path) -> TokenStream {
    let wire_predicates = emit_wire::erased_where_predicates(&ir.generics.original);
    let wire_type_bounds = wire_type_parameter_predicates(ir);
    let dependencies = emit_wire::parent_dependency_types(&ir.fields);
    let optional_dependencies = emit_wire::optional_dependency_types(&ir.fields);
    let array_source = fresh_generated_lifetime(ir, "__zero_schema_array_value");
    let relevant_parameters = match &field.ty {
        Type::Array(array) => array_element_parameters(ir, &array.elem),
        _ => std::collections::BTreeSet::new(),
    };
    let requires_source_bounds = matches!(&field.category, FieldCategory::Array { .. })
        && matches!(&field.ty, Type::Array(array) if type_uses_declared_source_lifetime(ir, &array.elem));
    let source_predicates = if requires_source_bounds {
        hrtb_source_predicates(ir, &array_source, &relevant_parameters)
    } else {
        Vec::new()
    };
    let source_type_bounds = if requires_source_bounds {
        hrtb_source_type_parameter_predicates(ir, &array_source, &relevant_parameters)
    } else {
        Vec::new()
    };
    let logical_bound = match &field.category {
        FieldCategory::Array { element, .. }
            if matches!(element.as_ref(), FieldCategory::Path { .. }) =>
        {
            let Type::Array(array) = &field.ty else {
                unreachable!("array category has an array type")
            };
            let child = analyze::support_type(&array.elem);
            let child_support = quote!(<#child as #runtime::__private::WireTypeSupport>::Support);
            let logical = array_value_type(ir, element, &array.elem, quote!(#array_source));
            quote!(for<#array_source> #child_support: #runtime::__private::SchemaLogicalMutation<#logical>,)
        }
        _ => TokenStream::new(),
    };
    if wire_predicates.is_empty()
        && wire_type_bounds.is_empty()
        && dependencies.is_empty()
        && optional_dependencies.is_empty()
        && source_predicates.is_empty()
        && source_type_bounds.is_empty()
        && logical_bound.is_empty()
    {
        return TokenStream::new();
    }
    quote!(where
        #(#wire_predicates,)*
        #(#wire_type_bounds,)*
        #(#dependencies: #runtime::__private::WireTypeSupport + 'static,)*
        #(#optional_dependencies: #runtime::__private::OptionalWireType + 'static,)*
        #(#source_predicates,)*
        #(#source_type_bounds,)*
        #logical_bound
    )
}
fn field_adapter_name(field: &FieldIr) -> Ident {
    format_ident!("{}Adapter", pascal(&field.logical_name))
}
fn array_adapter_name(field: &FieldIr) -> Ident {
    format_ident!("{}ArrayAdapter", pascal(&field.logical_name))
}
fn option_adapter_name(field: &FieldIr) -> Ident {
    format_ident!("{}OptionAdapter", pascal(&field.logical_name))
}
fn array_scalar_adapter_name(field: &FieldIr) -> Ident {
    format_ident!("{}ArrayScalarAdapter", pascal(&field.logical_name))
}

fn pascal(value: &str) -> String {
    let mut output = String::new();
    let mut uppercase = true;
    for character in value.chars() {
        if character == '_' {
            uppercase = true;
        } else if uppercase {
            output.extend(character.to_uppercase());
            uppercase = false;
        } else {
            output.push(character);
        }
    }
    output
}

fn array_logical_type(
    _ir: &SchemaIr,
    category: &FieldCategory,
    ty: &Type,
    runtime: &Path,
    lifetime: TokenStream,
) -> TokenStream {
    match category {
        FieldCategory::Primitive(_) => quote!(#ty),
        FieldCategory::Bool => quote!(bool),
        FieldCategory::Path { .. } => {
            let support = analyze::support_type(ty);
            quote!(<<#support as #runtime::__private::WireTypeSupport>::Support as #runtime::__private::SchemaSupport>::Ref<#lifetime>)
        }
        _ => quote!(#ty),
    }
}

fn array_value_type(
    ir: &SchemaIr,
    category: &FieldCategory,
    ty: &Type,
    lifetime: TokenStream,
) -> TokenStream {
    match category {
        FieldCategory::Primitive(_) => quote!(#ty),
        FieldCategory::Bool => quote!(bool),
        FieldCategory::Path { .. } => rebind_source_lifetime(ir, ty, lifetime),
        _ => quote!(#ty),
    }
}

/// Rebinds only declaration lifetimes. Lifetimes introduced by higher-ranked
/// bounds stay in their lexical scope, and `'static` remains exact.
fn rebind_source_lifetime(ir: &SchemaIr, ty: &Type, lifetime: TokenStream) -> TokenStream {
    let ty = rebind_source_type(ir, ty, lifetime);
    quote!(#ty)
}

fn rebind_source_type(ir: &SchemaIr, ty: &Type, lifetime: TokenStream) -> Type {
    let lifetime: Lifetime = syn::parse2(lifetime).expect("generated source lifetime");
    let mut ty = analyze::logical_source_type(ty);
    source_lifetime_rebinder(ir, lifetime).visit_type_mut(&mut ty);
    ty
}

fn source_lifetime_rebinder(ir: &SchemaIr, lifetime: Lifetime) -> SourceLifetimeRebinder {
    let source_lifetimes = ir
        .generics
        .original
        .lifetimes()
        .map(|parameter| parameter.lifetime.ident.to_string())
        .collect::<std::collections::BTreeSet<_>>();
    SourceLifetimeRebinder {
        source_lifetimes,
        bound_lifetimes: Vec::new(),
        lifetime,
    }
}

struct SourceLifetimeRebinder {
    source_lifetimes: std::collections::BTreeSet<String>,
    bound_lifetimes: Vec<std::collections::BTreeSet<String>>,
    lifetime: Lifetime,
}

impl VisitMut for SourceLifetimeRebinder {
    fn visit_bound_lifetimes_mut(&mut self, bound: &mut syn::BoundLifetimes) {
        let names = bound
            .lifetimes
            .iter()
            .filter_map(|parameter| match parameter {
                syn::GenericParam::Lifetime(lifetime) => Some(lifetime.lifetime.ident.to_string()),
                syn::GenericParam::Type(_) | syn::GenericParam::Const(_) => None,
            })
            .collect();
        self.bound_lifetimes.push(names);
        syn::visit_mut::visit_bound_lifetimes_mut(self, bound);
        self.bound_lifetimes.pop();
    }

    fn visit_lifetime_mut(&mut self, current: &mut Lifetime) {
        let is_bound = self
            .bound_lifetimes
            .iter()
            .any(|bound| bound.contains(&current.ident.to_string()));
        if self.source_lifetimes.contains(&current.ident.to_string()) && !is_bound {
            *current = self.lifetime.clone();
        }
    }
}

fn logical_source_lifetime(ir: &SchemaIr) -> Lifetime {
    fresh_generated_lifetime(ir, "__zero_schema_source")
}

fn fresh_generated_lifetime(ir: &SchemaIr, stem: &str) -> Lifetime {
    struct LifetimeNames(std::collections::BTreeSet<String>);
    impl<'ast> Visit<'ast> for LifetimeNames {
        fn visit_lifetime(&mut self, lifetime: &'ast Lifetime) {
            self.0.insert(lifetime.ident.to_string());
        }
    }

    let mut names = LifetimeNames(std::collections::BTreeSet::new());
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
            return Lifetime::new(&format!("'{candidate}"), proc_macro2::Span::call_site());
        }
        suffix += 1;
    }
}

fn logical_generic_parameters(ir: &SchemaIr, source: &Lifetime) -> Vec<TokenStream> {
    ir.generics
        .original
        .params
        .iter()
        .filter_map(|parameter| match parameter {
            syn::GenericParam::Lifetime(_) => None,
            syn::GenericParam::Type(parameter) => {
                let mut parameter = parameter.clone();
                source_lifetime_rebinder(ir, source.clone()).visit_type_param_mut(&mut parameter);
                Some(quote!(#parameter))
            }
            syn::GenericParam::Const(parameter) => {
                let ident = &parameter.ident;
                let ty = &parameter.ty;
                Some(quote!(const #ident: #ty))
            }
        })
        .collect()
}

fn logical_source_predicates(ir: &SchemaIr, source: &Lifetime) -> Vec<syn::WherePredicate> {
    let Some(where_clause) = &ir.generics.original.where_clause else {
        return Vec::new();
    };
    where_clause
        .predicates
        .iter()
        .filter_map(|predicate| match predicate {
            syn::WherePredicate::Lifetime(_) => None,
            syn::WherePredicate::Type(_) => {
                let mut predicate = predicate.clone();
                source_lifetime_rebinder(ir, source.clone())
                    .visit_where_predicate_mut(&mut predicate);
                Some(predicate)
            }
            _ => None,
        })
        .collect()
}

fn hrtb_source_predicates(
    ir: &SchemaIr,
    source: &Lifetime,
    relevant_parameters: &std::collections::BTreeSet<String>,
) -> Vec<syn::WherePredicate> {
    let Some(where_clause) = &ir.generics.original.where_clause else {
        return Vec::new();
    };
    where_clause
        .predicates
        .iter()
        .filter_map(|predicate| {
            let syn::WherePredicate::Type(_) = predicate else {
                return None;
            };
            if !predicate_mentions_parameters(predicate, relevant_parameters) {
                return None;
            }
            let mut predicate = predicate.clone();
            source_lifetime_rebinder(ir, source.clone()).visit_where_predicate_mut(&mut predicate);
            bind_predicate_lifetime(&mut predicate, source);
            Some(predicate)
        })
        .collect()
}

fn hrtb_source_type_parameter_predicates(
    ir: &SchemaIr,
    source: &Lifetime,
    relevant_parameters: &std::collections::BTreeSet<String>,
) -> Vec<syn::WherePredicate> {
    ir.generics
        .original
        .type_params()
        .filter_map(|parameter| {
            if !relevant_parameters.contains(&parameter.ident.to_string()) {
                return None;
            }
            let mut parameter = parameter.clone();
            source_lifetime_rebinder(ir, source.clone()).visit_type_param_mut(&mut parameter);
            if parameter.bounds.is_empty() {
                return None;
            }
            let ident = &parameter.ident;
            let bounds = &parameter.bounds;
            let mut predicate: syn::WherePredicate = syn::parse_quote!(#ident: #bounds);
            bind_predicate_lifetime(&mut predicate, source);
            Some(predicate)
        })
        .collect()
}

fn array_element_parameters(ir: &SchemaIr, ty: &Type) -> std::collections::BTreeSet<String> {
    let declared = ir
        .generics
        .original
        .params
        .iter()
        .filter_map(|parameter| match parameter {
            syn::GenericParam::Type(parameter) => Some(parameter.ident.to_string()),
            syn::GenericParam::Const(parameter) => Some(parameter.ident.to_string()),
            syn::GenericParam::Lifetime(_) => None,
        })
        .collect::<std::collections::BTreeSet<_>>();
    struct Collector {
        declared: std::collections::BTreeSet<String>,
        used: std::collections::BTreeSet<String>,
    }
    impl Collector {
        fn mark(&mut self, path: &syn::Path) {
            if let Some(segment) = path.segments.first() {
                let name = segment.ident.to_string();
                if self.declared.contains(&name) {
                    self.used.insert(name);
                }
            }
        }
    }
    impl<'ast> Visit<'ast> for Collector {
        fn visit_type_path(&mut self, path: &'ast syn::TypePath) {
            self.mark(&path.path);
            syn::visit::visit_type_path(self, path);
        }

        fn visit_expr_path(&mut self, path: &'ast syn::ExprPath) {
            self.mark(&path.path);
            syn::visit::visit_expr_path(self, path);
        }
    }
    let mut collector = Collector {
        declared,
        used: std::collections::BTreeSet::new(),
    };
    collector.visit_type(ty);
    collector.used
}

fn predicate_mentions_parameters(
    predicate: &syn::WherePredicate,
    parameters: &std::collections::BTreeSet<String>,
) -> bool {
    if parameters.is_empty() {
        return false;
    }
    struct Contains<'a> {
        parameters: &'a std::collections::BTreeSet<String>,
        found: bool,
    }
    impl Contains<'_> {
        fn contains(&mut self, path: &syn::Path) {
            if let Some(segment) = path.segments.first() {
                self.found |= self.parameters.contains(&segment.ident.to_string());
            }
        }
    }
    impl<'ast> Visit<'ast> for Contains<'_> {
        fn visit_type_path(&mut self, path: &'ast syn::TypePath) {
            self.contains(&path.path);
            syn::visit::visit_type_path(self, path);
        }

        fn visit_expr_path(&mut self, path: &'ast syn::ExprPath) {
            self.contains(&path.path);
            syn::visit::visit_expr_path(self, path);
        }
    }
    let mut contains = Contains {
        parameters,
        found: false,
    };
    contains.visit_where_predicate(predicate);
    contains.found
}

fn type_uses_declared_source_lifetime(ir: &SchemaIr, ty: &Type) -> bool {
    let replacement = fresh_generated_lifetime(ir, "__zero_schema_source_probe");
    quote!(#ty).to_string() != rebind_source_lifetime(ir, ty, quote!(#replacement)).to_string()
}

fn bind_predicate_lifetime(predicate: &mut syn::WherePredicate, source: &Lifetime) {
    let syn::WherePredicate::Type(predicate) = predicate else {
        return;
    };
    let parameter: syn::GenericParam = syn::parse_quote!(#source);
    match &mut predicate.lifetimes {
        Some(lifetimes) => lifetimes.lifetimes.insert(0, parameter),
        None => predicate.lifetimes = Some(syn::parse_quote!(for<#source>)),
    }
}

fn wire_type_parameter_predicates(ir: &SchemaIr) -> Vec<TokenStream> {
    let static_lifetime = Lifetime::new("'static", proc_macro2::Span::call_site());
    ir.generics
        .original
        .type_params()
        .filter_map(|parameter| {
            let mut parameter = parameter.clone();
            source_lifetime_rebinder(ir, static_lifetime.clone())
                .visit_type_param_mut(&mut parameter);
            let ident = &parameter.ident;
            (!parameter.bounds.is_empty()).then(|| {
                let bounds = &parameter.bounds;
                quote!(#ident: #bounds)
            })
        })
        .collect()
}
