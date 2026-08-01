//! Emit [`DecodeFromVisit`] / [`VisitBuilder`] — Glaze `decode_linear` shape.
//!
//! Property/child names use:
//! - Glaze `unique_index` / `unique_sized` / modular perfect-hash (P-G6/P-G7)
//! - else linear string `match` (`json/read.hpp` `decode_linear`)

use quote::{format_ident, quote};
use syn::{Ident, ImplGenerics, Type, TypeGenerics, WhereClause};

use crate::attr::{FieldInfo, FieldRole, UnwrapKind, kebab};
use crate::key_dispatch::emit_key_strategy_match;

/// Structs eligible for visit-fill (no flatten / properties map).
///
/// Nested `unwrap(argument|property)` peels via live parser helpers (P-G12/P-G13),
/// not a temporary [`tensor_kdl::Node`].
pub(crate) fn visit_fill_supported(fields: &[FieldInfo]) -> bool {
    !fields.is_empty()
        && fields
            .iter()
            .all(|f| !matches!(f.role, FieldRole::Flatten | FieldRole::Properties))
}

pub(crate) fn emit_decode_from_visit(
    name: &Ident,
    impl_generics: &ImplGenerics<'_>,
    ty_generics: &TypeGenerics<'_>,
    where_clause: Option<&WhereClause>,
    fields: &[FieldInfo],
) -> proc_macro2::TokenStream {
    if !visit_fill_supported(fields) {
        return quote! {};
    }

    let builder = format_ident!("__TensorKdl_{name}_VisitBuilder");

    let mut decls = Vec::new();
    let mut inits = Vec::new();
    let mut finish = Vec::new();
    let mut arg_arms = Vec::new();
    let mut prop_keys: Vec<String> = Vec::new();
    let mut prop_bodies: Vec<proc_macro2::TokenStream> = Vec::new();
    let mut child_keys: Vec<String> = Vec::new();
    let mut child_bodies: Vec<proc_macro2::TokenStream> = Vec::new();
    let mut take_child_arms = Vec::new();
    let mut header_stmts = Vec::new();
    let mut arg_i = 0usize;
    let mut args_rest: Option<&Ident> = None;

    for f in fields {
        let id = &f.ident;
        let ty = &f.ty;
        match &f.role {
            FieldRole::Argument if f.optional => {
                let idx = arg_i;
                arg_i += 1;
                decls.push(quote! { #id: #ty });
                inits.push(quote! { #id: ::std::option::Option::None });
                // ty is Option<Inner>; DecodeScalar for Option handles values.
                arg_arms.push(quote! {
                    #idx => {
                        self.#id = ::tensor_kdl::DecodeScalar::decode_scalar(&value)?;
                    }
                });
                finish.push(quote! { #id: self.#id });
            }
            FieldRole::Argument => {
                let idx = arg_i;
                arg_i += 1;
                decls.push(quote! { #id: ::std::option::Option<#ty> });
                inits.push(quote! { #id: ::std::option::Option::None });
                arg_arms.push(quote! {
                    #idx => {
                        self.#id = ::std::option::Option::Some(
                            ::tensor_kdl::DecodeScalar::decode_scalar(&value)?,
                        );
                    }
                });
                finish.push(quote! {
                    #id: self.#id.ok_or_else(|| ::tensor_kdl::missing_argument_at(#idx))?
                });
            }
            FieldRole::Arguments => {
                args_rest = Some(id);
                decls.push(quote! { #id: #ty });
                inits.push(quote! { #id: ::std::default::Default::default() });
                finish.push(quote! { #id: self.#id });
            }
            FieldRole::Property { name: pname } => {
                let key = pname.clone().unwrap_or_else(|| kebab(&id.to_string()));
                if f.optional {
                    decls.push(quote! { #id: #ty });
                    inits.push(quote! { #id: ::std::option::Option::None });
                    prop_keys.push(key.clone());
                    prop_bodies.push(quote! {
                        self.#id = ::tensor_kdl::DecodeScalar::decode_scalar(&value)?;
                        return ::std::result::Result::Ok(true);
                    });
                    finish.push(quote! { #id: self.#id });
                } else {
                    decls.push(quote! { #id: ::std::option::Option<#ty> });
                    inits.push(quote! { #id: ::std::option::Option::None });
                    prop_keys.push(key.clone());
                    prop_bodies.push(quote! {
                        self.#id = ::std::option::Option::Some(
                            ::tensor_kdl::DecodeScalar::decode_scalar(&value)?,
                        );
                        return ::std::result::Result::Ok(true);
                    });
                    finish.push(quote! {
                        #id: self.#id.ok_or_else(|| ::tensor_kdl::missing_field(#key))?
                    });
                }
            }
            FieldRole::Child { name: cname } => {
                let key = cname
                    .clone()
                    .or_else(|| f.rename.clone())
                    .unwrap_or_else(|| kebab(&id.to_string()));
                // Scalar type after Option peel (for unwrap peels and NestedProbe).
                let nested_ty = if f.optional {
                    option_inner_type(ty).unwrap_or(ty)
                } else {
                    ty
                };
                // P-G13: unwrap peels + full nested fill via take_child (no Node).
                // Generated `on_child` is cfg(feature = "dom") only — DOM fallback
                // if take_child is not used (manual visitors / older paths).
                let take_fill = match f.unwrap {
                    UnwrapKind::Argument => {
                        if f.optional {
                            quote! {
                                ::tensor_kdl::peel_opt_argument_after_header::<_>(
                                    parser, opts, type_name, name,
                                )?
                            }
                        } else {
                            quote! {
                                ::tensor_kdl::peel_argument_after_header::<_>(
                                    parser, opts, type_name, name,
                                )?
                            }
                        }
                    }
                    UnwrapKind::Property => {
                        let pk = key.clone();
                        if f.optional {
                            quote! {
                                ::tensor_kdl::peel_opt_property_after_header::<_>(
                                    parser, opts, type_name, name, #pk,
                                )?
                            }
                        } else {
                            quote! {
                                ::tensor_kdl::peel_property_after_header::<_>(
                                    parser, opts, type_name, name, #pk,
                                )?
                            }
                        }
                    }
                    UnwrapKind::None => {
                        quote! {
                            {
                                use ::tensor_kdl::{NestedFill as _, NestedProbe};
                                (&&NestedProbe::<#nested_ty>::new()).fill_nested(
                                    parser, opts, type_name, name,
                                )?
                            }
                        }
                    }
                };
                let take_assign = match f.unwrap {
                    UnwrapKind::Argument | UnwrapKind::Property if f.optional => {
                        // peel_opt already yields Option<Inner>.
                        quote! { self.#id = #take_fill; }
                    }
                    _ => {
                        quote! { self.#id = ::std::option::Option::Some(#take_fill); }
                    }
                };
                take_child_arms.push(quote! {
                    #key => {
                        #take_assign
                        return ::std::result::Result::Ok(true);
                    }
                });
                // DOM on_child fallback bodies (compiled only under feature dom).
                let decode_child = match f.unwrap {
                    UnwrapKind::Argument => quote! {
                        ::tensor_kdl::one_argument(&child)?
                    },
                    UnwrapKind::Property => {
                        let pk = key.clone();
                        quote! { ::tensor_kdl::one_property(&child, #pk)? }
                    }
                    UnwrapKind::None => quote! {
                        ::tensor_kdl::Decode::decode_node(&child)?
                    },
                };
                if f.optional {
                    decls.push(quote! { #id: #ty });
                    inits.push(quote! { #id: ::std::option::Option::None });
                    child_keys.push(key.clone());
                    child_bodies.push(quote! {
                        self.#id = ::std::option::Option::Some(#decode_child);
                        return ::std::result::Result::Ok(true);
                    });
                    finish.push(quote! { #id: self.#id });
                } else {
                    decls.push(quote! { #id: ::std::option::Option<#ty> });
                    inits.push(quote! { #id: ::std::option::Option::None });
                    child_keys.push(key.clone());
                    child_bodies.push(quote! {
                        self.#id = ::std::option::Option::Some(#decode_child);
                        return ::std::result::Result::Ok(true);
                    });
                    finish.push(quote! {
                        #id: self.#id.ok_or_else(|| ::tensor_kdl::missing_child_named(#key))?
                    });
                }
            }
            FieldRole::Children { name: cname } => {
                decls.push(quote! { #id: #ty });
                inits.push(quote! { #id: ::std::default::Default::default() });
                finish.push(quote! { #id: self.#id });
                let elem_ty = vec_inner_type(ty).unwrap_or(ty);
                if let Some(filter) = cname {
                    take_child_arms.push(quote! {
                        #filter => {
                            use ::tensor_kdl::{NestedFill as _, NestedProbe};
                            let __nested: #elem_ty =
                                (&&NestedProbe::<#elem_ty>::new()).fill_nested(
                                    parser, opts, type_name, name,
                                )?;
                            self.#id.push(__nested);
                            return ::std::result::Result::Ok(true);
                        }
                    });
                    child_keys.push(filter.clone());
                    child_bodies.push(quote! {
                        self.#id.push(::tensor_kdl::Decode::decode_node(&child)?);
                        return ::std::result::Result::Ok(true);
                    });
                } else {
                    // Unfiltered children: take every child via nested dispatch
                    // (visit-fill when element implements DecodeFromVisit).
                    take_child_arms.push(quote! {
                        _ if true => {
                            use ::tensor_kdl::{NestedFill as _, NestedProbe};
                            let __nested: #elem_ty =
                                (&&NestedProbe::<#elem_ty>::new()).fill_nested(
                                    parser, opts, type_name, name,
                                )?;
                            self.#id.push(__nested);
                            return ::std::result::Result::Ok(true);
                        }
                    });
                    // Catch-all: handled in child_fallback, not unique-index table.
                    let catch_id = id;
                    child_keys.push(String::new()); // marker for unfiltered — strip later
                    child_bodies.push(quote! {
                        self.#catch_id.push(::tensor_kdl::Decode::decode_node(&child)?);
                        return ::std::result::Result::Ok(true);
                    });
                }
            }
            FieldRole::NodeName => {
                decls.push(quote! { #id: #ty });
                inits.push(quote! { #id: ::std::default::Default::default() });
                header_stmts.push(quote! {
                    self.#id = ::std::convert::From::from(name.as_str());
                });
                finish.push(quote! { #id: self.#id });
            }
            FieldRole::TypeName if f.optional => {
                decls.push(quote! { #id: #ty });
                inits.push(quote! { #id: ::std::option::Option::None });
                header_stmts.push(quote! {
                    self.#id = type_name.as_ref().map(|t| ::std::convert::From::from(t.as_str()));
                });
                finish.push(quote! { #id: self.#id });
            }
            FieldRole::TypeName => {
                decls.push(quote! { #id: ::std::option::Option<#ty> });
                inits.push(quote! { #id: ::std::option::Option::None });
                header_stmts.push(quote! {
                    self.#id = type_name.as_ref().map(|t| ::std::convert::From::from(t.as_str()));
                });
                finish.push(quote! {
                    #id: self.#id.ok_or_else(|| {
                        ::tensor_kdl::ErrorCtx::new(
                            ::tensor_kdl::ErrorCode::TypeMismatch,
                            0,
                        )
                        .with_message("missing type annotation")
                    })?
                });
            }
            FieldRole::Skip | FieldRole::DefaultOnly => {
                decls.push(quote! { #id: #ty });
                inits.push(quote! { #id: ::std::default::Default::default() });
                finish.push(quote! { #id: self.#id });
            }
            _ => {}
        }
    }

    let args_rest_push = if let Some(id) = args_rest {
        let start = arg_i;
        quote! {
            if __i >= #start {
                let decoded = ::tensor_kdl::DecodeScalar::decode_scalar(&value)?;
                self.#id.push(decoded);
                return ::std::result::Result::Ok(true);
            }
        }
    } else {
        quote! {}
    };

    // children all-collect fallback: detect Children { name: None }
    let mut all_children_field: Option<&Ident> = None;
    for f in fields {
        if matches!(f.role, FieldRole::Children { name: None }) {
            all_children_field = Some(&f.ident);
        }
    }
    let child_fallback = if let Some(id) = all_children_field {
        quote! {
            self.#id.push(::tensor_kdl::Decode::decode_node(&child)?);
            return ::std::result::Result::Ok(true);
        }
    } else {
        quote! { ::std::result::Result::Ok(false) }
    };

    // Named children only for unique-index / string match (drop unfiltered markers).
    let named_child_pairs: Vec<(String, proc_macro2::TokenStream)> = child_keys
        .into_iter()
        .zip(child_bodies)
        .filter(|(k, _)| !k.is_empty())
        .collect();

    let on_property_body = emit_key_dispatch(
        quote! { key.as_str() },
        &prop_keys,
        &prop_bodies,
        quote! {
            let _ = value;
            ::std::result::Result::Ok(false)
        },
    );

    let named_child_keys: Vec<&str> = named_child_pairs.iter().map(|(k, _)| k.as_str()).collect();
    let named_child_bodies: Vec<_> = named_child_pairs.iter().map(|(_, b)| b.clone()).collect();
    let on_child_body = emit_key_dispatch(
        quote! { child.name.as_str() },
        &named_child_keys
            .iter()
            .map(|s| (*s).to_owned())
            .collect::<Vec<_>>(),
        &named_child_bodies,
        child_fallback,
    );

    quote! {
        struct #builder<'__kdl> {
            __arg_i: usize,
            #(#decls,)*
            _p: ::std::marker::PhantomData<&'__kdl ()>,
        }

        impl #impl_generics ::tensor_kdl::DecodeFromVisit<'__kdl> for #name #ty_generics
        #where_clause
        {
            type Builder = #builder<'__kdl>;

            fn start_visit() -> Self::Builder {
                #builder {
                    __arg_i: 0,
                    #(#inits,)*
                    _p: ::std::marker::PhantomData,
                }
            }
        }

        impl<'__kdl> ::tensor_kdl::VisitBuilder<'__kdl> for #builder<'__kdl> {
            type Output = #name #ty_generics;

            fn on_header(
                &mut self,
                type_name: ::std::option::Option<::tensor_kdl::KdlStr<'__kdl>>,
                name: ::tensor_kdl::KdlStr<'__kdl>,
            ) -> ::tensor_kdl::CtxResult<()> {
                #(#header_stmts)*
                let _ = (type_name, name);
                ::std::result::Result::Ok(())
            }

            fn on_argument(
                &mut self,
                _type_name: ::std::option::Option<::tensor_kdl::KdlStr<'__kdl>>,
                value: ::tensor_kdl::Value<'__kdl>,
            ) -> ::tensor_kdl::CtxResult<bool> {
                let __i = self.__arg_i;
                self.__arg_i = self.__arg_i.saturating_add(1);
                #args_rest_push
                match __i {
                    #(#arg_arms)*
                    _ => {
                        let _ = value;
                    }
                }
                ::std::result::Result::Ok(true)
            }

            fn on_property(
                &mut self,
                key: ::tensor_kdl::KdlStr<'__kdl>,
                _type_name: ::std::option::Option<::tensor_kdl::KdlStr<'__kdl>>,
                value: ::tensor_kdl::Value<'__kdl>,
            ) -> ::tensor_kdl::CtxResult<bool> {
                // P-G6: unique_index byte dispatch when keys admit it; else
                // Glaze decode_linear string match.
                #on_property_body
            }

            // DOM fallback only — primary nested path is take_child_after_header
            // (P-G13 peels + NestedProbe; no Node on the Glaze primary path).
            #[cfg(feature = "dom")]
            fn on_child(
                &mut self,
                child: ::tensor_kdl::Node<'__kdl>,
            ) -> ::tensor_kdl::CtxResult<bool> {
                #on_child_body
            }

            // P-G3d/P-G13: nested from::op — peel unwrap or NestedProbe fill,
            // never requiring an intermediate Node on the visit path.
            fn take_child_after_header(
                &mut self,
                parser: &mut ::tensor_kdl::Parser<'__kdl>,
                opts: ::tensor_kdl::Opts,
                type_name: ::std::option::Option<::tensor_kdl::KdlStr<'__kdl>>,
                name: ::tensor_kdl::KdlStr<'__kdl>,
            ) -> ::tensor_kdl::CtxResult<bool> {
                match name.as_str() {
                    #(#take_child_arms)*
                    _ => {
                        let _ = (parser, opts, type_name, name);
                        ::std::result::Result::Ok(false)
                    }
                }
            }

            fn finish(self) -> ::tensor_kdl::CtxResult<Self::Output> {
                ::std::result::Result::Ok(#name {
                    #(#finish,)*
                })
            }
        }
    }
}

/// Build property/child name match via P-G6/P-G7 strategy selection.
fn emit_key_dispatch(
    key_expr: proc_macro2::TokenStream,
    keys: &[String],
    bodies: &[proc_macro2::TokenStream],
    fallback: proc_macro2::TokenStream,
) -> proc_macro2::TokenStream {
    emit_key_strategy_match(key_expr, keys, bodies, fallback)
}

/// Peel `Option<T>` → `T` for nested visit-fill of optional child fields.
pub(crate) fn option_inner_type(ty: &Type) -> Option<&Type> {
    let syn::Type::Path(tp) = ty else {
        return None;
    };
    let seg = tp.path.segments.last()?;
    if seg.ident != "Option" {
        return None;
    }
    let syn::PathArguments::AngleBracketed(args) = &seg.arguments else {
        return None;
    };
    args.args.iter().find_map(|a| match a {
        syn::GenericArgument::Type(t) => Some(t),
        _ => None,
    })
}

/// Peel `Vec<T>` → `T` for nested visit-fill of children collectors.
pub(crate) fn vec_inner_type(ty: &Type) -> Option<&Type> {
    let syn::Type::Path(tp) = ty else {
        return None;
    };
    let seg = tp.path.segments.last()?;
    if seg.ident != "Vec" {
        return None;
    }
    let syn::PathArguments::AngleBracketed(args) = &seg.arguments else {
        return None;
    };
    args.args.iter().find_map(|a| match a {
        syn::GenericArgument::Type(t) => Some(t),
        _ => None,
    })
}
