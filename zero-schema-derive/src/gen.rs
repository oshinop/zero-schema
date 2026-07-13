#[path = "gen_struct.rs"]
mod gen_struct;
#[path = "gen_tagged.rs"]
mod gen_tagged;
use proc_macro2::TokenStream;
use quote::{format_ident, quote};
use syn::Error;

use crate::ir::*;

fn dependency_root(name: &str) -> syn::Result<syn::Path> {
    let normalized = name.replace('-', "_");
    let ident = syn::Ident::new_raw(&normalized, proc_macro2::Span::call_site());
    syn::parse_str(&format!("::{ident}")).map_err(|error| Error::new(ident.span(), error))
}

fn resolved_runtime(ir: &SchemaIr) -> syn::Result<syn::Path> {
    match proc_macro_crate::crate_name("zero-schema").map_err(|e| Error::new(ir.ident.span(), e))? {
        proc_macro_crate::FoundCrate::Itself => {
            syn::parse_str("::zero_schema").map_err(|e| Error::new(ir.ident.span(), e))
        }
        proc_macro_crate::FoundCrate::Name(name) => dependency_root(&name),
    }
}

fn resolved_zerocopy(ir: &SchemaIr) -> syn::Result<syn::Path> {
    match proc_macro_crate::crate_name("zerocopy").map_err(|e| Error::new(ir.ident.span(), e))? {
        proc_macro_crate::FoundCrate::Itself => dependency_root("zerocopy"),
        proc_macro_crate::FoundCrate::Name(name) => dependency_root(&name),
    }
}

fn runtime_paths(ir: &SchemaIr) -> syn::Result<(syn::Path, syn::Path)> {
    match ir.path_resolution.runtime_source {
        RuntimePathSource::Explicit => {
            let parent = ir
                .path_resolution
                .parent_runtime_path
                .clone()
                .ok_or_else(|| {
                    Error::new(ir.ident.span(), "internal explicit runtime path is missing")
                })?;
            let hidden = ir
                .path_resolution
                .hidden_runtime_path
                .clone()
                .ok_or_else(|| {
                    Error::new(ir.ident.span(), "internal hidden runtime path is missing")
                })?;
            Ok((parent, hidden))
        }
        RuntimePathSource::ResolveDirectDependency => {
            if ir.path_resolution.parent_runtime_path.is_some()
                || ir.path_resolution.hidden_runtime_path.is_some()
            {
                return Err(Error::new(
                    ir.ident.span(),
                    "internal default runtime path unexpectedly contains an override",
                ));
            }
            let resolved = resolved_runtime(ir)?;
            Ok((resolved.clone(), resolved))
        }
    }
}

pub fn generate(ir: &SchemaIr) -> syn::Result<TokenStream> {
    match ir.kind {
        SchemaKind::ScalarEnum => scalar(ir),
        SchemaKind::Struct => {
            let (parent, hidden) = runtime_paths(ir)?;
            let zerocopy = resolved_zerocopy(ir)?;
            gen_struct::generate(ir, &parent, &hidden, &zerocopy)
        }
        SchemaKind::TaggedEnum => {
            let (parent, hidden) = runtime_paths(ir)?;
            let zerocopy = resolved_zerocopy(ir)?;
            gen_tagged::generate(ir, &parent, &hidden, &zerocopy)
        }
    }
}

