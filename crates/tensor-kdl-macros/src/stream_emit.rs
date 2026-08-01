//! Document-root streaming emit (P-G5 / P-G9b / **P-G12**).
//!
//! Named children fill as top-level nodes arrive via
//! [`visit_document_at_nodes`](tensor_kdl::Parser::visit_document_at_nodes) —
//! Glaze key-fill without materializing a `Node` / `Document`.

use quote::quote;
use syn::{Ident, Type};

use crate::attr::{FieldInfo, FieldRole, UnwrapKind, kebab};
use crate::visit_emit::{option_inner_type, vec_inner_type};

/// P-G12: stream top-level nodes into a children-only document root (no tree).
///
/// First-wins for single `#[kdl(child)]`; append for `#[kdl(children)]`.
/// `unwrap(argument|property)` uses peel visitors; full children use
/// [`NestedProbe`] / [`TopLevelFill`].
///
/// `#[kdl(flatten)]` on document roots still requires feature `dom` +
/// [`DecodePartial`] (unknown free nodes as trees). Without flatten, unknown
/// names are structurally skipped.
pub(crate) fn emit_named_children_read_stream(
    fields: &[FieldInfo],
) -> syn::Result<proc_macro2::TokenStream> {
    let mut slot_decls = Vec::new();
    let mut match_arms = Vec::new();
    let mut finish_fields = Vec::new();
    let mut unfiltered: Option<(&Ident, &Type)> = None;
    let mut has_flatten = false;

    for f in fields {
        let id = &f.ident;
        let ty = &f.ty;
        match &f.role {
            FieldRole::Child { name: cname } => {
                let key = cname
                    .clone()
                    .or_else(|| f.rename.clone())
                    .unwrap_or_else(|| kebab(&id.to_string()));
                let fill = match f.unwrap {
                    UnwrapKind::Argument => {
                        if f.optional {
                            quote! {
                                ::tensor_kdl::peel_opt_argument_after_header::<_>(
                                    __p, opts, __ty, __name,
                                )?
                            }
                        } else {
                            quote! {
                                ::tensor_kdl::peel_argument_after_header::<_>(
                                    __p, opts, __ty, __name,
                                )?
                            }
                        }
                    }
                    UnwrapKind::Property => {
                        let pk = key.clone();
                        if f.optional {
                            quote! {
                                ::tensor_kdl::peel_opt_property_after_header::<_>(
                                    __p, opts, __ty, __name, #pk,
                                )?
                            }
                        } else {
                            quote! {
                                ::tensor_kdl::peel_property_after_header::<_>(
                                    __p, opts, __ty, __name, #pk,
                                )?
                            }
                        }
                    }
                    UnwrapKind::None => {
                        let inner = if f.optional {
                            option_inner_type(ty).unwrap_or(ty)
                        } else {
                            ty
                        };
                        quote! {
                            {
                                use ::tensor_kdl::{NestedFill as _, NestedProbe};
                                (&&NestedProbe::<#inner>::new()).fill_nested(
                                    __p, opts, __ty, __name,
                                )?
                            }
                        }
                    }
                };
                if f.optional {
                    slot_decls.push(quote! {
                        let mut #id: #ty = ::std::option::Option::None;
                    });
                    // unwrap(argument|property) peels already yield Option; full
                    // child fill yields Inner and needs Some(...).
                    let assign = match f.unwrap {
                        UnwrapKind::Argument | UnwrapKind::Property => quote! { #id = #fill; },
                        UnwrapKind::None => {
                            quote! { #id = ::std::option::Option::Some(#fill); }
                        }
                    };
                    match_arms.push(quote! {
                        #key if #id.is_none() => {
                            #assign
                        }
                    });
                    finish_fields.push(quote! { #id });
                } else {
                    slot_decls.push(quote! {
                        let mut #id: ::std::option::Option<#ty> = ::std::option::Option::None;
                    });
                    match_arms.push(quote! {
                        #key if #id.is_none() => {
                            #id = ::std::option::Option::Some(#fill);
                        }
                    });
                    finish_fields.push(quote! {
                        #id: #id.ok_or_else(|| {
                            ::tensor_kdl::ErrorCtx::new(
                                ::tensor_kdl::ErrorCode::MissingChild,
                                0,
                            )
                            .with_message(concat!("missing child `", #key, "`"))
                        })?
                    });
                }
            }
            FieldRole::Children { name: cname } => {
                slot_decls.push(quote! {
                    let mut #id: #ty = ::std::default::Default::default();
                });
                finish_fields.push(quote! { #id });
                let elem = vec_inner_type(ty).unwrap_or(ty);
                if let Some(filter) = cname {
                    match_arms.push(quote! {
                        #filter => {
                            use ::tensor_kdl::{NestedFill as _, NestedProbe};
                            let __item: #elem = (&&NestedProbe::<#elem>::new())
                                .fill_nested(__p, opts, __ty, __name)?;
                            #id.push(__item);
                        }
                    });
                } else if unfiltered.is_some() {
                    return Err(syn::Error::new_spanned(
                        id,
                        "only one unfiltered #[kdl(children)] is supported on a document root",
                    ));
                } else {
                    unfiltered = Some((id, ty));
                    match_arms.push(quote! {
                        // unfiltered handled in default arm via same id — placeholder
                    });
                    // remove empty arm - handle only in unfiltered_arm
                    match_arms.pop();
                }
            }
            FieldRole::Flatten => {
                has_flatten = true;
                slot_decls.push(quote! {
                    let mut #id: #ty = ::std::default::Default::default();
                });
                finish_fields.push(quote! { #id });
            }
            FieldRole::Skip | FieldRole::DefaultOnly => {
                finish_fields.push(quote! {
                    #id: ::std::default::Default::default()
                });
            }
            _ => {
                return Err(syn::Error::new_spanned(
                    id,
                    "named document stream only supports child/children/flatten/skip fields",
                ));
            }
        }
    }

    if has_flatten {
        // Flatten still needs DecodePartial + Node; keep DOM stream under feature.
        return emit_named_children_read_stream_dom(fields);
    }

    let unfiltered_arm = if let Some((id, ty)) = unfiltered {
        let elem = vec_inner_type(ty).unwrap_or(ty);
        quote! {
            _ => {
                use ::tensor_kdl::{NestedFill as _, NestedProbe};
                let __item: #elem = (&&NestedProbe::<#elem>::new())
                    .fill_nested(__p, opts, __ty, __name)?;
                #id.push(__item);
            }
        }
    } else {
        quote! {
            _ => {
                ::tensor_kdl::skip_node_after_header(__p, opts, __ty, __name)?;
            }
        }
    };

    Ok(quote! {
        fn read_stream(
            out: &mut Self,
            input: &'__kdl str,
            ctx: &mut ::tensor_kdl::Context,
            opts: ::tensor_kdl::Opts,
        ) -> ::tensor_kdl::ErrorCtx {
            // P-G12: Glaze key fill — no Document / Node materialization.
            ctx.clear_error();
            ctx.reset_depth();
            ctx.apply_opts(opts);
            #(#slot_decls)*
            let owned = ::tensor_kdl::take_context_for_parser(ctx);
            let mut parser = ::tensor_kdl::Parser::with_context(input, owned);
            let visit_result = parser.visit_document_at_nodes(opts, |__p| {
                let (__ty, __name) = __p.parse_node_header()?;
                match __name.as_str() {
                    #(#match_arms)*
                    #unfiltered_arm
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
            match (|| -> ::tensor_kdl::CtxResult<Self> {
                ::std::result::Result::Ok(Self {
                    #(#finish_fields,)*
                })
            })() {
                ::std::result::Result::Ok(decoded) => {
                    *out = decoded;
                    ::tensor_kdl::ErrorCtx::ok(consumed)
                }
                ::std::result::Result::Err(e) => {
                    ctx.error = e.code;
                    ctx.custom_error_message = e.message.clone();
                    e.with_consumed(consumed)
                }
            }
        }
    })
}

/// DOM-backed stream when `#[kdl(flatten)]` is present (feature `dom`).
fn emit_named_children_read_stream_dom(
    fields: &[FieldInfo],
) -> syn::Result<proc_macro2::TokenStream> {
    // Reuse prior Node-based path, gated so non-dom builds never compile flatten roots
    // without DecodePartial.
    let mut slot_decls = Vec::new();
    let mut match_arms = Vec::new();
    let mut finish_fields = Vec::new();
    let mut unfiltered: Option<&Ident> = None;
    let mut flatten_ids: Vec<&Ident> = Vec::new();

    for f in fields {
        let id = &f.ident;
        let ty = &f.ty;
        match &f.role {
            FieldRole::Child { name: cname } => {
                let key = cname
                    .clone()
                    .or_else(|| f.rename.clone())
                    .unwrap_or_else(|| kebab(&id.to_string()));
                let decode_expr = match f.unwrap {
                    UnwrapKind::Argument => quote! { ::tensor_kdl::one_argument(&__node)? },
                    UnwrapKind::Property => {
                        let pk = key.clone();
                        quote! { ::tensor_kdl::one_property(&__node, #pk)? }
                    }
                    UnwrapKind::None => quote! { ::tensor_kdl::Decode::decode_node(&__node)? },
                };
                if f.optional {
                    slot_decls.push(quote! { let mut #id: #ty = ::std::option::Option::None; });
                    match_arms.push(quote! {
                        #key if #id.is_none() => {
                            #id = ::std::option::Option::Some(#decode_expr);
                        }
                    });
                    finish_fields.push(quote! { #id });
                } else {
                    slot_decls.push(quote! {
                        let mut #id: ::std::option::Option<#ty> = ::std::option::Option::None;
                    });
                    match_arms.push(quote! {
                        #key if #id.is_none() => {
                            #id = ::std::option::Option::Some(#decode_expr);
                        }
                    });
                    finish_fields.push(quote! {
                        #id: #id.ok_or_else(|| {
                            ::tensor_kdl::ErrorCtx::new(
                                ::tensor_kdl::ErrorCode::MissingChild,
                                0,
                            )
                            .with_message(concat!("missing child `", #key, "`"))
                        })?
                    });
                }
            }
            FieldRole::Children { name: cname } => {
                slot_decls.push(quote! {
                    let mut #id: #ty = ::std::default::Default::default();
                });
                finish_fields.push(quote! { #id });
                if let Some(filter) = cname {
                    match_arms.push(quote! {
                        #filter => {
                            #id.push(::tensor_kdl::Decode::decode_node(&__node)?);
                        }
                    });
                } else {
                    unfiltered = Some(id);
                }
            }
            FieldRole::Flatten => {
                slot_decls.push(quote! {
                    let mut #id: #ty = ::std::default::Default::default();
                });
                finish_fields.push(quote! { #id });
                flatten_ids.push(id);
            }
            FieldRole::Skip | FieldRole::DefaultOnly => {
                finish_fields.push(quote! {
                    #id: ::std::default::Default::default()
                });
            }
            _ => {}
        }
    }

    let unfiltered_arm = if let Some(id) = unfiltered {
        quote! {
            _ => { #id.push(::tensor_kdl::Decode::decode_node(&__node)?); }
        }
    } else if !flatten_ids.is_empty() {
        let flatten_try: Vec<_> = flatten_ids
            .iter()
            .map(|fid| {
                quote! {
                    if !__consumed {
                        if ::tensor_kdl::DecodePartial::insert_child(&mut #fid, &__node)? {
                            __consumed = true;
                        }
                    }
                }
            })
            .collect();
        quote! {
            _ => {
                let mut __consumed = false;
                #(#flatten_try)*
                let _ = (__consumed, &__node);
            }
        }
    } else {
        quote! { _ => { let _ = __node; } }
    };

    Ok(quote! {
        #[cfg(feature = "dom")]
        fn read_stream(
            out: &mut Self,
            input: &'__kdl str,
            ctx: &mut ::tensor_kdl::Context,
            opts: ::tensor_kdl::Opts,
        ) -> ::tensor_kdl::ErrorCtx {
            ctx.clear_error();
            ctx.reset_depth();
            ctx.apply_opts(opts);
            #(#slot_decls)*
            let owned = ::tensor_kdl::take_context_for_parser(ctx);
            let mut parser = ::tensor_kdl::Parser::with_context(input, owned);
            let visit_result = parser.visit_document(opts, |__node| {
                match __node.name.as_str() {
                    #(#match_arms)*
                    #unfiltered_arm
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
            match (|| -> ::tensor_kdl::CtxResult<Self> {
                ::std::result::Result::Ok(Self { #(#finish_fields,)* })
            })() {
                ::std::result::Result::Ok(decoded) => {
                    *out = decoded;
                    ::tensor_kdl::ErrorCtx::ok(consumed)
                }
                ::std::result::Result::Err(e) => {
                    ctx.error = e.code;
                    ctx.custom_error_message = e.message.clone();
                    e.with_consumed(consumed)
                }
            }
        }
    })
}
