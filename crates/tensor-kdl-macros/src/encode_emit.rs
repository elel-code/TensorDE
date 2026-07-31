//! Emit [`Encode`] / [`EncodeDocument`] for structs and enums (mirror of decode roles).

use quote::quote;
use syn::{Data, DeriveInput, Fields, Ident, LitStr};

use crate::attr::{FieldInfo, FieldRole, UnwrapKind, kebab, parse_field};

pub(crate) fn expand_encode(input: &DeriveInput) -> syn::Result<proc_macro2::TokenStream> {
    let name = &input.ident;
    let (impl_generics, ty_generics, where_clause) = input.generics.split_for_impl();
    match &input.data {
        Data::Struct(data) => {
            let fields = match &data.fields {
                Fields::Named(fields) => fields
                    .named
                    .iter()
                    .map(parse_field)
                    .collect::<syn::Result<Vec<_>>>()?,
                Fields::Unit => Vec::new(),
                Fields::Unnamed(fields) if fields.unnamed.len() == 1 => {
                    let _ = &fields.unnamed[0].ty;
                    return Ok(quote! {
                        impl #impl_generics ::tensor_kdl::Encode for #name #ty_generics
                        #where_clause
                        {
                            fn encode_node(
                                &self,
                            ) -> ::tensor_kdl::CtxResult<::tensor_kdl::Node<'static>> {
                                ::tensor_kdl::Encode::encode_node(&self.0)
                            }
                        }
                    });
                }
                Fields::Unnamed(_) => {
                    return Err(syn::Error::new_spanned(
                        name,
                        "Encode supports named-field, unit, or single-field newtype structs",
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
        Data::Enum(data) => {
            expand_encode_enum(name, &impl_generics, &ty_generics, where_clause, data)
        }
        Data::Union(_) => Err(syn::Error::new_spanned(name, "unions are not supported")),
    }
}

fn expand_encode_enum(
    name: &Ident,
    impl_generics: &syn::ImplGenerics<'_>,
    ty_generics: &syn::TypeGenerics<'_>,
    where_clause: Option<&syn::WhereClause>,
    data: &syn::DataEnum,
) -> syn::Result<proc_macro2::TokenStream> {
    let mut arms = Vec::new();
    for variant in &data.variants {
        let vname = &variant.ident;
        let mut kdl_name = kebab(&vname.to_string());
        for attr in &variant.attrs {
            if attr.path().is_ident("kdl") {
                attr.parse_nested_meta(|meta| {
                    if meta.path.is_ident("name") {
                        let value: LitStr = meta.value()?.parse()?;
                        kdl_name = value.value();
                    }
                    Ok(())
                })?;
            }
        }
        match &variant.fields {
            Fields::Unit => {
                arms.push(quote! {
                    #name::#vname => ::std::result::Result::Ok(
                        ::tensor_kdl::flag_node(#kdl_name),
                    ),
                });
            }
            Fields::Unnamed(fields) if fields.unnamed.len() == 1 => {
                arms.push(quote! {
                    #name::#vname(__inner) => {
                        let mut __node = ::tensor_kdl::Encode::encode_node(__inner)?;
                        __node.name = ::tensor_kdl::KdlStr::owned(#kdl_name.to_owned());
                        ::std::result::Result::Ok(__node)
                    }
                });
            }
            Fields::Named(fields) => {
                let finfos: Vec<FieldInfo> = fields
                    .named
                    .iter()
                    .map(parse_field)
                    .collect::<syn::Result<_>>()?;
                reject_unsupported_encode_fields(&finfos)?;
                // Match on `&self`: edition 2024 match ergonomics bind fields by
                // shared ref without explicit `ref` (explicit `ref` is an error).
                let (entries, children, type_name_expr) =
                    field_encode_stmts(&finfos, FieldAccess::Local)?;
                let field_pats: Vec<_> = finfos.iter().map(|f| &f.ident).collect();
                arms.push(quote! {
                    #name::#vname { #(#field_pats),* } => {
                        let mut __entries = ::std::vec::Vec::new();
                        let mut __children = ::std::vec::Vec::new();
                        #(#entries)*
                        #(#children)*
                        ::std::result::Result::Ok(::tensor_kdl::Node {
                            type_name: #type_name_expr,
                            name: ::tensor_kdl::KdlStr::owned(#kdl_name.to_owned()),
                            entries: __entries,
                            children: __children,
                        })
                    }
                });
            }
            _ => {
                return Err(syn::Error::new_spanned(
                    vname,
                    "unsupported enum variant shape for Encode",
                ));
            }
        }
    }
    Ok(quote! {
        impl #impl_generics ::tensor_kdl::Encode for #name #ty_generics
        #where_clause
        {
            fn encode_node(&self) -> ::tensor_kdl::CtxResult<::tensor_kdl::Node<'static>> {
                match self {
                    #(#arms)*
                }
            }
        }
        impl #impl_generics ::tensor_kdl::EncodeDocument for #name #ty_generics
        #where_clause
        {
            fn encode_document(
                &self,
            ) -> ::tensor_kdl::CtxResult<::tensor_kdl::Document<'static>> {
                let __node = ::tensor_kdl::Encode::encode_node(self)?;
                ::std::result::Result::Ok(::tensor_kdl::Document {
                    nodes: ::std::vec![__node],
                })
            }
        }
    })
}

fn reject_unsupported_encode_fields(fields: &[FieldInfo]) -> syn::Result<()> {
    if let Some(field) = fields
        .iter()
        .find(|field| matches!(field.role, FieldRole::Flatten))
    {
        return Err(syn::Error::new_spanned(
            &field.ident,
            "Encode does not support flatten (no lossless reverse policy yet)",
        ));
    }
    Ok(())
}

#[derive(Clone, Copy)]
enum FieldAccess {
    /// Struct method body: `self.field`.
    SelfDot,
    /// Enum variant arm after `ref field` bind: bare `field`.
    Local,
}

impl FieldAccess {
    /// Value expression (`self.field` or bare `field` after `ref` bind).
    fn expr(self, id: &Ident) -> proc_macro2::TokenStream {
        match self {
            Self::SelfDot => quote! { self.#id },
            Self::Local => quote! { #id },
        }
    }

    /// `&T` receiver for `EncodeScalar` / `Encode` (Local `ref` binds are already `&T`).
    fn by_ref(self, id: &Ident) -> proc_macro2::TokenStream {
        match self {
            Self::SelfDot => quote! { &self.#id },
            Self::Local => quote! { #id },
        }
    }
}

/// Build entry/child push statements for struct or enum-variant fields.
fn field_encode_stmts(
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
        let field = access.expr(id);
        let field_ref = access.by_ref(id);
        match &f.role {
            FieldRole::NodeName => {
                // Node name is owned by the parent emitter for structs/enums.
            }
            FieldRole::TypeName => {
                if f.optional {
                    // `as_ref` works for both `Option<T>` and `&Option<T>` (auto-deref).
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
                    // Method call (not UFCS): `__v: &T` resolves `EncodeScalar for T`.
                    entries.push(quote! {
                        if let ::std::option::Option::Some(__v) = #field_ref.as_ref() {
                            __entries.push(::tensor_kdl::arg_entry(::tensor_kdl::EncodeScalar::encode_scalar(__v)?));
                        }
                    });
                } else {
                    entries.push(quote! {
                        __entries.push(::tensor_kdl::arg_entry(::tensor_kdl::EncodeScalar::encode_scalar(#field_ref)?));
                    });
                }
            }
            FieldRole::Arguments => {
                entries.push(quote! {
                    for __v in #field_ref {
                        __entries.push(::tensor_kdl::arg_entry(::tensor_kdl::EncodeScalar::encode_scalar(__v)?));
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
                // Map yields `&V`; method call keeps `Self = V`. Formatter sorts
                // keys (suite Translation Rules / `references/kdl/tests/README.md`).
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
                        // Reverse of decode unwrap(property): child named `key`
                        // with property `key=<value>`.
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
            FieldRole::Skip | FieldRole::DefaultOnly => {
                let _ = field;
            }
            FieldRole::Flatten => {
                let _ = field;
            }
        }
    }
    Ok((entries, children, type_name_expr))
}

pub(crate) fn emit_encode(
    name: &Ident,
    impl_generics: &syn::ImplGenerics<'_>,
    ty_generics: &syn::TypeGenerics<'_>,
    where_clause: Option<&syn::WhereClause>,
    fields: &[FieldInfo],
    children_only: bool,
) -> syn::Result<proc_macro2::TokenStream> {
    reject_unsupported_encode_fields(fields)?;

    let default_name = kebab(&name.to_string());
    let mut node_name_expr = quote! { #default_name };
    let mut has_node_name = false;
    for f in fields {
        if matches!(f.role, FieldRole::NodeName) {
            has_node_name = true;
            let id = &f.ident;
            node_name_expr = quote! { &self.#id };
        }
    }

    let (entries, children, type_name_expr) = field_encode_stmts(fields, FieldAccess::SelfDot)?;

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
                                    if let ::std::option::Option::Some(__v) = self.#id.as_ref() {
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
                        UnwrapKind::Property => {
                            if f.optional {
                                top.push(quote! {
                                    if let ::std::option::Option::Some(__v) = self.#id.as_ref() {
                                        __nodes.push(::tensor_kdl::Node {
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
                                top.push(quote! {
                                    __nodes.push(::tensor_kdl::Node {
                                        type_name: ::std::option::Option::None,
                                        name: ::tensor_kdl::KdlStr::owned(#key.to_owned()),
                                        entries: ::std::vec![::tensor_kdl::prop_entry(
                                            #key,
                                            ::tensor_kdl::EncodeScalar::encode_scalar(&self.#id)?,
                                        )],
                                        children: ::std::vec::Vec::new(),
                                    });
                                });
                            }
                        }
                        UnwrapKind::None => {
                            if f.optional {
                                top.push(quote! {
                                    if let ::std::option::Option::Some(__v) = self.#id.as_ref() {
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