fn scalar(ir: &SchemaIr) -> syn::Result<TokenStream> {
    let (rt, hidden_rt) = runtime_paths(ir)?;
    let repr = ir.scalar_repr.as_ref().ok_or_else(|| {
        Error::new(
            ir.ident.span(),
            "scalar enums require exactly one #[repr(u8)], #[repr(u16)], or #[repr(u32)]",
        )
    })?;
    let (wire, integer_repr) = match (repr.to_string().as_str(), ir.options.endian) {
        ("u8", _) => (format_ident!("U8"), format_ident!("U8")),
        ("u16", Endian::Native) => (format_ident!("NativeU16"), format_ident!("U16")),
        ("u16", Endian::Little) => (format_ident!("LittleU16"), format_ident!("U16")),
        ("u16", Endian::Big) => (format_ident!("BigU16"), format_ident!("U16")),
        ("u32", Endian::Native) => (format_ident!("NativeU32"), format_ident!("U32")),
        ("u32", Endian::Little) => (format_ident!("LittleU32"), format_ident!("U32")),
        ("u32", Endian::Big) => (format_ident!("BigU32"), format_ident!("U32")),
        _ => unreachable!(),
    };
    let endian = match ir.options.endian {
        Endian::Native => quote!(#rt::Endian::Native),
        Endian::Little => quote!(#rt::Endian::Little),
        Endian::Big => quote!(#rt::Endian::Big),
    };
    let name = &ir.ident;
    let vis = &ir.visibility;
    let module_vis = &ir.visibility_plan.module;
    let support_vis = &ir.visibility_plan.support;
    let logical_name = name.to_string().trim_start_matches("r#").to_owned();
    let module = &ir.generated_names.module;
    let decode_error = &ir.generated_names.decode_error;
    let encode_error = &ir.generated_names.encode_error;
    let values = ir.variants.iter().map(|v| {
        let variant = &v.ident;
        let logical = variant.to_string().trim_start_matches("r#").to_owned();
        quote!(#hidden_rt::EnumValueDescriptor::__new(#logical, #name::#variant as ::core::primitive::#repr as ::core::primitive::u64))
    });
    let from_raw = ir.variants.iter().map(|v| {
        let id = &v.ident;
        quote!(x if x == #name::#id as ::core::primitive::#repr => ::core::option::Option::Some(#name::#id),)
    });
    let to_raw = ir.variants.iter().map(|v| {
        let id = &v.ident;
        quote!(#name::#id => #name::#id as ::core::primitive::#repr,)
    });

    Ok(quote! {
        #[doc(hidden)]
        #module_vis mod #module {
            use super::*;
            #support_vis type Wire = #hidden_rt::__private::#wire;
            #support_vis static VALUES: &[#hidden_rt::EnumValueDescriptor] = &[#(#values),*];

            #[derive(::core::clone::Clone, ::core::marker::Copy, ::core::fmt::Debug, ::core::cmp::Eq, ::core::cmp::PartialEq)]
            #[non_exhaustive]
            #support_vis enum DecodeError { Layout(#hidden_rt::LayoutError), UnknownValue { value: ::core::primitive::#repr } }
            impl ::core::fmt::Display for DecodeError { fn fmt(&self, f: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result { #hidden_rt::__private::__fmt_schema_error(self, f) } }
            impl ::core::error::Error for DecodeError { fn source(&self) -> ::core::option::Option<&(dyn ::core::error::Error + 'static)> { match self { Self::Layout(e) => ::core::option::Option::Some(e), Self::UnknownValue { .. } => ::core::option::Option::None } } }
            impl #hidden_rt::SchemaError for DecodeError {
                fn kind(&self) -> #hidden_rt::ErrorKind { match self { Self::Layout(_) => #hidden_rt::ErrorKind::Layout, Self::UnknownValue { .. } => #hidden_rt::ErrorKind::UnknownEnumValue } }
                fn schema(&self) -> &'static ::core::primitive::str { #logical_name }
                fn segment(&self) -> ::core::option::Option<#hidden_rt::ErrorPathSegment> { ::core::option::Option::None }
                fn child(&self) -> ::core::option::Option<&dyn #hidden_rt::SchemaError> { ::core::option::Option::None }
                fn __fmt_leaf(&self, f: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result { match self { Self::Layout(e) => ::core::fmt::Display::fmt(e, f), Self::UnknownValue { value } => ::core::write!(f, "unknown enum value {}", value) } }
            }

            #[derive(::core::clone::Clone, ::core::marker::Copy, ::core::fmt::Debug, ::core::cmp::Eq, ::core::cmp::PartialEq)]
            #[non_exhaustive]
            #support_vis enum EncodeError { Layout(#hidden_rt::LayoutError) }
            impl ::core::fmt::Display for EncodeError { fn fmt(&self, f: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result { #hidden_rt::__private::__fmt_schema_error(self, f) } }
            impl ::core::error::Error for EncodeError { fn source(&self) -> ::core::option::Option<&(dyn ::core::error::Error + 'static)> { match self { Self::Layout(e) => ::core::option::Option::Some(e) } } }
            impl #hidden_rt::SchemaError for EncodeError {
                fn kind(&self) -> #hidden_rt::ErrorKind { #hidden_rt::ErrorKind::Layout }
                fn schema(&self) -> &'static ::core::primitive::str { #logical_name }
                fn segment(&self) -> ::core::option::Option<#hidden_rt::ErrorPathSegment> { ::core::option::Option::None }
                fn child(&self) -> ::core::option::Option<&dyn #hidden_rt::SchemaError> { ::core::option::Option::None }
                fn __fmt_leaf(&self, f: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result { match self { Self::Layout(e) => ::core::fmt::Display::fmt(e, f) } }
            }
        }
        #vis use #module::{DecodeError as #decode_error, EncodeError as #encode_error};

        impl #rt::ZeroSchemaType for #name {
            type Wire = #module::Wire;
            type DecodeError = #decode_error;
            type EncodeError = #encode_error;
            const WIRE_SIZE: ::core::primitive::usize = ::core::mem::size_of::<<Self as #rt::ZeroSchemaType>::Wire>();
            const WIRE_ALIGN: ::core::primitive::usize = ::core::mem::align_of::<<Self as #rt::ZeroSchemaType>::Wire>();
            const WIRE_STRIDE: ::core::primitive::usize = match #rt::__private::__checked_wire_stride(<Self as #rt::ZeroSchemaType>::WIRE_SIZE, <Self as #rt::ZeroSchemaType>::WIRE_ALIGN) { ::core::option::Option::Some(value) => value, ::core::option::Option::None => ::core::panic!("wire stride overflow") };
            const LAYOUT: &'static #rt::LayoutDescriptor = &#rt::LayoutDescriptor::__new(#logical_name, #rt::TypeKind::ScalarEnum { repr: #rt::IntegerRepr::#integer_repr, endian: #endian }, <Self as #rt::ZeroSchemaType>::WIRE_SIZE, <Self as #rt::ZeroSchemaType>::WIRE_ALIGN, <Self as #rt::ZeroSchemaType>::WIRE_STRIDE, #rt::PaddingPolicy::Ignore, &[], &[], #module::VALUES, &[]);
        }
        const _: () = ::core::assert!(#rt::__private::__layout_constants_match::<#name>());

        impl #rt::ScalarEnum for #name {
            fn from_raw(raw: ::core::primitive::#repr) -> ::core::option::Option<Self> { match raw { #(#from_raw)* _ => ::core::option::Option::None } }
            fn to_raw(&self) -> ::core::primitive::#repr { match self { #(#to_raw)* } }
            fn __unknown(value: ::core::primitive::#repr) -> #decode_error { #decode_error::UnknownValue { value } }
            fn __decode_layout(error: #rt::LayoutError) -> #decode_error { #decode_error::Layout(error) }
            fn __encode_layout(error: #rt::LayoutError) -> #encode_error { #encode_error::Layout(error) }
        }
        impl<'src> #rt::__private::DecodeWire<'src> for #name { fn decode_at(input: #rt::DecodeInput<'src, <Self as #rt::ZeroSchemaType>::Wire>) -> ::core::result::Result<Self, <Self as #rt::ZeroSchemaType>::DecodeError> { #rt::__private::decode_scalar(input) } }
        impl #rt::__private::EncodeWire for #name {
            fn validate_encode(&self) -> ::core::result::Result<(), <Self as #rt::ZeroSchemaType>::EncodeError> { ::core::result::Result::Ok(()) }
            fn encode_at(&self, destination: &mut #rt::__private::Prezeroed<'_>) -> ::core::result::Result<(), <Self as #rt::ZeroSchemaType>::EncodeError> { #rt::__private::encode_scalar(self, destination) }
        }

        impl #name {
            #vis const WIRE_SIZE: ::core::primitive::usize = <Self as #rt::ZeroSchemaType>::WIRE_SIZE;
            #vis const WIRE_ALIGN: ::core::primitive::usize = <Self as #rt::ZeroSchemaType>::WIRE_ALIGN;
            #vis const WIRE_STRIDE: ::core::primitive::usize = <Self as #rt::ZeroSchemaType>::WIRE_STRIDE;
            #vis const LAYOUT: &'static #rt::LayoutDescriptor = <Self as #rt::ZeroSchemaType>::LAYOUT;
            #vis fn parse<'src>(bytes: &'src [::core::primitive::u8]) -> ::core::result::Result<Self, #decode_error> { let input = #rt::DecodeInput::<<Self as #rt::ZeroSchemaType>::Wire>::from_exact(bytes).map_err(<Self as #rt::ScalarEnum>::__decode_layout)?; <Self as #rt::__private::DecodeWire<'src>>::decode_at(input) }
            #vis fn parse_prefix<'src>(bytes: &'src [::core::primitive::u8]) -> ::core::result::Result<(Self, &'src [::core::primitive::u8]), #decode_error> { let input = #rt::DecodeInput::<<Self as #rt::ZeroSchemaType>::Wire>::from_prefix(bytes).map_err(<Self as #rt::ScalarEnum>::__decode_layout)?; let value = <Self as #rt::__private::DecodeWire<'src>>::decode_at(input)?; ::core::result::Result::Ok((value, &bytes[Self::WIRE_SIZE..])) }
            #vis fn encode_into(&self, destination: &mut [::core::primitive::u8]) -> ::core::result::Result<(), #encode_error> {
                if destination.len() != Self::WIRE_SIZE { return ::core::result::Result::Err(<Self as #rt::ScalarEnum>::__encode_layout(#rt::LayoutError::IncorrectSize { expected: Self::WIRE_SIZE, actual: destination.len() })); }
                let address = destination.as_ptr() as ::core::primitive::usize;
                if address & (Self::WIRE_ALIGN - 1) != 0 { return ::core::result::Result::Err(<Self as #rt::ScalarEnum>::__encode_layout(#rt::LayoutError::Misaligned { required: Self::WIRE_ALIGN, address })); }
                <Self as #rt::__private::EncodeWire>::validate_encode(self)?;
                let mut root = #rt::__private::Prezeroed::new(destination);
                <Self as #rt::__private::EncodeWire>::encode_at(self, &mut root)
            }
            #vis fn encode(&self) -> ::core::result::Result<#rt::AlignedBytes<<Self as #rt::ZeroSchemaType>::Wire, { <Self as #rt::ZeroSchemaType>::WIRE_SIZE }>, #encode_error> {
                let mut output = #rt::AlignedBytes::<<Self as #rt::ZeroSchemaType>::Wire, { <Self as #rt::ZeroSchemaType>::WIRE_SIZE }>::zeroed();
                self.encode_into(output.as_bytes_mut())?;
                ::core::result::Result::Ok(output)
            }
            #vis const fn encoded_len(&self) -> ::core::primitive::usize { Self::WIRE_SIZE }
        }

    })
}

#[cfg(test)]
mod tests {
    use super::dependency_root;
    use quote::ToTokens as _;

    #[test]
    fn dependency_aliases_are_absolute_and_keyword_safe() {
        assert_eq!(
            dependency_root("renamed-runtime")
                .unwrap()
                .to_token_stream()
                .to_string(),
            ":: r#renamed_runtime"
        );
        assert_eq!(
            dependency_root("type")
                .unwrap()
                .to_token_stream()
                .to_string(),
            ":: r#type"
        );
    }
}
