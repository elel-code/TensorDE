//! Emit [`DecodeFromVisit`] / [`VisitBuilder`] — Glaze `decode_linear` shape.
//!
//! Property/child names use linear `match` on a static set of keys
//! (`json/read.hpp` `decode_linear`).

use quote::{format_ident, quote};
use syn::{Ident, ImplGenerics, TypeGenerics, WhereClause};

use crate::attr::{FieldInfo, FieldRole, UnwrapKind, kebab};
use syn::Type;

/// Structs eligible for visit-fill (no flatten / properties map / unwrap(property)).
pub(crate) fn visit_fill_supported(fields: &[FieldInfo]) -> bool {
    !fields.is_empty()
        && fields.iter().all(|f| {
            !matches!(f.role, FieldRole::Flatten | FieldRole::Properties)
                && !matches!(f.unwrap, UnwrapKind::Property)
        })
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
    let mut prop_arms = Vec::new();
    let mut child_arms = Vec::new();
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
                    prop_arms.push(quote! {
                        #key => {
                            self.#id = ::tensor_kdl::DecodeScalar::decode_scalar(&value)?;
                            return ::std::result::Result::Ok(true);
                        }
                    });
                    finish.push(quote! { #id: self.#id });
                } else {
                    decls.push(quote! { #id: ::std::option::Option<#ty> });
                    inits.push(quote! { #id: ::std::option::Option::None });
                    prop_arms.push(quote! {
                        #key => {
                            self.#id = ::std::option::Option::Some(
                                ::tensor_kdl::DecodeScalar::decode_scalar(&value)?,
                            );
                            return ::std::result::Result::Ok(true);
                        }
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
                let decode_child = match f.unwrap {
                    UnwrapKind::Argument => quote! {
                        ::tensor_kdl::one_argument(&child)?
                    },
                    _ => quote! {
                        ::tensor_kdl::Decode::decode_node(&child)?
                    },
                };
                // P-G3d: nested fill for full child nodes (Glaze nested from::op).
                // unwrap(*) peels scalars from a DOM node — keep on_child only.
                // Autoref specialization at this monomorphized site: DecodeFromVisit
                // when available, else Decode via temporary Node (NestedViaDom).
                let nested_ty = if f.optional {
                    option_inner_type(ty).unwrap_or(ty)
                } else {
                    ty
                };
                if matches!(f.unwrap, UnwrapKind::None) {
                    take_child_arms.push(quote! {
                        #key => {
                            use ::tensor_kdl::{NestedFill as _, NestedProbe};
                            let __nested: #nested_ty =
                                (&&NestedProbe::<#nested_ty>::new()).fill_nested(
                                    parser, opts, type_name, name,
                                )?;
                            self.#id = ::std::option::Option::Some(__nested);
                            return ::std::result::Result::Ok(true);
                        }
                    });
                }
                if f.optional {
                    decls.push(quote! { #id: #ty });
                    inits.push(quote! { #id: ::std::option::Option::None });
                    child_arms.push(quote! {
                        #key => {
                            self.#id = ::std::option::Option::Some(#decode_child);
                            return ::std::result::Result::Ok(true);
                        }
                    });
                    finish.push(quote! { #id: self.#id });
                } else {
                    decls.push(quote! { #id: ::std::option::Option<#ty> });
                    inits.push(quote! { #id: ::std::option::Option::None });
                    child_arms.push(quote! {
                        #key => {
                            self.#id = ::std::option::Option::Some(#decode_child);
                            return ::std::result::Result::Ok(true);
                        }
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
                    child_arms.push(quote! {
                        #filter => {
                            self.#id.push(::tensor_kdl::Decode::decode_node(&child)?);
                            return ::std::result::Result::Ok(true);
                        }
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
                    // Catch-all collector — must be last; use a flag in on_child default.
                    child_arms.push(quote! {
                        // collected in fallback
                    });
                    // Store id for fallback
                    let catch_id = id;
                    child_arms.push(quote! {
                        __tensor_kdl_collect_all if true => {
                            self.#catch_id.push(::tensor_kdl::Decode::decode_node(&child)?);
                            return ::std::result::Result::Ok(true);
                        }
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

    // Fix Children { None } arms — remove empty placeholders from botched emit
    let child_arms: Vec<_> = child_arms.into_iter().filter(|t| !t.is_empty()).collect();

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

    // Filter child_arms that are the broken collect_all
    let child_arms: Vec<_> = child_arms
        .into_iter()
        .filter(|tt| {
            let s = tt.to_string();
            !s.contains("__tensor_kdl_collect_all") && s.contains("=>")
        })
        .collect();

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
                // Glaze decode_linear: equality scan over known keys.
                match key.as_str() {
                    #(#prop_arms)*
                    _ => {
                        let _ = value;
                        ::std::result::Result::Ok(false)
                    }
                }
            }

            fn on_child(
                &mut self,
                child: ::tensor_kdl::Node<'__kdl>,
            ) -> ::tensor_kdl::CtxResult<bool> {
                match child.name.as_str() {
                    #(#child_arms)*
                    _ => { #child_fallback }
                }
            }

            // P-G3d: nested from::op — fill child without intermediate Node when
            // the child type implements DecodeFromVisit (Glaze json/read.hpp).
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

/// Peel `Option<T>` → `T` for nested visit-fill of optional child fields.
fn option_inner_type(ty: &Type) -> Option<&Type> {
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
fn vec_inner_type(ty: &Type) -> Option<&Type> {
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
