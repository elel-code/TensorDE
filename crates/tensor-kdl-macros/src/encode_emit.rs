//! Emit [`Encode`] / [`EncodeDocument`] with **direct** [`write_node`] /
//! [`write_document`] (Glaze `to::op` into buffer — no intermediate `Node`).

use quote::quote;
use syn::{Data, DeriveInput, Fields, Ident, LitStr};

use crate::attr::{FieldInfo, FieldRole, UnwrapKind, kebab, parse_field};
use crate::encode_dom::field_dom_stmts;

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
                    return Ok(quote! {
                        impl #impl_generics ::tensor_kdl::Encode for #name #ty_generics
                        #where_clause
                        {
                            fn write_node(
                                &self,
                                out: &mut ::tensor_kdl::WriteSink<'_>,
                                indent: usize,
                            ) -> ::std::result::Result<(), ::tensor_kdl::ErrorCtx> {
                                ::tensor_kdl::Encode::write_node(&self.0, out, indent)
                            }
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
                            | FieldRole::Flatten
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
    let mut write_arms = Vec::new();
    let mut dom_arms = Vec::new();
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
                write_arms.push(quote! {
                    #name::#vname => ::tensor_kdl::write_flag_line(out, indent, #kdl_name)
                });
                dom_arms.push(quote! {
                    #name::#vname => ::std::result::Result::Ok(
                        ::tensor_kdl::flag_node(#kdl_name),
                    ),
                });
            }
            Fields::Unnamed(fields) if fields.unnamed.len() == 1 => {
                write_arms.push(quote! {
                    #name::#vname(__inner) => {
                        let mut __node = ::tensor_kdl::Encode::encode_node(__inner)?;
                        __node.name = ::tensor_kdl::KdlStr::owned(#kdl_name.to_owned());
                        ::tensor_kdl::Encode::write_node(&__node, out, indent)
                    }
                });
                dom_arms.push(quote! {
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
                let field_pats: Vec<_> = finfos.iter().map(|f| &f.ident).collect();
                let body = field_write_body(
                    &finfos,
                    FieldAccess::Local,
                    quote! { #kdl_name },
                    quote! { ::std::option::Option::None },
                )?;
                write_arms.push(quote! {
                    #name::#vname { #(#field_pats),* } => { #body }
                });
                let (entries, children, type_name_expr) =
                    field_dom_stmts(&finfos, FieldAccess::Local)?;
                dom_arms.push(quote! {
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
            fn write_node(
                &self,
                out: &mut ::tensor_kdl::WriteSink<'_>,
                indent: usize,
            ) -> ::std::result::Result<(), ::tensor_kdl::ErrorCtx> {
                match self {
                    #(#write_arms,)*
                }
            }
            fn encode_node(&self) -> ::tensor_kdl::CtxResult<::tensor_kdl::Node<'static>> {
                match self {
                    #(#dom_arms)*
                }
            }
        }
        impl #impl_generics ::tensor_kdl::EncodeDocument for #name #ty_generics
        #where_clause
        {
            fn write_document(
                &self,
                out: &mut ::tensor_kdl::WriteSink<'_>,
            ) -> ::std::result::Result<(), ::tensor_kdl::ErrorCtx> {
                ::tensor_kdl::Encode::write_node(self, out, 0)
            }
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

#[derive(Clone, Copy)]
pub(crate) enum FieldAccess {
    SelfDot,
    Local,
}

impl FieldAccess {
    /// Parenthesized `&T` so method chains bind correctly
    /// (`(&self.field).iter()` not `&self.field.iter()`).
    pub(crate) fn by_ref(self, id: &Ident) -> proc_macro2::TokenStream {
        match self {
            Self::SelfDot => quote! { (&self.#id) },
            Self::Local => quote! { (#id) },
        }
    }
}

/// Direct-write body for a node with given name / type exprs.
fn field_write_body(
    fields: &[FieldInfo],
    access: FieldAccess,
    name_expr: proc_macro2::TokenStream,
    mut type_name_expr: proc_macro2::TokenStream,
) -> syn::Result<proc_macro2::TokenStream> {
    let mut args = Vec::new();
    let mut props = Vec::new();
    let mut children = Vec::new();
    let mut has_static_children = false;
    let mut flatten_fields: Vec<proc_macro2::TokenStream> = Vec::new();

    for f in fields {
        let id = &f.ident;
        let field_ref = access.by_ref(id);
        match &f.role {
            FieldRole::NodeName => {}
            FieldRole::TypeName => {
                if f.optional {
                    type_name_expr = quote! {
                        #field_ref.as_ref().map(|s| {
                            ::std::string::ToString::to_string(s)
                        })
                    };
                } else {
                    type_name_expr = quote! {
                        ::std::option::Option::Some(::std::string::ToString::to_string(#field_ref))
                    };
                }
            }
            FieldRole::Argument => {
                if f.optional {
                    args.push(quote! {
                        if let ::std::option::Option::Some(__v) = #field_ref.as_ref() {
                            ::tensor_kdl::write_argument_prefix(out)?;
                            ::tensor_kdl::EncodeScalar::write_scalar(__v, out)?;
                        }
                    });
                } else {
                    args.push(quote! {
                        ::tensor_kdl::write_argument_prefix(out)?;
                        ::tensor_kdl::EncodeScalar::write_scalar(#field_ref, out)?;
                    });
                }
            }
            FieldRole::Arguments => {
                args.push(quote! {
                    for __v in #field_ref {
                        ::tensor_kdl::write_argument_prefix(out)?;
                        ::tensor_kdl::EncodeScalar::write_scalar(__v, out)?;
                    }
                });
            }
            FieldRole::Property { name: pname } => {
                let key = pname.clone().unwrap_or_else(|| kebab(&id.to_string()));
                if f.optional {
                    props.push(quote! {
                        if let ::std::option::Option::Some(__v) = #field_ref.as_ref() {
                            ::tensor_kdl::write_property_key(out, #key)?;
                            ::tensor_kdl::EncodeScalar::write_scalar(__v, out)?;
                        }
                    });
                } else {
                    props.push(quote! {
                        ::tensor_kdl::write_property_key(out, #key)?;
                        ::tensor_kdl::EncodeScalar::write_scalar(#field_ref, out)?;
                    });
                }
            }
            FieldRole::Properties => {
                // Sort keys for suite Translation Rules.
                props.push(quote! {
                    {
                        let mut __items: ::std::vec::Vec<(&str, _)> = #field_ref
                            .iter()
                            .map(|(__k, __v)| (__k.as_str(), __v))
                            .collect();
                        __items.sort_by(|a, b| a.0.cmp(b.0));
                        for (__key, __val) in __items {
                            ::tensor_kdl::write_property_key(out, __key)?;
                            ::tensor_kdl::EncodeScalar::write_scalar(__val, out)?;
                        }
                    }
                });
            }
            FieldRole::Child { name: cname } => {
                has_static_children = true;
                let key = cname
                    .clone()
                    .or_else(|| f.rename.clone())
                    .unwrap_or_else(|| kebab(&id.to_string()));
                match f.unwrap {
                    UnwrapKind::Argument => {
                        if f.optional {
                            children.push(quote! {
                                if let ::std::option::Option::Some(__v) = #field_ref.as_ref() {
                                    ::tensor_kdl::write_arg_node_line(out, indent + 1, #key, __v)?;
                                }
                            });
                        } else {
                            children.push(quote! {
                                ::tensor_kdl::write_arg_node_line(
                                    out, indent + 1, #key, #field_ref,
                                )?;
                            });
                        }
                    }
                    UnwrapKind::Property => {
                        if f.optional {
                            children.push(quote! {
                                if let ::std::option::Option::Some(__v) = #field_ref.as_ref() {
                                    ::tensor_kdl::write_prop_node_line(
                                        out, indent + 1, #key, #key, __v,
                                    )?;
                                }
                            });
                        } else {
                            children.push(quote! {
                                ::tensor_kdl::write_prop_node_line(
                                    out, indent + 1, #key, #key, #field_ref,
                                )?;
                            });
                        }
                    }
                    UnwrapKind::None => {
                        // Nested Encode::write_node; force child name via thin DOM
                        // rename only when the nested type's own name differs.
                        if f.optional {
                            children.push(quote! {
                                if let ::std::option::Option::Some(__v) = #field_ref.as_ref() {
                                    let mut __node = ::tensor_kdl::Encode::encode_node(__v)?;
                                    __node.name = ::tensor_kdl::KdlStr::owned(#key.to_owned());
                                    ::tensor_kdl::Encode::write_node(&__node, out, indent + 1)?;
                                }
                            });
                        } else {
                            children.push(quote! {
                                {
                                    let mut __node = ::tensor_kdl::Encode::encode_node(#field_ref)?;
                                    __node.name = ::tensor_kdl::KdlStr::owned(#key.to_owned());
                                    ::tensor_kdl::Encode::write_node(&__node, out, indent + 1)?;
                                }
                            });
                        }
                    }
                }
            }
            FieldRole::Children { name: cname } => {
                has_static_children = true;
                if let Some(filter) = cname {
                    children.push(quote! {
                        for __v in #field_ref {
                            let mut __node = ::tensor_kdl::Encode::encode_node(__v)?;
                            __node.name = ::tensor_kdl::KdlStr::owned(#filter.to_owned());
                            ::tensor_kdl::Encode::write_node(&__node, out, indent + 1)?;
                        }
                    });
                } else {
                    children.push(quote! {
                        for __v in #field_ref {
                            ::tensor_kdl::Encode::write_node(__v, out, indent + 1)?;
                        }
                    });
                }
            }
            FieldRole::Flatten => {
                flatten_fields.push(field_ref);
            }
            FieldRole::Skip | FieldRole::DefaultOnly => {}
        }
    }

    let flatten_props: Vec<_> = flatten_fields
        .iter()
        .map(|field_ref| {
            quote! {
                ::tensor_kdl::EncodePartial::write_partial(#field_ref, out, indent)?;
            }
        })
        .collect();
    let flatten_children: Vec<_> = flatten_fields
        .iter()
        .map(|field_ref| {
            quote! {
                ::tensor_kdl::EncodePartial::write_partial_children(
                    #field_ref, out, indent + 1,
                )?;
            }
        })
        .collect();

    let children_block = if has_static_children || !flatten_fields.is_empty() {
        // Open a children block when static child roles exist, or when flatten
        // may contribute children (checked at runtime to avoid empty `{}`).
        if has_static_children {
            quote! {
                ::tensor_kdl::write_children_open(out)?;
                #(#children)*
                #(#flatten_children)*
                ::tensor_kdl::write_children_close(out, indent)?;
            }
        } else {
            // Only flatten children: open block only if any partial emits children.
            let checks: Vec<_> = flatten_fields
                .iter()
                .map(|fr| {
                    quote! {
                        if !::tensor_kdl::EncodePartial::encode_children(#fr)?.is_empty() {
                            __any_flat = true;
                        }
                    }
                })
                .collect();
            quote! {
                {
                    let mut __any_flat = false;
                    #(#checks)*
                    if __any_flat {
                        ::tensor_kdl::write_children_open(out)?;
                        #(#flatten_children)*
                        ::tensor_kdl::write_children_close(out, indent)?;
                    } else {
                        ::tensor_kdl::write_node_end_leaf(out)?;
                    }
                }
            }
        }
    } else {
        quote! {
            ::tensor_kdl::write_node_end_leaf(out)?;
        }
    };

    Ok(quote! {
        {
            let __ty: ::std::option::Option<::std::string::String> = #type_name_expr;
            ::tensor_kdl::write_node_header(
                out,
                indent,
                __ty.as_deref(),
                #name_expr,
            )?;
            #(#args)*
            #(#props)*
            #(#flatten_props)*
            #children_block
            ::std::result::Result::Ok(())
        }
    })
}

pub(crate) fn emit_encode(
    name: &Ident,
    impl_generics: &syn::ImplGenerics<'_>,
    ty_generics: &syn::TypeGenerics<'_>,
    where_clause: Option<&syn::WhereClause>,
    fields: &[FieldInfo],
    children_only: bool,
) -> syn::Result<proc_macro2::TokenStream> {
    let default_name = kebab(&name.to_string());
    let mut node_name_expr = quote! { #default_name };
    let mut has_node_name = false;
    for f in fields {
        if matches!(f.role, FieldRole::NodeName) {
            has_node_name = true;
            let id = &f.ident;
            node_name_expr = quote! { self.#id.as_str() };
        }
    }

    let write_body = field_write_body(
        fields,
        FieldAccess::SelfDot,
        node_name_expr.clone(),
        quote! { ::std::option::Option::None },
    )?;
    let (dom_entries, dom_children, dom_ty) = field_dom_stmts(fields, FieldAccess::SelfDot)?;

    let encode_node = quote! {
        impl #impl_generics ::tensor_kdl::Encode for #name #ty_generics
        #where_clause
        {
            fn write_node(
                &self,
                out: &mut ::tensor_kdl::WriteSink<'_>,
                indent: usize,
            ) -> ::std::result::Result<(), ::tensor_kdl::ErrorCtx> {
                #write_body
            }
            fn encode_node(&self) -> ::tensor_kdl::CtxResult<::tensor_kdl::Node<'static>> {
                let mut __entries = ::std::vec::Vec::new();
                let mut __children = ::std::vec::Vec::new();
                #(#dom_entries)*
                #(#dom_children)*
                let __name = ::tensor_kdl::KdlStr::owned(
                    ::std::string::ToString::to_string(#node_name_expr),
                );
                ::std::result::Result::Ok(::tensor_kdl::Node {
                    type_name: #dom_ty,
                    name: __name,
                    entries: __entries,
                    children: __children,
                })
            }
        }
    };

    let encode_doc = if children_only {
        let mut top_write = Vec::new();
        let mut top_dom = Vec::new();
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
                                top_write.push(quote! {
                                    if let ::std::option::Option::Some(__v) = self.#id.as_ref() {
                                        ::tensor_kdl::write_arg_node_line(out, 0, #key, __v)?;
                                    }
                                });
                                top_dom.push(quote! {
                                    if let ::std::option::Option::Some(__v) = self.#id.as_ref() {
                                        __nodes.push(::tensor_kdl::arg_node(
                                            #key,
                                            ::tensor_kdl::EncodeScalar::encode_scalar(__v)?,
                                        ));
                                    }
                                });
                            } else {
                                top_write.push(quote! {
                                    ::tensor_kdl::write_arg_node_line(out, 0, #key, &self.#id)?;
                                });
                                top_dom.push(quote! {
                                    __nodes.push(::tensor_kdl::arg_node(
                                        #key,
                                        ::tensor_kdl::EncodeScalar::encode_scalar(&self.#id)?,
                                    ));
                                });
                            }
                        }
                        UnwrapKind::Property => {
                            if f.optional {
                                top_write.push(quote! {
                                    if let ::std::option::Option::Some(__v) = self.#id.as_ref() {
                                        ::tensor_kdl::write_prop_node_line(
                                            out, 0, #key, #key, __v,
                                        )?;
                                    }
                                });
                            } else {
                                top_write.push(quote! {
                                    ::tensor_kdl::write_prop_node_line(
                                        out, 0, #key, #key, &self.#id,
                                    )?;
                                });
                            }
                            // DOM mirror
                            if f.optional {
                                top_dom.push(quote! {
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
                                top_dom.push(quote! {
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
                                top_write.push(quote! {
                                    if let ::std::option::Option::Some(__v) = self.#id.as_ref() {
                                        let mut __node = ::tensor_kdl::Encode::encode_node(__v)?;
                                        __node.name = ::tensor_kdl::KdlStr::owned(#key.to_owned());
                                        ::tensor_kdl::Encode::write_node(&__node, out, 0)?;
                                    }
                                });
                                top_dom.push(quote! {
                                    if let ::std::option::Option::Some(__v) = self.#id.as_ref() {
                                        let mut __node = ::tensor_kdl::Encode::encode_node(__v)?;
                                        __node.name = ::tensor_kdl::KdlStr::owned(#key.to_owned());
                                        __nodes.push(__node);
                                    }
                                });
                            } else {
                                top_write.push(quote! {
                                    {
                                        let mut __node =
                                            ::tensor_kdl::Encode::encode_node(&self.#id)?;
                                        __node.name = ::tensor_kdl::KdlStr::owned(#key.to_owned());
                                        ::tensor_kdl::Encode::write_node(&__node, out, 0)?;
                                    }
                                });
                                top_dom.push(quote! {
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
                        top_write.push(quote! {
                            for __v in &self.#id {
                                let mut __node = ::tensor_kdl::Encode::encode_node(__v)?;
                                __node.name = ::tensor_kdl::KdlStr::owned(#filter.to_owned());
                                ::tensor_kdl::Encode::write_node(&__node, out, 0)?;
                            }
                        });
                        top_dom.push(quote! {
                            for __v in &self.#id {
                                let mut __node = ::tensor_kdl::Encode::encode_node(__v)?;
                                __node.name = ::tensor_kdl::KdlStr::owned(#filter.to_owned());
                                __nodes.push(__node);
                            }
                        });
                    } else {
                        top_write.push(quote! {
                            for __v in &self.#id {
                                ::tensor_kdl::Encode::write_node(__v, out, 0)?;
                            }
                        });
                        top_dom.push(quote! {
                            for __v in &self.#id {
                                __nodes.push(::tensor_kdl::Encode::encode_node(__v)?);
                            }
                        });
                    }
                }
                FieldRole::Flatten => {
                    top_write.push(quote! {
                        ::tensor_kdl::EncodePartial::write_partial_children(
                            &self.#id, out, 0,
                        )?;
                    });
                    top_dom.push(quote! {
                        __nodes.extend(::tensor_kdl::EncodePartial::encode_children(&self.#id)?);
                    });
                }
                _ => {}
            }
        }
        quote! {
            impl #impl_generics ::tensor_kdl::EncodeDocument for #name #ty_generics
            #where_clause
            {
                fn write_document(
                    &self,
                    out: &mut ::tensor_kdl::WriteSink<'_>,
                ) -> ::std::result::Result<(), ::tensor_kdl::ErrorCtx> {
                    #(#top_write)*
                    ::std::result::Result::Ok(())
                }
                fn encode_document(
                    &self,
                ) -> ::tensor_kdl::CtxResult<::tensor_kdl::Document<'static>> {
                    let mut __nodes = ::std::vec::Vec::new();
                    #(#top_dom)*
                    ::std::result::Result::Ok(::tensor_kdl::Document { nodes: __nodes })
                }
            }
        }
    } else if !has_node_name {
        quote! {
            impl #impl_generics ::tensor_kdl::EncodeDocument for #name #ty_generics
            #where_clause
            {
                fn write_document(
                    &self,
                    out: &mut ::tensor_kdl::WriteSink<'_>,
                ) -> ::std::result::Result<(), ::tensor_kdl::ErrorCtx> {
                    ::tensor_kdl::Encode::write_node(self, out, 0)
                }
                fn encode_document(
                    &self,
                ) -> ::tensor_kdl::CtxResult<::tensor_kdl::Document<'static>> {
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
