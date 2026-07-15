use proc_macro2::{Ident, TokenStream};
use quote::{format_ident, quote};
use syn::{GenericParam, Lifetime, Path, Type};

use crate::{
    analyze::{self, FieldCategory, FieldIr, ItemKind, ScalarRepr, SchemaIr, VariantShape},
    emit_wire,
};

/// Emit the read-side capability implementation directly inside the generated
/// hidden support module.  Keeping this code beside the private wire is what
/// prevents public capabilities from ever spelling a wire reference.
pub(crate) fn emit_module(
    ir: &SchemaIr,
    runtime: &Path,
    support_runtime: &Path,
) -> syn::Result<TokenStream> {
    match ir.kind {
        ItemKind::Struct => emit_record_module(ir, runtime, support_runtime),
        ItemKind::ScalarEnum { repr } => emit_scalar_module(ir, repr, runtime, support_runtime),
        ItemKind::TaggedEnum => emit_tagged_module(ir, runtime, support_runtime),
    }
}

pub(crate) fn root_wire_invariant(ir: &SchemaIr, wire: &TokenStream) -> TokenStream {
    if matches!(ir.kind, ItemKind::TaggedEnum) {
        TokenStream::new()
    } else {
        quote!(assert!(::core::mem::size_of::<#wire>() != 0, "zero-sized root schemas are unsupported");)
    }
}

/// Root inherent APIs and the public aliases for capability/error types.
pub(crate) fn emit_root_surface(
    ir: &SchemaIr,
    runtime: &Path,
    _support_runtime: &Path,
) -> syn::Result<TokenStream> {
    if matches!(ir.kind, ItemKind::TaggedEnum) {
        let visibility = &ir.visibility;
        let module = &ir.names.support_module;
        let reference = &ir.names.reference;
        let mutable = &ir.names.mutable;
        let access_error = &ir.names.access_error;
        let mutation_error = &ir.names.mutation_error;
        let patch = &ir.names.patch;
        return Ok(quote!(
            #visibility use self::#module::{#reference, #mutable, #patch, #access_error, #mutation_error};
        ));
    }

    let module = &ir.names.support_module;
    let ident = &ir.ident;
    let visibility = &ir.visibility;
    let reference = &ir.names.reference;
    let mutable = &ir.names.mutable;
    let access_error = &ir.names.access_error;
    let mutation_error = &ir.names.mutation_error;
    let patch = &ir.names.patch;
    let plain_arguments = emit_wire::wire_arguments(&ir.generics.original);
    let original = &ir.generics.original;
    let mut surface_generics = match ir.kind {
        ItemKind::Struct => emit_wire::impl_generics_with_support_dependencies(
            original,
            &emit_wire::parent_dependency_types(&ir.fields),
            runtime,
        ),
        ItemKind::ScalarEnum { .. } => original.clone(),
        ItemKind::TaggedEnum => unreachable!(),
    };
    if matches!(ir.kind, ItemKind::Struct) {
        emit_wire::add_optional_wire_type_bounds(&mut surface_generics, &ir.fields, runtime);
        surface_generics
            .make_where_clause()
            .predicates
            .extend(record_support_predicates(ir, runtime));
    }
    let (impl_generics, original_arguments, where_clause) = surface_generics.split_for_impl();
    let support = support_name(ir);
    let cap_args = capability_arguments(ir);
    let patch_projection_arguments = patch_projection_arguments(ir, quote!('source));
    let logical_schema = logical_schema_impl(ir, runtime);

    let exports = match ir.kind {
        ItemKind::Struct => quote!(
            #visibility use self::#module::{#reference, #mutable, #patch, #access_error, #mutation_error};
        ),
        ItemKind::ScalarEnum { .. } => quote!(
            #visibility use self::#module::{#reference, #mutable, #access_error, #mutation_error, #patch};
        ),
        ItemKind::TaggedEnum => unreachable!(),
    };

    let methods = match ir.kind {
        ItemKind::Struct => quote!(
            pub fn access<'wire>(bytes: &'wire [::core::primitive::u8]) -> ::core::result::Result<#reference #cap_args, #access_error #plain_arguments> {
                let _ = Self::SCHEMA_SIZE;
                <#module::#support #plain_arguments>::__zero_schema_access(bytes)
            }

            pub fn access_mut<'wire>(bytes: &'wire mut [::core::primitive::u8]) -> ::core::result::Result<#mutable #cap_args, #access_error #plain_arguments> {
                let _ = Self::SCHEMA_SIZE;
                <#module::#support #plain_arguments>::__zero_schema_access_mut(bytes)
            }
        ),
        ItemKind::ScalarEnum { .. } => quote!(
            pub fn access<'wire>(bytes: &'wire [::core::primitive::u8]) -> ::core::result::Result<#reference<'wire>, #access_error> {
                #module::#support::__zero_schema_access(bytes)
            }

            pub fn access_mut<'wire>(bytes: &'wire mut [::core::primitive::u8]) -> ::core::result::Result<#mutable<'wire>, #access_error> {
                #module::#support::__zero_schema_access_mut(bytes)
            }
        ),
        ItemKind::TaggedEnum => unreachable!(),
    };

    let zero_state = wire_type_zero_state(ir, runtime)?;
    let wire_type_support = match ir.kind {
        ItemKind::Struct | ItemKind::ScalarEnum { .. } => quote!(
            impl #impl_generics #runtime::__private::WireTypeSupport for #ident #original_arguments #where_clause {
                type Support = #module::#support #plain_arguments;
                type ZeroState = #zero_state;
            }
            #logical_schema
        ),
        ItemKind::TaggedEnum => TokenStream::new(),
    };
    let patch_type_support = if matches!(ir.kind, ItemKind::Struct | ItemKind::ScalarEnum { .. }) {
        quote!(
            impl #impl_generics #runtime::__private::SchemaPatchType for #ident #original_arguments #where_clause {
                type Patch<'source> = #module::#patch #patch_projection_arguments;
            }
        )
    } else {
        TokenStream::new()
    };
    Ok(quote!(
        #exports
        #wire_type_support
        #patch_type_support
        impl #impl_generics #ident #original_arguments #where_clause {
            #methods
        }
    ))
}

fn emit_record_module(
    ir: &SchemaIr,
    runtime: &Path,
    support_runtime: &Path,
) -> syn::Result<TokenStream> {
    let ident = &ir.ident;
    let visibility = &ir.visibility;
    let reference = &ir.names.reference;
    let mutable = &ir.names.mutable;
    let access_error = &ir.names.access_error;
    let mutation_error = &ir.names.mutation_error;
    let patch = &ir.names.patch;
    let support_visibility = if matches!(visibility, syn::Visibility::Inherited) {
        quote!(pub(super))
    } else {
        quote!(#visibility)
    };
    let support = support_name(ir);
    let input_access_token = format_ident!("__ZeroSchemaInputAccessToken");
    let owner = owner_name(ir);
    let plain_generics = emit_wire::wire_generics(&ir.generics.original);
    let plain_arguments = emit_wire::wire_arguments(&ir.generics.original);
    let cap_generics = capability_generics(ir);
    let cap_args = capability_arguments(ir);
    let wire = &ir.names.wire;
    let root_wire = root_wire_inside(ir, support_runtime);
    let logical = logical_type(ir, quote!('wire));
    let nonzero_array_assertions = emit_wire::nonzero_assertions(&ir.fields);
    let mutable_logical = logical_type(ir, quote!('view));
    let field_getters_ref = getter_methods(ir, runtime, support_runtime, false)?;
    let field_getters_mut = getter_methods(ir, runtime, support_runtime, true)?;
    let materialized_ref = materialized_fields(ir, runtime, support_runtime, false)?;
    let materialized_mut = materialized_fields(ir, runtime, support_runtime, true)?;
    let errors = emit_record_errors(ir, runtime)?;
    let mutation_methods =
        crate::emit_mutation::emit_record_mutation_methods(ir, runtime, support_runtime)?;
    let mutation_adapters =
        crate::emit_mutation::emit_record_adapters(ir, runtime, support_runtime)?;
    let patches = crate::emit_patch::emit_record_patch(ir, runtime, support_runtime)?;
    let error_boundary = crate::errors::boundary_marker();
    let proof = record_proof(ir, runtime, support_runtime)?;
    let support_predicates = record_support_predicates(ir, runtime);
    let support_where = where_clause(&support_predicates);
    let ref_lifetime = Lifetime::new("'wire", proc_macro2::Span::call_site());
    let mut ref_logical_predicates = logical_value_predicates(ir, ref_lifetime.clone());
    ref_logical_predicates.extend(optional_materialize_predicates(ir, runtime, ref_lifetime));
    let ref_logical_where = where_clause(&ref_logical_predicates);
    let mut ref_logical_with_support = support_predicates.clone();
    ref_logical_with_support.extend(ref_logical_predicates);
    let ref_logical_with_support_where = where_clause(&ref_logical_with_support);
    let mut_lifetime = Lifetime::new("'view", proc_macro2::Span::call_site());
    let mut mut_logical_predicates = logical_value_predicates(ir, mut_lifetime.clone());
    mut_logical_predicates.extend(optional_materialize_predicates(ir, runtime, mut_lifetime));
    let mut_logical_where = where_clause(&mut_logical_predicates);
    let (patch_method_generics, patch_method_args) = patch_method_generics_and_args(ir);
    let copy_from_method = quote!(
        pub fn copy_from #patch_method_generics (
            &mut self,
            patch: &#patch #patch_method_args,
        ) -> ::core::result::Result<(), #mutation_error #plain_arguments> {
            <#support #plain_arguments as #runtime::__private::SchemaSupport>::preflight_patch(self.input.shared(), patch)?;
            <#support #plain_arguments as #runtime::__private::SchemaSupport>::commit_patch(self.input.reborrow(), patch, self.token);
            Ok(())
        }
    );

    Ok(quote!(
        #errors

        pub struct #owner #plain_generics(::core::marker::PhantomData<fn() -> #root_wire>) #support_where;

        impl #plain_generics #runtime::__private::OwnerAdapter for #owner #plain_arguments #support_where {
            type AccessError = #access_error #plain_arguments;
            type MutationError = #mutation_error #plain_arguments;

            fn access_layout(error: #runtime::LayoutError) -> Self::AccessError {
                #access_error::Layout(error)
            }

            fn mutation_layout(error: #runtime::LayoutError) -> Self::MutationError {
                #mutation_error::Layout(error)
            }
        }

        #[doc(hidden)]
        #[derive(Clone, Copy)]
        pub struct #input_access_token { _private: () }

        pub struct #reference #cap_generics #support_where {
            input: #runtime::__private::SharedInput<'wire, #root_wire>,
        }

        impl #cap_generics ::core::marker::Copy for #reference #cap_args #support_where {}
        impl #cap_generics ::core::clone::Clone for #reference #cap_args #support_where {
            fn clone(&self) -> Self { *self }
        }

        impl #cap_generics ::core::fmt::Debug for #reference #cap_args #support_where {
            fn fmt(&self, formatter: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result { formatter.write_str(stringify!(#reference)) }
        }
        impl #cap_generics #reference #cap_args #support_where {
            #field_getters_ref

            pub fn copy_into(&self) -> #logical #ref_logical_where {
                super::#ident { #materialized_ref }
            }
        }

        impl #cap_generics #runtime::__private::Materialize<#logical> for #reference #cap_args #ref_logical_with_support_where {
            fn materialize(&self) -> #logical { self.copy_into() }
        }

        pub struct #mutable #cap_generics #support_where {
            input: #runtime::__private::ExclusiveInput<'wire, #root_wire>,
            token: #input_access_token,
        }

        impl #cap_generics #mutable #cap_args #support_where {
            #field_getters_mut
            #mutation_methods

            pub fn copy_into<'view>(&'view self) -> #mutable_logical #mut_logical_where {
                super::#ident { #materialized_mut }
            }

            #copy_from_method
        }

        #error_boundary
        #mutation_adapters

        #support_visibility struct #support #plain_generics(::core::marker::PhantomData<fn() -> #root_wire>) #support_where;

        impl #plain_generics #runtime::__private::InputAccess for #support #plain_arguments #support_where {
            type Token = #input_access_token;
        }

        impl #plain_generics #runtime::__private::RootInputAccess for #wire #plain_arguments #support_where {
            type Token = #input_access_token;
        }
        impl #plain_generics #support #plain_arguments #support_where {
            const __ZERO_SCHEMA_NONZERO_ARRAYS: () = {
                #(#nonzero_array_assertions)*
            };
            pub(super) fn __zero_schema_access<'wire>(
                bytes: &'wire [::core::primitive::u8],
            ) -> ::core::result::Result<#reference #cap_args, #access_error #plain_arguments> {
                let input = #runtime::__private::SharedInput::<#root_wire>::from_exact(
                    bytes,
                    #input_access_token { _private: () },
                )
                .map_err(<#owner #plain_arguments as #runtime::__private::OwnerAdapter>::access_layout)?;
                let proof = <Self as #runtime::__private::SchemaSupport>::prove(input)?;
                Ok(<Self as #runtime::__private::SchemaSupport>::make_ref(proof))
            }

            pub(super) fn __zero_schema_access_mut<'wire>(
                bytes: &'wire mut [::core::primitive::u8],
            ) -> ::core::result::Result<#mutable #cap_args, #access_error #plain_arguments> {
                let input = #runtime::__private::ExclusiveInput::<#root_wire>::from_exact(
                    bytes,
                    #input_access_token { _private: () },
                )
                .map_err(<#owner #plain_arguments as #runtime::__private::OwnerAdapter>::access_layout)?;
                let proof = <Self as #runtime::__private::SchemaSupport>::prove_mut(input)?;
                Ok(<Self as #runtime::__private::SchemaSupport>::make_mut(proof))
            }
        }

        #patches
        impl #plain_generics #runtime::__private::SchemaSupport for #support #plain_arguments #support_where {
            type Wire = #root_wire;
            type Owner = #owner #plain_arguments;
            type Ref<'wire> = #reference #cap_args;
            type Mut<'wire> = #mutable #cap_args;

            fn validate<'wire>(
                input: #runtime::__private::SharedInput<'wire, Self::Wire>,
            ) -> ::core::result::Result<(), <Self::Owner as #runtime::__private::OwnerAdapter>::AccessError> {
                let _ = Self::__ZERO_SCHEMA_NONZERO_ARRAYS;
                #proof
                Ok(())
            }

            fn make_ref<'wire>(
                proof: #runtime::__private::ProvedShared<'wire, Self, Self::Wire>,
            ) -> Self::Ref<'wire> {
                #reference { input: proof.into_input(#input_access_token { _private: () }) }
            }

            fn make_mut<'wire>(
                proof: #runtime::__private::ProvedExclusive<'wire, Self, Self::Wire>,
            ) -> Self::Mut<'wire> {
                #mutable {
                    input: proof.into_input(#input_access_token { _private: () }),
                    token: #input_access_token { _private: () },
                }
            }

            fn input_token(_: &#runtime::__private::ExclusiveInput<'_, Self::Wire>) -> Self::Token {
                #input_access_token { _private: () }
            }

            fn preflight_patch<'wire, P>(
                input: #runtime::__private::SharedInput<'wire, Self::Wire>,
                patch: &P,
            ) -> ::core::result::Result<(), <Self::Owner as #runtime::__private::OwnerAdapter>::MutationError>
            where P: #runtime::__private::SchemaPatch<Self> {
                Self::validate(input).map_err(<Self::Owner as #runtime::__private::OwnerAdapter>::MutationError::from)?;
                patch.preflight(input)
            }

            fn commit_patch<'wire, P>(
                input: #runtime::__private::ExclusiveInput<'wire, Self::Wire>,
                patch: &P,
                token: Self::Token,
            )
            where P: #runtime::__private::SchemaPatch<Self> {
                patch.commit(input, token)
            }
        }
    ))
}

