use crate::ir::*;
use proc_macro2::TokenStream;
use quote::{format_ident, quote};
use syn::{GenericParam, Generics, Path, Type, visit_mut::VisitMut};

fn erased(ty: &Type) -> Type {
    struct E;
    impl VisitMut for E {
        fn visit_lifetime_mut(&mut self, l: &mut syn::Lifetime) {
            *l = syn::parse_quote!('static);
        }
    }
    let mut ty = ty.clone();
    E.visit_type_mut(&mut ty);
    ty
}
fn rebase_erased_marker_type(mut ty: Type, wire: &syn::Ident) -> Type {
    struct Rebase<'a> {
        wire: &'a syn::Ident,
    }
    impl VisitMut for Rebase<'_> {
        fn visit_path_mut(&mut self, path: &mut Path) {
            syn::visit_mut::visit_path_mut(self, path);
            if path.leading_colon.is_none() {
                let collides = path.segments.first().is_some_and(|segment| {
                    segment.ident == "Payload" || segment.ident == *self.wire
                });
                if collides {
                    path.segments.insert(0, syn::parse_quote!(super));
                }
            }
        }
    }
    Rebase { wire }.visit_type_mut(&mut ty);
    ty
}
fn args(ir: &SchemaIr) -> TokenStream {
    let xs = ir.original_generics.params.iter().map(|p| match p {
        GenericParam::Lifetime(x) => {
            let l = &x.lifetime;
            quote!(#l)
        }
        GenericParam::Type(x) => {
            let i = &x.ident;
            quote!(#i)
        }
        GenericParam::Const(x) => {
            let i = &x.ident;
            quote!(#i)
        }
    });
    if ir.original_generics.params.is_empty() {
        quote!()
    } else {
        quote!(<#(#xs),*>)
    }
}

pub fn generate(ir: &SchemaIr, rt: &Path, hidden: &Path, zc: &Path) -> syn::Result<TokenStream> {
    let name = &ir.ident;
    let vis = &ir.visibility;
    let module = &ir.generated_names.module;
    let wire = &ir.generated_names.wire;
    let de = &ir.generated_names.decode_error;
    let ee = &ir.generated_names.encode_error;
    let module_vis = &ir.visibility_plan.module;
    let support_vis = &ir.visibility_plan.support;
    let logical = name.to_string().trim_start_matches("r#").to_owned();
    let zerocopy_crate = syn::LitStr::new(
        zc.segments
            .first()
            .expect("resolved zerocopy path has a root")
            .ident
            .to_string()
            .trim_start_matches("r#"),
        zc.segments
            .first()
            .expect("resolved zerocopy path has a root")
            .ident
            .span(),
    );
    let tag = ir.options.tag.as_ref().ok_or_else(|| {
        syn::Error::new(name.span(), "tagged enums require #[zero(tag = TagType)]")
    })?;
    let original_args = args(ir);
    let source_lt = &ir.source_lifetime;
    let nested_types: Vec<(Type, Type)> = ir
        .variants
        .iter()
        .filter_map(|v| match &v.shape {
            VariantShape::Newtype(t) => Some(((**t).clone(), erased(t))),
            VariantShape::Unit => None,
        })
        .collect();
    let wire_params: Vec<_> = nested_types
        .iter()
        .enumerate()
        .map(|(i, _)| format_ident!("W{i}"))
        .collect();
    let mut payload_generics = Generics::default();
    for parameter in &wire_params {
        payload_generics.params.push(syn::parse_quote!(#parameter: #zc::FromBytes + #zc::KnownLayout + #zc::Immutable + 'static));
    }
    let projected_payload_args: Vec<_> = nested_types
        .iter()
        .map(|(live, _)| quote!(<#live as #rt::ZeroSchemaType>::Wire))
        .collect();
    let erased_payload_args: Vec<_> = nested_types
        .iter()
        .map(|(_, erased)| {
            let hidden_erased = rebase_erased_marker_type(erased.clone(), wire);
            quote!(<#hidden_erased as #rt::ZeroSchemaType>::Wire)
        })
        .collect();
    let projected_payload = if projected_payload_args.is_empty() {
        quote!(#module::Payload)
    } else {
        quote!(#module::Payload<#(#projected_payload_args),*>)
    };
    let wire_generics: Generics = syn::parse_quote!(<TW: #zc::FromBytes + #zc::KnownLayout + #zc::Immutable + 'static, P: #zc::FromBytes + #zc::KnownLayout + #zc::Immutable + 'static>);
    let wire_ty = quote!(#module::#wire<<#tag as #rt::ZeroSchemaType>::Wire,#projected_payload>);
    let erased_payload = if erased_payload_args.is_empty() {
        quote!(#module::Payload)
    } else {
        quote!(#module::Payload<#(#erased_payload_args),*>)
    };
    let (wire_ig, wire_tg, wire_wc) = wire_generics.split_for_impl();
    let erased_wire_ty =
        quote!(#module::#wire<<#tag as #rt::ZeroSchemaType>::Wire,#erased_payload>);
    let mut layout_generics = ir.cleaned_generics.clone();
    layout_generics
        .make_where_clause()
        .predicates
        .push(syn::parse_quote!(#tag: #rt::ZeroSchemaType));
    for (live, _) in &nested_types {
        layout_generics
            .make_where_clause()
            .predicates
            .push(syn::parse_quote!(#live: #rt::ZeroSchemaType));
    }
    let mut decode_generics = layout_generics.clone();
    decode_generics.params.insert(
        0,
        GenericParam::Lifetime(syn::LifetimeParam::new(source_lt.clone())),
    );
    if let Some(borrow_lifetime) = &ir.borrow_lifetime {
        decode_generics
            .make_where_clause()
            .predicates
            .push(syn::parse_quote!(#source_lt:#borrow_lifetime));
    }
    for (live, _) in &nested_types {
        decode_generics.make_where_clause().predicates.push(syn::parse_quote!(#live:#rt::__private::DecodeWire<#source_lt> + #rt::__private::EncodeWire));
    }
    let (dig, _, dwc) = decode_generics.split_for_impl();
    let mut encode_generics = layout_generics.clone();
    for (live, _) in &nested_types {
        encode_generics
            .make_where_clause()
            .predicates
            .push(syn::parse_quote!(#live:#rt::__private::EncodeWire));
    }
    let (eig, _, ewc) = encode_generics.split_for_impl();
    let (lig, ltg, lwc) = layout_generics.split_for_impl();
    let payloads: Vec<_> = ir
        .variants
        .iter()
        .filter_map(|v| match &v.shape {
            VariantShape::Newtype(t) => Some((v.ident.clone(), erased(t))),
            VariantShape::Unit => None,
        })
        .collect();
    let error_params: Vec<_> = nested_types
        .iter()
        .enumerate()
        .map(|(i, _)| format_ident!("E{i}"))
        .collect();
    let mut error_generics = Generics::default();
    for e in &error_params {
        error_generics
            .params
            .push(syn::parse_quote!(#e:#rt::SchemaError));
    }
    let error_args = if error_params.is_empty() {
        quote!()
    } else {
        quote!(<#(#error_params),*>)
    };
    let decode_projections: Vec<_> = nested_types
        .iter()
        .map(|(live, _)| quote!(<#live as #rt::ZeroSchemaType>::DecodeError))
        .collect();
    let encode_projections: Vec<_> = nested_types
        .iter()
        .map(|(live, _)| quote!(<#live as #rt::ZeroSchemaType>::EncodeError))
        .collect();
    let decode_error_ty = if decode_projections.is_empty() {
        quote!(#de)
    } else {
        quote!(#de<#(#decode_projections),*>)
    };
    let encode_error_ty = if encode_projections.is_empty() {
        quote!(#ee)
    } else {
        quote!(#ee<#(#encode_projections),*>)
    };
    let (err_ig, err_tg, err_wc) = error_generics.split_for_impl();
    let decode_storage = payloads
        .iter()
        .zip(error_params.iter())
        .map(|((id, _), e)| quote!(#id(#e)));
    let encode_storage = payloads
        .iter()
        .zip(error_params.iter())
        .map(|((id, _), e)| quote!(#id(#e)));
    let decode_ctors = ir
        .variants
        .iter()
        .enumerate()
        .filter_map(|(i, v)| match &v.shape {
            VariantShape::Newtype(_) => Some((i, &v.ident)),
            VariantShape::Unit => None,
        })
        .map(|(i, id)| {
            let ctor = format_ident!("__variant_{}", i);
            let e = &error_params[payloads
                .iter()
                .position(|(candidate, _)| candidate == id)
                .expect("payload variant")];
            quote!(#support_vis fn #ctor(source:#e)->Self{Self(__NestedDecodeStorage::#id(source))})
        });
    let encode_ctors = ir
        .variants
        .iter()
        .enumerate()
        .filter_map(|(i, v)| match &v.shape {
            VariantShape::Newtype(_) => Some((i, &v.ident)),
            VariantShape::Unit => None,
        })
        .map(|(i, id)| {
            let ctor = format_ident!("__variant_{}", i);
            let e = &error_params[payloads
                .iter()
                .position(|(candidate, _)| candidate == id)
                .expect("payload variant")];
            quote!(#support_vis fn #ctor(source:#e)->Self{Self(__NestedEncodeStorage::#id(source))})
        });
    let decode_parts = payloads
        .iter()
        .map(|(id, _)| quote!(__NestedDecodeStorage::#id(source)=>(stringify!(#id),source)));
    let encode_parts = payloads
        .iter()
        .map(|(id, _)| quote!(__NestedEncodeStorage::#id(source)=>(stringify!(#id),source)));
    let decode_sources = payloads
        .iter()
        .map(|(id, _)| quote!(__NestedDecodeStorage::#id(source)=>source));
    let encode_sources = payloads
        .iter()
        .map(|(id, _)| quote!(__NestedEncodeStorage::#id(source)=>source));
    let decode_debug=payloads.iter().map(|(id,_)|quote!(__NestedDecodeStorage::#id(source)=>f.debug_tuple(stringify!(#id)).field(source).finish()));
    let encode_debug=payloads.iter().map(|(id,_)|quote!(__NestedEncodeStorage::#id(source)=>f.debug_tuple(stringify!(#id)).field(source).finish()));
    let has_nested = !payloads.is_empty();
    let has_validator = ir.options.validate_with.is_some();
    let has_zero_tail = matches!(ir.options.tail, Tail::Zero);
    let has_zero_padding = matches!(ir.options.padding, Padding::Zero);
    let decode_tail_variant=has_zero_tail.then(||quote!(NonZeroTail{variant:&'static ::core::primitive::str,offset: ::core::primitive::usize},));
    let decode_padding_variant = has_zero_padding.then(|| {
        quote!(NonZeroPadding {
            offset: ::core::primitive::usize
        },)
    });
    let decode_custom_variant=has_validator.then(||quote!(Custom{field: ::core::option::Option<&'static ::core::primitive::str>,variant: ::core::option::Option<&'static ::core::primitive::str>,source:#rt::ValidationFailure},));
    let encode_custom_variant = decode_custom_variant.clone();
    let nested_decode_variant =
        has_nested.then(|| quote!(Nested(#module::NestedDecodeError #error_args),));
    let nested_encode_variant =
        has_nested.then(|| quote!(Nested(#module::NestedEncodeError #error_args),));
    let mut wi = 0usize;
    let union_fields = ir.variants.iter().enumerate().map(|(i, v)| {
        let f = format_ident!("v{i}");
        match &v.shape {
            VariantShape::Newtype(_) => {
                let w = &wire_params[wi];
                wi += 1;
                quote!(#f: ::core::mem::ManuallyDrop<#w>)
            }
            VariantShape::Unit => quote!(#f:[::core::primitive::u8;0]),
        }
    });
    let variants=ir.variants.iter().map(|v|{let id=&v.ident;let text=id.to_string().trim_start_matches("r#").to_owned();let tp=v.tag.as_ref().unwrap();match &v.shape{VariantShape::Unit=>quote!(#rt::VariantDescriptor::__new(#text,#tp as ::core::primitive::u64,::core::option::Option::None,0,1)),VariantShape::Newtype(t)=>{quote!(#rt::VariantDescriptor::__new(#text,#tp as ::core::primitive::u64,::core::option::Option::Some(<#t as #rt::ZeroSchemaType>::LAYOUT),<#t as #rt::ZeroSchemaType>::WIRE_SIZE,<#t as #rt::ZeroSchemaType>::WIRE_ALIGN))}}});
    let tags: Vec<_> = ir
        .variants
        .iter()
        .map(|v| {
            let id = &v.ident;
            let tp = v.tag.as_ref().unwrap();
            match v.shape {
                VariantShape::Unit => quote!(Self::#id=>#tp),
                VariantShape::Newtype(_) => quote!(Self::#id(_)=>#tp),
            }
        })
        .collect();
    let check_tail = matches!(ir.options.tail, Tail::Zero);
    let decode_arms=ir.variants.iter().enumerate().map(|(variant_index,v)| { let id=&v.ident; let tp=v.tag.as_ref().unwrap(); match &v.shape {
        VariantShape::Unit => { let variant=id.to_string().trim_start_matches("r#").to_owned(); let check=check_tail.then(||quote!(if let ::core::option::Option::Some(offset)=input.bytes().iter().position(|byte|*byte!=0){return ::core::result::Result::Err(#de::NonZeroTail{variant:#variant,offset});})); quote!(x if <#tag as #rt::ScalarEnum>::to_raw(x)==<#tag as #rt::ScalarEnum>::to_raw(&#tp)=>{#check ::core::result::Result::Ok(Self::#id)}) }
        VariantShape::Newtype(t) => { let variant=id.to_string().trim_start_matches("r#").to_owned(); let ctor=format_ident!("__variant_{}",variant_index);let check=check_tail.then(||quote!(if let ::core::option::Option::Some(relative)=input.bytes()[<#t as #rt::ZeroSchemaType>::WIRE_SIZE..].iter().position(|byte|*byte!=0){return ::core::result::Result::Err(#de::NonZeroTail{variant:#variant,offset:<#t as #rt::ZeroSchemaType>::WIRE_SIZE+relative});})); quote!(x if <#tag as #rt::ScalarEnum>::to_raw(x)==<#tag as #rt::ScalarEnum>::to_raw(&#tp)=>{::core::assert!(<#t as #rt::ZeroSchemaType>::WIRE_SIZE>0);let child=input.subrange::<<#t as #rt::ZeroSchemaType>::Wire>(0).map_err(#de::Layout)?;let value=<#t as #rt::__private::DecodeWire<#source_lt>>::decode_at(child).map_err(|source|#de::Nested(#module::NestedDecodeError::#ctor(source)))?;#check ::core::result::Result::Ok(Self::#id(value))}) }
    }});
    let validate_arms=ir.variants.iter().enumerate().map(|(variant_index,v)|{let id=&v.ident;let ctor=format_ident!("__variant_{}",variant_index);match &v.shape{VariantShape::Unit=>quote!(Self::#id=>::core::result::Result::Ok(())),VariantShape::Newtype(t)=>quote!(Self::#id(value)=>{::core::assert!(<#t as #rt::ZeroSchemaType>::WIRE_SIZE>0);<#t as #rt::__private::EncodeWire>::validate_encode(value).map_err(|source|#ee::Nested(#module::NestedEncodeError::#ctor(source)))})}});
    let encode_arms=ir.variants.iter().enumerate().map(|(variant_index,v)|{let id=&v.ident;let ctor=format_ident!("__variant_{}",variant_index);match &v.shape{VariantShape::Unit=>quote!(Self::#id=>::core::result::Result::Ok(())),VariantShape::Newtype(t)=>quote!(Self::#id(value)=>{::core::assert!(<#t as #rt::ZeroSchemaType>::WIRE_SIZE>0);let mut child=destination.subrange(0,<#t as #rt::ZeroSchemaType>::WIRE_SIZE).map_err(#ee::Layout)?;<#t as #rt::__private::EncodeWire>::encode_at(value,&mut child).map_err(|source|#ee::Nested(#module::NestedEncodeError::#ctor(source)))})}});
    let whole_variant_arms: Vec<_> = ir
        .variants
        .iter()
        .map(|v| {
            let id = &v.ident;
            let text = id.to_string().trim_start_matches("r#").to_owned();
            match v.shape {
                VariantShape::Unit => quote!(Self::#id=>#text),
                VariantShape::Newtype(_) => quote!(Self::#id(_)=>#text),
            }
        })
        .collect();
    let decode_whole=ir.options.validate_with.as_ref().map(|v|quote!({ let validator: fn(&#name #original_args, &#rt::ValidationContext<'_>) -> #rt::ValidationResult = #v; let variant=match self { #(#whole_variant_arms),* }; validator(self, &#rt::ValidationContext::__whole(<#name #original_args as #rt::ZeroSchemaType>::LAYOUT, ::core::option::Option::Some(variant), #rt::ValidationOperation::Decode)).map_err(|source|#de::Custom{field: ::core::option::Option::None,variant: ::core::option::Option::Some(variant),source})?; }));
    let encode_whole=ir.options.validate_with.as_ref().map(|v|quote!({ let validator: fn(&#name #original_args, &#rt::ValidationContext<'_>) -> #rt::ValidationResult = #v; let variant=match self { #(#whole_variant_arms),* }; validator(self, &#rt::ValidationContext::__whole(<#name #original_args as #rt::ZeroSchemaType>::LAYOUT, ::core::option::Option::Some(variant), #rt::ValidationOperation::Encode)).map_err(|source|#ee::Custom{field: ::core::option::Option::None,variant: ::core::option::Option::Some(variant),source})?; }));
    let decode_padding=matches!(ir.options.padding,Padding::Zero).then(||quote!(for range in <#wire_ty>::PADDING{if let ::core::option::Option::Some(relative)=input.bytes()[range.start()..range.end()].iter().position(|byte|*byte!=0){return ::core::result::Result::Err(#de::NonZeroPadding{offset:range.start()+relative});}}));
    let member_assertions=ir.variants.iter().enumerate().filter_map(|(i,v)|match &v.shape{VariantShape::Newtype(t)=>{let f=format_ident!("v{i}");::core::option::Option::Some(quote!(::core::assert!(<#t as #rt::ZeroSchemaType>::WIRE_SIZE==::core::mem::size_of::<<#t as #rt::ZeroSchemaType>::Wire>());::core::assert!(<#t as #rt::ZeroSchemaType>::WIRE_SIZE>0);::core::assert!(::core::mem::offset_of!(#projected_payload,#f)==0);::core::assert!(::core::mem::size_of::<<#t as #rt::ZeroSchemaType>::Wire>()<=::core::mem::size_of::<#projected_payload>());::core::assert!(::core::mem::align_of::<<#t as #rt::ZeroSchemaType>::Wire>()<=::core::mem::align_of::<#projected_payload>());))},VariantShape::Unit=>None});
    let nested_support=has_nested.then(||quote!(enum __NestedDecodeStorage #error_generics {#(#decode_storage),*} #support_vis struct NestedDecodeError #error_generics (__NestedDecodeStorage #err_tg); impl #err_ig NestedDecodeError #err_tg #err_wc {#(#decode_ctors)* #support_vis fn __parts(&self)->(&'static str,&dyn #rt::SchemaError){match &self.0{#(#decode_parts),*}} #support_vis fn __source(&self)->&(dyn ::core::error::Error+'static){match &self.0{#(#decode_sources),*}}} impl #err_ig ::core::fmt::Debug for NestedDecodeError #err_tg #err_wc{fn fmt(&self,f:&mut ::core::fmt::Formatter<'_>)->::core::fmt::Result{match &self.0{#(#decode_debug),*}}} enum __NestedEncodeStorage #error_generics {#(#encode_storage),*} #support_vis struct NestedEncodeError #error_generics (__NestedEncodeStorage #err_tg); impl #err_ig NestedEncodeError #err_tg #err_wc {#(#encode_ctors)* #support_vis fn __parts(&self)->(&'static str,&dyn #rt::SchemaError){match &self.0{#(#encode_parts),*}} #support_vis fn __source(&self)->&(dyn ::core::error::Error+'static){match &self.0{#(#encode_sources),*}}} impl #err_ig ::core::fmt::Debug for NestedEncodeError #err_tg #err_wc{fn fmt(&self,f:&mut ::core::fmt::Formatter<'_>)->::core::fmt::Result{match &self.0{#(#encode_debug),*}}}));
    let de_nested_source =
        has_nested.then(|| quote!(Self::Nested(w)=>::core::option::Option::Some(w.__source()),));
    let ee_nested_source =
        has_nested.then(|| quote!(Self::Nested(w)=>::core::option::Option::Some(w.__source()),));
    let de_nested_kind = has_nested.then(|| quote!(Self::Nested(w)=>w.__parts().1.kind(),));
    let ee_nested_kind = has_nested.then(|| quote!(Self::Nested(w)=>w.__parts().1.kind(),));
    let de_nested_segment=has_nested.then(||quote!(Self::Nested(w)=>::core::option::Option::Some(#rt::ErrorPathSegment::Variant(w.__parts().0)),));
    let ee_nested_segment=has_nested.then(||quote!(Self::Nested(w)=>::core::option::Option::Some(#rt::ErrorPathSegment::Variant(w.__parts().0)),));
    let de_nested_child =
        has_nested.then(|| quote!(Self::Nested(w)=>::core::option::Option::Some(w.__parts().1),));
    let ee_nested_child =
        has_nested.then(|| quote!(Self::Nested(w)=>::core::option::Option::Some(w.__parts().1),));
    let de_nested_leaf = has_nested.then(|| quote!(Self::Nested(w)=>w.__parts().1.__fmt_leaf(f),));
    let ee_nested_leaf = has_nested.then(|| quote!(Self::Nested(w)=>w.__parts().1.__fmt_leaf(f),));
    let de_nested_code =
        has_nested.then(|| quote!(Self::Nested(w)=>w.__parts().1.validation_code(),));
    let ee_nested_code =
        has_nested.then(|| quote!(Self::Nested(w)=>w.__parts().1.validation_code(),));
    let de_custom_source = has_validator
        .then(|| quote!(Self::Custom{source,..}=>::core::option::Option::Some(source),));
    let ee_custom_source = has_validator
        .then(|| quote!(Self::Custom{source,..}=>::core::option::Option::Some(source),));
    let de_tail_kind =
        has_zero_tail.then(|| quote!(Self::NonZeroTail{..}=>#rt::ErrorKind::NonZeroTail,));
    let de_padding_kind =
        has_zero_padding.then(|| quote!(Self::NonZeroPadding{..}=>#rt::ErrorKind::NonZeroPadding,));
    let de_custom_kind =
        has_validator.then(|| quote!(Self::Custom{..}=>#rt::ErrorKind::CustomValidation,));
    let ee_custom_kind =
        has_validator.then(|| quote!(Self::Custom{..}=>#rt::ErrorKind::CustomValidation,));
    let de_tail_leaf=has_zero_tail.then(||quote!(Self::NonZeroTail{offset,..}=>::core::write!(f,"nonzero inactive payload byte at offset {}",offset),));
    let de_padding_leaf=has_zero_padding.then(||quote!(Self::NonZeroPadding{offset}=>::core::write!(f,"nonzero padding byte at offset {}",offset),));
    let de_custom_leaf = has_validator
        .then(|| quote!(Self::Custom{source,..}=>::core::fmt::Display::fmt(source,f),));
    let ee_custom_leaf = has_validator
        .then(|| quote!(Self::Custom{source,..}=>::core::fmt::Display::fmt(source,f),));
    let de_custom_code = has_validator
        .then(|| quote!(Self::Custom{source,..}=>::core::option::Option::Some(source.code()),));
    let ee_custom_code = has_validator
        .then(|| quote!(Self::Custom{source,..}=>::core::option::Option::Some(source.code()),));
    let root_attr = ir
        .options
        .align
        .map(|n| quote!(#[repr(C,align(#n))]))
        .unwrap_or_else(|| quote!(#[repr(C)]));
    let tail = match ir.options.tail {
        Tail::Ignore => quote!(#rt::TailPolicy::Ignore),
        Tail::Zero => quote!(#rt::TailPolicy::Zero),
    };
    let padding = match ir.options.padding {
        Padding::Ignore => quote!(#rt::PaddingPolicy::Ignore),
        Padding::Zero => quote!(#rt::PaddingPolicy::Zero),
    };
    let encoded_storage = (!ir.original_generics.params.iter().any(|p| matches!(p, GenericParam::Type(_) | GenericParam::Const(_)))).then(|| quote! {
        #[repr(C)] #support_vis struct EncodedAlignment { _align: [#erased_wire_ty; 0] }
        #support_vis const ENCODED_SIZE: ::core::primitive::usize = ::core::mem::size_of::<#erased_wire_ty>();
        const _: () = { ::core::assert!(::core::mem::size_of::<EncodedAlignment>() == 0); ::core::assert!(::core::mem::align_of::<EncodedAlignment>() == ::core::mem::align_of::<#erased_wire_ty>()); };
    });
    let encode_method=(!ir.original_generics.params.iter().any(|p|matches!(p,GenericParam::Type(_)|GenericParam::Const(_)))).then(||quote! {
        impl #eig #name #original_args #ewc {
            #vis fn encode(&self)->::core::result::Result<#rt::AlignedBytes<#module::EncodedAlignment,{#module::ENCODED_SIZE}>,#encode_error_ty> {
                let mut output=#rt::AlignedBytes::<#module::EncodedAlignment,{#module::ENCODED_SIZE}>::zeroed();
                self.encode_into(output.as_bytes_mut())?;
                ::core::result::Result::Ok(output)
            }
        }
    });
    Ok(quote! {
      #[doc(hidden)] #module_vis mod #module { use super::*; #[repr(C)] #[derive(#zc::FromBytes,#zc::KnownLayout,#zc::Immutable)] #[zerocopy(crate = #zerocopy_crate)] #support_vis union Payload #payload_generics {#(#support_vis #union_fields,)*} #root_attr #[derive(#zc::FromBytes,#zc::KnownLayout,#zc::Immutable)] #[zerocopy(crate = #zerocopy_crate)] #support_vis struct #wire #wire_generics {#support_vis tag:TW,#support_vis payload:P} #encoded_storage #nested_support impl #wire_ig #wire #wire_tg #wire_wc { #support_vis const PADDING:[#rt::ByteRange;2]=[#rt::ByteRange::__new(::core::mem::size_of::<TW>(),::core::mem::offset_of!(Self,payload)),#rt::ByteRange::__new(::core::mem::offset_of!(Self,payload)+::core::mem::size_of::<P>(),::core::mem::size_of::<Self>())]; } }
      #[non_exhaustive] #vis enum #de #error_generics { Layout(#rt::LayoutError), UnknownUnionTag{value: ::core::primitive::u64}, #decode_tail_variant #decode_padding_variant #decode_custom_variant #nested_decode_variant }
      #[non_exhaustive] #vis enum #ee #error_generics { Layout(#rt::LayoutError), #encode_custom_variant #nested_encode_variant }
      impl #err_ig ::core::fmt::Debug for #de #err_tg #err_wc {fn fmt(&self,f:&mut ::core::fmt::Formatter<'_>)->::core::fmt::Result{f.debug_struct(#logical).field("kind",&#rt::SchemaError::kind(self)).finish()}} impl #err_ig ::core::fmt::Debug for #ee #err_tg #err_wc {fn fmt(&self,f:&mut ::core::fmt::Formatter<'_>)->::core::fmt::Result{f.debug_struct(#logical).field("kind",&#rt::SchemaError::kind(self)).finish()}}
      impl #err_ig ::core::fmt::Display for #de #err_tg #err_wc {fn fmt(&self,f:&mut ::core::fmt::Formatter<'_>)->::core::fmt::Result{#hidden::__private::__fmt_schema_error(self,f)}} impl #err_ig ::core::fmt::Display for #ee #err_tg #err_wc {fn fmt(&self,f:&mut ::core::fmt::Formatter<'_>)->::core::fmt::Result{#hidden::__private::__fmt_schema_error(self,f)}}
      impl #err_ig ::core::error::Error for #de #err_tg #err_wc {fn source(&self)->::core::option::Option<&(dyn ::core::error::Error+'static)>{match self{Self::Layout(e)=>::core::option::Option::Some(e),#de_custom_source #de_nested_source _=>::core::option::Option::None}}} impl #err_ig ::core::error::Error for #ee #err_tg #err_wc {fn source(&self)->::core::option::Option<&(dyn ::core::error::Error+'static)>{match self{Self::Layout(e)=>::core::option::Option::Some(e),#ee_custom_source #ee_nested_source}}}
      impl #err_ig #rt::SchemaError for #de #err_tg #err_wc {fn kind(&self)->#rt::ErrorKind{match self{Self::Layout(_)=>#rt::ErrorKind::Layout,Self::UnknownUnionTag{..}=>#rt::ErrorKind::UnknownUnionTag,#de_tail_kind #de_padding_kind #de_custom_kind #de_nested_kind}}fn schema(&self)->&'static str{#logical}fn segment(&self)->::core::option::Option<#rt::ErrorPathSegment>{match self{#de_nested_segment _=>::core::option::Option::None}}fn child(&self)->::core::option::Option<&dyn #rt::SchemaError>{match self{#de_nested_child _=>::core::option::Option::None}}fn __fmt_leaf(&self,f:&mut ::core::fmt::Formatter<'_>)->::core::fmt::Result{match self{Self::Layout(e)=>::core::fmt::Display::fmt(e,f),Self::UnknownUnionTag{value}=>::core::write!(f,"unknown union tag {}",value),#de_tail_leaf #de_padding_leaf #de_custom_leaf #de_nested_leaf}}fn validation_code(&self)->::core::option::Option<::core::primitive::u32>{match self{#de_custom_code #de_nested_code _=>::core::option::Option::None}}}
      impl #err_ig #rt::SchemaError for #ee #err_tg #err_wc {fn kind(&self)->#rt::ErrorKind{match self{Self::Layout(_)=>#rt::ErrorKind::Layout,#ee_custom_kind #ee_nested_kind}}fn schema(&self)->&'static str{#logical}fn segment(&self)->::core::option::Option<#rt::ErrorPathSegment>{match self{#ee_nested_segment _=>::core::option::Option::None}}fn child(&self)->::core::option::Option<&dyn #rt::SchemaError>{match self{#ee_nested_child _=>::core::option::Option::None}}fn __fmt_leaf(&self,f:&mut ::core::fmt::Formatter<'_>)->::core::fmt::Result{match self{Self::Layout(e)=>::core::fmt::Display::fmt(e,f),#ee_custom_leaf #ee_nested_leaf}}fn validation_code(&self)->::core::option::Option<::core::primitive::u32>{match self{#ee_custom_code #ee_nested_code _=>::core::option::Option::None}}}
      impl #lig #rt::ZeroSchemaType for #name #ltg #lwc{type Wire=#wire_ty;type DecodeError=#decode_error_ty;type EncodeError=#encode_error_ty;const WIRE_SIZE: ::core::primitive::usize={::core::assert!(::core::mem::offset_of!(#wire_ty,tag)==0);::core::assert!(::core::mem::offset_of!(#wire_ty,payload)>=<#tag as #rt::ZeroSchemaType>::WIRE_SIZE);#(#member_assertions)* ::core::mem::size_of::<Self::Wire>()};const WIRE_ALIGN: ::core::primitive::usize=::core::mem::align_of::<Self::Wire>();const WIRE_STRIDE: ::core::primitive::usize=match #rt::__private::__checked_wire_stride(Self::WIRE_SIZE,Self::WIRE_ALIGN){::core::option::Option::Some(x)=>x,::core::option::Option::None=>::core::panic!("wire stride overflow")};const LAYOUT:&'static #rt::LayoutDescriptor=&#rt::LayoutDescriptor::__new(#logical,#rt::TypeKind::TaggedUnion{tag_layout:<#tag as #rt::ZeroSchemaType>::LAYOUT,tag_offset:0,payload_offset: ::core::mem::offset_of!(#wire_ty,payload),payload_size: ::core::mem::size_of::<#projected_payload>(),payload_align: ::core::mem::align_of::<#projected_payload>(),tail:#tail},Self::WIRE_SIZE,Self::WIRE_ALIGN,Self::WIRE_STRIDE,#padding,&<#wire_ty>::PADDING,&[],&[],&[#(#variants),*]);}
      impl #eig #rt::TaggedUnion for #name #original_args #ewc{type Tag=#tag;type PayloadWire=#projected_payload;fn tag(&self)->#tag{match self{#(#tags),*}}fn validate_payload_encode(&self)->::core::result::Result<(),#encode_error_ty>{match self{#(#validate_arms),*}}fn encode_payload_at(&self,destination:&mut #rt::__private::Prezeroed<'_>)->::core::result::Result<(),#encode_error_ty>{match self{#(#encode_arms),*}}}
      impl #dig #rt::__private::DecodeTaggedUnion<#source_lt> for #name #original_args #dwc{fn decode_payload(tag:&#tag,input:#rt::DecodeInput<#source_lt,#projected_payload>)->::core::result::Result<Self,#decode_error_ty>{match tag{#(#decode_arms),*,other=>::core::result::Result::Err(#de::UnknownUnionTag{value:<#tag as #rt::ScalarEnum>::to_raw(other).into()})}}fn validate_decoded(&self)->::core::result::Result<(),#decode_error_ty>{#decode_whole ::core::result::Result::Ok(())}}
      impl #dig #rt::__private::DecodeWire<#source_lt> for #name #original_args #dwc{fn decode_at(input:#rt::DecodeInput<#source_lt,Self::Wire>)->::core::result::Result<Self,#decode_error_ty>{let tag_input=input.subrange::<<#tag as #rt::ZeroSchemaType>::Wire>(0).map_err(#de::Layout)?;let raw=#rt::__private::read_scalar_raw::<#tag>(tag_input.wire());let tag=<#tag as #rt::ScalarEnum>::from_raw(raw).ok_or_else(||#de::UnknownUnionTag{value:raw.into()})?;let payload=input.subrange::<#projected_payload>(::core::mem::offset_of!(#wire_ty,payload)).map_err(#de::Layout)?;let value=<Self as #rt::__private::DecodeTaggedUnion<#source_lt>>::decode_payload(&tag,payload)?;#decode_padding <Self as #rt::__private::DecodeTaggedUnion<#source_lt>>::validate_decoded(&value)?;::core::result::Result::Ok(value)}}
      impl #eig #rt::__private::EncodeWire for #name #original_args #ewc{fn validate_encode(&self)->::core::result::Result<(),#encode_error_ty>{<Self as #rt::TaggedUnion>::validate_payload_encode(self)?;#encode_whole ::core::result::Result::Ok(())}fn encode_at(&self,destination:&mut #rt::__private::Prezeroed<'_>)->::core::result::Result<(),#encode_error_ty>{let tag=<Self as #rt::TaggedUnion>::tag(self);let mut tag_dst=destination.subrange(0,<#tag as #rt::ZeroSchemaType>::WIRE_SIZE).map_err(#ee::Layout)?;#rt::__private::write_scalar_raw::<#tag>(<#tag as #rt::ScalarEnum>::to_raw(&tag),&mut tag_dst).map_err(#ee::Layout)?;let mut payload=destination.subrange(::core::mem::offset_of!(#wire_ty,payload),::core::mem::size_of::<#projected_payload>()).map_err(#ee::Layout)?;<Self as #rt::TaggedUnion>::encode_payload_at(self,&mut payload)}}
      #[allow(clippy::modulo_one)]
      impl #lig #name #ltg #lwc{#vis const WIRE_SIZE: ::core::primitive::usize=<Self as #rt::ZeroSchemaType>::WIRE_SIZE;#vis const WIRE_ALIGN: ::core::primitive::usize=<Self as #rt::ZeroSchemaType>::WIRE_ALIGN;#vis const WIRE_STRIDE: ::core::primitive::usize=<Self as #rt::ZeroSchemaType>::WIRE_STRIDE;#vis const LAYOUT:&'static #rt::LayoutDescriptor=<Self as #rt::ZeroSchemaType>::LAYOUT;#vis fn parse<'src>(bytes:&'src[::core::primitive::u8])->::core::result::Result<Self,<Self as #rt::ZeroSchemaType>::DecodeError>where Self:#rt::__private::DecodeWire<'src> + #rt::ZeroSchemaType<DecodeError=#decode_error_ty>{let input=#rt::DecodeInput::from_exact(bytes).map_err(#de::Layout)?;<Self as #rt::__private::DecodeWire<'src>>::decode_at(input)}#vis fn parse_prefix<'src>(bytes:&'src[::core::primitive::u8])->::core::result::Result<(Self,&'src[::core::primitive::u8]),<Self as #rt::ZeroSchemaType>::DecodeError>where Self:#rt::__private::DecodeWire<'src> + #rt::ZeroSchemaType<DecodeError=#decode_error_ty>{let input=#rt::DecodeInput::from_prefix(bytes).map_err(#de::Layout)?;let value=<Self as #rt::__private::DecodeWire<'src>>::decode_at(input)?;::core::result::Result::Ok((value,&bytes[Self::WIRE_SIZE..]))}#vis const fn encoded_len(&self)->::core::primitive::usize{Self::WIRE_SIZE}#vis fn encode_into(&self,bytes:&mut[::core::primitive::u8])->::core::result::Result<(),#encode_error_ty> where Self:#rt::__private::EncodeWire + #rt::ZeroSchemaType<EncodeError=#encode_error_ty>{if bytes.len()!=Self::WIRE_SIZE{return ::core::result::Result::Err(#ee::Layout(#rt::LayoutError::IncorrectSize{expected:Self::WIRE_SIZE,actual:bytes.len()}))}let address=bytes.as_ptr() as ::core::primitive::usize;if address%Self::WIRE_ALIGN!=0{return ::core::result::Result::Err(#ee::Layout(#rt::LayoutError::Misaligned{required:Self::WIRE_ALIGN,address}))}<Self as #rt::__private::EncodeWire>::validate_encode(self)?;let mut destination=#rt::__private::Prezeroed::new(bytes);<Self as #rt::__private::EncodeWire>::encode_at(self,&mut destination)}}
      #encode_method
    })
}
