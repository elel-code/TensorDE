//! Emit [`Encode`] / [`EncodeDocument`] for structs (mirror of decode roles).

use quote::quote;
use syn::{Data, DeriveInput, Fields, Ident};

use crate::attr::{FieldInfo, FieldRole, UnwrapKind, kebab, parse_field};

pub(crate) fn expand_encode(input: &DeriveInput) -> syn::Result<proc_macro2::TokenStream> {
    let name = &input.ident;
    let (impl_generics, ty_generics, where_clause) = input.generics.split_for_impl();
    let fields = match &input.data {
        Data::Struct(data) => match &data.fields {
            Fields::Named(fields) => fields
                .named
                .iter()
                .map(parse_field)
                .collect::<syn::Result<Vec<_>>>()?,
            Fields::Unit => Vec::new(),
            _ => {
                return Err(syn::Error::new_spanned(
                    name,
                    "Encode supports named-field and unit structs",
                ));
            }
        },
        _ => {
            return Err(syn::Error::new_spanned(
                name,
                "Encode currently supports structs only",
            ));
        }
    };
    let children_only = !fields.is_empty()
        && fields.iter().all(|field| {
            matches!(
                field.role,
                FieldRole::Child { .. }
                    | FieldRole::Children { .. }
                    | FieldRole::Skip
                    | FieldRole::DefaultOnly
            )
        });
    emit_encode(
        name,
        &impl_generics,
        &ty_generics,
        where_clause,
        &fields,
        children_only,
    )
}

