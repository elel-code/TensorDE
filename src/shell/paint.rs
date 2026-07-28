use crate::shell::file_item_view::style::FileManagerItemPalette;
use crate::shell::theme::ShellTheme;

#[derive(Clone, Copy, Debug)]
pub(crate) struct ShellPaintPalettes {
    pub(crate) shell: ShellTheme,
    pub(crate) file_manager_item: FileManagerItemPalette,
}

impl ShellPaintPalettes {
    pub(crate) fn from_shell_theme(shell: ShellTheme) -> Self {
        Self {
            shell,
            file_manager_item: FileManagerItemPalette::from_shell_theme(shell),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paint_palettes_reuse_shell_theme_adapters() {
        let shell = ShellTheme::for_dark_mode(true);
        let palettes = ShellPaintPalettes::from_shell_theme(shell);

        assert!(palettes.shell.is_dark());
        assert_eq!(
            palettes.file_manager_item,
            FileManagerItemPalette::from_shell_theme(shell)
        );
    }
}
