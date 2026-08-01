//! Top-level Decode derive expansion.

use quote::{format_ident, quote};
use syn::{Data, DeriveInput, Fields, GenericParam, Lifetime, LifetimeParam, LitStr};

use crate::attr::{FieldInfo, kebab, parse_field};
use crate::emit::{expand_struct_decode, field_emitters};

pub(crate) fn expand_decode(input: &DeriveInput) -> syn::Result<proc_macro2::TokenStream> {
    let name = &input.ident;
    let mut generics = input.generics.clone();
    let lifetime = Lifetime::new("'__kdl", proc_macro2::Span::call_site());
    generics
        .params
        .insert(0, GenericParam::Lifetime(LifetimeParam::new(lifetime)));
    let (impl_generics, _, where_clause) = generics.split_for_impl();
    let (_, ty_generics, _) = input.generics.split_for_impl();

    match &input.data {
        Data::Struct(data) => {
            let fields = match &data.fields {
                Fields::Named(n) => n
                    .named
                    .iter()
                    .map(parse_field)
                    .collect::<syn::Result<Vec<_>>>()?,
                Fields::Unit => Vec::new(),
                Fields::Unnamed(fields) if fields.unnamed.len() == 1 => {
                    // Newtype: delegate Decode to inner.
                    let inner = &fields.unnamed[0].ty;
                    return Ok(quote! {
                        #[cfg(feature = "dom")]
                        impl #impl_generics ::tensor_kdl::Decode<'__kdl>
                            for #name #ty_generics
                        #where_clause
                        {
                            fn decode_node(
                                __node: &::tensor_kdl::Node<'__kdl>,
                            ) -> ::tensor_kdl::CtxResult<Self> {
                                ::std::result::Result::Ok(#name(
                                    <#inner as ::tensor_kdl::Decode<'__kdl>>::decode_node(__node)?,
                                ))
                            }
                        }
                    });
                }
                Fields::Unnamed(_) => {
                    return Err(syn::Error::new_spanned(
                        name,
                        "only single-field newtype tuple structs are supported",
                    ));
                }
            };
            expand_struct_decode(name, &impl_generics, &ty_generics, where_clause, &fields)
        }
        Data::Enum(data) => {
            let mut arms = Vec::new();
            for variant in &data.variants {
                let vname = &variant.ident;
                // Allow `#[kdl(name = "...")]` on variants.
                let mut kdl_name = kebab(&vname.to_string());
                for attr in &variant.attrs {
                    if attr.path().is_ident("kdl") {
                        attr.parse_nested_meta(|meta| {
                            if meta.path.is_ident("name") {
                                let v: LitStr = meta.value()?.parse()?;
                                kdl_name = v.value();
                            }
                            Ok(())
                        })?;
                    }
                }
                match &variant.fields {
                    Fields::Unit => {
                        arms.push(quote! {
                            #kdl_name => ::std::result::Result::Ok(#name::#vname),
                        });
                    }
                    Fields::Unnamed(fields) if fields.unnamed.len() == 1 => {
                        let inner_ty = &fields.unnamed[0].ty;
                        arms.push(quote! {
                            #kdl_name => ::std::result::Result::Ok(#name::#vname(
                                <#inner_ty as ::tensor_kdl::Decode<'__kdl>>::decode_node(__node)?
                            )),
                        });
                    }
                    Fields::Named(fields) => {
                        let finfos: Vec<FieldInfo> = fields
                            .named
                            .iter()
                            .map(parse_field)
                            .collect::<syn::Result<_>>()?;
                        let (builders, _, uses_args) = field_emitters(&finfos)?;
                        let arg_counter = format_ident!("__arg_i");
                        let field_names: Vec<_> = finfos.iter().map(|f| &f.ident).collect();
                        let args_setup = if uses_args {
                            quote! {
                                let __args: ::std::vec::Vec<_> = __node.arguments().collect();
                                let mut #arg_counter: usize = 0;
                            }
                        } else {
                            quote! {}
                        };
                        arms.push(quote! {
                            #kdl_name => {
                                #args_setup
                                #(#builders)*
                                ::std::result::Result::Ok(#name::#vname {
                                    #(#field_names),*
                                })
                            }
                        });
                    }
                    _ => {
                        return Err(syn::Error::new_spanned(
                            vname,
                            "unsupported enum variant shape",
                        ));
                    }
                }
            }
            Ok(quote! {
                #[cfg(feature = "dom")]
                impl #impl_generics ::tensor_kdl::Decode<'__kdl>
                    for #name #ty_generics
                #where_clause
                {
                    fn decode_node(
                        __node: &::tensor_kdl::Node<'__kdl>,
                    ) -> ::tensor_kdl::CtxResult<Self> {
                        match __node.name.as_str() {
                            #(#arms)*
                            other => ::std::result::Result::Err(::tensor_kdl::ErrorCtx::new(
                                ::tensor_kdl::ErrorCode::UnknownChild,
                                0,
                            )
                            .with_message(::std::format!("unknown node `{other}`"))),
                        }
                    }
                }
            })
        }
        Data::Union(_) => Err(syn::Error::new_spanned(name, "unions are not supported")),
    }
}
