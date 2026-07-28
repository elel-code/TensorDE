use fika_core::file_ops;

use crate::ui::service_menu::ServiceMenuLaunchRequest;

pub(crate) async fn execute_ark_extract_and_trash(
    request: ServiceMenuLaunchRequest,
) -> Result<String, String> {
    let command = match request.plan.commands.as_slice() {
        [command] => command,
        [] => return Err("Ark extract produced no launch command".to_string()),
        _ => return Err("Ark extract produced multiple launch commands".to_string()),
    };
    let status = async_process::Command::new(&command.program)
        .args(&command.args)
        .status()
        .await
        .map_err(|err| format!("failed to start Ark: {err}"))?;
    if !status.success() {
        return Err(format!("Ark exited with {status}"));
    }

    let summary = file_ops::trash_paths_async(request.paths.clone()).await;
    if !summary.failures.is_empty() {
        return Err(format!(
            "extracted, but moving archives to Trash failed: {}",
            summary.failures.join("; ")
        ));
    }
    Ok(format!(
        "extracted and moved {} archive(s) to Trash",
        summary.successes.len()
    ))
}
