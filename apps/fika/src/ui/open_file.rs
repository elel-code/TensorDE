use std::path::{Path, PathBuf};

use fika_core::{DesktopLaunchPlan, MimeApplicationCache, MimeDatabase, path_uri_from_path};

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct OpenFileRequest {
    pub(crate) path: PathBuf,
    pub(crate) uri: String,
    pub(crate) mime_type: Option<String>,
}

impl OpenFileRequest {
    pub(crate) fn from_path(path: PathBuf, mime_type: Option<&str>) -> Self {
        Self {
            uri: launch_uri_for_path(&path),
            mime_type: mime_type.map(str::to_string),
            path,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct OpenFileLaunchRequest {
    pub(crate) path: PathBuf,
    pub(crate) app_name: String,
    pub(crate) plan: DesktopLaunchPlan,
}

pub(crate) fn launch_uri_for_path(path: &Path) -> String {
    path_uri_from_path(path)
}

pub(crate) fn default_open_file_launch_request(
    cache: &MimeApplicationCache,
    request: &OpenFileRequest,
) -> Result<OpenFileLaunchRequest, String> {
    let mime = request
        .mime_type
        .as_deref()
        .filter(|mime| !mime.trim().is_empty())
        .map(str::to_string)
        .or_else(|| {
            MimeDatabase::shared()
                .mime_for_path(&request.path, false, None)
                .map(|mime| mime.to_string())
        })
        .ok_or_else(|| {
            format!(
                "no MIME type available for {} ({})",
                request.path.display(),
                request.uri
            )
        })?;
    let application = cache
        .applications_for_mime(&mime)
        .into_iter()
        .next()
        .ok_or_else(|| format!("no desktop application found for MIME {mime}"))?;
    let app = cache
        .application(&application.id)
        .ok_or_else(|| format!("application not found: {}", application.id))?;
    let plan = app
        .launch_plan(std::slice::from_ref(&request.path))
        .ok_or_else(|| format!("{} did not produce a launch command", app.name))?;
    Ok(OpenFileLaunchRequest {
        path: request.path.clone(),
        app_name: plan.app_name.clone(),
        plan,
    })
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use fika_core::{DesktopApplication, MimeAppsList};

    use super::*;

    fn desktop_application(
        id: &str,
        name: &str,
        exec: &str,
        mime_types: &[&str],
    ) -> DesktopApplication {
        DesktopApplication {
            id: id.to_string(),
            desktop_file: PathBuf::from(format!("/apps/{id}")),
            name: name.to_string(),
            exec: exec.to_string(),
            icon: None,
            categories: Vec::new(),
            mime_types: mime_types.iter().map(|mime| mime.to_string()).collect(),
            actions: Vec::new(),
        }
    }

    #[test]
    fn launch_uri_for_path_percent_encodes_local_file_without_gio() {
        assert_eq!(
            launch_uri_for_path(Path::new("/tmp/Fika Test/value#1.txt")),
            "file:///tmp/Fika%20Test/value%231.txt"
        );
    }

    #[test]
    fn default_open_file_launch_request_falls_back_to_path_mime() {
        let cache = MimeApplicationCache::from_applications_and_mimeapps(
            vec![desktop_application(
                "viewer.desktop",
                "Viewer",
                "viewer %f",
                &[fika_core::GENERIC_BINARY_MIME],
            )],
            &[MimeAppsList {
                default_apps: HashMap::from([(
                    fika_core::GENERIC_BINARY_MIME.to_string(),
                    vec!["viewer.desktop".to_string()],
                )]),
                ..Default::default()
            }],
        );
        let request = OpenFileRequest::from_path(PathBuf::from("/tmp/payload.unknown-fika"), None);

        let launch = default_open_file_launch_request(&cache, &request).unwrap();

        assert_eq!(launch.app_name, "Viewer");
        assert_eq!(launch.plan.commands[0].program, "viewer");
        assert_eq!(
            launch.plan.commands[0].args,
            vec!["/tmp/payload.unknown-fika"]
        );
    }
}
