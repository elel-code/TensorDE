//! DecodeScalar derive expansion.

use quote::quote;
use syn::{Data, DeriveInput, Fields, GenericParam, Lifetime, LifetimeParam, LitStr};

use crate::attr::kebab;

pub(crate) fn expand_decode_scalar(input: &DeriveInput) -> syn::Result<proc_macro2::TokenStream> {
    let name = &input.ident;
    let mut generics = input.generics.clone();
    let lifetime = Lifetime::new("'__kdl", proc_macro2::Span::call_site());
    generics
        .params
        .insert(0, GenericParam::Lifetime(LifetimeParam::new(lifetime)));
    let (impl_generics, _, where_clause) = generics.split_for_impl();
    let (_, ty_generics, _) = input.generics.split_for_impl();

    match &input.data {
        Data::Enum(data) => {
            let mut arms = Vec::new();
            for variant in &data.variants {
                if !matches!(variant.fields, Fields::Unit) {
                    return Err(syn::Error::new_spanned(
                        &variant.ident,
                        "DecodeScalar enums must be unit variants",
                    ));
                }
                let vname = &variant.ident;
                let mut label = kebab(&vname.to_string());
                for attr in &variant.attrs {
                    if attr.path().is_ident("kdl") {
                        attr.parse_nested_meta(|meta| {
                            if meta.path.is_ident("name") {
                                let v: LitStr = meta.value()?.parse()?;
                                label = v.value();
                            }
                            Ok(())
                        })?;
                    }
                }
                arms.push(quote! {
                    #label => ::std::result::Result::Ok(#name::#vname),
                });
            }
            Ok(quote! {
                impl #impl_generics ::tensor_kdl::DecodeScalar<'__kdl>
                    for #name #ty_generics
                #where_clause
                {
                    fn decode_scalar(
                        value: &::tensor_kdl::Value<'__kdl>,
                    ) -> ::tensor_kdl::CtxResult<Self> {
                        let s = value.as_str().ok_or_else(|| {
                            ::tensor_kdl::ErrorCtx::new(
                                ::tensor_kdl::ErrorCode::TypeMismatch,
                                0,
                            )
                            .with_expected("string")
                        })?;
                        match s {
                            #(#arms)*
                            other => ::std::result::Result::Err(::tensor_kdl::ErrorCtx::new(
                                ::tensor_kdl::ErrorCode::TypeMismatch,
                                0,
                            )
                            .with_message(::std::format!("unknown value `{other}`"))),
                        }
                    }
                }
            })
        }
        Data::Struct(data) => match &data.fields {
            Fields::Unnamed(fields) if fields.unnamed.len() == 1 => {
                let inner = &fields.unnamed[0].ty;
                Ok(quote! {
                    impl #impl_generics ::tensor_kdl::DecodeScalar<'__kdl>
                        for #name #ty_generics
                    #where_clause
                    {
                        fn decode_scalar(
                            value: &::tensor_kdl::Value<'__kdl>,
                        ) -> ::tensor_kdl::CtxResult<Self> {
                            ::std::result::Result::Ok(#name(
                                <#inner as ::tensor_kdl::DecodeScalar<'__kdl>>::decode_scalar(value)?,
                            ))
                        }
                    }
                })
            }
            _ => Err(syn::Error::new_spanned(
                name,
                "DecodeScalar supports unit enums or single-field newtypes",
            )),
        },
        _ => Err(syn::Error::new_spanned(
            name,
            "DecodeScalar supports enums and newtypes only",
        )),
    }
}

pub(crate) fn expand_encode_scalar(input: &DeriveInput) -> syn::Result<proc_macro2::TokenStream> {
    let name = &input.ident;
    let (impl_generics, ty_generics, where_clause) = input.generics.split_for_impl();

    match &input.data {
        Data::Enum(data) => {
            let mut arms = Vec::new();
            for variant in &data.variants {
                if !matches!(variant.fields, Fields::Unit) {
                    return Err(syn::Error::new_spanned(
                        &variant.ident,
                        "EncodeScalar enums must be unit variants",
                    ));
                }
                let variant_name = &variant.ident;
                let mut label = kebab(&variant_name.to_string());
                for attr in &variant.attrs {
                    if attr.path().is_ident("kdl") {
                        attr.parse_nested_meta(|meta| {
                            if meta.path.is_ident("name") {
                                let value: LitStr = meta.value()?.parse()?;
                                label = value.value();
                            }
                            Ok(())
                        })?;
                    }
                }
                arms.push(quote! { #name::#variant_name => #label });
            }
            Ok(quote! {
                impl #impl_generics ::tensor_kdl::EncodeScalar for #name #ty_generics
                #where_clause
                {
                    fn write_scalar(
                        &self,
                        out: &mut ::tensor_kdl::WriteSink<'_>,
                    ) -> ::std::result::Result<(), ::tensor_kdl::ErrorCtx> {
                        let __value = match self {
                            #(#arms,)*
                        };
                        ::tensor_kdl::write_ident_or_string(out, __value)
                    }
                    fn encode_scalar(
                        &self,
                    ) -> ::tensor_kdl::CtxResult<::tensor_kdl::Value<'static>> {
                        let __value = match self {
                            #(#arms,)*
                        };
                        ::std::result::Result::Ok(::tensor_kdl::Value::String(
                            ::tensor_kdl::KdlStr::owned(__value.to_owned()),
                        ))
                    }
                }
            })
        }
        Data::Struct(data) => match &data.fields {
            Fields::Unnamed(fields) if fields.unnamed.len() == 1 => Ok(quote! {
                impl #impl_generics ::tensor_kdl::EncodeScalar for #name #ty_generics
                #where_clause
                {
                    fn write_scalar(
                        &self,
                        out: &mut ::tensor_kdl::WriteSink<'_>,
                    ) -> ::std::result::Result<(), ::tensor_kdl::ErrorCtx> {
                        ::tensor_kdl::EncodeScalar::write_scalar(&self.0, out)
                    }
                    fn encode_scalar(
                        &self,
                    ) -> ::tensor_kdl::CtxResult<::tensor_kdl::Value<'static>> {
                        ::tensor_kdl::EncodeScalar::encode_scalar(&self.0)
                    }
                }
            }),
            _ => Err(syn::Error::new_spanned(
                name,
                "EncodeScalar supports unit enums or single-field newtypes",
            )),
        },
        _ => Err(syn::Error::new_spanned(
            name,
            "EncodeScalar supports enums and newtypes only",
        )),
    }
}