fn emit_scalar_module(
    ir: &SchemaIr,
    repr: ScalarRepr,
    runtime: &Path,
    support_runtime: &Path,
) -> syn::Result<TokenStream> {
    let reference = &ir.names.reference;
    let mutable = &ir.names.mutable;
    let access_error = &ir.names.access_error;
    let mutation_error = &ir.names.mutation_error;
    let input_access_token = format_ident!("__ZeroSchemaInputAccessToken");
    let patch = &ir.names.patch;
    let support = support_name(ir);
    let owner = owner_name(ir);
    let ident = &ir.ident;
    let visibility = &ir.visibility;
    let logical_name = &ir.logical_name;
    let wire = &ir.names.wire;
    let root_wire = root_wire_inside(ir, support_runtime);
    let wire_inner = repr.wire_ident(ir.options.endian);
    let scalar_offset = if ir.options.align.is_some() {
        quote!(<#root_wire>::VALUE_OFFSET)
    } else {
        quote!(0)
    };
    let raw_type = match repr {
        ScalarRepr::U8 => quote!(::core::primitive::u8),
        ScalarRepr::U16 => quote!(::core::primitive::u16),
        ScalarRepr::U32 => quote!(::core::primitive::u32),
    };
    let support_visibility = if matches!(visibility, syn::Visibility::Inherited) {
        quote!(pub(super))
    } else {
        quote!(#visibility)
    };
    let from_arms = ir.variants.iter().map(|variant| {
        let variant = &variant.ident;
        quote!(raw if raw == super::#ident::#variant as #raw_type => Some(super::#ident::#variant),)
    });
    let to_arms = ir.variants.iter().map(|variant| {
        let variant = &variant.ident;
        quote!(super::#ident::#variant => super::#ident::#variant as #raw_type,)
    });

    Ok(quote!(
        #[derive(Debug)]
        pub enum #access_error {
            Layout(#runtime::LayoutError),
            UnknownEnumValue,
        }

        impl ::core::fmt::Display for #access_error {
            fn fmt(&self, formatter: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
                #runtime::__private::__fmt_schema_error(self, formatter)
            }
        }
        impl ::core::error::Error for #access_error {
            fn source(&self) -> Option<&(dyn ::core::error::Error + 'static)> {
                match self { Self::Layout(source) => Some(source), Self::UnknownEnumValue => None }
            }
        }
        impl #runtime::SchemaError for #access_error {
            fn kind(&self) -> #runtime::ErrorKind {
                match self {
                    Self::Layout(_) => #runtime::ErrorKind::Layout,
                    Self::UnknownEnumValue => #runtime::ErrorKind::UnknownEnumValue,
                }
            }
            fn schema(&self) -> &'static str { #logical_name }
            fn segment(&self) -> Option<#runtime::ErrorPathSegment> { None }
            fn child(&self) -> Option<&dyn #runtime::SchemaError> { None }
            fn __fmt_leaf(&self, formatter: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
                match self {
                    Self::Layout(error) => ::core::fmt::Display::fmt(error, formatter),
                    Self::UnknownEnumValue => formatter.write_str("unknown scalar enum value"),
                }
            }
        }

        #[derive(Debug)]
        pub enum #mutation_error {
            Access(#access_error),
            Layout(#runtime::LayoutError),
        }
        impl From<#access_error> for #mutation_error {
            fn from(error: #access_error) -> Self { Self::Access(error) }
        }
        impl ::core::fmt::Display for #mutation_error {
            fn fmt(&self, formatter: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
                #runtime::__private::__fmt_schema_error(self, formatter)
            }
        }
        impl ::core::error::Error for #mutation_error {
            fn source(&self) -> Option<&(dyn ::core::error::Error + 'static)> {
                match self { Self::Access(source) => Some(source), Self::Layout(source) => Some(source) }
            }
        }
        impl #runtime::SchemaError for #mutation_error {
            fn kind(&self) -> #runtime::ErrorKind {
                match self { Self::Access(source) => source.kind(), Self::Layout(_) => #runtime::ErrorKind::Layout }
            }
            fn schema(&self) -> &'static str { #logical_name }
            fn segment(&self) -> Option<#runtime::ErrorPathSegment> { None }
            fn child(&self) -> Option<&dyn #runtime::SchemaError> {
                match self { Self::Access(source) => Some(source), Self::Layout(_) => None }
            }
            fn __fmt_leaf(&self, formatter: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
                match self { Self::Access(source) => source.__fmt_leaf(formatter), Self::Layout(source) => ::core::fmt::Display::fmt(source, formatter) }
            }
        }

        pub struct #owner;
        impl #runtime::__private::OwnerAdapter for #owner {
            type AccessError = #access_error;
            type MutationError = #mutation_error;
            fn access_layout(error: #runtime::LayoutError) -> Self::AccessError { #access_error::Layout(error) }
            fn mutation_layout(error: #runtime::LayoutError) -> Self::MutationError { #mutation_error::Layout(error) }
        }

        #[doc(hidden)]
        #[derive(Clone, Copy)]
        pub struct #input_access_token { _private: () }

        pub struct #reference<'wire> { input: #runtime::__private::SharedInput<'wire, #root_wire> }
        impl ::core::marker::Copy for #reference<'_> {}
        impl ::core::clone::Clone for #reference<'_> { fn clone(&self) -> Self { *self } }
        impl<'wire> #reference<'wire> {
            pub fn get(&self) -> super::#ident { match <#support as #runtime::__private::ScalarEnumSupport>::from_raw(<#support as #runtime::__private::ScalarEnumSupport>::raw(self.input)) { Some(value) => value, None => unreachable!("scalar capability contains only a proved value") } }
            pub fn copy_into(&self) -> super::#ident { self.get() }
        }

        impl #runtime::__private::Materialize<super::#ident> for super::#ident {
            fn materialize(&self) -> super::#ident { *self }
        }

        impl<'wire> #runtime::__private::Materialize<super::#ident> for #reference<'wire> {
            fn materialize(&self) -> super::#ident { self.copy_into() }
        }

        pub struct #mutable<'wire> { input: #runtime::__private::ExclusiveInput<'wire, #root_wire>, token: #input_access_token }
        impl<'wire> #mutable<'wire> {
            pub fn get(&self) -> super::#ident { match <#support as #runtime::__private::ScalarEnumSupport>::from_raw(<#support as #runtime::__private::ScalarEnumSupport>::raw(self.input.shared())) { Some(value) => value, None => unreachable!("scalar capability contains only a proved value") } }
            pub fn set(&mut self, value: super::#ident) -> ::core::result::Result<(), #mutation_error> {
                <#support as #runtime::__private::ScalarEnumSupport>::commit(self.input.reborrow(), value, self.token);
                Ok(())
            }
            pub fn copy_into<'view>(&'view self) -> super::#ident { self.get() }
            pub fn copy_from(&mut self, patch: &#patch) -> ::core::result::Result<(), #mutation_error> {
                <#support as #runtime::__private::SchemaSupport>::preflight_patch(self.input.shared(), patch)?;
                <#support as #runtime::__private::SchemaSupport>::commit_patch(self.input.reborrow(), patch, self.token);
                Ok(())
            }
        }

        #[derive(Clone, Copy, Debug, Eq, PartialEq)]
        pub struct #patch { pub value: Option<super::#ident> }
        impl Default for #patch { fn default() -> Self { Self { value: None } } }
        impl From<super::#ident> for #patch { fn from(value: super::#ident) -> Self { Self { value: Some(value) } } }
        impl #patch { pub(crate) fn is_complete(&self) -> bool { self.value.is_some() } }

        #support_visibility struct #support;
        impl #runtime::__private::InputAccess for #support {
            type Token = #input_access_token;
        }
        impl #runtime::__private::RootInputAccess for #wire { type Token = #input_access_token; }
        impl #support {
            pub(super) fn __zero_schema_access<'wire>(
                bytes: &'wire [::core::primitive::u8],
            ) -> ::core::result::Result<#reference<'wire>, #access_error> {
                let input = #runtime::__private::SharedInput::<#root_wire>::from_exact(
                    bytes,
                    #input_access_token { _private: () },
                )
                .map_err(<#owner as #runtime::__private::OwnerAdapter>::access_layout)?;
                let proof = <Self as #runtime::__private::SchemaSupport>::prove(input)?;
                Ok(Self::make_view(proof))
            }

            pub(super) fn __zero_schema_access_mut<'wire>(
                bytes: &'wire mut [::core::primitive::u8],
            ) -> ::core::result::Result<#mutable<'wire>, #access_error> {
                let input = #runtime::__private::ExclusiveInput::<#root_wire>::from_exact(
                    bytes,
                    #input_access_token { _private: () },
                )
                .map_err(<#owner as #runtime::__private::OwnerAdapter>::access_layout)?;
                let proof = <Self as #runtime::__private::SchemaSupport>::prove_mut(input)?;
                Ok(<Self as #runtime::__private::SchemaSupport>::make_mut(proof))
            }
            pub(super) fn make_view<'wire>(proof: #runtime::__private::ProvedShared<'wire, Self, #root_wire>) -> #reference<'wire> {
                #reference { input: proof.into_input(#input_access_token { _private: () }) }
            }
        }
        impl #runtime::__private::SchemaSupport for #support {
            type Wire = #root_wire;
            type Owner = #owner;
            type Ref<'wire> = super::#ident;
            type Mut<'wire> = #mutable<'wire>;
            fn validate<'wire>(input: #runtime::__private::SharedInput<'wire, Self::Wire>) -> ::core::result::Result<(), #access_error> {
                if <Self as #runtime::__private::ScalarEnumSupport>::from_raw(<Self as #runtime::__private::ScalarEnumSupport>::raw(input)).is_some() { Ok(()) } else { Err(#access_error::UnknownEnumValue) }
            }
            fn make_ref<'wire>(proof: #runtime::__private::ProvedShared<'wire, Self, Self::Wire>) -> Self::Ref<'wire> {
                match <Self as #runtime::__private::ScalarEnumSupport>::from_raw(<Self as #runtime::__private::ScalarEnumSupport>::raw(proof.into_input(#input_access_token { _private: () }))) { Some(value) => value, None => unreachable!("a scalar token was validated before construction") }
            }
            fn make_mut<'wire>(proof: #runtime::__private::ProvedExclusive<'wire, Self, Self::Wire>) -> Self::Mut<'wire> {
                #mutable { input: proof.into_input(#input_access_token { _private: () }), token: #input_access_token { _private: () } }
            }
            fn input_token(_: &#runtime::__private::ExclusiveInput<'_, Self::Wire>) -> Self::Token { #input_access_token { _private: () } }
            fn preflight_patch<'wire, P>(input: #runtime::__private::SharedInput<'wire, Self::Wire>, patch: &P) -> ::core::result::Result<(), #mutation_error>
            where P: #runtime::__private::SchemaPatch<Self> {
                Self::validate(input).map_err(#mutation_error::from)?;
                patch.preflight(input)
            }
            fn commit_patch<'wire, P>(input: #runtime::__private::ExclusiveInput<'wire, Self::Wire>, patch: &P, token: Self::Token)
            where P: #runtime::__private::SchemaPatch<Self> { patch.commit(input, token) }
        }
        impl #runtime::__private::SchemaLogicalMutation<super::#ident> for #support {
            fn preflight_logical<'wire>(_: #runtime::__private::SharedInput<'wire, Self::Wire>, _: &super::#ident) -> ::core::result::Result<(), <Self::Owner as #runtime::__private::OwnerAdapter>::MutationError> { Ok(()) }
            fn commit_logical<'wire>(input: #runtime::__private::ExclusiveInput<'wire, Self::Wire>, value: &super::#ident, token: Self::Token) {
                <Self as #runtime::__private::ScalarEnumSupport>::commit(input, *value, token);
            }
        }
        impl #runtime::__private::ScalarEnumSupport for #support {
            type Raw = <#runtime::__private::#wire_inner as #runtime::__private::ScalarWire>::Raw;
            type Value = super::#ident;
            fn raw(input: #runtime::__private::SharedInput<'_, Self::Wire>) -> Self::Raw {
                let wire = match input.read_copy::<#runtime::__private::#wire_inner>(#scalar_offset) {
                    Ok(wire) => wire,
                    Err(_) => unreachable!("compiler-derived scalar storage remains selectable"),
                };
                <#runtime::__private::#wire_inner as #runtime::__private::ScalarWire>::load(&wire)
            }
            fn from_raw(raw: Self::Raw) -> Option<Self::Value> { match raw { #(#from_arms)* _ => None } }
            fn to_raw(value: Self::Value) -> Self::Raw { match value { #(#to_arms)* } }
            fn commit(mut input: #runtime::__private::ExclusiveInput<'_, Self::Wire>, value: Self::Value, token: Self::Token) {
                let bytes = match input.subrange_bytes_mut::<Self>(0, ::core::mem::size_of::<#root_wire>(), token) { Ok(bytes) => bytes, Err(_) => unreachable!("checked scalar enum input remains exact") };
                <#runtime::__private::#wire_inner as #runtime::__private::ScalarWire>::store_preflighted(Self::to_raw(value), bytes);
            }
        }
        impl #runtime::__private::SchemaPatch<#support> for #patch {
            fn is_complete(&self) -> bool { self.value.is_some() }
            fn preflight<'wire>(&self, _: #runtime::__private::SharedInput<'wire, #root_wire>) -> ::core::result::Result<(), #mutation_error> { Ok(()) }
            fn commit<'wire>(&self, input: #runtime::__private::ExclusiveInput<'wire, #root_wire>, token: <#support as #runtime::__private::InputAccess>::Token) {
                if let Some(value) = self.value {
                    <#support as #runtime::__private::ScalarEnumSupport>::commit(input, value, token);
                }
            }
        }
    ))
}