pub(crate) fn emit_encode(
    name: &Ident,
    impl_generics: &syn::ImplGenerics<'_>,
    ty_generics: &syn::TypeGenerics<'_>,
    where_clause: Option<&syn::WhereClause>,
    fields: &[FieldInfo],
    children_only: bool,
) -> syn::Result<proc_macro2::TokenStream> {
    if let Some(field) = fields.iter().find(|field| {
        matches!(field.role, FieldRole::Flatten | FieldRole::Properties)
            || matches!(field.unwrap, UnwrapKind::Property)
    }) {
        return Err(syn::Error::new_spanned(
            &field.ident,
            "Encode does not yet support flatten, properties maps, or unwrap(property)",
        ));
    }

    let mut entries = Vec::new();
    let mut children = Vec::new();
    let default_name = kebab(&name.to_string());
    let mut node_name_expr = quote! { #default_name };
    let mut type_name_expr = quote! { ::std::option::Option::None };
    let mut has_node_name = false;

    for f in fields {
        let id = &f.ident;
        match &f.role {
            FieldRole::NodeName => {
                has_node_name = true;
                node_name_expr = quote! { &self.#id };
            }
            FieldRole::TypeName => {
                if f.optional {
                    type_name_expr = quote! {
                        self.#id.as_ref().map(|s| {
                            ::tensor_kdl::KdlStr::owned(::std::string::ToString::to_string(s))
                        })
                    };
                } else {
                    type_name_expr = quote! {
                        ::std::option::Option::Some(::tensor_kdl::KdlStr::owned(
                            ::std::string::ToString::to_string(&self.#id),
                        ))
                    };
                }
            }
            FieldRole::Argument => {
                if f.optional {
                    entries.push(quote! {
                        if let ::std::option::Option::Some(ref __v) = self.#id {
                            __entries.push(::tensor_kdl::arg_entry(
                                ::tensor_kdl::EncodeScalar::encode_scalar(__v)?,
                            ));
                        }
                    });
                } else {
                    entries.push(quote! {
                        __entries.push(::tensor_kdl::arg_entry(
                            ::tensor_kdl::EncodeScalar::encode_scalar(&self.#id)?,
                        ));
                    });
                }
            }
            FieldRole::Arguments => {
                entries.push(quote! {
                    for __v in &self.#id {
                        __entries.push(::tensor_kdl::arg_entry(
                            ::tensor_kdl::EncodeScalar::encode_scalar(__v)?,
                        ));
                    }
                });
            }
            FieldRole::Property { name: pname } => {
                let key = pname.clone().unwrap_or_else(|| kebab(&id.to_string()));
                if f.optional {
                    entries.push(quote! {
                        if let ::std::option::Option::Some(ref __v) = self.#id {
                            __entries.push(::tensor_kdl::prop_entry(
                                #key,
                                ::tensor_kdl::EncodeScalar::encode_scalar(__v)?,
                            ));
                        }
                    });
                } else {
                    entries.push(quote! {
                        __entries.push(::tensor_kdl::prop_entry(
                            #key,
                            ::tensor_kdl::EncodeScalar::encode_scalar(&self.#id)?,
                        ));
                    });
                }
            }
            FieldRole::Child { name: cname } => {
                let key = cname
                    .clone()
                    .or_else(|| f.rename.clone())
                    .unwrap_or_else(|| kebab(&id.to_string()));
                match f.unwrap {
                    UnwrapKind::Argument => {
                        if f.optional {
                            children.push(quote! {
                                if let ::std::option::Option::Some(ref __v) = self.#id {
                                    __children.push(::tensor_kdl::arg_node(
                                        #key,
                                        ::tensor_kdl::EncodeScalar::encode_scalar(__v)?,
                                    ));
                                }
                            });
                        } else {
                            children.push(quote! {
                                __children.push(::tensor_kdl::arg_node(
                                    #key,
                                    ::tensor_kdl::EncodeScalar::encode_scalar(&self.#id)?,
                                ));
                            });
                        }
                    }
                    _ => {
                        if f.optional {
                            children.push(quote! {
                                if let ::std::option::Option::Some(ref __v) = self.#id {
                                    let mut __node = ::tensor_kdl::Encode::encode_node(__v)?;
                                    __node.name = ::tensor_kdl::KdlStr::owned(#key.to_owned());
                                    __children.push(__node);
                                }
                            });
                        } else {
                            children.push(quote! {
                                let mut __node = ::tensor_kdl::Encode::encode_node(&self.#id)?;
                                __node.name = ::tensor_kdl::KdlStr::owned(#key.to_owned());
                                __children.push(__node);
                            });
                        }
                    }
                }
            }
            FieldRole::Children { name: cname } => {
                if let Some(filter) = cname {
                    children.push(quote! {
                        for __v in &self.#id {
                            let mut __n = ::tensor_kdl::Encode::encode_node(__v)?;
                            __n.name = ::tensor_kdl::KdlStr::owned(#filter.to_owned());
                            __children.push(__n);
                        }
                    });
                } else {
                    children.push(quote! {
                        for __v in &self.#id {
                            __children.push(::tensor_kdl::Encode::encode_node(__v)?);
                        }
                    });
                }
            }
            FieldRole::Skip | FieldRole::DefaultOnly => {}
            _ => {}
        }
    }

    let encode_node = quote! {
        impl #impl_generics ::tensor_kdl::Encode for #name #ty_generics
        #where_clause
        {
            fn encode_node(&self) -> ::tensor_kdl::CtxResult<::tensor_kdl::Node<'static>> {
                let mut __entries = ::std::vec::Vec::new();
                let mut __children = ::std::vec::Vec::new();
                #(#entries)*
                #(#children)*
                let __name = ::tensor_kdl::KdlStr::owned(
                    ::std::string::ToString::to_string(#node_name_expr),
                );
                ::std::result::Result::Ok(::tensor_kdl::Node {
                    type_name: #type_name_expr,
                    name: __name,
                    entries: __entries,
                    children: __children,
                })
            }
        }
    };

    let encode_doc = if children_only {
        // Document root: each child field becomes top-level node(s).
        let mut top = Vec::new();
        for f in fields {
            let id = &f.ident;
            match &f.role {
                FieldRole::Child { name: cname } => {
                    let key = cname
                        .clone()
                        .or_else(|| f.rename.clone())
                        .unwrap_or_else(|| kebab(&id.to_string()));
                    match f.unwrap {
                        UnwrapKind::Argument => {
                            if f.optional {
                                top.push(quote! {
                                    if let ::std::option::Option::Some(ref __v) = self.#id {
                                        __nodes.push(::tensor_kdl::arg_node(
                                            #key,
                                            ::tensor_kdl::EncodeScalar::encode_scalar(__v)?,
                                        ));
                                    }
                                });
                            } else {
                                top.push(quote! {
                                    __nodes.push(::tensor_kdl::arg_node(
                                        #key,
                                        ::tensor_kdl::EncodeScalar::encode_scalar(&self.#id)?,
                                    ));
                                });
                            }
                        }
                        _ => {
                            if f.optional {
                                top.push(quote! {
                                    if let ::std::option::Option::Some(ref __v) = self.#id {
                                        let mut __node = ::tensor_kdl::Encode::encode_node(__v)?;
                                        __node.name = ::tensor_kdl::KdlStr::owned(#key.to_owned());
                                        __nodes.push(__node);
                                    }
                                });
                            } else {
                                top.push(quote! {
                                    let mut __node = ::tensor_kdl::Encode::encode_node(&self.#id)?;
                                    __node.name = ::tensor_kdl::KdlStr::owned(#key.to_owned());
                                    __nodes.push(__node);
                                });
                            }
                        }
                    }
                }
                FieldRole::Children { name: filter } => {
                    if let Some(filter) = filter {
                        top.push(quote! {
                            for __v in &self.#id {
                                let mut __node = ::tensor_kdl::Encode::encode_node(__v)?;
                                __node.name = ::tensor_kdl::KdlStr::owned(#filter.to_owned());
                                __nodes.push(__node);
                            }
                        });
                    } else {
                        top.push(quote! {
                            for __v in &self.#id {
                                __nodes.push(::tensor_kdl::Encode::encode_node(__v)?);
                            }
                        });
                    }
                }
                _ => {}
            }
        }
        quote! {
            impl #impl_generics ::tensor_kdl::EncodeDocument for #name #ty_generics
            #where_clause
            {
                fn encode_document(&self) -> ::tensor_kdl::CtxResult<::tensor_kdl::Document<'static>> {
                    let mut __nodes = ::std::vec::Vec::new();
                    #(#top)*
                    ::std::result::Result::Ok(::tensor_kdl::Document { nodes: __nodes })
                }
            }
        }
    } else if !has_node_name {
        // Single-node document root (P-G8b shape).
        quote! {
            impl #impl_generics ::tensor_kdl::EncodeDocument for #name #ty_generics
            #where_clause
            {
                fn encode_document(&self) -> ::tensor_kdl::CtxResult<::tensor_kdl::Document<'static>> {
                    let __node = ::tensor_kdl::Encode::encode_node(self)?;
                    ::std::result::Result::Ok(::tensor_kdl::Document {
                        nodes: ::std::vec![__node],
                    })
                }
            }
        }
    } else {
        quote! {}
    };

    Ok(quote! {
        #encode_node
        #encode_doc
    })
}
