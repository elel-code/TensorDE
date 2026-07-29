//! Field attribute parsing for `#[kdl(...)]`.

use syn::{Ident, LitStr, Type};

#[derive(Clone, Debug)]
pub(crate) enum FieldRole {
    Argument,
    Arguments,
    Property { name: Option<String> },
    Properties,
    Child { name: Option<String> },
    Children { name: Option<String> },
    NodeName,
    TypeName,
    Flatten,
    Skip,
    DefaultOnly,
}

#[derive(Clone, Copy, Debug, Default)]
pub(crate) enum UnwrapKind {
    #[default]
    None,
    Argument,
    Property,
}

#[derive(Clone, Debug)]
pub(crate) struct FieldInfo {
    pub(crate) ident: Ident,
    pub(crate) ty: Type,
    pub(crate) role: FieldRole,
    pub(crate) optional: bool,
    pub(crate) unwrap: UnwrapKind,
    /// Explicit KDL name override from `name = "..."` on any role.
    pub(crate) rename: Option<String>,
}

pub(crate) fn is_option(ty: &Type) -> bool {
    if let Type::Path(p) = ty
        && let Some(seg) = p.path.segments.last()
    {
        return seg.ident == "Option";
    }
    false
}

pub(crate) fn kebab(s: &str) -> String {
    let mut out = String::new();
    for (i, c) in s.chars().enumerate() {
        if c.is_uppercase() {
            if i > 0 {
                out.push('-');
            }
            out.extend(c.to_lowercase());
        } else if c == '_' {
            out.push('-');
        } else {
            out.push(c);
        }
    }
    out
}

pub(crate) fn parse_name_meta(
    meta: syn::meta::ParseNestedMeta<'_>,
    into: &mut Option<String>,
) -> syn::Result<()> {
    if meta.path.is_ident("name") {
        let value: LitStr = meta.value()?.parse()?;
        *into = Some(value.value());
        Ok(())
    } else {
        Err(meta.error("unsupported option (expected `name = \"...\"`)"))
    }
}

pub(crate) fn parse_field(field: &syn::Field) -> syn::Result<FieldInfo> {
    let ident = field
        .ident
        .clone()
        .ok_or_else(|| syn::Error::new_spanned(field, "tuple fields are not yet supported"))?;
    let mut role = FieldRole::DefaultOnly;
    let mut unwrap = UnwrapKind::None;
    let mut rename = None;
    let optional = is_option(&field.ty);

    for attr in &field.attrs {
        if !attr.path().is_ident("kdl") {
            continue;
        }
        attr.parse_nested_meta(|meta| {
            if meta.path.is_ident("argument") {
                role = FieldRole::Argument;
            } else if meta.path.is_ident("arguments") {
                role = FieldRole::Arguments;
            } else if meta.path.is_ident("property") {
                let mut name = None;
                if meta.input.peek(syn::token::Paren) {
                    meta.parse_nested_meta(|m| parse_name_meta(m, &mut name))?;
                }
                role = FieldRole::Property { name };
            } else if meta.path.is_ident("properties") {
                role = FieldRole::Properties;
            } else if meta.path.is_ident("child") {
                let mut name = None;
                if meta.input.peek(syn::token::Paren) {
                    meta.parse_nested_meta(|m| parse_name_meta(m, &mut name))?;
                }
                role = FieldRole::Child { name };
            } else if meta.path.is_ident("children") {
                let mut name = None;
                if meta.input.peek(syn::token::Paren) {
                    meta.parse_nested_meta(|m| parse_name_meta(m, &mut name))?;
                }
                role = FieldRole::Children { name };
            } else if meta.path.is_ident("node_name") {
                role = FieldRole::NodeName;
            } else if meta.path.is_ident("type_name") {
                role = FieldRole::TypeName;
            } else if meta.path.is_ident("flatten") {
                role = FieldRole::Flatten;
            } else if meta.path.is_ident("skip") {
                role = FieldRole::Skip;
            } else if meta.path.is_ident("unwrap") {
                meta.parse_nested_meta(|m| {
                    if m.path.is_ident("argument") {
                        unwrap = UnwrapKind::Argument;
                    } else if m.path.is_ident("property") {
                        unwrap = UnwrapKind::Property;
                    } else {
                        return Err(m.error("unwrap expects `argument` or `property`"));
                    }
                    Ok(())
                })?;
            } else if meta.path.is_ident("name") {
                let value: LitStr = meta.value()?.parse()?;
                rename = Some(value.value());
            } else if meta.path.is_ident("default") {
                // accepted marker
            } else {
                return Err(meta.error("unsupported kdl attribute"));
            }
            Ok(())
        })?;
    }

    // Apply top-level rename onto roles that carry names.
    if let Some(ref r) = rename {
        match &mut role {
            FieldRole::Property { name } if name.is_none() => *name = Some(r.clone()),
            FieldRole::Child { name } if name.is_none() => *name = Some(r.clone()),
            FieldRole::Children { name } if name.is_none() => *name = Some(r.clone()),
            _ => {}
        }
    }

    Ok(FieldInfo {
        ident,
        ty: field.ty.clone(),
        role,
        optional,
        unwrap,
        rename,
    })
}