fn emit_tagged_module(
    ir: &SchemaIr,
    runtime: &Path,
    _support_runtime: &Path,
) -> syn::Result<TokenStream> {
    let patches = crate::emit_patch::emit_tagged_patch(ir, runtime, _support_runtime)?;
    let logical_mutation = crate::emit_mutation::emit_tagged_logical_mutation(ir, runtime)?;
    let wire = &ir.names.wire;
    let reference = &ir.names.reference;
    let mutable = &ir.names.mutable;
    let access_error = &ir.names.access_error;
    let mutation_error = &ir.names.mutation_error;
    let patch = &ir.names.patch;
    let input_access_token = format_ident!("__ZeroSchemaInputAccessToken");
    let support = support_name(ir);
    let owner = owner_name(ir);
    let plain_generics = emit_wire::wire_generics(&ir.generics.original);
    let plain_arguments = emit_wire::wire_arguments(&ir.generics.original);
    let cap_generics = capability_generics(ir);
    let cap_args = capability_arguments(ir);
    let ident = &ir.ident;
    let visibility = &ir.visibility;
    let tag_type = &ir.variants[0].tag_type;
    let logical_name = &ir.logical_name;
    let tagged_logical = logical_tagged_type(ir, quote!('wire));
    let mutable_tagged_logical = logical_tagged_type(ir, quote!('view));
    let (patch_method_generics, patch_method_args) = patch_method_generics_and_args(ir);

    // The owner type is not an error; regenerate the concrete variants below.
    let support_visibility = if matches!(visibility, syn::Visibility::Inherited) {
        quote!(pub(super))
    } else {
        quote!(#visibility)
    };
    let child_error_variants = ir.variants.iter().filter_map(|variant| match &variant.shape {
        VariantShape::Newtype(ty) => {
            let name = &variant.ident;
            let support_ty = analyze::support_type(ty);
            Some(quote!(#name(<<<#support_ty as #runtime::__private::WireTypeSupport>::Support as #runtime::__private::SchemaSupport>::Owner as #runtime::__private::OwnerAdapter>::AccessError)))
        }
        VariantShape::Unit => None,
    });
    let error_kind_arms = ir
        .variants
        .iter()
        .filter_map(|variant| match &variant.shape {
            VariantShape::Newtype(_) => {
                let name = &variant.ident;
                Some(quote!(Self::#name(source) => source.kind(),))
            }
            VariantShape::Unit => None,
        });
    let error_segment_arms = ir
        .variants
        .iter()
        .filter_map(|variant| match &variant.shape {
            VariantShape::Newtype(_) => {
                let name = &variant.ident;
                let logical = &variant.logical_name;
                Some(quote!(Self::#name(_) => Some(#runtime::ErrorPathSegment::Variant(#logical)),))
            }
            VariantShape::Unit => None,
        });
    let error_child_arms = ir
        .variants
        .iter()
        .filter_map(|variant| match &variant.shape {
            VariantShape::Newtype(_) => {
                let name = &variant.ident;
                Some(quote!(Self::#name(source) => Some(source),))
            }
            VariantShape::Unit => None,
        });
    let error_leaf_arms = ir
        .variants
        .iter()
        .filter_map(|variant| match &variant.shape {
            VariantShape::Newtype(_) => {
                let name = &variant.ident;
                Some(quote!(Self::#name(source) => source.__fmt_leaf(formatter),))
            }
            VariantShape::Unit => None,
        });
    let ref_accessors = ir.variants.iter().map(|variant| {
        let method = snake_ident(&variant.logical_name);
        let tag = &variant.tag;
        match &variant.shape {
            VariantShape::Unit => quote!(pub fn #method(&self) -> Option<()> { if self.tag == #tag { Some(()) } else { None } }),
            VariantShape::Newtype(ty) => {
                let support_ty = analyze::support_type(ty);
                let return_ty = schema_ref_type(&support_ty, runtime, quote!('wire));
                let wire_ty = quote!(<#support_ty as #runtime::__private::WireType>::Wire);
                quote!(
                    pub fn #method(&self) -> Option<#return_ty> {
                        if self.tag != #tag { return None; }
                        let input = match self.payload.subrange::<#wire_ty>(0) { Ok(input) => input, Err(_) => unreachable!("selected tagged payload range is asserted") };
                        let proof = match <#support_ty as #runtime::__private::WireTypeSupport>::Support::prove(input) {
                            Ok(proof) => proof,
                            Err(_) => unreachable!("a proved selected payload retains its active child"),
                        };
                        Some(<#support_ty as #runtime::__private::WireTypeSupport>::Support::make_ref(proof))
                    }
                )
            }
        }
    });
    let mut_accessors = ir.variants.iter().map(|variant| {
        let method = format_ident!("{}_mut", snake_case(&variant.logical_name));
        let tag = &variant.tag;
        match &variant.shape {
            VariantShape::Unit => quote!(pub fn #method(&mut self) -> Option<()> { if self.tag == #tag { Some(()) } else { None } }),
            VariantShape::Newtype(ty) => {
                let support_ty = analyze::support_type(ty);
                let ret = schema_mut_type(&support_ty, runtime, quote!('view));
                let wire_ty = quote!(<#support_ty as #runtime::__private::WireType>::Wire);
                quote!(
                    pub fn #method<'view>(&'view mut self) -> Option<#ret> {
                        if self.tag != #tag { return None; }
                        let input = match self.payload.subrange_mut::<#wire_ty>(0) { Ok(input) => input, Err(_) => unreachable!("selected tagged payload range is asserted") };
                        let proof = match <#support_ty as #runtime::__private::WireTypeSupport>::Support::prove_mut(input) {
                            Ok(proof) => proof,
                            Err(_) => unreachable!("a proved selected payload retains its active child"),
                        };
                        Some(<#support_ty as #runtime::__private::WireTypeSupport>::Support::make_mut(proof))
                    }
                )
            }
        }
    });
    let prove_arms = ir.variants.iter().map(|variant| {
        let tag = &variant.tag;
        let name = &variant.ident;
        match &variant.shape {
            VariantShape::Unit => quote!(#tag => Ok(()),),
            VariantShape::Newtype(ty) => {
                let support_ty = analyze::support_type(ty);
                let wire_ty = quote!(<#support_ty as #runtime::__private::WireType>::Wire);
                quote!(#tag => {
                    let input = payload.subrange::<#wire_ty>(0).map_err(|error| #owner::access_layout(error))?;
                    <#support_ty as #runtime::__private::WireTypeSupport>::Support::prove(input)
                        .map(|_| ())
                        .map_err(|source| #access_error::#name(source))
                },)
            }
        }
    });
    let materialize_arms = ir.variants.iter().map(|variant| {
        let tag = &variant.tag;
        let name = &variant.ident;
        match &variant.shape {
            VariantShape::Unit => quote!(#tag => super::#ident::#name,),
            VariantShape::Newtype(ty) => {
                let logical = rebound_support_type(ir, ty, quote!('wire));
                let storage = analyze::support_type(ty);
                let wire_ty = quote!(<#storage as #runtime::__private::WireType>::Wire);
                let support_ty = quote!(<#storage as #runtime::__private::WireTypeSupport>::Support);
                quote!(#tag => {
                    let input = match payload.subrange::<#wire_ty>(0) { Ok(input) => input, Err(_) => unreachable!("selected tagged payload range is asserted") };
                    let proof = match <#support_ty as #runtime::__private::SchemaSupport>::prove(input) {
                        Ok(proof) => proof,
                        Err(_) => unreachable!("a proved selected payload retains its active child"),
                    };
                    super::#ident::#name(<#logical as #runtime::__private::LogicalSchema<'wire>>::materialize(proof))
                },)
            }
        }
    });

    Ok(quote!(
        #[derive(Debug)]
        pub enum #access_error #plain_generics {
            Layout(#runtime::LayoutError),
            UnknownUnionTag,
            #(#child_error_variants,)*
        }
        impl #plain_generics ::core::fmt::Display for #access_error #plain_arguments {
            fn fmt(&self, formatter: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result { #runtime::__private::__fmt_schema_error(self, formatter) }
        }
        impl #plain_generics ::core::error::Error for #access_error #plain_arguments {}
        impl #plain_generics #runtime::SchemaError for #access_error #plain_arguments {
            fn kind(&self) -> #runtime::ErrorKind { match self { Self::Layout(_) => #runtime::ErrorKind::Layout, Self::UnknownUnionTag => #runtime::ErrorKind::UnknownUnionTag, #(#error_kind_arms)* } }
            fn schema(&self) -> &'static str { #logical_name }
            fn segment(&self) -> Option<#runtime::ErrorPathSegment> { match self { #(#error_segment_arms)* _ => None } }
            fn child(&self) -> Option<&dyn #runtime::SchemaError> { match self { #(#error_child_arms)* _ => None } }
            fn __fmt_leaf(&self, formatter: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result { match self { Self::Layout(source) => ::core::fmt::Display::fmt(source, formatter), Self::UnknownUnionTag => formatter.write_str("unknown external union tag"), #(#error_leaf_arms)* } }
        }
        #[derive(Debug)] pub enum #mutation_error #plain_generics { Access(#access_error #plain_arguments), Layout(#runtime::LayoutError), TagMismatch }
        impl #plain_generics From<#access_error #plain_arguments> for #mutation_error #plain_arguments { fn from(error: #access_error #plain_arguments) -> Self { Self::Access(error) } }
        impl #plain_generics ::core::fmt::Display for #mutation_error #plain_arguments { fn fmt(&self, formatter: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result { #runtime::__private::__fmt_schema_error(self, formatter) } }
        impl #plain_generics ::core::error::Error for #mutation_error #plain_arguments {}
        impl #plain_generics #runtime::SchemaError for #mutation_error #plain_arguments { fn kind(&self)->#runtime::ErrorKind { match self { Self::Access(source)=>source.kind(), Self::Layout(_)=>#runtime::ErrorKind::Layout, Self::TagMismatch=>#runtime::ErrorKind::TagMismatch } } fn schema(&self)->&'static str { #logical_name } fn segment(&self)->Option<#runtime::ErrorPathSegment>{None} fn child(&self)->Option<&dyn #runtime::SchemaError>{match self {Self::Access(source)=>Some(source),Self::Layout(_) | Self::TagMismatch=>None}} fn __fmt_leaf(&self, f:&mut ::core::fmt::Formatter<'_>)->::core::fmt::Result{match self{Self::Access(source)=>source.__fmt_leaf(f),Self::Layout(source)=>::core::fmt::Display::fmt(source,f),Self::TagMismatch=>f.write_str("selected payload patch cannot change the external tag")}} }
        pub struct #owner #plain_generics(::core::marker::PhantomData<fn() -> #wire #plain_arguments>);
        impl #plain_generics #runtime::__private::OwnerAdapter for #owner #plain_arguments { type AccessError=#access_error #plain_arguments; type MutationError=#mutation_error #plain_arguments; fn access_layout(error:#runtime::LayoutError)->Self::AccessError{#access_error::Layout(error)} fn mutation_layout(error:#runtime::LayoutError)->Self::MutationError{#mutation_error::Layout(error)} }
        #[doc(hidden)]
        #[derive(Clone, Copy)]
        pub struct #input_access_token { _private: () }

        pub struct #reference #cap_generics { tag:#tag_type, payload:#runtime::__private::SharedInput<'wire,#wire #plain_arguments> }
        impl #cap_generics ::core::marker::Copy for #reference #cap_args {}
        impl #cap_generics ::core::clone::Clone for #reference #cap_args { fn clone(&self)->Self{*self} }
        impl #cap_generics #reference #cap_args {
            pub const fn tag(&self) -> #tag_type { self.tag }
            #(#ref_accessors)*
            pub fn copy_into(&self) -> #tagged_logical {
                let selection = match #runtime::__private::TaggedRefSelection::<#support #plain_arguments>::prove_selected(self.tag, self.payload, #input_access_token { _private: () }) {
                    Ok(selection) => selection,
                    Err(_) => unreachable!("a tagged capability retains its selected proof"),
                };
                <#support #plain_arguments as #runtime::__private::TaggedPayloadSupport>::materialize_selected(selection)
            }
        }
        pub struct #mutable #cap_generics { tag:#tag_type, payload:#runtime::__private::ExclusiveInput<'wire,#wire #plain_arguments>, token:#input_access_token }
        impl #cap_generics #mutable #cap_args {
            pub const fn tag(&self) -> #tag_type { self.tag }
            #(#mut_accessors)*
            pub fn copy_into<'view>(&'view self) -> #mutable_tagged_logical {
                let selection = match #runtime::__private::TaggedRefSelection::<#support #plain_arguments>::prove_selected(self.tag, self.payload.shared(), #input_access_token { _private: () }) {
                    Ok(selection) => selection,
                    Err(_) => unreachable!("a tagged capability retains its selected proof"),
                };
                <#support #plain_arguments as #runtime::__private::TaggedPayloadSupport>::materialize_selected(selection)
            }
            pub fn copy_from #patch_method_generics (&mut self, patch: &#patch #patch_method_args) -> ::core::result::Result<(), #mutation_error #plain_arguments> {
                if <#support #plain_arguments as #runtime::__private::TaggedPayloadSupport>::patch_tag(patch) != self.tag {
                    return Err(#mutation_error::TagMismatch);
                }
                <#support #plain_arguments as #runtime::__private::TaggedPayloadSupport>::preflight_patch(self.tag, self.payload.shared(), patch)?;
                <#support #plain_arguments as #runtime::__private::TaggedPayloadSupport>::commit_patch(self.payload.reborrow(), patch, self.token);
                Ok(())
            }
        }

        impl #cap_generics #runtime::__private::Materialize<#tagged_logical> for #reference #cap_args {
            fn materialize(&self) -> #tagged_logical { self.copy_into() }
        }
        #support_visibility struct #support #plain_generics(::core::marker::PhantomData<fn()->#wire #plain_arguments>);
        impl #plain_generics #runtime::__private::InputAccess for #support #plain_arguments {
            type Token = #input_access_token;
        }

        #patches
        #logical_mutation
        impl #plain_generics #runtime::__private::TaggedPayloadSupport for #support #plain_arguments {

            type Tag=#tag_type; type Wire=#wire #plain_arguments; type Owner=#owner #plain_arguments; type Logical<'wire>=#tagged_logical; type Ref<'wire>=#reference #cap_args; type Mut<'wire>=#mutable #cap_args;
            fn validate_selected<'wire>(tag:Self::Tag,payload:#runtime::__private::SharedInput<'wire,Self::Wire>)->::core::result::Result<(),#access_error #plain_arguments>{match tag {#(#prove_arms)* _=>Err(#access_error::UnknownUnionTag)}}
            fn input_token(_: &#runtime::__private::ExclusiveInput<'_, Self::Wire>) -> Self::Token { #input_access_token { _private: () } }
            fn make_ref<'wire>(selection:#runtime::__private::TaggedRefSelection<'wire,Self>)->Self::Ref<'wire>{let(tag,payload)=selection.into_parts(#input_access_token { _private: () });#reference{tag,payload}}
            fn make_mut<'wire>(selection:#runtime::__private::TaggedMutSelection<'wire,Self>)->Self::Mut<'wire>{let(tag,payload)=selection.into_parts(#input_access_token { _private: () });#mutable{tag,payload,token:#input_access_token { _private: () }}}
            fn materialize_selected<'wire>(selection:#runtime::__private::TaggedRefSelection<'wire,Self>)->Self::Logical<'wire>{let(tag,payload)=selection.into_parts(#input_access_token { _private: () });match tag {#(#materialize_arms)* _=>unreachable!("tagged payload materializes only a proved selection")}}
            fn patch_tag<P>(patch:&P)->Self::Tag where P:#runtime::__private::TaggedPayloadPatch<Self>{patch.tag()}
            fn patch_is_complete<P>(patch:&P)->bool where P:#runtime::__private::TaggedPayloadPatch<Self>{patch.is_complete()}
            fn preflight_patch<'wire,P>(current_tag:Self::Tag,payload:#runtime::__private::SharedInput<'wire,Self::Wire>,patch:&P)->::core::result::Result<(),#mutation_error #plain_arguments> where P:#runtime::__private::TaggedPayloadPatch<Self>{patch.preflight(current_tag,payload)}
            fn preflight_patch_init<'wire,P>(payload:#runtime::__private::SharedInput<'wire,Self::Wire>,patch:&P)->::core::result::Result<(),#mutation_error #plain_arguments> where P:#runtime::__private::TaggedPayloadPatch<Self>{if !patch.is_complete(){return Err(<#owner #plain_arguments as #runtime::__private::OwnerAdapter>::mutation_layout(#runtime::LayoutError::OffsetOverflow));}patch.preflight_init(payload)}
            fn commit_patch<'wire,P>(payload:#runtime::__private::ExclusiveInput<'wire,Self::Wire>,patch:&P,token:Self::Token) where P:#runtime::__private::TaggedPayloadPatch<Self>{patch.commit(payload,token)}
        }
    ))
}

// Remaining helpers intentionally use projections rather than generated child
// names.  That lets public cross-crate composition stay opaque while rustc
// still normalizes a concrete macro-generated support implementation at calls.
fn getter_methods(
    ir: &SchemaIr,
    runtime: &Path,
    support_runtime: &Path,
    mutable: bool,
) -> syn::Result<TokenStream> {
    let lifetime = if mutable {
        quote!('view)
    } else {
        quote!('wire)
    };
    let methods = ir
        .fields
        .iter()
        .map(|field| {
            let method = &field.ident;
            let ret = field_read_type(ir, field, runtime, lifetime.clone())?;
            let input = if mutable {
                quote!(self.input.shared())
            } else {
                quote!(self.input)
            };
            let expr = field_read_expr(
                ir,
                field,
                runtime,
                support_runtime,
                quote!(input),
                lifetime.clone(),
            )?;
            if mutable {
                Ok(quote!(pub fn #method<'view>(&'view self)->#ret{let input=#input;#expr}))
            } else {
                Ok(quote!(pub fn #method(&self)->#ret{let input=#input;#expr}))
            }
        })
        .collect::<syn::Result<Vec<_>>>()?;
    Ok(quote!(#(#methods)*))
}

fn field_read_type(
    ir: &SchemaIr,
    field: &FieldIr,
    runtime: &Path,
    lifetime: TokenStream,
) -> syn::Result<TokenStream> {
    match &field.category {
        FieldCategory::Primitive(_) | FieldCategory::Bool => {
            let ty = &field.ty;
            Ok(quote!(#ty))
        }
        FieldCategory::BorrowedStr { .. }
        | FieldCategory::BorrowedCStr { .. }
        | FieldCategory::BorrowedU16Str { .. }
        | FieldCategory::BorrowedU16CStr { .. }
        | FieldCategory::FixedBytes { .. } => Ok(rebind_type(ir, &field.ty, lifetime)),
        FieldCategory::Path { tagged: false, .. } => {
            let logical = rebound_support_type(ir, &field.ty, lifetime.clone());
            Ok(schema_ref_type(&logical, runtime, lifetime))
        }
        FieldCategory::Path { tagged: true, .. } => {
            let logical = rebound_support_type(ir, &field.ty, lifetime.clone());
            Ok(tagged_ref_type(&logical, runtime, lifetime))
        }
        FieldCategory::Array { element, .. } => {
            let Type::Array(array) = &field.ty else {
                return Err(syn::Error::new_spanned(
                    &field.ty,
                    "internal array type mismatch",
                ));
            };
            let elem = array_logical_type(element, &array.elem, runtime, lifetime.clone());
            let n = emit_wire::array_length(&field.support_ty)?;
            let adapter = array_adapter_name(field);
            let args = emit_wire::wire_arguments(&ir.generics.original);
            Ok(quote!(#runtime::ArrayRef<#lifetime,#elem,#n,#adapter #args>))
        }
        FieldCategory::Optional { .. } => {
            let adapter = option_adapter_name(field);
            let args = emit_wire::wire_arguments(&ir.generics.original);
            Ok(
                quote!(Option<<#adapter #args as #runtime::__private::OptionFieldAdapter>::Read<#lifetime>>),
            )
        }
    }
}

fn field_read_expr(
    ir: &SchemaIr,
    field: &FieldIr,
    runtime: &Path,
    support_runtime: &Path,
    input: TokenStream,
    lifetime: TokenStream,
) -> syn::Result<TokenStream> {
    let support = support_name(ir);
    let support_args = emit_wire::wire_arguments(&ir.generics.original);
    let input_access_token = format_ident!("__ZeroSchemaInputAccessToken");
    let offset = field_value_offset(ir, field, support_runtime)?;
    match &field.category {
        FieldCategory::Primitive(_) => {
            let wire = emit_wire::wire_type(
                &field.category,
                &field.support_ty,
                support_runtime,
                field.wire_endian,
            )?;
            Ok(quote!({
                let wire=match #input.read_copy::<#wire>(#offset){Ok(value)=>value,Err(_)=>unreachable!("proved scalar field remains selectable")};
                wire.get()
            }))
        }
        FieldCategory::Bool => Ok(quote!({
            let wire=match #input.read_copy::<#runtime::__private::BoolWire>(#offset){Ok(value)=>value,Err(_)=>unreachable!("proved Boolean field remains selectable")};
            match wire.decode(){Some(value)=>value,None=>unreachable!("a capability contains only proved bool storage")}
        })),
        FieldCategory::BorrowedStr {
            len_type,
            endian,
            capacity,
            ..
        } => {
            let length = emit_wire::length_wire(len_type, *endian)?;
            let wire = emit_wire::wire_type(
                &field.category,
                &field.support_ty,
                support_runtime,
                field.wire_endian,
            )?;
            Ok(quote!({
                let length=match #input.read_copy::<#runtime::__private::#length>(#offset + <#wire>::LEN_OFFSET){Ok(value)=>value,Err(_)=>unreachable!("proved string length remains selectable")};
                let data=match #input.subrange_bytes::<#support #support_args>(#offset + <#wire>::DATA_OFFSET,#capacity,#input_access_token { _private: () }){Ok(value)=>value,Err(_)=>unreachable!("proved string data remains selectable")};
                match #runtime::__private::prove_str(&length,data){Ok(value)=>value,Err(_)=>unreachable!("a capability contains only proved UTF-8 storage")}
            }))
        }
        FieldCategory::BorrowedCStr { capacity, .. } => {
            let wire = emit_wire::wire_type(
                &field.category,
                &field.support_ty,
                support_runtime,
                field.wire_endian,
            )?;
            Ok(quote!({
                let data=match #input.subrange_bytes::<#support #support_args>(#offset + <#wire>::DATA_OFFSET,#capacity,#input_access_token { _private: () }){Ok(value)=>value,Err(_)=>unreachable!("proved C string data remains selectable")};
                match #runtime::__private::prove_c_str(data){Ok(value)=>value,Err(_)=>unreachable!("a capability contains only proved C string storage")}
            }))
        }
        FieldCategory::BorrowedU16Str {
            len_type,
            endian,
            capacity,
            ..
        } => {
            let length = emit_wire::length_wire(len_type, *endian)?;
            let wire = emit_wire::wire_type(
                &field.category,
                &field.support_ty,
                support_runtime,
                field.wire_endian,
            )?;
            Ok(quote!({
                let length=match #input.read_copy::<#runtime::__private::#length>(#offset + <#wire>::LEN_OFFSET){Ok(value)=>value,Err(_)=>unreachable!("proved wide string length remains selectable")};
                let data=match #input.subrange_bytes::<#support #support_args>(#offset + <#wire>::DATA_OFFSET,#capacity * ::core::mem::size_of::<::core::primitive::u16>(),#input_access_token { _private: () }){Ok(value)=>value,Err(_)=>unreachable!("proved wide string data remains selectable")};
                match #runtime::__private::prove_u16_str_bytes::<#runtime::__private::#length,#capacity>(&length,data){Ok(value)=>value,Err(_)=>unreachable!("a capability contains only proved wide string storage")}
            }))
        }
        FieldCategory::BorrowedU16CStr { capacity, .. } => {
            let wire = emit_wire::wire_type(
                &field.category,
                &field.support_ty,
                support_runtime,
                field.wire_endian,
            )?;
            Ok(quote!({
                let data=match #input.subrange_bytes::<#support #support_args>(#offset + <#wire>::DATA_OFFSET,#capacity * ::core::mem::size_of::<::core::primitive::u16>(),#input_access_token { _private: () }){Ok(value)=>value,Err(_)=>unreachable!("proved wide C string data remains selectable")};
                match #runtime::__private::prove_u16_c_str_bytes::<#capacity>(data){Ok(value)=>value,Err(_)=>unreachable!("a capability contains only proved wide C string storage")}
            }))
        }
        FieldCategory::FixedBytes { .. } => {
            let wire = emit_wire::wire_type(
                &field.category,
                &field.support_ty,
                support_runtime,
                field.wire_endian,
            )?;
            let length = emit_wire::array_length(&field.support_ty)?;
            Ok(quote!({
                let bytes=match #input.subrange_bytes::<#support #support_args>(#offset,::core::mem::size_of::<#wire>(),#input_access_token { _private: () }){Ok(value)=>value,Err(_)=>unreachable!("proved fixed-byte field remains selectable")};
                match <&[::core::primitive::u8;#length] as ::core::convert::TryFrom<&[::core::primitive::u8]>>::try_from(bytes){Ok(value)=>value,Err(_)=>unreachable!("proved fixed-byte field retains its exact length")}
            }))
        }
        FieldCategory::Path { tagged: false, .. } => {
            let ty = &field.support_ty;
            let wire =
                emit_wire::wire_type(&field.category, ty, support_runtime, field.wire_endian)?;
            Ok(quote!({
                let selected=match #input.subrange::<#wire>(#offset){Ok(value)=>value,Err(_)=>unreachable!("field offsets are compiler asserted")};
                let proof=match <#ty as #runtime::__private::WireTypeSupport>::Support::prove(selected){Ok(proof)=>proof,Err(_)=>unreachable!("a parent capability retains proved child storage")};
                <#ty as #runtime::__private::WireTypeSupport>::Support::make_ref(proof)
            }))
        }
        FieldCategory::Path {
            tagged: true,
            tag_field: Some(tag_index),
        } => {
            let payload = &field.support_ty;
            let payload_support = tagged_support_type(payload, runtime);
            let tag_field = &ir.fields[*tag_index];
            let tag_ty = &tag_field.support_ty;
            let tag_wire = emit_wire::wire_type(
                &tag_field.category,
                tag_ty,
                support_runtime,
                tag_field.wire_endian,
            )?;
            let tag_offset = field_value_offset(ir, tag_field, support_runtime)?;
            Ok(quote!({
                let tag_input=match #input.subrange::<#tag_wire>(#tag_offset){Ok(value)=>value,Err(_)=>unreachable!("tag offset is compiler asserted")};
                let tag_proof=match <#tag_ty as #runtime::__private::WireTypeSupport>::Support::prove(tag_input){Ok(proof)=>proof,Err(_)=>unreachable!("a parent capability retains a proved tag")};
                let tag=<#tag_ty as #runtime::__private::WireTypeSupport>::Support::make_ref(tag_proof);
                match #runtime::__private::TaggedRefSelection::<#payload_support>::prove_at::<#support #support_args>(#input, tag, #offset, #input_access_token { _private: () }){Ok(selection)=>selection.make_ref(),Err(_)=>unreachable!("a capability contains only its proved tagged selection")}
            }))
        }
        FieldCategory::Path { tagged: true, .. } => Err(syn::Error::new_spanned(
            &field.ty,
            "tagged field is missing its sibling",
        )),
        FieldCategory::Array { element, .. } => {
            let Type::Array(array) = &field.ty else {
                return Err(syn::Error::new_spanned(
                    &field.ty,
                    "internal array type mismatch",
                ));
            };
            let ew = emit_wire::wire_type(
                element,
                &analyze::support_type(&array.elem),
                support_runtime,
                field.wire_endian,
            )?;
            let logical = array_logical_type(element, &array.elem, runtime, lifetime.clone());
            let n = emit_wire::array_length(&field.support_ty)?;
            let adapter = array_adapter_name(field);
            let args = emit_wire::wire_arguments(&ir.generics.original);
            Ok(quote!({
                let selected=match #input.subrange::<[#ew;#n]>(#offset){Ok(value)=>value,Err(_)=>unreachable!("array offset is compiler asserted")};
                match #runtime::ArrayRef::<#logical,#n,#adapter #args>::prove(selected){Ok(view)=>view,Err(_)=>unreachable!("a parent capability retains proved array elements")}
            }))
        }
        FieldCategory::Optional { .. } => {
            let adapter = option_adapter_name(field);
            let args = emit_wire::wire_arguments(&ir.generics.original);
            let storage_wire = emit_wire::wire_field_type(field, support_runtime)?;
            let value_offset =
                quote!(<#adapter #args as #runtime::__private::OptionFieldAdapter>::VALUE_OFFSET);
            let value_wire =
                quote!(<#adapter #args as #runtime::__private::OptionFieldAdapter>::ValueWire);
            let storage_offset = field_storage_offset(ir, field, support_runtime);
            Ok(quote!({
                let storage = match #input.subrange::<#storage_wire>(#storage_offset) {
                    Ok(value) => value,
                    Err(_) => unreachable!("optional storage range is compiler asserted"),
                };
                if storage.is_all_zero() {
                    None
                } else {
                    let value = match storage.subrange::<#value_wire>(#value_offset) {
                        Ok(value) => value,
                        Err(_) => unreachable!("optional value range is compiler asserted"),
                    };
                    Some(match <#adapter #args as #runtime::__private::OptionFieldAdapter>::read_present(value) {
                        Ok(value) => value,
                        Err(_) => unreachable!("a parent capability retains proved optional storage"),
                    })
                }
            }))
        }
    }
}
fn materialized_fields(
    ir: &SchemaIr,
    runtime: &Path,
    support_runtime: &Path,
    mutable: bool,
) -> syn::Result<TokenStream> {
    let fields = ir.fields.iter().map(|field| -> syn::Result<TokenStream> {
        let name = &field.ident;
        match &field.category {
            FieldCategory::Path { tagged: false, .. } => {
                let lifetime = if mutable { quote!('view) } else { quote!('wire) };
                let logical = rebound_support_type(ir, &field.ty, lifetime.clone());
                let storage = &field.support_ty;
                let wire = quote!(<#storage as #runtime::__private::WireType>::Wire);
                let support = quote!(<#storage as #runtime::__private::WireTypeSupport>::Support);
                let offset = field_value_offset(ir, field, support_runtime)?;
                let input = if mutable { quote!(self.input.shared()) } else { quote!(self.input) };
                Ok(quote!(#name: {
                    let selected = match #input.subrange::<#wire>(#offset) { Ok(value) => value, Err(_) => unreachable!("proved child field remains selectable") };
                    let proof = match <#support as #runtime::__private::SchemaSupport>::prove(selected) { Ok(proof) => proof, Err(_) => unreachable!("a parent capability retains proved child storage") };
                    <#logical as #runtime::__private::LogicalSchema<#lifetime>>::materialize(proof)
                }))
            }
            FieldCategory::Path { tagged: true, tag_field: Some(tag_index) } => {
                let payload = &field.support_ty;
                let payload_support = quote!(<#payload as #runtime::__private::TaggedPayloadTypeSupport>::Support);
                let payload_offset = field_value_offset(ir, field, support_runtime)?;
                let tag_field = &ir.fields[*tag_index];
                let tag_ty = &tag_field.support_ty;
                let tag_wire = emit_wire::wire_type(&tag_field.category, tag_ty, support_runtime, tag_field.wire_endian)?;
                let tag_offset = field_value_offset(ir, tag_field, support_runtime)?;
                let input = if mutable { quote!(self.input.shared()) } else { quote!(self.input) };
                let root_support = support_name(ir);
                let root_args = emit_wire::wire_arguments(&ir.generics.original);
                let input_access_token = format_ident!("__ZeroSchemaInputAccessToken");
                Ok(quote!(#name: {
                    let tag_input = match #input.subrange::<#tag_wire>(#tag_offset) { Ok(value) => value, Err(_) => unreachable!("proved tag field remains selectable") };
                    let tag_proof = match <#tag_ty as #runtime::__private::WireTypeSupport>::Support::prove(tag_input) { Ok(proof) => proof, Err(_) => unreachable!("a parent capability retains a proved tag") };
                    let tag = <#tag_ty as #runtime::__private::WireTypeSupport>::Support::make_ref(tag_proof);
                    let selection = match #runtime::__private::TaggedRefSelection::<#payload_support>::prove_at::<#root_support #root_args>(#input, tag, #payload_offset, #input_access_token { _private: () }) { Ok(selection) => selection, Err(_) => unreachable!("a parent capability retains a proved selected payload") };
                    <#payload_support as #runtime::__private::TaggedPayloadSupport>::materialize_selected(selection)
                }))
            }
            FieldCategory::Path { tagged: true, .. } => Err(syn::Error::new_spanned(&field.ty, "tagged field is missing sibling")),
            FieldCategory::Array { element, .. } if matches!(element.as_ref(), FieldCategory::Path { .. }) => Ok(quote!(#name: ::core::array::from_fn(|index| match self.#name().get(index) { Some(value) => #runtime::__private::Materialize::materialize(&value), None => unreachable!("proved array has every element") }))),
            FieldCategory::Array { .. } => Ok(quote!(#name: self.#name().copy_into())),
            FieldCategory::Optional { inner, .. } => match inner.as_ref() {
                FieldCategory::Path { .. } => Ok(quote!(#name: self.#name().map(|value| #runtime::__private::Materialize::materialize(&value)))),
                FieldCategory::Array { element, .. }
                    if matches!(element.as_ref(), FieldCategory::Path { .. }) => Ok(quote!(#name: self.#name().map(|values| ::core::array::from_fn(|index| match values.get(index) { Some(value) => #runtime::__private::Materialize::materialize(&value), None => unreachable!("proved optional array has every element") })))),
                FieldCategory::Array { .. } => Ok(quote!(#name: self.#name().map(|value| value.copy_into()))),
                _ => unreachable!("optional analysis accepts only path values or path arrays"),
            },
            _ => Ok(quote!(#name: self.#name())),
        }
    }).collect::<syn::Result<Vec<_>>>()?;
    Ok(quote!(#(#fields,)*))
}

fn record_proof(ir: &SchemaIr, runtime: &Path, support_runtime: &Path) -> syn::Result<TokenStream> {
    let steps = ir
        .fields
        .iter()
        .map(|field| proof_field(ir, field, runtime, support_runtime))
        .collect::<syn::Result<Vec<_>>>()?;
    Ok(quote!(#(#steps)*))
}

fn proof_field(
    ir: &SchemaIr,
    field: &FieldIr,
    runtime: &Path,
    support_runtime: &Path,
) -> syn::Result<TokenStream> {
    let offset = field_value_offset(ir, field, support_runtime)?;
    let support = support_name(ir);
    let support_args = emit_wire::wire_arguments(&ir.generics.original);
    let input_access_token = format_ident!("__ZeroSchemaInputAccessToken");
    let error = &ir.names.access_error;
    match &field.category {
        FieldCategory::Primitive(_) | FieldCategory::FixedBytes { .. } => Ok(TokenStream::new()),
        FieldCategory::Bool => {
            let variant = error_bool_variant(field);
            Ok(quote!({
                let wire=input.read_copy::<#runtime::__private::BoolWire>(#offset).map_err(|error| #error::Layout(error))?;
                if wire.decode().is_none() { return Err(#error::#variant { raw: wire.raw() }); }
            }))
        }
        FieldCategory::BorrowedStr {
            len_type,
            endian,
            capacity,
            ..
        } => {
            let variant = error_string_variant(field);
            let length = emit_wire::length_wire(len_type, *endian)?;
            let wire = emit_wire::wire_type(
                &field.category,
                &field.support_ty,
                support_runtime,
                field.wire_endian,
            )?;
            Ok(quote!({
                let length=input.read_copy::<#runtime::__private::#length>(#offset + <#wire>::LEN_OFFSET).map_err(|error| #error::Layout(error))?;
                let data=input.subrange_bytes::<#support #support_args>(#offset + <#wire>::DATA_OFFSET,#capacity,#input_access_token { _private: () }).map_err(|error| #error::Layout(error))?;
                if let Err(source)=#runtime::__private::prove_str(&length,data) {
                    return Err(#error::#variant { kind: match source {
                        #runtime::__private::StringProofError::LengthOutOfBounds { .. } => #runtime::ErrorKind::LengthOutOfBounds,
                        #runtime::__private::StringProofError::InvalidUtf8(_) => #runtime::ErrorKind::InvalidUtf8,
                        #runtime::__private::StringProofError::MissingNul => #runtime::ErrorKind::MissingNul,
                    } });
                }
            }))
        }
        FieldCategory::BorrowedCStr { capacity, .. } => {
            let variant = error_string_variant(field);
            let wire = emit_wire::wire_type(
                &field.category,
                &field.support_ty,
                support_runtime,
                field.wire_endian,
            )?;
            Ok(quote!({
                let data=input.subrange_bytes::<#support #support_args>(#offset + <#wire>::DATA_OFFSET,#capacity,#input_access_token { _private: () }).map_err(|error| #error::Layout(error))?;
                if let Err(source)=#runtime::__private::prove_c_str(data) {
                    return Err(#error::#variant { kind: match source {
                        #runtime::__private::StringProofError::LengthOutOfBounds { .. } => #runtime::ErrorKind::LengthOutOfBounds,
                        #runtime::__private::StringProofError::InvalidUtf8(_) => #runtime::ErrorKind::InvalidUtf8,
                        #runtime::__private::StringProofError::MissingNul => #runtime::ErrorKind::MissingNul,
                    } });
                }
            }))
        }
        FieldCategory::BorrowedU16Str {
            len_type,
            endian,
            capacity,
            ..
        } => {
            let variant = error_string_variant(field);
            let length = emit_wire::length_wire(len_type, *endian)?;
            let wire = emit_wire::wire_type(
                &field.category,
                &field.support_ty,
                support_runtime,
                field.wire_endian,
            )?;
            Ok(quote!({
                let length=input.read_copy::<#runtime::__private::#length>(#offset + <#wire>::LEN_OFFSET).map_err(|error| #error::Layout(error))?;
                let data=input.subrange_bytes::<#support #support_args>(#offset + <#wire>::DATA_OFFSET,#capacity * ::core::mem::size_of::<::core::primitive::u16>(),#input_access_token { _private: () }).map_err(|error| #error::Layout(error))?;
                if let Err(source)=#runtime::__private::prove_u16_str_bytes::<#runtime::__private::#length,#capacity>(&length,data) {
                    return Err(#error::#variant { kind: match source {
                        #runtime::__private::StringProofError::LengthOutOfBounds { .. } => #runtime::ErrorKind::LengthOutOfBounds,
                        #runtime::__private::StringProofError::InvalidUtf8(_) => #runtime::ErrorKind::InvalidUtf8,
                        #runtime::__private::StringProofError::MissingNul => #runtime::ErrorKind::MissingNul,
                    } });
                }
            }))
        }
        FieldCategory::BorrowedU16CStr { capacity, .. } => {
            let variant = error_string_variant(field);
            let wire = emit_wire::wire_type(
                &field.category,
                &field.support_ty,
                support_runtime,
                field.wire_endian,
            )?;
            Ok(quote!({
                let data=input.subrange_bytes::<#support #support_args>(#offset + <#wire>::DATA_OFFSET,#capacity * ::core::mem::size_of::<::core::primitive::u16>(),#input_access_token { _private: () }).map_err(|error| #error::Layout(error))?;
                if let Err(source)=#runtime::__private::prove_u16_c_str_bytes::<#capacity>(data) {
                    return Err(#error::#variant { kind: match source {
                        #runtime::__private::StringProofError::LengthOutOfBounds { .. } => #runtime::ErrorKind::LengthOutOfBounds,
                        #runtime::__private::StringProofError::InvalidUtf8(_) => #runtime::ErrorKind::InvalidUtf8,
                        #runtime::__private::StringProofError::MissingNul => #runtime::ErrorKind::MissingNul,
                    } });
                }
            }))
        }
        FieldCategory::Path { tagged: false, .. } => {
            let ty = &field.support_ty;
            let wire =
                emit_wire::wire_type(&field.category, ty, support_runtime, field.wire_endian)?;
            let variant = error_child_variant(field);
            Ok(quote!({
                let selected = input.subrange::<#wire>(#offset).map_err(|error| #error::Layout(error))?;
                <#ty as #runtime::__private::WireTypeSupport>::Support::prove(selected)
                    .map_err(|source| #error::#variant(source))?;
            }))
        }
        FieldCategory::Path {
            tagged: true,
            tag_field: Some(tag_index),
        } => {
            let tag = &ir.fields[*tag_index];
            let tag_ty = &tag.support_ty;
            let tag_wire =
                emit_wire::wire_type(&tag.category, tag_ty, support_runtime, tag.wire_endian)?;
            let tag_offset = field_value_offset(ir, tag, support_runtime)?;
            let tag_variant = error_child_variant(tag);
            let payload_support = tagged_support_type(&field.support_ty, runtime);
            let payload_variant = error_child_variant(field);
            Ok(quote!({
                let tag_input = input.subrange::<#tag_wire>(#tag_offset).map_err(|error| #error::Layout(error))?;
                let tag_proof = <#tag_ty as #runtime::__private::WireTypeSupport>::Support::prove(tag_input)
                    .map_err(|source| #error::#tag_variant(source))?;
                let tag = <#tag_ty as #runtime::__private::WireTypeSupport>::Support::make_ref(tag_proof);
                #runtime::__private::TaggedRefSelection::<#payload_support>::prove_at::<#support #support_args>(input, tag, #offset, #input_access_token { _private: () })
                    .map_err(|source| #error::#payload_variant(source))?;
            }))
        }
        FieldCategory::Path { tagged: true, .. } => Err(syn::Error::new_spanned(
            &field.ty,
            "tagged field is missing sibling",
        )),
        FieldCategory::Array { element, .. } => {
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
            let adapter = array_adapter_name(field);
            let adapter_args = emit_wire::wire_arguments(&ir.generics.original);
            let logical = array_logical_type(element, &array.elem, runtime, quote!('wire));
            Ok(quote!({
                let selected = input.subrange::<[#element_wire; #length]>(#offset).map_err(|error| #error::Layout(error))?;
                #runtime::ArrayRef::<#logical, #length, #adapter #adapter_args>::prove(selected)?;
            }))
        }
        FieldCategory::Optional { .. } => {
            let adapter = option_adapter_name(field);
            let args = emit_wire::wire_arguments(&ir.generics.original);
            let storage_wire = emit_wire::wire_field_type(field, support_runtime)?;
            let value_wire =
                quote!(<#adapter #args as #runtime::__private::OptionFieldAdapter>::ValueWire);
            let storage_offset = field_storage_offset(ir, field, support_runtime);
            Ok(quote!({
                let storage = input.subrange::<#storage_wire>(#storage_offset)
                    .map_err(|error| #error::Layout(error))?;
                if !storage.is_all_zero() {
                    let value = storage.subrange::<#value_wire>(<#adapter #args as #runtime::__private::OptionFieldAdapter>::VALUE_OFFSET)
                        .map_err(|error| #error::Layout(error))?;
                    <#adapter #args as #runtime::__private::OptionFieldAdapter>::validate_present(value)?;
                }
            }))
        }
    }
}

fn emit_record_errors(ir: &SchemaIr, runtime: &Path) -> syn::Result<TokenStream> {
    let error = &ir.names.access_error;
    let mutation = &ir.names.mutation_error;
    let plain_generics = emit_wire::wire_generics(&ir.generics.original);
    let plain_args = emit_wire::wire_arguments(&ir.generics.original);
    let logical_name = &ir.logical_name;
    let support_where = record_support_where(ir, runtime);
    let mut inner_items = Vec::new();
    let mut variants = Vec::new();
    let mut kind_arms = Vec::new();
    let mut segment_arms = Vec::new();
    let mut child_arms = Vec::new();
    let mut source_arms = Vec::new();
    let mut leaf_arms = Vec::new();

    for field in &ir.fields {
        let field_name = &field.logical_name;
        match &field.category {
            FieldCategory::Bool => {
                let variant = error_bool_variant(field);
                variants.push(quote!(#variant { raw: ::core::primitive::u8 }));
                kind_arms.push(quote!(Self::#variant { .. } => #runtime::ErrorKind::InvalidBool,));
                segment_arms.push(quote!(Self::#variant { .. } => Some(#runtime::ErrorPathSegment::Field(#field_name)),));
            }
            FieldCategory::BorrowedStr { .. }
            | FieldCategory::BorrowedCStr { .. }
            | FieldCategory::BorrowedU16Str { .. }
            | FieldCategory::BorrowedU16CStr { .. } => {
                let variant = error_string_variant(field);
                variants.push(quote!(#variant { kind: #runtime::ErrorKind }));
                kind_arms.push(quote!(Self::#variant { kind, .. } => *kind,));
                segment_arms.push(quote!(Self::#variant { .. } => Some(#runtime::ErrorPathSegment::Field(#field_name)),));
            }
            FieldCategory::Path { tagged: false, .. } => {
                let variant = error_child_variant(field);
                let source = schema_access_error_type(&field.support_ty, runtime);
                variants.push(quote!(#variant(#source)));
                kind_arms
                    .push(quote!(Self::#variant(source) => #runtime::SchemaError::kind(source),));
                segment_arms.push(quote!(Self::#variant(_) => Some(#runtime::ErrorPathSegment::Field(#field_name)),));
                child_arms.push(quote!(Self::#variant(source) => Some(source),));
                source_arms.push(quote!(Self::#variant(source) => Some(source),));
                leaf_arms.push(
                    quote!(Self::#variant(source) => #runtime::SchemaError::__fmt_leaf(source, f),),
                );
            }
            FieldCategory::Path { tagged: true, .. } => {
                let variant = error_child_variant(field);
                let source = tagged_access_error_type(&field.support_ty, runtime);
                variants.push(quote!(#variant(#source)));
                kind_arms
                    .push(quote!(Self::#variant(source) => #runtime::SchemaError::kind(source),));
                segment_arms.push(quote!(Self::#variant(_) => Some(#runtime::ErrorPathSegment::Field(#field_name)),));
                child_arms.push(quote!(Self::#variant(source) => Some(source),));
                source_arms.push(quote!(Self::#variant(source) => Some(source),));
                leaf_arms.push(
                    quote!(Self::#variant(source) => #runtime::SchemaError::__fmt_leaf(source, f),),
                );
            }
            FieldCategory::Array { element, .. }
                if matches!(element.as_ref(), FieldCategory::Bool) =>
            {
                let variant = error_array_bool_variant(field);
                let inner = error_array_bool_error_type(field);
                inner_items.push(quote!(
                    #[derive(Debug)]
                    pub struct #inner { index: usize, raw: ::core::primitive::u8 }
                    impl ::core::fmt::Display for #inner {
                        fn fmt(&self, f: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
                            #runtime::__private::__fmt_schema_error(self, f)
                        }
                    }
                    impl ::core::error::Error for #inner {}
                    impl #runtime::SchemaError for #inner {
                        fn kind(&self) -> #runtime::ErrorKind { #runtime::ErrorKind::InvalidBool }
                        fn schema(&self) -> &'static str { #logical_name }
                        fn segment(&self) -> Option<#runtime::ErrorPathSegment> { Some(#runtime::ErrorPathSegment::Index(self.index)) }
                        fn child(&self) -> Option<&dyn #runtime::SchemaError> { None }
                        fn __fmt_leaf(&self, f: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
                            write!(f, "invalid Boolean representation {}", self.raw)
                        }
                    }
                ));
                variants.push(quote!(#variant(#inner)));
                kind_arms
                    .push(quote!(Self::#variant(source) => #runtime::SchemaError::kind(source),));
                segment_arms.push(quote!(Self::#variant(_) => Some(#runtime::ErrorPathSegment::Field(#field_name)),));
                child_arms.push(quote!(Self::#variant(source) => Some(source),));
                source_arms.push(quote!(Self::#variant(source) => Some(source),));
                leaf_arms.push(
                    quote!(Self::#variant(source) => #runtime::SchemaError::__fmt_leaf(source, f),),
                );
            }
            FieldCategory::Array { element, .. }
                if matches!(element.as_ref(), FieldCategory::Path { .. }) =>
            {
                let variant = error_array_child_variant(field);
                let inner = error_array_child_error_type(field);
                let Type::Array(array) = &field.ty else {
                    return Err(syn::Error::new_spanned(
                        &field.ty,
                        "internal array type mismatch",
                    ));
                };
                let element_ty = analyze::support_type(&array.elem);
                let source = schema_access_error_type(&element_ty, runtime);
                inner_items.push(quote!(
                    pub struct #inner #plain_generics #support_where { index: usize, source: #source }
                    impl #plain_generics ::core::fmt::Debug for #inner #plain_args #support_where {
                        fn fmt(&self, f: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result { f.write_str(stringify!(#inner)) }
                    }
                    impl #plain_generics ::core::fmt::Display for #inner #plain_args #support_where {
                        fn fmt(&self, f: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result { #runtime::__private::__fmt_schema_error(self, f) }
                    }
                    impl #plain_generics ::core::error::Error for #inner #plain_args #support_where {
                        fn source(&self) -> Option<&(dyn ::core::error::Error + 'static)> { Some(&self.source) }
                    }
                    impl #plain_generics #runtime::SchemaError for #inner #plain_args #support_where {
                        fn kind(&self) -> #runtime::ErrorKind { #runtime::SchemaError::kind(&self.source) }
                        fn schema(&self) -> &'static str { #logical_name }
                        fn segment(&self) -> Option<#runtime::ErrorPathSegment> { Some(#runtime::ErrorPathSegment::Index(self.index)) }
                        fn child(&self) -> Option<&dyn #runtime::SchemaError> { Some(&self.source) }
                        fn __fmt_leaf(&self, f: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result { #runtime::SchemaError::__fmt_leaf(&self.source, f) }
                    }
                ));
                variants.push(quote!(#variant(#inner #plain_args)));
                kind_arms
                    .push(quote!(Self::#variant(source) => #runtime::SchemaError::kind(source),));
                segment_arms.push(quote!(Self::#variant(_) => Some(#runtime::ErrorPathSegment::Field(#field_name)),));
                child_arms.push(quote!(Self::#variant(source) => Some(source),));
                source_arms.push(quote!(Self::#variant(source) => Some(source),));
                leaf_arms.push(
                    quote!(Self::#variant(source) => #runtime::SchemaError::__fmt_leaf(source, f),),
                );
            }
            FieldCategory::Optional {
                inner,
                inner_support_ty,
                ..
            } => match inner.as_ref() {
                FieldCategory::Path { .. } => {
                    let variant = error_child_variant(field);
                    let source = schema_access_error_type(inner_support_ty, runtime);
                    variants.push(quote!(#variant(#source)));
                    kind_arms.push(
                        quote!(Self::#variant(source) => #runtime::SchemaError::kind(source),),
                    );
                    segment_arms.push(quote!(Self::#variant(_) => Some(#runtime::ErrorPathSegment::Field(#field_name)),));
                    child_arms.push(quote!(Self::#variant(source) => Some(source),));
                    source_arms.push(quote!(Self::#variant(source) => Some(source),));
                    leaf_arms.push(quote!(Self::#variant(source) => #runtime::SchemaError::__fmt_leaf(source, f),));
                }
                FieldCategory::Array { element, .. }
                    if matches!(element.as_ref(), FieldCategory::Path { .. }) =>
                {
                    let variant = error_array_child_variant(field);
                    let inner_error = error_array_child_error_type(field);
                    let Type::Array(array) = inner_support_ty.as_ref() else {
                        unreachable!("optional array support type must be an array")
                    };
                    let element_ty = analyze::support_type(&array.elem);
                    let source = schema_access_error_type(&element_ty, runtime);
                    inner_items.push(quote!(
                        pub struct #inner_error #plain_generics #support_where { index: usize, source: #source }
                        impl #plain_generics ::core::fmt::Debug for #inner_error #plain_args #support_where { fn fmt(&self, f: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result { f.write_str(stringify!(#inner_error)) } }
                        impl #plain_generics ::core::fmt::Display for #inner_error #plain_args #support_where { fn fmt(&self, f: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result { #runtime::__private::__fmt_schema_error(self, f) } }
                        impl #plain_generics ::core::error::Error for #inner_error #plain_args #support_where { fn source(&self) -> Option<&(dyn ::core::error::Error + 'static)> { Some(&self.source) } }
                        impl #plain_generics #runtime::SchemaError for #inner_error #plain_args #support_where {
                            fn kind(&self) -> #runtime::ErrorKind { #runtime::SchemaError::kind(&self.source) }
                            fn schema(&self) -> &'static str { #logical_name }
                            fn segment(&self) -> Option<#runtime::ErrorPathSegment> { Some(#runtime::ErrorPathSegment::Index(self.index)) }
                            fn child(&self) -> Option<&dyn #runtime::SchemaError> { Some(&self.source) }
                            fn __fmt_leaf(&self, f: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result { #runtime::SchemaError::__fmt_leaf(&self.source, f) }
                        }
                    ));
                    variants.push(quote!(#variant(#inner_error #plain_args)));
                    kind_arms.push(
                        quote!(Self::#variant(source) => #runtime::SchemaError::kind(source),),
                    );
                    segment_arms.push(quote!(Self::#variant(_) => Some(#runtime::ErrorPathSegment::Field(#field_name)),));
                    child_arms.push(quote!(Self::#variant(source) => Some(source),));
                    source_arms.push(quote!(Self::#variant(source) => Some(source),));
                    leaf_arms.push(quote!(Self::#variant(source) => #runtime::SchemaError::__fmt_leaf(source, f),));
                }
                _ => unreachable!("optional analysis accepts only path values or path arrays"),
            },
            FieldCategory::Primitive(_)
            | FieldCategory::FixedBytes { .. }
            | FieldCategory::Array { .. } => {}
        }
    }

    Ok(quote!(
        #(#inner_items)*
        pub enum #error #plain_generics #support_where { Layout(#runtime::LayoutError), #(#variants,)* }
        impl #plain_generics ::core::fmt::Debug for #error #plain_args #support_where {
            fn fmt(&self, f: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result { f.write_str(stringify!(#error)) }
        }
        impl #plain_generics ::core::fmt::Display for #error #plain_args #support_where {
            fn fmt(&self, f: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result { #runtime::__private::__fmt_schema_error(self, f) }
        }
        impl #plain_generics ::core::error::Error for #error #plain_args #support_where {
            fn source(&self) -> Option<&(dyn ::core::error::Error + 'static)> {
                match self { Self::Layout(source) => Some(source), #(#source_arms)* _ => None }
            }
        }
        impl #plain_generics #runtime::SchemaError for #error #plain_args #support_where {
            fn kind(&self) -> #runtime::ErrorKind { match self { Self::Layout(_) => #runtime::ErrorKind::Layout, #(#kind_arms)* } }
            fn schema(&self) -> &'static str { #logical_name }
            fn segment(&self) -> Option<#runtime::ErrorPathSegment> { match self { #(#segment_arms)* _ => None } }
            fn child(&self) -> Option<&dyn #runtime::SchemaError> { match self { #(#child_arms)* _ => None } }
            fn __fmt_leaf(&self, f: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
                match self { Self::Layout(source) => ::core::fmt::Display::fmt(source, f), #(#leaf_arms)* _ => f.write_str("schema representation is invalid") }
            }
        }
        pub struct __MutationArrayError { field: &'static str, index: usize, kind: #runtime::ErrorKind }
        impl ::core::fmt::Debug for __MutationArrayError { fn fmt(&self, f: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result { f.write_str("array mutation error") } }
        impl ::core::fmt::Display for __MutationArrayError { fn fmt(&self, f: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result { #runtime::__private::__fmt_schema_error(self, f) } }
        impl ::core::error::Error for __MutationArrayError {}
        impl #runtime::SchemaError for __MutationArrayError {
            fn kind(&self) -> #runtime::ErrorKind { self.kind }
            fn schema(&self) -> &'static str { #logical_name }
            fn segment(&self) -> Option<#runtime::ErrorPathSegment> { Some(#runtime::ErrorPathSegment::Index(self.index)) }
            fn child(&self) -> Option<&dyn #runtime::SchemaError> { None }
            fn __fmt_leaf(&self, f: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result { f.write_str("array mutation rejected") }
        }
        #[derive(Debug)]
        pub enum #mutation #plain_generics #support_where {
            Access(#error #plain_args),
            Layout(#runtime::LayoutError),
            Field { field: &'static str, kind: #runtime::ErrorKind },
            Array(__MutationArrayError),
        }
        impl #plain_generics #mutation #plain_args #support_where {
            fn field(field: &'static str, error: #runtime::__private::StringMutationError) -> Self {
                let kind = match error {
                    #runtime::__private::StringMutationError::CapacityExceeded { .. } => #runtime::ErrorKind::CapacityExceeded,
                    #runtime::__private::StringMutationError::LengthUnrepresentable { .. } => #runtime::ErrorKind::LengthUnrepresentable,
                    #runtime::__private::StringMutationError::PrefixSize { .. } | #runtime::__private::StringMutationError::WideByteSize { .. } | #runtime::__private::StringMutationError::Layout(_) => #runtime::ErrorKind::Layout,
                };
                Self::Field { field, kind }
            }
            fn field_kind(field: &'static str, kind: #runtime::ErrorKind) -> Self { Self::Field { field, kind } }
            fn array(field: &'static str, index: usize, kind: #runtime::ErrorKind) -> Self { Self::Array(__MutationArrayError { field, index, kind }) }
        }
        impl #plain_generics From<#error #plain_args> for #mutation #plain_args #support_where {
            fn from(error: #error #plain_args) -> Self { Self::Access(error) }
        }
        impl #plain_generics ::core::fmt::Display for #mutation #plain_args #support_where {
            fn fmt(&self, formatter: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result { #runtime::__private::__fmt_schema_error(self, formatter) }
        }
        impl #plain_generics ::core::error::Error for #mutation #plain_args #support_where {
            fn source(&self) -> Option<&(dyn ::core::error::Error + 'static)> {
                match self { Self::Access(source) => Some(source), Self::Layout(source) => Some(source), Self::Field { .. } | Self::Array(_) => None }
            }
        }
        impl #plain_generics #runtime::SchemaError for #mutation #plain_args #support_where {
            fn kind(&self) -> #runtime::ErrorKind { match self { Self::Access(source) => source.kind(), Self::Layout(_) => #runtime::ErrorKind::Layout, Self::Field { kind, .. } => *kind, Self::Array(source) => source.kind() } }
            fn schema(&self) -> &'static str { #logical_name }
            fn segment(&self) -> Option<#runtime::ErrorPathSegment> { match self { Self::Field { field, .. } => Some(#runtime::ErrorPathSegment::Field(field)), Self::Array(source) => Some(#runtime::ErrorPathSegment::Field(source.field)), _ => None } }
            fn child(&self) -> Option<&dyn #runtime::SchemaError> { match self { Self::Access(source) => Some(source), Self::Array(source) => Some(source), Self::Layout(_) | Self::Field { .. } => None } }
            fn __fmt_leaf(&self, f: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result { match self { Self::Access(source) => #runtime::SchemaError::__fmt_leaf(source, f), Self::Layout(source) => ::core::fmt::Display::fmt(source, f), Self::Field { .. } => f.write_str("field mutation rejected"), Self::Array(source) => #runtime::SchemaError::__fmt_leaf(source, f) } }
        }
    ))
}
fn schema_access_error_type(ty: &Type, runtime: &Path) -> TokenStream {
    quote!(
        <<<#ty as #runtime::__private::WireTypeSupport>::Support
            as #runtime::__private::SchemaSupport>::Owner
            as #runtime::__private::OwnerAdapter>::AccessError
    )
}

fn tagged_access_error_type(ty: &Type, runtime: &Path) -> TokenStream {
    quote!(
        <<<#ty as #runtime::__private::TaggedPayloadTypeSupport>::Support
            as #runtime::__private::TaggedPayloadSupport>::Owner
            as #runtime::__private::OwnerAdapter>::AccessError
    )
}

pub(crate) fn record_support_where(ir: &SchemaIr, runtime: &Path) -> TokenStream {
    where_clause(&record_support_predicates(ir, runtime))
}

pub(crate) fn record_support_predicates(ir: &SchemaIr, runtime: &Path) -> Vec<syn::WherePredicate> {
    let mut predicates = emit_wire::erased_where_predicates(&ir.generics.original);
    for dependency in emit_wire::parent_dependency_types(&ir.fields) {
        predicates
            .push(syn::parse_quote!(#dependency: #runtime::__private::WireTypeSupport + 'static));
    }
    for dependency in emit_wire::optional_dependency_types(&ir.fields) {
        predicates
            .push(syn::parse_quote!(#dependency: #runtime::__private::OptionalWireType + 'static));
    }
    let optional_lifetime = analyze::fresh_generated_lifetime(ir, "__zero_schema_optional_logical");
    for field in &ir.fields {
        let FieldCategory::Optional {
            inner,
            inner_ty,
            inner_support_ty,
        } = &field.category
        else {
            continue;
        };
        let (child_support, logical) = match inner.as_ref() {
            FieldCategory::Path { .. } => (
                quote!(<#inner_support_ty as #runtime::__private::WireTypeSupport>::Support),
                rebound_support_type(ir, inner_ty, quote!(#optional_lifetime)),
            ),
            FieldCategory::Array { .. } => {
                let Type::Array(array) = inner_ty.as_ref() else {
                    unreachable!("optional array category has an array type")
                };
                let child = analyze::support_type(&array.elem);
                (
                    quote!(<#child as #runtime::__private::WireTypeSupport>::Support),
                    rebound_support_type(ir, &array.elem, quote!(#optional_lifetime)),
                )
            }
            _ => unreachable!("optional analysis accepts only path values or path arrays"),
        };
        predicates.push(syn::parse_quote!(for<#optional_lifetime> #child_support: #runtime::__private::SchemaLogicalMutation<#logical>));
    }
    let tagged_lifetime = analyze::fresh_generated_lifetime(ir, "__zero_schema_tag_logical");
    for field in &ir.fields {
        let FieldCategory::Path {
            tagged: true,
            tag_field: Some(tag_index),
        } = field.category
        else {
            continue;
        };
        let payload = &field.support_ty;
        let tag = &ir.fields[tag_index].support_ty;
        let logical = rebound_support_type(ir, &field.ty, quote!(#tagged_lifetime));
        predicates.push(syn::parse_quote!(for<#tagged_lifetime> #payload: #runtime::__private::TaggedPayloadTypeSupport<Tag = #tag, Logical<#tagged_lifetime> = #logical> + 'static));
    }
    predicates
}

fn logical_value_predicates(ir: &SchemaIr, lifetime: Lifetime) -> Vec<syn::WherePredicate> {
    let logical_generics = analyze::logical_view_generics(&ir.generics.original, lifetime);
    let mut predicates = Vec::new();

    for (original, logical) in ir
        .generics
        .original
        .type_params()
        .zip(logical_generics.type_params())
    {
        for (original_bound, logical_bound) in original.bounds.iter().zip(logical.bounds.iter()) {
            if quote!(#original_bound).to_string() != quote!(#logical_bound).to_string() {
                let ident = &logical.ident;
                predicates.push(syn::parse_quote!(#ident: #logical_bound));
            }
        }
    }

    if let (Some(original_where), Some(logical_where)) = (
        &ir.generics.original.where_clause,
        logical_generics.where_clause.as_ref(),
    ) {
        for (original, logical) in original_where
            .predicates
            .iter()
            .zip(logical_where.predicates.iter())
        {
            let (syn::WherePredicate::Type(original), syn::WherePredicate::Type(logical)) =
                (original, logical)
            else {
                continue;
            };
            for (original_bound, logical_bound) in original.bounds.iter().zip(logical.bounds.iter())
            {
                if quote!(#original_bound).to_string() != quote!(#logical_bound).to_string() {
                    let mut predicate = logical.clone();
                    predicate.bounds.clear();
                    predicate.bounds.push(logical_bound.clone());
                    predicates.push(syn::WherePredicate::Type(predicate));
                }
            }
        }
    }

    predicates
}

fn optional_materialize_predicates(
    ir: &SchemaIr,
    runtime: &Path,
    lifetime: Lifetime,
) -> Vec<syn::WherePredicate> {
    let mut predicates = Vec::new();
    for field in &ir.fields {
        let FieldCategory::Optional {
            inner,
            inner_ty,
            inner_support_ty,
        } = &field.category
        else {
            continue;
        };
        let (child_support, logical) = match inner.as_ref() {
            FieldCategory::Path { .. } => (
                quote!(<#inner_support_ty as #runtime::__private::WireTypeSupport>::Support),
                rebound_support_type(ir, inner_ty, quote!(#lifetime)),
            ),
            FieldCategory::Array { .. } => {
                let Type::Array(array) = inner_ty.as_ref() else {
                    unreachable!("optional array category has an array type")
                };
                let child = analyze::support_type(&array.elem);
                (
                    quote!(<#child as #runtime::__private::WireTypeSupport>::Support),
                    rebound_support_type(ir, &array.elem, quote!(#lifetime)),
                )
            }
            _ => unreachable!("optional analysis accepts only path values or path arrays"),
        };
        predicates.push(syn::parse_quote!(<#child_support as #runtime::__private::SchemaSupport>::Ref<#lifetime>: #runtime::__private::Materialize<#logical>));
    }
    predicates
}

fn where_clause(predicates: &[syn::WherePredicate]) -> TokenStream {
    if predicates.is_empty() {
        TokenStream::new()
    } else {
        quote!(where #(#predicates,)*)
    }
}

fn root_wire_inside(ir: &SchemaIr, runtime: &Path) -> TokenStream {
    let wire = &ir.names.wire;
    let args = emit_wire::wire_arguments(&ir.generics.original);
    let base = quote!(#wire #args);
    emit_wire::aligned_root_wire(ir, base, runtime)
}

fn field_offset(ir: &SchemaIr, field: &FieldIr, runtime: &Path) -> TokenStream {
    let wire = &ir.names.wire;
    let args = emit_wire::wire_arguments(&ir.generics.original);
    let n = &field.ident;
    let base = quote!(::core::mem::offset_of!(#wire #args,#n));
    if let Some(align) = &ir.options.align {
        let marker = format_ident!("Align{}", align.value);
        quote!(<#runtime::__private::AlignedWire<#wire #args,#runtime::__private::#marker>>::VALUE_OFFSET + #base)
    } else {
        base
    }
}

fn wire_type_zero_state(ir: &SchemaIr, runtime: &Path) -> syn::Result<TokenStream> {
    match ir.kind {
        ItemKind::ScalarEnum { .. } => {
            if ir
                .variants
                .iter()
                .any(|variant| variant.raw_discriminant == Some(0))
            {
                Ok(quote!(#runtime::__private::ZeroValid))
            } else {
                Ok(quote!(#runtime::__private::ZeroInvalid))
            }
        }
        ItemKind::Struct => {
            let mut state = quote!(#runtime::__private::ZeroValid);
            for field in ir.fields.iter().rev() {
                let source_ty = analyze::erased_source_type(&field.ty);
                let term = field_zero_state(&field.category, &source_ty, runtime)?;
                state = quote!(<#term as #runtime::__private::ZeroState>::Or<#state>);
            }
            Ok(state)
        }
        ItemKind::TaggedEnum => unreachable!("tagged payloads do not implement WireTypeSupport"),
    }
}

pub(crate) fn tagged_payload_zero_state(ir: &SchemaIr, runtime: &Path) -> syn::Result<TokenStream> {
    let mut state = quote!(#runtime::__private::ZeroInvalid);
    for variant in &ir.variants {
        let term = match &variant.shape {
            VariantShape::Unit => quote!(#runtime::__private::ZeroValid),
            VariantShape::Newtype(ty) => {
                let source_ty = analyze::erased_source_type(ty);
                quote!(<#source_ty as #runtime::__private::WireTypeSupport>::ZeroState)
            }
        };
        state = quote!(<#term as #runtime::__private::ZeroState>::And<#state>);
    }
    Ok(state)
}

fn field_zero_state(
    category: &FieldCategory,
    support_ty: &Type,
    runtime: &Path,
) -> syn::Result<TokenStream> {
    match category {
        FieldCategory::Primitive(_)
        | FieldCategory::Bool
        | FieldCategory::BorrowedStr { .. }
        | FieldCategory::BorrowedCStr { .. }
        | FieldCategory::BorrowedU16Str { .. }
        | FieldCategory::BorrowedU16CStr { .. }
        | FieldCategory::FixedBytes { .. }
        | FieldCategory::Optional { .. } => Ok(quote!(#runtime::__private::ZeroValid)),
        FieldCategory::Path { tagged: true, .. } => {
            Ok(quote!(<#support_ty as #runtime::__private::TaggedPayloadTypeSupport>::ZeroState))
        }
        FieldCategory::Path { tagged: false, .. } => {
            Ok(quote!(<#support_ty as #runtime::__private::WireTypeSupport>::ZeroState))
        }
        FieldCategory::Array { element, .. } => {
            let Type::Array(array) = support_ty else {
                return Err(syn::Error::new_spanned(
                    support_ty,
                    "internal array support type mismatch",
                ));
            };
            field_zero_state(element, &array.elem, runtime)
        }
    }
}
pub(crate) fn field_value_offset(
    ir: &SchemaIr,
    field: &FieldIr,
    runtime: &Path,
) -> syn::Result<TokenStream> {
    let offset = field_offset(ir, field, runtime);
    if let Some(align) = &field.options.align {
        let base = emit_wire::wire_type(
            &field.category,
            &field.support_ty,
            runtime,
            field.wire_endian,
        )?;
        let marker = format_ident!("Align{}", align.value);
        Ok(
            quote!(#offset + <#runtime::__private::AlignedWire<#base,#runtime::__private::#marker>>::VALUE_OFFSET),
        )
    } else {
        Ok(offset)
    }
}

pub(crate) fn field_storage_offset(ir: &SchemaIr, field: &FieldIr, runtime: &Path) -> TokenStream {
    field_offset(ir, field, runtime)
}

pub(crate) fn owner_name(ir: &SchemaIr) -> Ident {
    format_ident!("{}Owner", ir.logical_name)
}
fn option_adapter_name(field: &FieldIr) -> Ident {
    format_ident!("{}OptionAdapter", pascal(&field.logical_name))
}

fn array_adapter_name(field: &FieldIr) -> Ident {
    format_ident!("{}ArrayAdapter", pascal(&field.logical_name))
}
pub(crate) fn error_bool_variant(field: &FieldIr) -> Ident {
    format_ident!("{}InvalidBool", pascal(&field.logical_name))
}
pub(crate) fn support_name(ir: &SchemaIr) -> Ident {
    format_ident!("{}Support", ir.logical_name)
}

pub(crate) fn error_string_variant(field: &FieldIr) -> Ident {
    format_ident!("{}String", pascal(&field.logical_name))
}
pub(crate) fn error_child_variant(field: &FieldIr) -> Ident {
    format_ident!("{}Child", pascal(&field.logical_name))
}
pub(crate) fn error_array_bool_variant(field: &FieldIr) -> Ident {
    format_ident!("{}ArrayInvalidBool", pascal(&field.logical_name))
}
pub(crate) fn error_array_bool_error_type(field: &FieldIr) -> Ident {
    format_ident!("{}ArrayBoolError", pascal(&field.logical_name))
}
pub(crate) fn error_array_child_error_type(field: &FieldIr) -> Ident {
    format_ident!("{}ArrayChildError", pascal(&field.logical_name))
}
pub(crate) fn error_array_child_variant(field: &FieldIr) -> Ident {
    format_ident!("{}ArrayChild", pascal(&field.logical_name))
}
fn pascal(value: &str) -> String {
    let mut out = String::new();
    let mut upper = true;
    for c in value.chars() {
        if c == '_' {
            upper = true
        } else if upper {
            out.extend(c.to_uppercase());
            upper = false
        } else {
            out.push(c)
        }
    }
    out
}
fn snake_case(value: &str) -> String {
    let mut result = String::new();
    for (index, c) in value.chars().enumerate() {
        if c.is_uppercase() {
            if index != 0 {
                result.push('_')
            }
            result.extend(c.to_lowercase())
        } else {
            result.push(c)
        }
    }
    result
}
fn capability_generics(ir: &SchemaIr) -> TokenStream {
    let params = emit_wire::wire_generic_parameters(&ir.generics.original);
    if params.is_empty() {
        quote!(<'wire>)
    } else {
        quote!(<'wire,#(#params),*>)
    }
}
fn capability_arguments(ir: &SchemaIr) -> TokenStream {
    let args = ir
        .generics
        .original
        .params
        .iter()
        .filter_map(|p| match p {
            GenericParam::Type(p) => {
                let i = &p.ident;
                Some(quote!(#i))
            }
            GenericParam::Const(p) => {
                let i = &p.ident;
                Some(quote!(#i))
            }
            GenericParam::Lifetime(_) => None,
        })
        .collect::<Vec<_>>();
    if args.is_empty() {
        quote!(<'wire>)
    } else {
        quote!(<'wire,#(#args),*>)
    }
}

fn snake_ident(value: &str) -> Ident {
    let value = snake_case(value);
    match value.as_str() {
        "as" | "break" | "const" | "continue" | "crate" | "else" | "enum" | "extern" | "false"
        | "fn" | "for" | "if" | "impl" | "in" | "let" | "loop" | "match" | "mod" | "move"
        | "mut" | "pub" | "ref" | "return" | "self" | "Self" | "static" | "struct" | "super"
        | "trait" | "true" | "type" | "unsafe" | "use" | "where" | "while" | "async" | "await"
        | "dyn" | "abstract" | "become" | "box" | "do" | "final" | "macro" | "override"
        | "priv" | "try" | "typeof" | "unsized" | "virtual" | "yield" => {
            Ident::new_raw(&value, proc_macro2::Span::call_site())
        }
        _ => Ident::new(&value, proc_macro2::Span::call_site()),
    }
}
/// capability itself intentionally erases declaration lifetimes, so referring
/// to the original names from its impl would leave them undeclared.
fn patch_method_generics_and_args(ir: &SchemaIr) -> (TokenStream, TokenStream) {
    let mut lifetimes = ir
        .generics
        .original
        .params
        .iter()
        .enumerate()
        .filter(|(_, parameter)| matches!(parameter, GenericParam::Lifetime(_)))
        .map(|(index, _)| {
            syn::Lifetime::new(
                &format!("'__zero_schema_patch_{index}"),
                proc_macro2::Span::call_site(),
            )
        })
        .collect::<Vec<_>>();
    if lifetimes.is_empty() && crate::emit_patch::needs_generated_patch_lifetime(ir) {
        lifetimes.push(syn::Lifetime::new(
            "'__zero_schema_patch_source",
            proc_macro2::Span::call_site(),
        ));
    }
    let mut lifetime_index = 0;
    let mut arguments = Vec::new();
    if crate::emit_patch::needs_generated_patch_lifetime(ir) {
        let source = lifetimes.first().expect("generated patch source lifetime");
        arguments.push(quote!(#source));
    }
    arguments.extend(
        ir.generics
            .original
            .params
            .iter()
            .map(|parameter| match parameter {
                GenericParam::Lifetime(_) => {
                    let lifetime = &lifetimes[lifetime_index];
                    lifetime_index += 1;
                    quote!(#lifetime)
                }
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
    let generics = if lifetimes.is_empty() {
        TokenStream::new()
    } else {
        quote!(<#(#lifetimes),*>)
    };
    let arguments = if arguments.is_empty() {
        TokenStream::new()
    } else {
        quote!(<#(#arguments),*>)
    };
    (generics, arguments)
}

fn patch_projection_arguments(ir: &SchemaIr, lifetime: TokenStream) -> TokenStream {
    crate::emit_patch::patch_projection_arguments(ir, lifetime)
}
fn root_logical_type(ir: &SchemaIr, lifetime: TokenStream) -> TokenStream {
    let ident = &ir.ident;
    let args = ir
        .generics
        .original
        .params
        .iter()
        .map(|p| match p {
            GenericParam::Lifetime(_) => lifetime.clone(),
            GenericParam::Type(p) => {
                let i = &p.ident;
                quote!(#i)
            }
            GenericParam::Const(p) => {
                let i = &p.ident;
                quote!(#i)
            }
        })
        .collect::<Vec<_>>();
    if args.is_empty() {
        quote!(#ident)
    } else {
        quote!(#ident<#(#args),*>)
    }
}
fn logical_type(ir: &SchemaIr, lifetime: TokenStream) -> TokenStream {
    let ident = &ir.ident;
    let args = ir
        .generics
        .original
        .params
        .iter()
        .map(|p| match p {
            GenericParam::Lifetime(_) => lifetime.clone(),
            GenericParam::Type(p) => {
                let i = &p.ident;
                quote!(#i)
            }
            GenericParam::Const(p) => {
                let i = &p.ident;
                quote!(#i)
            }
        })
        .collect::<Vec<_>>();
    if args.is_empty() {
        quote!(super::#ident)
    } else {
        quote!(super::#ident<#(#args),*>)
    }
}
fn logical_tagged_type(ir: &SchemaIr, lifetime: TokenStream) -> TokenStream {
    logical_type(ir, lifetime)
}
fn schema_ref_type(ty: &Type, runtime: &Path, lifetime: TokenStream) -> TokenStream {
    quote!(<<#ty as #runtime::__private::WireTypeSupport>::Support as #runtime::__private::SchemaSupport>::Ref<#lifetime>)
}
fn schema_mut_type(ty: &Type, runtime: &Path, lifetime: TokenStream) -> TokenStream {
    quote!(<<#ty as #runtime::__private::WireTypeSupport>::Support as #runtime::__private::SchemaSupport>::Mut<#lifetime>)
}
fn tagged_support_type(ty: &Type, runtime: &Path) -> TokenStream {
    quote!(<#ty as #runtime::__private::TaggedPayloadTypeSupport>::Support)
}
fn tagged_ref_type(ty: &Type, runtime: &Path, lifetime: TokenStream) -> TokenStream {
    let support = tagged_support_type(ty, runtime);
    quote!(<#support as #runtime::__private::TaggedPayloadSupport>::Ref<#lifetime>)
}
fn array_logical_type(
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
            schema_ref_type(&support, runtime, lifetime)
        }
        _ => quote!(#ty),
    }
}
pub(crate) fn rebind_type(ir: &SchemaIr, ty: &Type, lifetime: TokenStream) -> TokenStream {
    let lifetime: Lifetime = syn::parse2(lifetime).expect("generated lifetime");
    let rebound = analyze::rebind_ir_source_lifetimes(ir, ty, lifetime);
    quote!(#rebound)
}

fn rebound_support_type(ir: &SchemaIr, ty: &Type, lifetime: TokenStream) -> Type {
    let lifetime: Lifetime = syn::parse2(lifetime).expect("generated lifetime");
    let rebased = analyze::logical_source_type(ty);
    analyze::rebind_ir_source_lifetimes(ir, &rebased, lifetime)
}

fn logical_schema_impl(ir: &SchemaIr, runtime: &Path) -> TokenStream {
    let view = analyze::fresh_generated_lifetime(ir, "__zero_schema_view");
    let mut generics = analyze::logical_view_generics(&ir.generics.original, view.clone());
    generics
        .make_where_clause()
        .predicates
        .extend(record_support_predicates(ir, runtime));
    generics
        .make_where_clause()
        .predicates
        .extend(optional_materialize_predicates(ir, runtime, view.clone()));
    let (impl_generics, _, where_clause) = generics.split_for_impl();
    let logical = root_logical_type(ir, quote!(#view));
    let module = &ir.names.support_module;
    let support = support_name(ir);
    let args = emit_wire::wire_arguments(&ir.generics.original);
    quote!(
        impl #impl_generics #runtime::__private::LogicalSchema<#view> for #logical #where_clause {
            fn materialize(
                proof: #runtime::__private::ProvedShared<#view, <#logical as #runtime::__private::WireTypeSupport>::Support, <#logical as #runtime::__private::WireType>::Wire>,
            ) -> Self {
                let reference = <#module::#support #args as #runtime::__private::SchemaSupport>::make_ref(proof);
                #runtime::__private::Materialize::materialize(&reference)
            }
        }
    )
}
