//! GTK 4 + layer-shell renderer for static wallpapers.

use super::StaticWallpaperPlan;
use crate::core::FitMode;
use gtk::gdk;
use gtk::gio;
use gtk::prelude::*;
use gtk4_layer_shell::{Edge, KeyboardMode, Layer, LayerShell};
use std::collections::BTreeMap;

pub struct GtkStaticRenderer {
    application: gtk::Application,
    windows: BTreeMap<String, gtk::ApplicationWindow>,
}

impl GtkStaticRenderer {
    pub fn new(application_id: &str) -> Self {
        let application = gtk::Application::builder()
            .application_id(application_id)
            .build();
        Self {
            application,
            windows: BTreeMap::new(),
        }
    }

    pub fn application(&self) -> &gtk::Application {
        &self.application
    }

    pub fn set_static_wallpaper(&mut self, plan: &StaticWallpaperPlan) {
        let window = self
            .windows
            .entry(plan.output_name.clone())
            .or_insert_with(|| build_background_window(&self.application, &plan.output_name));
        apply_static_wallpaper(window, plan);
        window.present();
    }

    pub fn remove_output(&mut self, output_name: &str) {
        if let Some(window) = self.windows.remove(output_name) {
            window.close();
        }
    }
}

pub fn gdk_desktop_outputs() -> Vec<crate::desktop::DesktopOutput> {
    if !gtk::is_initialized_main_thread() {
        return Vec::new();
    }
    let Some(display) = gdk::Display::default() else {
        return Vec::new();
    };
    let monitors = display.monitors();
    let mut outputs = Vec::new();
    for index in 0..monitors.n_items() {
        let Some(item) = monitors.item(index) else {
            continue;
        };
        let Ok(monitor) = item.downcast::<gdk::Monitor>() else {
            continue;
        };
        let geometry = monitor.geometry();
        let name = monitor
            .connector()
            .map(|value| value.to_string())
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| format!("gdk-monitor-{index}"));
        outputs.push(crate::desktop::DesktopOutput {
            name,
            make: monitor.manufacturer().map(|value| value.to_string()),
            model: monitor.model().map(|value| value.to_string()),
            width: u32::try_from(geometry.width()).ok(),
            height: u32::try_from(geometry.height()).ok(),
            scale: monitor.scale_factor() as f32,
            focused: index == 0,
            visible: true,
            has_fullscreen: false,
            active_workspace: None,
        });
    }
    outputs
}

fn build_background_window(
    application: &gtk::Application,
    output_name: &str,
) -> gtk::ApplicationWindow {
    let window = gtk::ApplicationWindow::builder()
        .application(application)
        .decorated(false)
        .resizable(false)
        .focusable(false)
        .title(format!("Gilder Wallpaper {output_name}"))
        .build();
    window.init_layer_shell();
    window.set_namespace(Some("gilder-wallpaper"));
    window.set_layer(Layer::Background);
    window.set_keyboard_mode(KeyboardMode::None);
    window.set_exclusive_zone(-1);
    for edge in [Edge::Left, Edge::Right, Edge::Top, Edge::Bottom] {
        window.set_anchor(edge, true);
    }

    let surface = gtk::Box::new(gtk::Orientation::Vertical, 0);
    surface.set_hexpand(true);
    surface.set_vexpand(true);
    surface.set_widget_name(&css_widget_name(output_name));
    window.set_child(Some(&surface));
    window
}

fn apply_static_wallpaper(window: &gtk::ApplicationWindow, plan: &StaticWallpaperPlan) {
    let display = gtk::prelude::WidgetExt::display(window);
    let provider = gtk::CssProvider::new();
    provider.load_from_data(&static_wallpaper_css(plan));
    gtk::style_context_add_provider_for_display(
        &display,
        &provider,
        gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
    );
}

fn static_wallpaper_css(plan: &StaticWallpaperPlan) -> String {
    let file = gio::File::for_path(&plan.source);
    let uri = file.uri();
    let background = plan.background.as_deref().unwrap_or("#000000");
    let mode = css_background_mode(plan.fit);
    format!(
        "#{widget} {{
            background-color: {background};
            background-image: url(\"{uri}\");
            background-position: {position};
            background-repeat: {repeat};
            background-size: {size};
        }}",
        widget = css_widget_name(&plan.output_name),
        position = mode.position,
        repeat = mode.repeat,
        size = mode.size,
    )
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct CssBackgroundMode {
    position: &'static str,
    repeat: &'static str,
    size: &'static str,
}

fn css_background_mode(fit: FitMode) -> CssBackgroundMode {
    match fit {
        FitMode::Cover => CssBackgroundMode {
            position: "center",
            repeat: "no-repeat",
            size: "cover",
        },
        FitMode::Contain => CssBackgroundMode {
            position: "center",
            repeat: "no-repeat",
            size: "contain",
        },
        FitMode::Stretch => CssBackgroundMode {
            position: "center",
            repeat: "no-repeat",
            size: "100% 100%",
        },
        FitMode::Tile => CssBackgroundMode {
            position: "top left",
            repeat: "repeat",
            size: "auto",
        },
        FitMode::Center => CssBackgroundMode {
            position: "center",
            repeat: "no-repeat",
            size: "auto",
        },
    }
}

fn css_widget_name(output_name: &str) -> String {
    let mut name = String::from("gilder-wallpaper-");
    for character in output_name.chars() {
        if character.is_ascii_alphanumeric() || character == '-' || character == '_' {
            name.push(character);
        } else {
            name.push('-');
        }
    }
    name
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_fit_modes_to_css_background_modes() {
        assert_eq!(css_background_mode(FitMode::Cover).size, "cover");
        assert_eq!(css_background_mode(FitMode::Contain).size, "contain");
        assert_eq!(css_background_mode(FitMode::Stretch).size, "100% 100%");
        assert_eq!(css_background_mode(FitMode::Tile).repeat, "repeat");
        assert_eq!(css_background_mode(FitMode::Center).position, "center");
    }

    #[test]
    fn sanitizes_output_names_for_css_ids() {
        assert_eq!(
            css_widget_name("HDMI-A-1 workspace"),
            "gilder-wallpaper-HDMI-A-1-workspace"
        );
    }
}
