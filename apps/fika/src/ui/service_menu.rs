use std::path::PathBuf;

use fika_core::DesktopLaunchPlan;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ServiceMenuLaunchRequest {
    pub(crate) paths: Vec<PathBuf>,
    pub(crate) app_name: String,
    pub(crate) plan: DesktopLaunchPlan,
}
