//! Document-root streaming emit (P-G5 / P-G9b mixed flatten).
//!
//! Named children fill as top-level nodes arrive without a full `Document`.

use quote::quote;
use syn::{Ident, Type};

use crate::attr::{FieldInfo, FieldRole, UnwrapKind, kebab};

/// P-G5 / P-G9b: stream top-level nodes into a children-only document root.
///
/// First-wins for single `#[kdl(child)]`; append for `#[kdl(children)]`.
/// Optional `#[kdl(flatten)]` absorbs unknown names via [`DecodePartial`]
/// (mixed document roots with extra free nodes). Without flatten, unknown
/// names are skipped (slice helpers only match known keys).
pub(crate) fn emit_named_children_read_stream(
    fields: &[FieldInfo],
) -> syn::Result<proc_macro2::TokenStream> {
    let mut slot_decls = Vec::new();
    let mut match_arms = Vec::new();
    let mut finish_fields = Vec::new();
    let mut unfiltered: Option<(&Ident, &Type)> = None;
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
                    UnwrapKind::Argument => quote! {
                        ::tensor_kdl::one_argument(&__node)?
                    },
                    UnwrapKind::Property => {
                        let pk = key.clone();
                        quote! { ::tensor_kdl::one_property(&__node, #pk)? }
                    }
                    UnwrapKind::None => quote! {
                        ::tensor_kdl::Decode::decode_node(&__node)?
                    },
                };
                if f.optional {
                    // Field type is Option<Inner>; store as Option.
                    slot_decls.push(quote! {
                        let mut #id: #ty = ::std::option::Option::None;
                    });
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
                } else if unfiltered.is_some() {
                    return Err(syn::Error::new_spanned(
                        id,
                        "only one unfiltered #[kdl(children)] is supported on a document root",
                    ));
                } else {
                    unfiltered = Some((id, ty));
                }
            }
            FieldRole::Flatten => {
                // DecodePartial targets need Default (knus *Part pattern).
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
            _ => {
                return Err(syn::Error::new_spanned(
                    id,
                    "named document stream only supports child/children/flatten/skip fields",
                ));
            }
        }
    }

    let unfiltered_arm = if let Some((id, _ty)) = unfiltered {
        quote! {
            _ => {
                #id.push(::tensor_kdl::Decode::decode_node(&__node)?);
            }
        }
    } else if !flatten_ids.is_empty() {
        // P-G9b: try each flatten partial in order (Glaze unknown-key → custom handler).
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
        quote! {
            _ => {
                // Unknown top-level name: skip (slice helpers only match known keys).
                let _ = __node;
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
            // P-G5: fill named children as top-level nodes arrive (Glaze key fill).
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
