use super::bus::{BusCallTarget, BusController, BusError, BusKind};
use std::collections::{HashMap, HashSet};
use std::env;
use std::error::Error;
use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::time::{SystemTime, UNIX_EPOCH};
use zbus::zvariant::{OwnedObjectPath, OwnedValue, Value};

mod ark;
mod results;

pub use ark::{
    ArkCompressionMode, ark_compress_launch_plan, ark_extract_and_trash_launch_plan,
    ark_extract_here_launch_plan, ark_extract_to_launch_plan,
};
pub use results::{
    NewWindowLaunchResult, OpenWithLaunchResult, ServiceMenuLaunchResult, service_menu_target_label,
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DesktopApplication {
    pub id: String,
    pub desktop_file: PathBuf,
    pub name: String,
    pub exec: String,
    pub icon: Option<String>,
    pub categories: Vec<String>,
    pub mime_types: Vec<String>,
    pub actions: Vec<DesktopAction>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DesktopServiceMenu {
    pub id: String,
    pub desktop_file: PathBuf,
    pub name: String,
    pub icon: Option<String>,
    pub mime_types: Vec<String>,
    pub service_types: Vec<String>,
    pub protocols: Vec<String>,
    pub submenu: Option<String>,
    pub priority: ServiceMenuPriority,
    pub required_url_count: Option<usize>,
    pub min_url_count: Option<usize>,
    pub max_url_count: Option<usize>,
    pub show_if_executable: Option<String>,
    pub actions: Vec<DesktopAction>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DesktopAction {
    pub id: String,
    pub name: String,
    pub exec: String,
    pub icon: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MimeApplication {
    pub id: String,
    pub desktop_file: PathBuf,
    pub name: String,
    pub exec: String,
    pub icon: Option<String>,
    pub is_default: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ServiceMenuAction {
    pub id: String,
    pub label: String,
    pub source_name: String,
    pub icon: Option<String>,
    pub submenu: Option<String>,
    pub priority: ServiceMenuPriority,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum ServiceMenuPriority {
    #[default]
    Normal,
    TopLevel,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ServiceMenuTarget {
    pub mime_type: Option<String>,
    pub is_dir: bool,
}

impl ServiceMenuTarget {
    pub fn new(mime_type: Option<&str>, is_dir: bool) -> Self {
        Self {
            mime_type: mime_type
                .map(str::trim)
                .filter(|mime| !mime.is_empty())
                .map(str::to_string),
            is_dir,
        }
    }
}

impl From<(&DesktopApplication, bool)> for MimeApplication {
    fn from((app, is_default): (&DesktopApplication, bool)) -> Self {
        Self {
            id: app.id.clone(),
            desktop_file: app.desktop_file.clone(),
            name: app.name.clone(),
            exec: app.exec.clone(),
            icon: app.icon.clone(),
            is_default,
        }
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct MimeAppsList {
    pub default_apps: HashMap<String, Vec<String>>,
    pub added_associations: HashMap<String, Vec<String>>,
    pub removed_associations: HashMap<String, Vec<String>>,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct MimeInfoCache {
    pub associations: HashMap<String, Vec<String>>,
}

#[derive(Clone, Debug, Default)]
pub struct MimeApplicationCache {
    apps: Vec<DesktopApplication>,
    service_menus: Vec<DesktopServiceMenu>,
    apps_by_id: HashMap<String, usize>,
    apps_by_filename: HashMap<String, usize>,
    apps_by_mime: HashMap<String, Vec<usize>>,
    default_by_mime: HashMap<String, String>,
    removed_by_mime: HashMap<String, HashSet<usize>>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DesktopLaunchPlan {
    pub desktop_id: String,
    pub desktop_file: PathBuf,
    pub app_name: String,
    pub commands: Vec<DesktopLaunchCommand>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DesktopLaunchCommand {
    pub program: String,
    pub args: Vec<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SystemdLaunchUnit {
    pub unit_name: String,
    pub description: String,
    pub command: DesktopLaunchCommand,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SystemdLaunchResult {
    pub units: Vec<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum LauncherError {
    EmptyLaunchPlan {
        app_name: String,
    },
    EmptyCommand {
        app_name: String,
    },
    CurrentExecutable {
        message: String,
    },
    TerminalNotFound,
    ProgramNotFound {
        program: String,
    },
    InvalidSystemdProperty {
        property: &'static str,
        message: String,
    },
    SessionBus {
        message: String,
    },
    SystemdManager {
        message: String,
    },
    StartTransientUnit {
        unit_name: String,
        message: String,
    },
}

impl fmt::Display for LauncherError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LauncherError::EmptyLaunchPlan { app_name } => {
                write!(f, "{app_name} did not produce a launch command")
            }
            LauncherError::EmptyCommand { app_name } => {
                write!(f, "{app_name} produced an empty launch command")
            }
            LauncherError::CurrentExecutable { message } => {
                write!(f, "cannot resolve current executable: {message}")
            }
            LauncherError::TerminalNotFound => {
                write!(f, "cannot find a supported terminal emulator")
            }
            LauncherError::ProgramNotFound { program } => {
                write!(f, "cannot find executable {program}")
            }
            LauncherError::InvalidSystemdProperty { property, message } => {
                write!(f, "cannot build systemd property {property}: {message}")
            }
            LauncherError::SessionBus { message } => {
                write!(f, "cannot connect to session bus: {message}")
            }
            LauncherError::SystemdManager { message } => {
                write!(f, "cannot create systemd user manager proxy: {message}")
            }
            LauncherError::StartTransientUnit { unit_name, message } => {
                write!(f, "cannot start {unit_name}: {message}")
            }
        }
    }
}

impl Error for LauncherError {}

impl DesktopApplication {
    pub fn launch_plan(&self, paths: &[PathBuf]) -> Option<DesktopLaunchPlan> {
        desktop_launch_plan_for_exec(
            self.id.clone(),
            self.desktop_file.clone(),
            self.name.clone(),
            &self.exec,
            paths,
        )
    }
}

impl DesktopServiceMenu {
    pub fn action_launch_plan(
        &self,
        action_id: &str,
        paths: &[PathBuf],
    ) -> Option<DesktopLaunchPlan> {
        let action = self.actions.iter().find(|action| action.id == action_id)?;
        if !desktop_action_supports_path_count(action, paths.len()) {
            return None;
        }
        desktop_launch_plan_for_exec(
            service_action_launch_desktop_id(&self.id, &action.id),
            self.desktop_file.clone(),
            service_action_display_name(&self.name, &action.name),
            &action.exec,
            paths,
        )
    }
}

pub fn systemd_units_for_launch_plan(
    plan: &DesktopLaunchPlan,
) -> Result<Vec<SystemdLaunchUnit>, LauncherError> {
    let nonce = systemd_launch_nonce();
    systemd_units_for_launch_plan_with_nonce(plan, nonce)
}

pub async fn launch_with_systemd_user(
    plan: DesktopLaunchPlan,
) -> Result<SystemdLaunchResult, LauncherError> {
    let units = systemd_units_for_launch_plan(&plan)?;
    let bus = BusController::shared();
    let target = systemd_manager_target().map_err(|err| LauncherError::SystemdManager {
        message: err.to_string(),
    })?;
    let manager = bus.proxy(&target).await.map_err(|err| {
        let message = err.to_string();
        match err {
            BusError::Connect { .. } => LauncherError::SessionBus { message },
            _ => LauncherError::SystemdManager { message },
        }
    })?;

    let mut started = Vec::with_capacity(units.len());
    for unit in &units {
        start_systemd_launch_unit(bus, &target, &manager, unit).await?;
        started.push(unit.unit_name.clone());
    }
    Ok(SystemdLaunchResult { units: started })
}

pub fn current_executable_launch_plan(
    desktop_id: impl Into<String>,
    app_name: impl Into<String>,
    args: Vec<String>,
) -> Result<DesktopLaunchPlan, LauncherError> {
    let executable = env::current_exe().map_err(|err| LauncherError::CurrentExecutable {
        message: err.to_string(),
    })?;
    Ok(DesktopLaunchPlan {
        desktop_id: desktop_id.into(),
        desktop_file: executable.clone(),
        app_name: app_name.into(),
        commands: vec![DesktopLaunchCommand {
            program: executable.display().to_string(),
            args,
        }],
    })
}

pub fn terminal_launch_plan_for_directory(
    directory: &Path,
) -> Result<DesktopLaunchPlan, LauncherError> {
    terminal_launch_plan_for_commands(terminal_launch_commands_for_directory(directory))
}

impl MimeApplicationCache {
    pub fn load() -> Self {
        let applications = load_desktop_applications();
        let service_menus = load_desktop_service_menus();
        let mimeinfo_caches = mimeinfo_cache_paths()
            .into_iter()
            .filter_map(|path| fs::read_to_string(path).ok())
            .map(|contents| parse_mimeinfo_cache(&contents))
            .collect::<Vec<_>>();
        let lists = mimeapps_list_paths()
            .into_iter()
            .filter_map(|path| fs::read_to_string(path).ok())
            .map(|contents| parse_mimeapps_list(&contents))
            .collect::<Vec<_>>();
        Self::from_applications_service_menus_mimeinfo_and_mimeapps(
            applications,
            service_menus,
            &mimeinfo_caches,
            &lists,
        )
    }

    pub fn shared() -> &'static Self {
        static CACHE: OnceLock<MimeApplicationCache> = OnceLock::new();
        CACHE.get_or_init(Self::load)
    }

    pub fn empty() -> Self {
        Self::default()
    }

    pub fn from_applications_and_mimeapps(
        apps: Vec<DesktopApplication>,
        lists: &[MimeAppsList],
    ) -> Self {
        Self::from_applications_service_menus_and_mimeapps(apps, Vec::new(), lists)
    }

    pub fn from_applications_mimeinfo_and_mimeapps(
        apps: Vec<DesktopApplication>,
        mimeinfo_caches: &[MimeInfoCache],
        lists: &[MimeAppsList],
    ) -> Self {
        Self::from_applications_service_menus_mimeinfo_and_mimeapps(
            apps,
            Vec::new(),
            mimeinfo_caches,
            lists,
        )
    }

    pub fn from_applications_service_menus_and_mimeapps(
        apps: Vec<DesktopApplication>,
        service_menus: Vec<DesktopServiceMenu>,
        lists: &[MimeAppsList],
    ) -> Self {
        Self::from_applications_service_menus_mimeinfo_and_mimeapps(apps, service_menus, &[], lists)
    }

    pub fn from_applications_service_menus_mimeinfo_and_mimeapps(
        mut apps: Vec<DesktopApplication>,
        mut service_menus: Vec<DesktopServiceMenu>,
        mimeinfo_caches: &[MimeInfoCache],
        lists: &[MimeAppsList],
    ) -> Self {
        apps.sort_by(desktop_application_cmp);
        service_menus.sort_by(desktop_service_menu_cmp);
        let mut cache = Self {
            service_menus,
            ..Self::default()
        };
        for app in apps {
            cache.insert_application(app);
        }
        cache.apply_mimeinfo_caches(mimeinfo_caches);
        cache.apply_mimeapps_lists(lists);
        cache
    }

    pub fn applications_for_mime(&self, mime: &str) -> Vec<MimeApplication> {
        let mime = mime.trim();
        if mime.is_empty() {
            return Vec::new();
        }
        let mut ordered = Vec::new();
        let mut seen = HashSet::new();

        self.append_applications_for_mime(mime, &mut seen, &mut ordered);
        if ordered.is_empty() {
            for parent in mime_parent_candidates(mime) {
                self.append_applications_for_mime(&parent, &mut seen, &mut ordered);
                if !ordered.is_empty() {
                    break;
                }
            }
        }

        ordered
    }

    pub fn application(&self, desktop_id: &str) -> Option<&DesktopApplication> {
        self.application_index(desktop_id)
            .and_then(|index| self.apps.get(index))
    }

    pub fn all_applications(&self) -> Vec<MimeApplication> {
        self.apps
            .iter()
            .map(|app| MimeApplication::from((app, false)))
            .collect()
    }

    pub fn service_actions_for_target(
        &self,
        mime: Option<&str>,
        is_dir: bool,
    ) -> Vec<ServiceMenuAction> {
        self.service_actions_for_targets(&[ServiceMenuTarget::new(mime, is_dir)])
    }

    pub fn service_actions_for_targets(
        &self,
        targets: &[ServiceMenuTarget],
    ) -> Vec<ServiceMenuAction> {
        if targets.is_empty() {
            return Vec::new();
        }
        let mut actions = Vec::new();
        let mut seen = HashSet::new();
        let multi_target_count = targets.len();

        for menu in &self.service_menus {
            if !service_menu_matches_targets(menu, targets) {
                continue;
            }
            for action in &menu.actions {
                if !desktop_action_supports_path_count(action, multi_target_count) {
                    continue;
                }
                let id = desktop_service_menu_action_id(&menu.id, &action.id);
                if seen.insert(id.clone()) {
                    actions.push(ServiceMenuAction {
                        id,
                        label: action.name.clone(),
                        source_name: menu.name.clone(),
                        icon: action.icon.clone().or_else(|| menu.icon.clone()),
                        submenu: menu.submenu.clone(),
                        priority: menu.priority,
                    });
                }
            }
        }

        dedup_service_actions(actions)
    }

    pub fn service_action_launch_plan(
        &self,
        action_id: &str,
        paths: &[PathBuf],
    ) -> Option<DesktopLaunchPlan> {
        for menu in &self.service_menus {
            for action in &menu.actions {
                if desktop_service_menu_action_id(&menu.id, &action.id) == action_id {
                    return menu.action_launch_plan(&action.id, paths);
                }
            }
        }
        None
    }

    fn application_indexes_for_mime_application(&self, mime: &str) -> Vec<usize> {
        let mut indexes = Vec::new();
        let mut seen = HashSet::new();
        if let Some(mime_indexes) = self.apps_by_mime.get(mime) {
            for index in mime_indexes {
                if !self.application_removed_for_mime(*index, mime) && seen.insert(*index) {
                    indexes.push(*index);
                }
            }
        }
        for (index, app) in self.apps.iter().enumerate() {
            if seen.contains(&index) {
                continue;
            }
            if self.application_removed_for_mime(index, mime) {
                continue;
            }
            if desktop_mimes_match_target(&app.mime_types, Some(mime), false) && seen.insert(index)
            {
                indexes.push(index);
            }
        }
        indexes
    }

    fn insert_application(&mut self, app: DesktopApplication) {
        let index = self.apps.len();
        self.apps_by_id.insert(app.id.clone(), index);
        if let Some(filename) = app.desktop_file.file_name().and_then(|name| name.to_str()) {
            self.apps_by_filename
                .entry(filename.to_string())
                .or_insert(index);
        }
        for mime in &app.mime_types {
            self.apps_by_mime
                .entry(mime.clone())
                .or_default()
                .push(index);
        }
        self.apps.push(app);
    }

    fn apply_mimeinfo_caches(&mut self, caches: &[MimeInfoCache]) {
        for cache in caches {
            for (mime, desktop_ids) in &cache.associations {
                for desktop_id in desktop_ids {
                    self.append_mime_application(mime, desktop_id);
                }
            }
        }
    }

    fn apply_mimeapps_lists(&mut self, lists: &[MimeAppsList]) {
        let mut removed_by_mime: HashMap<String, HashSet<String>> = HashMap::new();
        for list in lists {
            for (mime, ids) in &list.removed_associations {
                let removed = removed_by_mime.entry(mime.clone()).or_default();
                removed.extend(ids.iter().cloned());
            }
        }

        let mut added_offsets: HashMap<String, usize> = HashMap::new();
        for list in lists {
            for (mime, ids) in &list.default_apps {
                if self.default_by_mime.contains_key(mime) {
                    continue;
                }
                if let Some(id) = ids.iter().find(|id| self.application_index(id).is_some()) {
                    self.default_by_mime.insert(mime.clone(), id.clone());
                    self.prepend_mime_application(mime, id);
                }
            }
            for (mime, ids) in &list.added_associations {
                for id in ids {
                    if removed_by_mime
                        .get(mime)
                        .is_some_and(|removed| removed.contains(id))
                    {
                        continue;
                    }
                    let offset = *added_offsets.get(mime).unwrap_or(&0);
                    if self.insert_added_mime_application(mime, id, offset) {
                        *added_offsets.entry(mime.clone()).or_default() += 1;
                    }
                }
            }
        }

        for (mime, removed) in removed_by_mime {
            let removed_indexes = removed
                .iter()
                .filter_map(|id| self.application_index(id))
                .collect::<HashSet<_>>();
            if !removed_indexes.is_empty() {
                self.removed_by_mime
                    .insert(mime.clone(), removed_indexes.clone());
            }
            if let Some(indexes) = self.apps_by_mime.get_mut(&mime) {
                indexes.retain(|index| !removed_indexes.contains(index));
            }
        }
    }

    fn prepend_mime_application(&mut self, mime: &str, desktop_id: &str) {
        let Some(index) = self.application_index(desktop_id) else {
            return;
        };
        let indexes = self.apps_by_mime.entry(mime.to_string()).or_default();
        indexes.retain(|existing| *existing != index);
        indexes.insert(0, index);
    }

    fn append_mime_application(&mut self, mime: &str, desktop_id: &str) -> bool {
        let Some(index) = self.application_index(desktop_id) else {
            return false;
        };
        let indexes = self.apps_by_mime.entry(mime.to_string()).or_default();
        if indexes.contains(&index) {
            return false;
        }
        indexes.push(index);
        true
    }

    fn insert_added_mime_application(
        &mut self,
        mime: &str,
        desktop_id: &str,
        added_offset: usize,
    ) -> bool {
        let Some(index) = self.application_index(desktop_id) else {
            return false;
        };
        if self
            .default_by_mime
            .get(mime)
            .is_some_and(|default_id| self.application_matches(index, default_id))
        {
            return false;
        }
        let default_slots = self.default_by_mime.get(mime).map_or(0, |default_id| {
            usize::from(
                self.apps_by_mime
                    .get(mime)
                    .and_then(|indexes| indexes.first())
                    .is_some_and(|first| self.application_matches(*first, default_id)),
            )
        });
        let indexes = self.apps_by_mime.entry(mime.to_string()).or_default();
        indexes.retain(|existing| *existing != index);
        let insert_at = (default_slots + added_offset).min(indexes.len());
        indexes.insert(insert_at, index);
        true
    }

    fn application_index(&self, desktop_id: &str) -> Option<usize> {
        self.apps_by_id
            .get(desktop_id)
            .copied()
            .or_else(|| self.apps_by_filename.get(desktop_id).copied())
    }

    fn application_matches(&self, index: usize, desktop_id: &str) -> bool {
        self.application_index(desktop_id) == Some(index)
    }

    fn application_removed_for_mime(&self, index: usize, mime: &str) -> bool {
        self.removed_by_mime
            .get(mime)
            .is_some_and(|removed| removed.contains(&index))
    }

    fn append_applications_for_mime(
        &self,
        mime: &str,
        seen: &mut HashSet<usize>,
        ordered: &mut Vec<MimeApplication>,
    ) {
        let default_id = self.default_by_mime.get(mime);

        if let Some(default_id) = default_id
            && let Some(index) = self.application_index(default_id)
            && !self.application_removed_for_mime(index, mime)
            && seen.insert(index)
        {
            ordered.push(MimeApplication::from((&self.apps[index], true)));
        }

        for index in self.application_indexes_for_mime_application(mime) {
            if seen.insert(index) {
                let is_default = default_id.is_some_and(|id| self.application_matches(index, id));
                ordered.push(MimeApplication::from((&self.apps[index], is_default)));
            }
        }
    }
}

pub fn parse_desktop_application(
    id: impl Into<String>,
    desktop_file: impl Into<PathBuf>,
    contents: &str,
) -> Option<DesktopApplication> {
    let sections = parse_desktop_sections(contents);
    let entry = sections.get("Desktop Entry")?;
    if entry.get("Hidden").is_some_and(|value| desktop_bool(value)) {
        return None;
    }
    if entry.get("Type").map(String::as_str) != Some("Application") {
        return None;
    }
    let name = entry.get("Name")?.trim();
    let exec = entry.get("Exec")?.trim();
    if name.is_empty() || exec.is_empty() {
        return None;
    }

    let action_ids = entry
        .get("Actions")
        .map(|value| desktop_list(value))
        .unwrap_or_default();
    let actions = action_ids
        .into_iter()
        .filter_map(|action_id| {
            let section = sections.get(&format!("Desktop Action {action_id}"))?;
            let name = section.get("Name")?.trim();
            let exec = section.get("Exec")?.trim();
            (!name.is_empty() && !exec.is_empty()).then(|| DesktopAction {
                id: action_id,
                name: name.to_string(),
                exec: exec.to_string(),
                icon: section.get("Icon").filter(|icon| !icon.is_empty()).cloned(),
            })
        })
        .collect();

    Some(DesktopApplication {
        id: id.into(),
        desktop_file: desktop_file.into(),
        name: name.to_string(),
        exec: exec.to_string(),
        icon: entry.get("Icon").filter(|icon| !icon.is_empty()).cloned(),
        categories: entry
            .get("Categories")
            .map(|value| desktop_list(value))
            .unwrap_or_default(),
        mime_types: entry
            .get("MimeType")
            .map(|value| desktop_list(value))
            .unwrap_or_default(),
        actions,
    })
}

pub fn parse_desktop_service_menu(
    id: impl Into<String>,
    desktop_file: impl Into<PathBuf>,
    contents: &str,
) -> Option<DesktopServiceMenu> {
    let sections = parse_desktop_sections(contents);
    let entry = sections.get("Desktop Entry")?;
    if entry.get("Hidden").is_some_and(|value| desktop_bool(value)) {
        return None;
    }
    if entry.get("Type").map(String::as_str) != Some("Service") {
        return None;
    }

    let service_types = entry
        .get("X-KDE-ServiceTypes")
        .or_else(|| entry.get("ServiceTypes"))
        .map(|value| desktop_list(value))
        .unwrap_or_default();
    if !service_types.iter().any(|service| {
        matches!(
            service.as_str(),
            "KonqPopupMenu/Plugin" | "KFileItemAction/Plugin"
        )
    }) {
        return None;
    }

    let action_ids = entry
        .get("Actions")
        .map(|value| desktop_list(value))
        .unwrap_or_default();
    let actions = action_ids
        .into_iter()
        .filter_map(|action_id| {
            let section = sections.get(&format!("Desktop Action {action_id}"))?;
            let name = section.get("Name")?.trim();
            let exec = section.get("Exec")?.trim();
            (!name.is_empty() && !exec.is_empty()).then(|| DesktopAction {
                id: action_id,
                name: name.to_string(),
                exec: exec.to_string(),
                icon: section.get("Icon").filter(|icon| !icon.is_empty()).cloned(),
            })
        })
        .collect::<Vec<_>>();
    if actions.is_empty() {
        return None;
    }

    let id = id.into();
    Some(DesktopServiceMenu {
        id: id.clone(),
        desktop_file: desktop_file.into(),
        name: entry
            .get("Name")
            .map(|name| name.trim())
            .filter(|name| !name.is_empty())
            .unwrap_or(&id)
            .to_string(),
        icon: entry.get("Icon").filter(|icon| !icon.is_empty()).cloned(),
        mime_types: entry
            .get("MimeType")
            .map(|value| desktop_list(value))
            .unwrap_or_default(),
        service_types,
        protocols: entry
            .get("X-KDE-Protocols")
            .map(|value| desktop_list(value))
            .unwrap_or_default(),
        submenu: entry
            .get("X-KDE-Submenu")
            .map(|value| value.trim())
            .filter(|value| !value.is_empty())
            .map(str::to_string),
        priority: entry
            .get("X-KDE-Priority")
            .map(|value| service_menu_priority(value))
            .unwrap_or_default(),
        required_url_count: entry
            .get("X-KDE-RequiredNumberOfUrls")
            .and_then(|value| desktop_usize(value)),
        min_url_count: entry
            .get("X-KDE-MinNumberOfUrls")
            .and_then(|value| desktop_usize(value)),
        max_url_count: entry
            .get("X-KDE-MaxNumberOfUrls")
            .and_then(|value| desktop_usize(value)),
        show_if_executable: entry
            .get("X-KDE-ShowIfExecutable")
            .map(|value| value.trim())
            .filter(|value| !value.is_empty())
            .map(str::to_string),
        actions,
    })
}

pub fn parse_mimeapps_list(contents: &str) -> MimeAppsList {
    let mut list = MimeAppsList::default();
    let mut section = "";
    for raw_line in contents.lines() {
        let line = raw_line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if line.starts_with('[') && line.ends_with(']') {
            section = &line[1..line.len() - 1];
            continue;
        }
        let Some((mime, value)) = line.split_once('=') else {
            continue;
        };
        let apps = desktop_list(value);
        if apps.is_empty() {
            continue;
        }
        match section {
            "Default Applications" => append_mimeapps(&mut list.default_apps, mime, apps),
            "Added Associations" => append_mimeapps(&mut list.added_associations, mime, apps),
            "Removed Associations" => append_mimeapps(&mut list.removed_associations, mime, apps),
            _ => {}
        }
    }
    list
}

pub fn parse_mimeinfo_cache(contents: &str) -> MimeInfoCache {
    let mut cache = MimeInfoCache::default();
    let mut section = "";
    for raw_line in contents.lines() {
        let line = raw_line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if line.starts_with('[') && line.ends_with(']') {
            section = &line[1..line.len() - 1];
            continue;
        }
        if section != "MIME Cache" {
            continue;
        }
        let Some((mime, value)) = line.split_once('=') else {
            continue;
        };
        let apps = desktop_list(value);
        if apps.is_empty() {
            continue;
        }
        append_mimeapps(&mut cache.associations, mime, apps);
    }
    cache
}

pub fn default_mimeapps_list_path() -> PathBuf {
    if let Some(config_home) = env::var_os("XDG_CONFIG_HOME").filter(|path| !path.is_empty()) {
        PathBuf::from(config_home).join("mimeapps.list")
    } else if let Some(home) = env::var_os("HOME").filter(|path| !path.is_empty()) {
        PathBuf::from(home).join(".config/mimeapps.list")
    } else {
        PathBuf::from("mimeapps.list")
    }
}

pub fn set_default_mime_application(mime: &str, desktop_id: &str) -> Result<PathBuf, String> {
    let path = default_mimeapps_list_path();
    set_default_mime_application_at(&path, mime, desktop_id)?;
    Ok(path)
}

pub fn set_default_mime_application_at(
    path: &Path,
    mime: &str,
    desktop_id: &str,
) -> Result<(), String> {
    let contents = match fs::read_to_string(path) {
        Ok(contents) => contents,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(error) => {
            return Err(format!(
                "failed to read mimeapps list {}: {error}",
                path.display()
            ));
        }
    };
    let updated = set_default_mime_application_in_contents(&contents, mime, desktop_id)?;
    if let Some(parent) = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        fs::create_dir_all(parent).map_err(|error| {
            format!(
                "failed to create mimeapps list directory {}: {error}",
                parent.display()
            )
        })?;
    }
    fs::write(path, updated)
        .map_err(|error| format!("failed to write mimeapps list {}: {error}", path.display()))
}

pub fn set_default_mime_application_in_contents(
    contents: &str,
    mime: &str,
    desktop_id: &str,
) -> Result<String, String> {
    let mime = validate_mimeapps_key(mime)?;
    let desktop_id = validate_mimeapps_desktop_id(desktop_id)?;
    let mut lines = contents.lines().map(str::to_string).collect::<Vec<_>>();

    rewrite_mimeapps_key(
        &mut lines,
        "Default Applications",
        &mime,
        Some(mimeapps_value(std::slice::from_ref(&desktop_id))),
        true,
    );

    let mut added = mimeapps_key_value(&lines, "Added Associations", &mime)
        .map(|value| desktop_list(&value))
        .unwrap_or_default();
    added.retain(|id| id != &desktop_id);
    added.insert(0, desktop_id.clone());
    rewrite_mimeapps_key(
        &mut lines,
        "Added Associations",
        &mime,
        Some(mimeapps_value(&added)),
        true,
    );

    let mut removed = mimeapps_key_value(&lines, "Removed Associations", &mime)
        .map(|value| desktop_list(&value))
        .unwrap_or_default();
    removed.retain(|id| id != &desktop_id);
    rewrite_mimeapps_key(
        &mut lines,
        "Removed Associations",
        &mime,
        (!removed.is_empty()).then(|| mimeapps_value(&removed)),
        false,
    );

    let mut updated = lines.join("\n");
    updated.push('\n');
    Ok(updated)
}

include!("launcher/exec_systemd.rs");
include!("launcher/discovery.rs");

#[cfg(test)]
#[path = "launcher/tests.rs"]
mod tests;
