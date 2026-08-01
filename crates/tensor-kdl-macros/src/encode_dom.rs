//! DOM encode_node field emission (tooling / fallback path).
//!
//! Direct write lives in `encode_emit` / `field_write_body`.

use quote::quote;

use crate::attr::{FieldInfo, FieldRole, UnwrapKind, kebab};
use crate::encode_emit::FieldAccess;

pub(crate) fn field_dom_stmts(
    fields: &[FieldInfo],
    access: FieldAccess,
) -> syn::Result<(
    Vec<proc_macro2::TokenStream>,
    Vec<proc_macro2::TokenStream>,
    proc_macro2::TokenStream,
)> {
    let mut entries = Vec::new();
    let mut children = Vec::new();
    let mut type_name_expr = quote! { ::std::option::Option::None };

    for f in fields {
        let id = &f.ident;
        let field_ref = access.by_ref(id);
        match &f.role {
            FieldRole::TypeName => {
                if f.optional {
                    type_name_expr = quote! {
                        #field_ref.as_ref().map(|s| {
                            ::tensor_kdl::KdlStr::owned(::std::string::ToString::to_string(s))
                        })
                    };
                } else {
                    type_name_expr = quote! {
                        ::std::option::Option::Some(::tensor_kdl::KdlStr::owned(
                            ::std::string::ToString::to_string(#field_ref),
                        ))
                    };
                }
            }
            FieldRole::Argument => {
                if f.optional {
                    entries.push(quote! {
                        if let ::std::option::Option::Some(__v) = #field_ref.as_ref() {
                            __entries.push(::tensor_kdl::arg_entry(
                                ::tensor_kdl::EncodeScalar::encode_scalar(__v)?,
                            ));
                        }
                    });
                } else {
                    entries.push(quote! {
                        __entries.push(::tensor_kdl::arg_entry(
                            ::tensor_kdl::EncodeScalar::encode_scalar(#field_ref)?,
                        ));
                    });
                }
            }
            FieldRole::Arguments => {
                entries.push(quote! {
                    for __v in #field_ref {
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
                        if let ::std::option::Option::Some(__v) = #field_ref.as_ref() {
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
                            ::tensor_kdl::EncodeScalar::encode_scalar(#field_ref)?,
                        ));
                    });
                }
            }
            FieldRole::Properties => {
                entries.push(quote! {
                    for (__key, __val) in #field_ref {
                        __entries.push(::tensor_kdl::prop_entry(
                            __key.clone(),
                            ::tensor_kdl::EncodeScalar::encode_scalar(__val)?,
                        ));
                    }
                });
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
                                if let ::std::option::Option::Some(__v) = #field_ref.as_ref() {
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
                                    ::tensor_kdl::EncodeScalar::encode_scalar(#field_ref)?,
                                ));
                            });
                        }
                    }
                    UnwrapKind::Property => {
                        if f.optional {
                            children.push(quote! {
                                if let ::std::option::Option::Some(__v) = #field_ref.as_ref() {
                                    __children.push(::tensor_kdl::Node {
                                        type_name: ::std::option::Option::None,
                                        name: ::tensor_kdl::KdlStr::owned(#key.to_owned()),
                                        entries: ::std::vec![::tensor_kdl::prop_entry(
                                            #key,
                                            ::tensor_kdl::EncodeScalar::encode_scalar(__v)?,
                                        )],
                                        children: ::std::vec::Vec::new(),
                                    });
                                }
                            });
                        } else {
                            children.push(quote! {
                                __children.push(::tensor_kdl::Node {
                                    type_name: ::std::option::Option::None,
                                    name: ::tensor_kdl::KdlStr::owned(#key.to_owned()),
                                    entries: ::std::vec![::tensor_kdl::prop_entry(
                                        #key,
                                        ::tensor_kdl::EncodeScalar::encode_scalar(#field_ref)?,
                                    )],
                                    children: ::std::vec::Vec::new(),
                                });
                            });
                        }
                    }
                    UnwrapKind::None => {
                        if f.optional {
                            children.push(quote! {
                                if let ::std::option::Option::Some(__v) = #field_ref.as_ref() {
                                    let mut __node = ::tensor_kdl::Encode::encode_node(__v)?;
                                    __node.name = ::tensor_kdl::KdlStr::owned(#key.to_owned());
                                    __children.push(__node);
                                }
                            });
                        } else {
                            children.push(quote! {
                                let mut __node = ::tensor_kdl::Encode::encode_node(#field_ref)?;
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
                        for __v in #field_ref {
                            let mut __n = ::tensor_kdl::Encode::encode_node(__v)?;
                            __n.name = ::tensor_kdl::KdlStr::owned(#filter.to_owned());
                            __children.push(__n);
                        }
                    });
                } else {
                    children.push(quote! {
                        for __v in #field_ref {
                            __children.push(::tensor_kdl::Encode::encode_node(__v)?);
                        }
                    });
                }
            }
            FieldRole::Flatten => {
                entries.push(quote! {
                    __entries.extend(::tensor_kdl::EncodePartial::encode_entries(#field_ref)?);
                });
                children.push(quote! {
                    __children.extend(::tensor_kdl::EncodePartial::encode_children(#field_ref)?);
                });
            }
            FieldRole::NodeName | FieldRole::Skip | FieldRole::DefaultOnly => {}
        }
    }
    Ok((entries, children, type_name_expr))
}
