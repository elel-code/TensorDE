//! Code generation for struct/enum field decode.

use quote::{format_ident, quote};
use syn::Ident;

use crate::attr::{FieldInfo, FieldRole, UnwrapKind, kebab};
use crate::visit_emit::emit_decode_from_visit;

pub(crate) fn field_emitters(
    fields: &[FieldInfo],
) -> syn::Result<(Vec<proc_macro2::TokenStream>, bool, bool)> {
    let arg_counter = format_ident!("__arg_i");
    let mut builders = Vec::new();
    let mut has_only_children = true;
    let mut uses_args = false;
    let mut flatten_fields = Vec::new();

    for field in fields {
        let id = &field.ident;
        let ty = &field.ty;
        match &field.role {
            FieldRole::Argument => {
                has_only_children = false;
                uses_args = true;
                if field.optional {
                    builders.push(quote! {
                        let #id: #ty = {
                            let idx = #arg_counter;
                            #arg_counter += 1;
                            if idx < __args.len() {
                                Some(::tensor_kdl::DecodeScalar::decode_scalar(__args[idx])?)
                            } else {
                                None
                            }
                        };
                    });
                } else {
                    builders.push(quote! {
                        let #id: #ty = {
                            let idx = #arg_counter;
                            #arg_counter += 1;
                            let v = __args.get(idx).ok_or_else(|| {
                                ::tensor_kdl::ErrorCtx::new(
                                    ::tensor_kdl::ErrorCode::MissingArgument,
                                    0,
                                )
                            })?;
                            ::tensor_kdl::DecodeScalar::decode_scalar(v)?
                        };
                    });
                }
            }
            FieldRole::Arguments => {
                has_only_children = false;
                uses_args = true;
                builders.push(quote! {
                    let #id: #ty = {
                        __args
                            .iter()
                            .skip(#arg_counter)
                            .map(|v| ::tensor_kdl::DecodeScalar::decode_scalar(v))
                            .collect::<::std::result::Result<#ty, _>>()?
                    };
                });
            }
            FieldRole::Property { name: prop_name } => {
                has_only_children = false;
                let key = prop_name.clone().unwrap_or_else(|| kebab(&id.to_string()));
                if field.optional {
                    builders.push(quote! {
                        let #id: #ty = ::tensor_kdl::opt_property(__node, #key)?;
                    });
                } else {
                    builders.push(quote! {
                        let #id: #ty = ::tensor_kdl::property(__node, #key)?;
                    });
                }
            }
            FieldRole::Properties => {
                has_only_children = false;
                builders.push(quote! {
                    let #id: #ty = {
                        __node
                            .properties()
                            .map(|(k, v)| {
                                let val = ::tensor_kdl::DecodeScalar::decode_scalar(v)?;
                                ::std::result::Result::Ok((::std::string::String::from(k), val))
                            })
                            .collect::<::std::result::Result<#ty, _>>()?
                    };
                });
            }
            FieldRole::Child { name: child_name } => {
                // Always go through slice helpers so `DecodeChildren` can bind
                // `__children` to `&doc.nodes` without cloning (Glaze P-G2).
                let key = child_name
                    .clone()
                    .or_else(|| field.rename.clone())
                    .unwrap_or_else(|| kebab(&id.to_string()));
                match field.unwrap {
                    UnwrapKind::Argument => {
                        if field.optional {
                            builders.push(quote! {
                                let #id: #ty =
                                    ::tensor_kdl::opt_one_argument_in(__children, #key)?;
                            });
                        } else {
                            builders.push(quote! {
                                let #id: #ty =
                                    ::tensor_kdl::one_argument_in(__children, #key)?;
                            });
                        }
                    }
                    UnwrapKind::Property => {
                        let prop_key = key.clone();
                        if field.optional {
                            builders.push(quote! {
                                let #id: #ty = match __children.iter().find(|n| n.name.as_str() == #key) {
                                    None => None,
                                    Some(c) => Some(::tensor_kdl::one_property(c, #prop_key)?),
                                };
                            });
                        } else {
                            builders.push(quote! {
                                let #id: #ty = {
                                    let c = __children.iter().find(|n| n.name.as_str() == #key)
                                        .ok_or_else(|| {
                                            ::tensor_kdl::ErrorCtx::new(
                                                ::tensor_kdl::ErrorCode::MissingChild,
                                                0,
                                            )
                                            .with_message(concat!("missing child `", #key, "`"))
                                        })?;
                                    ::tensor_kdl::one_property(c, #prop_key)?
                                };
                            });
                        }
                    }
                    UnwrapKind::None => {
                        if field.optional {
                            builders.push(quote! {
                                let #id: #ty = ::tensor_kdl::opt_child_in(__children, #key)?;
                            });
                        } else {
                            builders.push(quote! {
                                let #id: #ty = ::tensor_kdl::child_in(__children, #key)?;
                            });
                        }
                    }
                }
            }
            FieldRole::Children { name: child_name } => {
                let name_tok = match child_name {
                    Some(n) => quote! { ::std::option::Option::Some(#n) },
                    None => quote! { ::std::option::Option::None },
                };
                builders.push(quote! {
                    let #id: #ty = ::tensor_kdl::children_in(__children, #name_tok)?;
                });
            }
            FieldRole::NodeName => {
                builders.push(quote! {
                    let #id: #ty = ::std::convert::From::from(__node.name.as_str());
                });
            }
            FieldRole::TypeName => {
                if field.optional {
                    builders.push(quote! {
                        let #id: #ty = __node.type_name.as_ref().map(|t| {
                            ::std::convert::From::from(t.as_str())
                        });
                    });
                } else {
                    builders.push(quote! {
                        let #id: #ty = {
                            let t = __node.type_name.as_ref().ok_or_else(|| {
                                ::tensor_kdl::ErrorCtx::new(
                                    ::tensor_kdl::ErrorCode::TypeMismatch,
                                    0,
                                )
                                .with_message("missing type annotation")
                            })?;
                            ::std::convert::From::from(t.as_str())
                        };
                    });
                }
            }
            FieldRole::Flatten => {
                flatten_fields.push(id.clone());
                builders.push(quote! {
                    let mut #id: #ty = ::std::default::Default::default();
                });
            }
            FieldRole::Skip | FieldRole::DefaultOnly => {
                builders.push(quote! {
                    let #id: #ty = ::std::default::Default::default();
                });
            }
        }
    }

    // After known fields, feed remaining children/props into flatten targets.
    if !flatten_fields.is_empty() {
        let known_child_names: Vec<String> = fields
            .iter()
            .filter_map(|f| match &f.role {
                FieldRole::Child { name } => {
                    Some(name.clone().unwrap_or_else(|| kebab(&f.ident.to_string())))
                }
                FieldRole::Children { name: Some(n) } => Some(n.clone()),
                _ => None,
            })
            .collect();
        let known_prop_names: Vec<String> = fields
            .iter()
            .filter_map(|f| match &f.role {
                FieldRole::Property { name } => {
                    Some(name.clone().unwrap_or_else(|| kebab(&f.ident.to_string())))
                }
                _ => None,
            })
            .collect();

        builders.push(quote! {
            {
                let __known_children: &[&str] = &[#(#known_child_names),*];
                let __known_props: &[&str] = &[#(#known_prop_names),*];
                for __child in __children {
                    if __known_children.iter().any(|n| *n == __child.name.as_str()) {
                        continue;
                    }
                    let mut __consumed = false;
                    #(
                        if !__consumed {
                            __consumed = ::tensor_kdl::DecodePartial::insert_child(
                                &mut #flatten_fields,
                                __child,
                            )?;
                        }
                    )*
                    let _ = __consumed;
                }
                // Properties only exist on a real parent node (not document roots).
                if let Some(__node) = __parent {
                    for (__key, __val) in __node.properties() {
                        if __known_props.iter().any(|n| *n == __key) {
                            continue;
                        }
                        let mut __consumed = false;
                        #(
                            if !__consumed {
                                __consumed = ::tensor_kdl::DecodePartial::insert_property(
                                    &mut #flatten_fields,
                                    __key,
                                    __val,
                                )?;
                            }
                        )*
                        let _ = __consumed;
                    }
                }
            }
        });
    }

    let _ = arg_counter;
    Ok((builders, has_only_children, uses_args))
}

pub(crate) fn expand_struct_decode(
    name: &Ident,
    impl_generics: &syn::ImplGenerics<'_>,
    ty_generics: &syn::TypeGenerics<'_>,
    where_clause: Option<&syn::WhereClause>,
    fields: &[FieldInfo],
    validate: Option<&Ident>,
) -> syn::Result<proc_macro2::TokenStream> {
    let (builders, has_only_children, uses_args) = field_emitters(fields)?;
    let arg_counter = format_ident!("__arg_i");
    let field_names: Vec<_> = fields.iter().map(|f| &f.ident).collect();
    let args_setup = if uses_args {
        quote! {
            let __args: ::std::vec::Vec<_> = __node.arguments().collect();
            let mut #arg_counter: usize = 0;
        }
    } else {
        quote! {}
    };

    // Document / children-block roots: only child-shaped fields (no node_name /
    // type_name — those need a real parent node).
    let children_only = has_only_children
        && fields.iter().all(|f| {
            matches!(
                f.role,
                FieldRole::Child { .. }
                    | FieldRole::Children { .. }
                    | FieldRole::Flatten
                    | FieldRole::Skip
                    | FieldRole::DefaultOnly
            )
        });

    // DecodePartial only when the type is useful as a `flatten` target: no
    // required arguments, and at least one optional prop/child or a named
    // children collector / properties map / nested flatten.
    // Auto-`DecodePartial` is for knus-style *Part fragments (all-optional
    // props/children). Document roots that merely `flatten` another type should
    // not get it (and therefore should not require `Default`).
    let has_flatten = fields.iter().any(|f| matches!(f.role, FieldRole::Flatten));
    // DecodePartial is for knus-style *Part fragments: no required slots
    // (arguments, children, or properties). Required `#[kdl(child)]` document
    // roots must not get Partial (would force `Default` on the whole type).
    let no_required_slots = fields.iter().all(|f| {
        f.optional
            || matches!(
                f.role,
                FieldRole::Skip
                    | FieldRole::DefaultOnly
                    | FieldRole::Children { .. }
                    | FieldRole::Properties
                    | FieldRole::Flatten
            )
    });
    let has_partial_surface = fields.iter().any(|f| {
        (matches!(f.role, FieldRole::Child { .. } | FieldRole::Property { .. }) && f.optional)
            || matches!(f.role, FieldRole::Children { name: Some(_) })
    });
    let partial_ok = !has_flatten && no_required_slots && has_partial_surface && !fields.is_empty();

    let decode_partial = if partial_ok {
        let mut child_arms = Vec::new();
        let mut prop_arms = Vec::new();
        for f in fields {
            let id = &f.ident;
            match &f.role {
                FieldRole::Child { name } if f.optional => {
                    let key = name.clone().unwrap_or_else(|| kebab(&id.to_string()));
                    match f.unwrap {
                        UnwrapKind::Argument => {
                            child_arms.push(quote! {
                                #key => {
                                    self.#id = Some(::tensor_kdl::one_argument(node)?);
                                    return ::std::result::Result::Ok(true);
                                }
                            });
                        }
                        UnwrapKind::None => {
                            child_arms.push(quote! {
                                #key => {
                                    self.#id = Some(::tensor_kdl::Decode::decode_node(node)?);
                                    return ::std::result::Result::Ok(true);
                                }
                            });
                        }
                        UnwrapKind::Property => {
                            let pk = key.clone();
                            child_arms.push(quote! {
                                #key => {
                                    self.#id = Some(::tensor_kdl::one_property(node, #pk)?);
                                    return ::std::result::Result::Ok(true);
                                }
                            });
                        }
                    }
                }
                FieldRole::Children { name: Some(n) } => {
                    let key = n.clone();
                    child_arms.push(quote! {
                        #key => {
                            self.#id.push(::tensor_kdl::Decode::decode_node(node)?);
                            return ::std::result::Result::Ok(true);
                        }
                    });
                }
                FieldRole::Property { name } if f.optional => {
                    let key = name.clone().unwrap_or_else(|| kebab(&id.to_string()));
                    prop_arms.push(quote! {
                        #key => {
                            self.#id = Some(::tensor_kdl::DecodeScalar::decode_scalar(value)?);
                            return ::std::result::Result::Ok(true);
                        }
                    });
                }
                FieldRole::Flatten => {
                    child_arms.push(quote! {
                        _ if ::tensor_kdl::DecodePartial::insert_child(&mut self.#id, node)? => {
                            return ::std::result::Result::Ok(true);
                        }
                    });
                    prop_arms.push(quote! {
                        _ if ::tensor_kdl::DecodePartial::insert_property(
                            &mut self.#id, key, value
                        )? => {
                            return ::std::result::Result::Ok(true);
                        }
                    });
                }
                _ => {}
            }
        }
        quote! {
            #[cfg(feature = "dom")]
            impl #impl_generics ::tensor_kdl::DecodePartial<'__kdl>
                for #name #ty_generics
            #where_clause
            {
                fn insert_child(
                    &mut self,
                    node: &::tensor_kdl::Node<'__kdl>,
                ) -> ::tensor_kdl::CtxResult<bool> {
                    match node.name.as_str() {
                        #(#child_arms)*
                        _ => ::std::result::Result::Ok(false),
                    }
                }

                fn insert_property(
                    &mut self,
                    key: &str,
                    value: &::tensor_kdl::Value<'__kdl>,
                ) -> ::tensor_kdl::CtxResult<bool> {
                    match key {
                        #(#prop_arms)*
                        _ => ::std::result::Result::Ok(false),
                    }
                }
            }
        }
    } else {
        quote! {}
    };

    // P-G2 (docs/kdl/glaze-alignment.md): DecodeChildren over `&[Node]` — no
    // `doc.nodes.clone()` into a synthetic parent (Glaze never clones a DOM to
    // fill T). `decode_node` still uses a stack-only borrowed view when needed.
    //
    // P-G3e: single unfiltered `#[kdl(children)]` document root streams via
    // TopLevelFill (Glaze array element loop) instead of buffering Document.
    //
    // P-G5: multi-named children-only roots stream top-level nodes into field
    // slots (first-wins for single `child`, push for `children`) without
    // retaining a full `Document` — Glaze object key fill as keys arrive.
    let stream_all_children = if children_only && fields.len() == 1 {
        match &fields[0].role {
            FieldRole::Children { name: None } => {
                let id = &fields[0].ident;
                let ty = &fields[0].ty;
                Some((id, ty))
            }
            _ => None,
        }
    } else {
        None
    };

    // P-G5 / P-G9b: children-only roots (optional `flatten` for unknown names).
    let named_stream = if children_only
        && stream_all_children.is_none()
        && fields.iter().all(|f| {
            matches!(
                f.role,
                FieldRole::Child { .. }
                    | FieldRole::Children { .. }
                    | FieldRole::Flatten
                    | FieldRole::Skip
                    | FieldRole::DefaultOnly
            )
        }) {
        Some(crate::stream_emit::emit_named_children_read_stream(fields)?)
    } else {
        None
    };

    // P-G8b / P-G10b: non-children-only visit-fill types are document roots.
    // First top-level node fills T (Glaze root object). Optional unfiltered
    // `#[kdl(children)]` collects *subsequent* top-level nodes (mixed multi-node
    // documents: primary node + free top-level siblings).
    let single_node_document =
        !children_only && crate::visit_emit::visit_fill_supported(fields) && !has_flatten;
    let sibling_children_field: Option<&syn::Ident> = fields.iter().find_map(|f| {
        if matches!(f.role, FieldRole::Children { name: None }) {
            Some(&f.ident)
        } else {
            None
        }
    });

    let decode_doc = if children_only {
        let read_stream_override = if let Some((id, ty)) = stream_all_children {
            // `ty` is Vec<Elem>; stream into a local Vec then wrap.
            // Element type is monomorphized at the call site of TopLevelFill.
            quote! {
                fn read_stream(
                    out: &mut Self,
                    input: &'__kdl str,
                    ctx: &mut ::tensor_kdl::Context,
                    opts: ::tensor_kdl::Opts,
                ) -> ::tensor_kdl::ErrorCtx {
                    // Reuse Vec element streaming (P-G3e) then place into root.
                    // `ty` is the field type (typically Vec<Item>).
                    let mut __items: #ty = ::std::default::Default::default();
                    let ec = <#ty as ::tensor_kdl::DecodeDocument<'__kdl>>::read_stream(
                        &mut __items, input, ctx, opts,
                    );
                    if ec.is_err() {
                        return ec;
                    }
                    *out = Self { #id: __items };
                    ec
                }
            }
        } else if let Some(body) = named_stream {
            body
        } else {
            quote! {}
        };

        quote! {
            #[cfg(feature = "dom")]
            impl #impl_generics ::tensor_kdl::DecodeChildren<'__kdl>
                for #name #ty_generics
            #where_clause
            {
                fn decode_children(
                    __nodes: &[::tensor_kdl::Node<'__kdl>],
                ) -> ::tensor_kdl::CtxResult<Self> {
                    // Glaze P-G2: no clone of the node list — bind slice directly.
                    let __children: &[::tensor_kdl::Node<'__kdl>] = __nodes;
                    let __parent: ::std::option::Option<&::tensor_kdl::Node<'__kdl>> =
                        ::std::option::Option::None;
                    let _ = __parent;
                    #(#builders)*
                    ::std::result::Result::Ok(Self { #(#field_names),* })
                }
            }

            impl #impl_generics ::tensor_kdl::DecodeDocument<'__kdl>
                for #name #ty_generics
            #where_clause
            {
                #[cfg(feature = "dom")]
                fn decode_document(
                    doc: &::tensor_kdl::Document<'__kdl>,
                ) -> ::tensor_kdl::CtxResult<Self> {
                    <Self as ::tensor_kdl::DecodeChildren<'__kdl>>::decode_children(&doc.nodes)
                }

                #read_stream_override
            }
        }
    } else if single_node_document {
        let (decode_doc_body, read_stream_body) =
            emit_single_node_document_bodies(name, ty_generics, sibling_children_field);
        quote! {
            impl #impl_generics ::tensor_kdl::DecodeDocument<'__kdl>
                for #name #ty_generics
            #where_clause
            {
                #[cfg(feature = "dom")]
                fn decode_document(
                    doc: &::tensor_kdl::Document<'__kdl>,
                ) -> ::tensor_kdl::CtxResult<Self> {
                    #decode_doc_body
                }

                fn read_stream(
                    out: &mut Self,
                    input: &'__kdl str,
                    ctx: &mut ::tensor_kdl::Context,
                    opts: ::tensor_kdl::Opts,
                ) -> ::tensor_kdl::ErrorCtx {
                    #read_stream_body
                }
            }
        }
    } else {
        quote! {}
    };

    let dom_finish = if let Some(method) = validate {
        quote! {
            let __decoded = Self { #(#field_names),* };
            __decoded.#method(0)?;
            ::std::result::Result::Ok(__decoded)
        }
    } else {
        quote! {
            ::std::result::Result::Ok(Self { #(#field_names),* })
        }
    };

    let visit_fill = emit_decode_from_visit(
        name,
        impl_generics,
        ty_generics,
        where_clause,
        fields,
        validate,
    );

    Ok(quote! {
        #[cfg(feature = "dom")]
        impl #impl_generics ::tensor_kdl::Decode<'__kdl>
            for #name #ty_generics
        #where_clause
        {
            fn decode_node(
                __node: &::tensor_kdl::Node<'__kdl>,
            ) -> ::tensor_kdl::CtxResult<Self> {
                #args_setup
                let __children: &[::tensor_kdl::Node<'__kdl>] = &__node.children;
                let __parent: ::std::option::Option<&::tensor_kdl::Node<'__kdl>> =
                    ::std::option::Option::Some(__node);
                let _ = __parent;
                #(#builders)*
                #dom_finish
            }
        }
        #decode_doc
        #decode_partial
        #visit_fill
    })
}

/// P-G8b / P-G10b: decode_document + read_stream bodies for single-node roots.
fn emit_single_node_document_bodies(
    name: &Ident,
    ty_generics: &syn::TypeGenerics<'_>,
    sibling_children: Option<&Ident>,
) -> (proc_macro2::TokenStream, proc_macro2::TokenStream) {
    let decode_doc_body = if let Some(ch) = sibling_children {
        quote! {
            let __node = doc.nodes.first().ok_or_else(|| {
                ::tensor_kdl::ErrorCtx::new(
                    ::tensor_kdl::ErrorCode::ExpectedNodeName,
                    0,
                )
                .with_message("document has no nodes for single-node root")
            })?;
            let mut __root = <Self as ::tensor_kdl::Decode<'__kdl>>::decode_node(__node)?;
            // P-G10b: remaining top-level nodes → unfiltered children collector.
            for __sib in doc.nodes.iter().skip(1) {
                __root.#ch.push(::tensor_kdl::Decode::decode_node(__sib)?);
            }
            ::std::result::Result::Ok(__root)
        }
    } else {
        quote! {
            let __node = doc.nodes.first().ok_or_else(|| {
                ::tensor_kdl::ErrorCtx::new(
                    ::tensor_kdl::ErrorCode::ExpectedNodeName,
                    0,
                )
                .with_message("document has no nodes for single-node root")
            })?;
            <Self as ::tensor_kdl::Decode<'__kdl>>::decode_node(__node)
        }
    };

    let read_stream_body = if let Some(ch) = sibling_children {
        quote! {
            // P-G10b: first node → T; later top-level nodes → #ch (Glaze multi-value doc).
            use ::tensor_kdl::{NestedProbe, TopLevelFill as _};
            ctx.clear_error();
            ctx.reset_depth();
            ctx.apply_opts(opts);
            let mut __phase = 0u8;
            let owned = ::tensor_kdl::take_context_for_parser(ctx);
            let mut parser = ::tensor_kdl::Parser::with_context(input, owned);
            let visit_result = parser.visit_document_at_nodes(opts, |parser| {
                if __phase == 0 {
                    #[allow(clippy::needless_borrow)]
                    let item = (&&NestedProbe::<#name #ty_generics>::new())
                        .fill_top(parser, opts)?;
                    *out = item;
                    __phase = 1;
                } else {
                    let mut __dom = ::tensor_kdl::DomNodeBuilder::default();
                    parser.visit_node(opts, &mut __dom)?;
                    let __node = __dom.finish()?;
                    out.#ch.push(::tensor_kdl::Decode::decode_node(&__node)?);
                }
                ::std::result::Result::Ok(())
            });
            let consumed = parser.offset();
            ::tensor_kdl::restore_context_from_parser(ctx, parser);
            if let ::std::result::Result::Err(e) = visit_result {
                ctx.error = e.code;
                ctx.custom_error_message = e.message.clone();
                return e;
            }
            if __phase == 0 {
                let e = ::tensor_kdl::ErrorCtx::new(
                    ::tensor_kdl::ErrorCode::ExpectedNodeName,
                    consumed,
                )
                .with_message("document has no nodes for single-node root");
                ctx.error = e.code;
                ctx.custom_error_message = e.message.clone();
                return e;
            }
            ::tensor_kdl::ErrorCtx::ok(consumed)
        }
    } else {
        quote! {
            // P-G8b: first top-level node only (Glaze root is one value).
            use ::tensor_kdl::{NestedProbe, TopLevelFill as _};
            ctx.clear_error();
            ctx.reset_depth();
            ctx.apply_opts(opts);
            let mut __got = false;
            let owned = ::tensor_kdl::take_context_for_parser(ctx);
            let mut parser = ::tensor_kdl::Parser::with_context(input, owned);
            let stream_opts = ::tensor_kdl::Opts {
                partial_read: true,
                validate_trailing: false,
                ..opts
            };
            let visit_result = parser.visit_document_at_nodes(stream_opts, |parser| {
                #[allow(clippy::needless_borrow)]
                let item = (&&NestedProbe::<#name #ty_generics>::new())
                    .fill_top(parser, opts)?;
                *out = item;
                __got = true;
                ::std::result::Result::Ok(())
            });
            let consumed = parser.offset();
            ::tensor_kdl::restore_context_from_parser(ctx, parser);
            if let ::std::result::Result::Err(e) = visit_result {
                ctx.error = e.code;
                ctx.custom_error_message = e.message.clone();
                return e;
            }
            if !__got {
                let e = ::tensor_kdl::ErrorCtx::new(
                    ::tensor_kdl::ErrorCode::ExpectedNodeName,
                    consumed,
                )
                .with_message("document has no nodes for single-node root");
                ctx.error = e.code;
                ctx.custom_error_message = e.message.clone();
                return e;
            }
            ::tensor_kdl::ErrorCtx::ok(consumed)
        }
    };

    (decode_doc_body, read_stream_body)
}
