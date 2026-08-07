mod shell;
mod typed;

use std::path::Path;

use crate::ProductKind;

pub use shell::ShellLayoutPreview;
pub use typed::{
    FilesConfigPreview, GreeterConfigPreview, IdleConfigPreview, LauncherConfigPreview,
    PowerPolicyPreview, XdpColorScheme, XdpConfigPreview, XdpContrast,
};

#[derive(Clone, Debug, PartialEq)]
pub enum ConfigPreview {
    Kdl { top_level_nodes: usize },
    Shell { layout: ShellLayoutPreview },
    Launcher(LauncherConfigPreview),
    Greeter(GreeterConfigPreview),
    Files(FilesConfigPreview),
    Idle(IdleConfigPreview),
    Xdp(XdpConfigPreview),
    Unsupported,
}

#[derive(Clone, Debug, Eq, PartialEq, thiserror::Error)]
#[error("{message}")]
pub struct ConfigDiagnostic {
    pub message: String,
}

pub fn validate_product_config(
    product: ProductKind,
    path: &Path,
    source: &str,
) -> Result<ConfigPreview, ConfigDiagnostic> {
    let mut parser = tensor_kdl::Parser::new(source);
    let opts = tensor_kdl::Opts::new();
    let mut top_level_nodes = 0usize;
    parser
        .visit_document_at_nodes(opts, |parser| {
            let mut visitor = tensor_kdl::CountingVisitor::default();
            parser.visit_node(opts, &mut visitor)?;
            top_level_nodes += 1;
            Ok(())
        })
        .map_err(|error| ConfigDiagnostic {
            message: tensor_kdl::format_error_named(&error, source, &path.display().to_string()),
        })?;
    if product == ProductKind::Shell {
        return shell::validate(path, source);
    }
    if let Some(preview) = typed::validate(product, path, source)? {
        return Ok(preview);
    }
    Ok(ConfigPreview::Kdl { top_level_nodes })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generic_products_still_receive_named_kdl_diagnostics() {
        let error =
            validate_product_config(ProductKind::Idle, Path::new("idle.kdl"), "ac {").unwrap_err();
        assert!(error.message.contains("idle.kdl"));
    }

    #[test]
    fn typed_products_validate_runtime_bounds() {
        let preview = validate_product_config(
            ProductKind::Launcher,
            Path::new("launcher.kdl"),
            "max-results 24\napplication-directory \"/usr/share/applications\"",
        )
        .unwrap();
        assert!(matches!(preview, ConfigPreview::Launcher(_)));
        assert!(
            validate_product_config(
                ProductKind::Xdp,
                Path::new("xdp.kdl"),
                "appearance color-scheme=\"sepia\""
            )
            .is_err()
        );
        assert!(
            validate_product_config(
                ProductKind::Greeter,
                Path::new("greeter.kdl"),
                "session \"land\" { label \"Land\" command }"
            )
            .is_err()
        );
    }
}
