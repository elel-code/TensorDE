//! Emit Encode / EncodeDocument with direct write_node / write_document only.
//!
//! Glaze shape: monomorphized dump into WriteSink. No encode_node / DOM path.

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
                            fn write_node_body(
                                &self,
                                out: &mut ::tensor_kdl::WriteSink<'_>,
                                indent: usize,
                            ) -> ::std::result::Result<(), ::tensor_kdl::ErrorCtx> {
                                ::tensor_kdl::Encode::write_node_body(&self.0, out, indent)
                            }
                            fn write_node_named(
                                &self,
                                out: &mut ::tensor_kdl::WriteSink<'_>,
                                indent: usize,
                                name: &str,
                            ) -> ::std::result::Result<(), ::tensor_kdl::ErrorCtx> {
                                ::tensor_kdl::Encode::write_node_named(&self.0, out, indent, name)
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
    let mut body_arms = Vec::new();
    let mut named_arms = Vec::new();
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
                body_arms.push(quote! {
                    #name::#vname => ::tensor_kdl::write_node_end_leaf(out)
                });
                named_arms.push(quote! {
                    #name::#vname => ::tensor_kdl::write_flag_line(out, indent, name)
                });
            }
            Fields::Unnamed(fields) if fields.unnamed.len() == 1 => {
                write_arms.push(quote! {
                    #name::#vname(__inner) => {
                        ::tensor_kdl::Encode::write_node_named(__inner, out, indent, #kdl_name)
                    }
                });
                body_arms.push(quote! {
                    #name::#vname(__inner) => {
                        ::tensor_kdl::Encode::write_node_body(__inner, out, indent)
                    }
                });
                named_arms.push(quote! {
                    #name::#vname(__inner) => {
                        ::tensor_kdl::Encode::write_node_named(__inner, out, indent, name)
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
                let (header_ty, body) = field_write_parts(&finfos, FieldAccess::Local)?;
                write_arms.push(quote! {
                    #name::#vname { #(#field_pats),* } => {
                        let __ty: ::std::option::Option<::std::string::String> = #header_ty;
                        ::tensor_kdl::write_node_header(out, indent, __ty.as_deref(), #kdl_name)?;
                        #body
                    }
                });
                body_arms.push(quote! {
                    #name::#vname { #(#field_pats),* } => { #body }
                });
                named_arms.push(quote! {
                    #name::#vname { #(#field_pats),* } => {
                        let __ty: ::std::option::Option<::std::string::String> = #header_ty;
                        ::tensor_kdl::write_node_header(out, indent, __ty.as_deref(), name)?;
                        #body
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
                match self { #(#write_arms,)* }
            }
            fn write_node_body(
                &self,
                out: &mut ::tensor_kdl::WriteSink<'_>,
                indent: usize,
            ) -> ::std::result::Result<(), ::tensor_kdl::ErrorCtx> {
                match self { #(#body_arms,)* }
            }
            fn write_node_named(
                &self,
                out: &mut ::tensor_kdl::WriteSink<'_>,
                indent: usize,
                name: &str,
            ) -> ::std::result::Result<(), ::tensor_kdl::ErrorCtx> {
                match self { #(#named_arms,)* }
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
        }
    })
}

#[derive(Clone, Copy)]
enum FieldAccess {
    SelfDot,
    Local,
}

impl FieldAccess {
    fn by_ref(self, id: &Ident) -> proc_macro2::TokenStream {
        match self {
            Self::SelfDot => quote! { (&self.#id) },
            Self::Local => quote! { (#id) },
        }
    }
}

/// Returns (type_name_expr: Option<String>, body_stmts after header).
fn field_write_parts(
    fields: &[FieldInfo],
    access: FieldAccess,
) -> syn::Result<(proc_macro2::TokenStream, proc_macro2::TokenStream)> {
    let mut type_name_expr = quote! { ::std::option::Option::None };
    let mut args = Vec::new();
    let mut props = Vec::new();
    let mut children = Vec::new();
    let mut has_static_children = false;
    let mut flatten_fields = Vec::new();

    for f in fields {
        let id = &f.ident;
        let field_ref = access.by_ref(id);
        match &f.role {
            FieldRole::NodeName => {}
            FieldRole::TypeName => {
                if f.optional {
                    type_name_expr = quote! {
                        #field_ref.as_ref().map(|s| ::std::string::ToString::to_string(s))
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
                                ::tensor_kdl::write_arg_node_line(out, indent + 1, #key, #field_ref)?;
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
                        if f.optional {
                            children.push(quote! {
                                if let ::std::option::Option::Some(__v) = #field_ref.as_ref() {
                                    ::tensor_kdl::Encode::write_node_named(
                                        __v, out, indent + 1, #key,
                                    )?;
                                }
                            });
                        } else {
                            children.push(quote! {
                                ::tensor_kdl::Encode::write_node_named(
                                    #field_ref, out, indent + 1, #key,
                                )?;
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
                            ::tensor_kdl::Encode::write_node_named(
                                __v, out, indent + 1, #filter,
                            )?;
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
        .map(|fr| quote! { ::tensor_kdl::EncodePartial::write_partial(#fr, out, indent)?; })
        .collect();
    let flatten_children: Vec<_> = flatten_fields
        .iter()
        .map(|fr| {
            quote! {
                ::tensor_kdl::EncodePartial::write_partial_children(#fr, out, indent + 1)?;
            }
        })
        .collect();
    let flatten_any: Vec<_> = flatten_fields
        .iter()
        .map(|fr| quote! { || ::tensor_kdl::EncodePartial::has_partial_children(#fr) })
        .collect();

    let children_block = if has_static_children {
        quote! {
            ::tensor_kdl::write_children_open(out)?;
            #(#children)*
            #(#flatten_children)*
            ::tensor_kdl::write_children_close(out, indent)?;
        }
    } else if !flatten_fields.is_empty() {
        quote! {
            if false #(#flatten_any)* {
                ::tensor_kdl::write_children_open(out)?;
                #(#flatten_children)*
                ::tensor_kdl::write_children_close(out, indent)?;
            } else {
                ::tensor_kdl::write_node_end_leaf(out)?;
            }
        }
    } else {
        quote! { ::tensor_kdl::write_node_end_leaf(out)?; }
    };

    let body = quote! {
        #(#args)*
        #(#props)*
        #(#flatten_props)*
        #children_block
        ::std::result::Result::Ok(())
    };
    Ok((type_name_expr, body))
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

    let (type_name_expr, body) = field_write_parts(fields, FieldAccess::SelfDot)?;

    let encode_impl = quote! {
        impl #impl_generics ::tensor_kdl::Encode for #name #ty_generics
        #where_clause
        {
            fn write_node(
                &self,
                out: &mut ::tensor_kdl::WriteSink<'_>,
                indent: usize,
            ) -> ::std::result::Result<(), ::tensor_kdl::ErrorCtx> {
                let __ty: ::std::option::Option<::std::string::String> = #type_name_expr;
                ::tensor_kdl::write_node_header(out, indent, __ty.as_deref(), #node_name_expr)?;
                #body
            }
            fn write_node_body(
                &self,
                out: &mut ::tensor_kdl::WriteSink<'_>,
                indent: usize,
            ) -> ::std::result::Result<(), ::tensor_kdl::ErrorCtx> {
                #body
            }
            fn write_node_named(
                &self,
                out: &mut ::tensor_kdl::WriteSink<'_>,
                indent: usize,
                name: &str,
            ) -> ::std::result::Result<(), ::tensor_kdl::ErrorCtx> {
                let __ty: ::std::option::Option<::std::string::String> = #type_name_expr;
                ::tensor_kdl::write_node_header(out, indent, __ty.as_deref(), name)?;
                #body
            }
        }
    };

    let encode_doc = if children_only {
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
                                        ::tensor_kdl::write_arg_node_line(out, 0, #key, __v)?;
                                    }
                                });
                            } else {
                                top.push(quote! {
                                    ::tensor_kdl::write_arg_node_line(out, 0, #key, &self.#id)?;
                                });
                            }
                        }
                        UnwrapKind::Property => {
                            if f.optional {
                                top.push(quote! {
                                    if let ::std::option::Option::Some(__v) = self.#id.as_ref() {
                                        ::tensor_kdl::write_prop_node_line(
                                            out, 0, #key, #key, __v,
                                        )?;
                                    }
                                });
                            } else {
                                top.push(quote! {
                                    ::tensor_kdl::write_prop_node_line(
                                        out, 0, #key, #key, &self.#id,
                                    )?;
                                });
                            }
                        }
                        UnwrapKind::None => {
                            if f.optional {
                                top.push(quote! {
                                    if let ::std::option::Option::Some(__v) = self.#id.as_ref() {
                                        ::tensor_kdl::Encode::write_node_named(
                                            __v, out, 0, #key,
                                        )?;
                                    }
                                });
                            } else {
                                top.push(quote! {
                                    ::tensor_kdl::Encode::write_node_named(
                                        &self.#id, out, 0, #key,
                                    )?;
                                });
                            }
                        }
                    }
                }
                FieldRole::Children { name: filter } => {
                    if let Some(filter) = filter {
                        top.push(quote! {
                            for __v in &self.#id {
                                ::tensor_kdl::Encode::write_node_named(__v, out, 0, #filter)?;
                            }
                        });
                    } else {
                        top.push(quote! {
                            for __v in &self.#id {
                                ::tensor_kdl::Encode::write_node(__v, out, 0)?;
                            }
                        });
                    }
                }
                FieldRole::Flatten => {
                    top.push(quote! {
                        ::tensor_kdl::EncodePartial::write_partial_children(&self.#id, out, 0)?;
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
                    #(#top)*
                    ::std::result::Result::Ok(())
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
            }
        }
    } else {
        quote! {}
    };

    Ok(quote! {
        #encode_impl
        #encode_doc
    })
}
