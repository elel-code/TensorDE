use async_process::{Child, ChildStderr, ChildStdout, Command, Stdio};
use futures_lite::StreamExt;
use futures_lite::future;
use futures_lite::io::AsyncReadExt;
use std::collections::HashMap;
use std::env;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::process::ExitStatus;
use std::sync::OnceLock;
use std::time::Duration;
use zbus::fdo;
use zbus::message::Type;
use zbus::zvariant::{OwnedObjectPath, OwnedValue, Value};
use zbus::{Connection, MatchRule, MessageStream};

const BUS_NAME: &str = "org.freedesktop.impl.portal.desktop.tensor_files";
const OBJECT_PATH: &str = "/org/freedesktop/portal/desktop";
type PortalChoice = (String, String, Vec<(String, String)>, String);
type PortalFilter = (String, Vec<(u32, String)>);

#[derive(Debug, Default)]
struct ChooserFilterMap {
    portal_filters: Vec<PortalFilter>,
    chooser_specs: Vec<String>,
    portal_indices: Vec<usize>,
    initial_chooser_index: Option<usize>,
    mime_mapped_filters: usize,
    hidden_filters: usize,
}

#[derive(Debug, Default)]
struct ChooserResult {
    paths: Vec<PathBuf>,
    filter_index: Option<usize>,
    choices: Vec<(String, String)>,
}

#[derive(Debug)]
struct ChooserProcessOutput {
    status: ExitStatus,
    stdout: Vec<u8>,
    stderr: Vec<u8>,
}

struct ChooserProcess {
    child: Child,
    stdout: ChildStdout,
    stderr: ChildStderr,
}

enum ChooserWaitOutcome {
    Exited(ExitStatus),
    TerminatedByClose {
        status: ExitStatus,
        close: Option<Result<zbus::Message, zbus::Error>>,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ParentWindowStatus {
    Accepted,
    Empty,
    Malformed,
    EmptyHandle,
    UnsupportedScheme,
}

impl ParentWindowStatus {
    fn as_str(self) -> &'static str {
        match self {
            Self::Accepted => "accepted",
            Self::Empty => "empty",
            Self::Malformed => "malformed",
            Self::EmptyHandle => "empty-handle",
            Self::UnsupportedScheme => "unsupported-scheme",
        }
    }
}

fn parent_window_binding(status: ParentWindowStatus) -> &'static str {
    match status {
        ParentWindowStatus::Accepted => "metadata-only",
        ParentWindowStatus::Empty => "none",
        ParentWindowStatus::Malformed
        | ParentWindowStatus::EmptyHandle
        | ParentWindowStatus::UnsupportedScheme => "rejected",
    }
}

fn parent_window_binding_reason(status: ParentWindowStatus) -> &'static str {
    match status {
        ParentWindowStatus::Accepted => "parent-token-binding-unavailable",
        ParentWindowStatus::Empty => "no-parent-window",
        ParentWindowStatus::Malformed => "malformed-parent-window",
        ParentWindowStatus::EmptyHandle => "empty-parent-window-handle",
        ParentWindowStatus::UnsupportedScheme => "unsupported-parent-window-scheme",
    }
}

#[derive(Debug, Eq, PartialEq)]
struct ParentWindowDecision {
    handle: Option<String>,
    status: ParentWindowStatus,
}

#[derive(Clone, Copy, Debug)]
struct PortalRequestDebug<'a> {
    method: &'a str,
    handle: &'a str,
    start_dir: Option<&'a Path>,
    directory: bool,
    multiple: bool,
    save_kind: &'a str,
    save_files: usize,
    portal_filters: usize,
    chooser_filters: usize,
    mime_mapped_filters: usize,
    hidden_filters: usize,
    initial_filter_index: Option<usize>,
    portal_choices: usize,
    parent_status: ParentWindowStatus,
    parent_forwarded: bool,
}

#[derive(Debug, Default)]
struct ChooserArgs {
    start_dir: Option<PathBuf>,
    directory: bool,
    multiple: bool,
    title: Option<String>,
    save_name: Option<String>,
    save_files: Option<Vec<String>>,
    accept_label: Option<String>,
    filters: Vec<String>,
    filter_index: Option<usize>,
    return_filter: bool,
    choices: Vec<String>,
    return_choices: bool,
    parent_window: Option<String>,
}

enum ChooserRun {
    Selected(ChooserResult),
    Cancelled(ChooserCancelReason),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ChooserCancelReason {
    UserCancelled,
    EmptyOutput,
    RequestClose,
    CloseStreamEnded,
}

impl ChooserCancelReason {
    fn as_str(self) -> &'static str {
        match self {
            Self::UserCancelled => "user-cancelled",
            Self::EmptyOutput => "empty-output",
            Self::RequestClose => "request-close",
            Self::CloseStreamEnded => "close-stream-ended",
        }
    }
}

fn main() {
    if env::args()
        .skip(1)
        .any(|arg| matches!(arg.as_str(), "-h" | "--help"))
    {
        print_help();
        return;
    }

    if let Err(err) = async_io::block_on(run()) {
        eprintln!("{err}");
        std::process::exit(1);
    }
}

async fn run() -> Result<(), String> {
    let _connection = zbus::connection::Builder::session()
        .map_err(|err| format!("cannot connect to session D-Bus: {err}"))?
        .name(BUS_NAME)
        .map_err(|err| format!("cannot request portal backend bus name: {err}"))?
        .serve_at(OBJECT_PATH, FileChooser)
        .map_err(|err| format!("cannot register FileChooser portal object: {err}"))?
        .build()
        .await
        .map_err(|err| format!("cannot build portal backend D-Bus service: {err}"))?;

    std::future::pending::<()>().await;
    Ok(())
}

struct FileChooser;

#[zbus::interface(name = "org.freedesktop.impl.portal.FileChooser")]
impl FileChooser {
    #[zbus(out_args("response", "results"))]
    async fn open_file(
        &self,
        handle: OwnedObjectPath,
        _app_id: String,
        parent_window: String,
        title: String,
        options: HashMap<String, OwnedValue>,
        #[zbus(connection)] connection: &Connection,
    ) -> fdo::Result<(u32, HashMap<String, OwnedValue>)> {
        let directory = option_bool(&options, "directory").unwrap_or(false);
        let multiple = option_bool(&options, "multiple").unwrap_or(false);
        let filters = portal_filters(&options);
        let filter_map = chooser_filter_map(&options, filters);
        let choices = portal_choices(&options);
        let choice_specs = chooser_choice_specs(&choices);
        let return_choices = !choice_specs.is_empty();
        let parent_window = portal_parent_window(parent_window);
        let start_dir = current_folder(&options);
        portal_debug_log_request(PortalRequestDebug {
            method: "OpenFile",
            handle: handle.as_str(),
            start_dir: start_dir.as_deref(),
            directory,
            multiple,
            save_kind: "none",
            save_files: 0,
            portal_filters: filter_map.portal_filters.len(),
            chooser_filters: filter_map.chooser_specs.len(),
            mime_mapped_filters: filter_map.mime_mapped_filters,
            hidden_filters: filter_map.hidden_filters,
            initial_filter_index: filter_map.initial_chooser_index,
            portal_choices: choices.len(),
            parent_status: parent_window.status,
            parent_forwarded: parent_window.handle.is_some(),
        });
        let args = chooser_args(ChooserArgs {
            start_dir,
            directory,
            multiple,
            title: portal_title(title),
            accept_label: option_string(&options, "accept_label"),
            filters: filter_map.chooser_specs.clone(),
            filter_index: filter_map.initial_chooser_index,
            return_filter: !filter_map.chooser_specs.is_empty(),
            choices: choice_specs,
            return_choices,
            parent_window: parent_window.handle,
            ..ChooserArgs::default()
        });
        match run_chooser_for_request(connection, &handle, args).await {
            Ok(ChooserRun::Selected(result)) => {
                Ok((0, results_for_paths(result, &options, &filter_map)))
            }
            Ok(ChooserRun::Cancelled(_)) => Ok(cancelled()),
            Err(err) => Err(fdo::Error::Failed(err)),
        }
    }

    #[zbus(out_args("response", "results"))]
    async fn save_file(
        &self,
        handle: OwnedObjectPath,
        _app_id: String,
        parent_window: String,
        title: String,
        options: HashMap<String, OwnedValue>,
        #[zbus(connection)] connection: &Connection,
    ) -> fdo::Result<(u32, HashMap<String, OwnedValue>)> {
        let (start_dir, name) = save_file_start_and_name(&options);
        let Some(name) = name else {
            return Ok(cancelled());
        };
        let filters = portal_filters(&options);
        let filter_map = chooser_filter_map(&options, filters);
        let choices = portal_choices(&options);
        let choice_specs = chooser_choice_specs(&choices);
        let return_choices = !choice_specs.is_empty();
        let parent_window = portal_parent_window(parent_window);
        portal_debug_log_request(PortalRequestDebug {
            method: "SaveFile",
            handle: handle.as_str(),
            start_dir: start_dir.as_deref(),
            directory: false,
            multiple: false,
            save_kind: "file",
            save_files: 0,
            portal_filters: filter_map.portal_filters.len(),
            chooser_filters: filter_map.chooser_specs.len(),
            mime_mapped_filters: filter_map.mime_mapped_filters,
            hidden_filters: filter_map.hidden_filters,
            initial_filter_index: filter_map.initial_chooser_index,
            portal_choices: choices.len(),
            parent_status: parent_window.status,
            parent_forwarded: parent_window.handle.is_some(),
        });
        let args = chooser_args(ChooserArgs {
            start_dir,
            title: portal_title(title),
            save_name: Some(name),
            accept_label: option_string(&options, "accept_label"),
            filters: filter_map.chooser_specs.clone(),
            filter_index: filter_map.initial_chooser_index,
            return_filter: !filter_map.chooser_specs.is_empty(),
            choices: choice_specs,
            return_choices,
            parent_window: parent_window.handle,
            ..ChooserArgs::default()
        });
        match run_chooser_for_request(connection, &handle, args).await {
            Ok(ChooserRun::Selected(result)) => {
                Ok((0, results_for_paths(result, &options, &filter_map)))
            }
            Ok(ChooserRun::Cancelled(_)) => Ok(cancelled()),
            Err(err) => Err(fdo::Error::Failed(err)),
        }
    }

    #[zbus(out_args("response", "results"))]
    async fn save_files(
        &self,
        handle: OwnedObjectPath,
        _app_id: String,
        parent_window: String,
        title: String,
        options: HashMap<String, OwnedValue>,
        #[zbus(connection)] connection: &Connection,
    ) -> fdo::Result<(u32, HashMap<String, OwnedValue>)> {
        let files = save_files(&options);
        if files.is_empty() {
            return Ok(cancelled());
        }
        let filters = portal_filters(&options);
        let filter_map = chooser_filter_map(&options, filters);
        let choices = portal_choices(&options);
        let choice_specs = chooser_choice_specs(&choices);
        let return_choices = !choice_specs.is_empty();
        let parent_window = portal_parent_window(parent_window);
        let start_dir = current_folder(&options);
        portal_debug_log_request(PortalRequestDebug {
            method: "SaveFiles",
            handle: handle.as_str(),
            start_dir: start_dir.as_deref(),
            directory: true,
            multiple: false,
            save_kind: "files",
            save_files: files.len(),
            portal_filters: filter_map.portal_filters.len(),
            chooser_filters: filter_map.chooser_specs.len(),
            mime_mapped_filters: filter_map.mime_mapped_filters,
            hidden_filters: filter_map.hidden_filters,
            initial_filter_index: filter_map.initial_chooser_index,
            portal_choices: choices.len(),
            parent_status: parent_window.status,
            parent_forwarded: parent_window.handle.is_some(),
        });
        let args = chooser_args(ChooserArgs {
            start_dir,
            directory: true,
            title: portal_title(title),
            save_files: Some(files),
            accept_label: option_string(&options, "accept_label"),
            filters: filter_map.chooser_specs.clone(),
            filter_index: filter_map.initial_chooser_index,
            return_filter: !filter_map.chooser_specs.is_empty(),
            choices: choice_specs,
            return_choices,
            parent_window: parent_window.handle,
            ..ChooserArgs::default()
        });
        match run_chooser_for_request(connection, &handle, args).await {
            Ok(ChooserRun::Selected(result)) => {
                Ok((0, results_for_paths(result, &options, &filter_map)))
            }
            Ok(ChooserRun::Cancelled(_)) => Ok(cancelled()),
            Err(err) => Err(fdo::Error::Failed(err)),
        }
    }
}

fn chooser_args(request: ChooserArgs) -> Vec<String> {
    let mut args = vec!["--chooser".to_string()];
    if request.directory {
        args.push("--chooser-directory".to_string());
    }
    if request.multiple {
        args.push("--chooser-multiple".to_string());
    }
    if let Some(save_name) = request.save_name {
        args.push("--chooser-save".to_string());
        args.push(save_name);
    }
    if let Some(save_files) = request.save_files {
        args.push("--chooser-save-files".to_string());
        args.push(save_files.join("\n"));
    }
    if let Some(title) = request.title {
        args.push("--chooser-title".to_string());
        args.push(title);
    }
    if let Some(accept_label) = request.accept_label {
        args.push("--chooser-accept-label".to_string());
        args.push(accept_label);
    }
    if !request.filters.is_empty() {
        args.push("--chooser-filters".to_string());
        args.push(request.filters.join("\n"));
    }
    if let Some(filter_index) = request.filter_index {
        args.push("--chooser-filter-index".to_string());
        args.push(filter_index.to_string());
    }
    if request.return_filter {
        args.push("--chooser-return-filter".to_string());
    }
    if !request.choices.is_empty() {
        args.push("--chooser-choices".to_string());
        args.push(request.choices.join("\n"));
    }
    if request.return_choices {
        args.push("--chooser-return-choices".to_string());
    }
    if let Some(parent_window) = request.parent_window {
        args.push("--chooser-parent-window".to_string());
        args.push(parent_window);
    }
    if let Some(start_dir) = request.start_dir {
        args.push(start_dir.display().to_string());
    }
    args
}

fn chooser_run_from_output(output: ChooserProcessOutput) -> Result<ChooserRun, String> {
    if !output.status.success() {
        if output.status.code() == Some(tensor_files_core::CHOOSER_CANCEL_EXIT_CODE) {
            return Ok(ChooserRun::Cancelled(ChooserCancelReason::UserCancelled));
        }
        return Err(chooser_failure_message(
            output.status.code(),
            String::from_utf8_lossy(&output.stderr).trim(),
        ));
    }

    let stdout = String::from_utf8(output.stdout)
        .map_err(|err| format!("chooser stdout was not UTF-8: {err}"))?;
    let result = parse_chooser_output(&stdout);
    if result.paths.is_empty() {
        Ok(ChooserRun::Cancelled(ChooserCancelReason::EmptyOutput))
    } else {
        Ok(ChooserRun::Selected(result))
    }
}

async fn run_chooser_for_request(
    connection: &Connection,
    handle: &OwnedObjectPath,
    args: Vec<String>,
) -> Result<ChooserRun, String> {
    let mut close_stream = request_close_stream(connection, handle).await?;
    let mut process = ChooserProcess::spawn(chooser_command(gui_executable()?, args))?;
    let wait = process.wait_for_exit_or_close(&mut close_stream).await?;
    let outcome = match wait {
        ChooserWaitOutcome::Exited(status) => {
            chooser_run_from_output(process.collect(status).await?)
        }
        ChooserWaitOutcome::TerminatedByClose { status, close } => {
            let _output = process.collect(status).await?;
            match close {
                Some(Ok(_)) => Ok(ChooserRun::Cancelled(ChooserCancelReason::RequestClose)),
                Some(Err(err)) => Err(format!("portal request Close signal failed: {err}")),
                None => Ok(ChooserRun::Cancelled(ChooserCancelReason::CloseStreamEnded)),
            }
        }
    };
    portal_debug_log_chooser_lifecycle(handle.as_str(), &outcome);
    outcome
}

impl ChooserProcess {
    fn spawn(mut command: Command) -> Result<Self, String> {
        let mut child = command
            .spawn()
            .map_err(|err| format!("cannot launch tensor-files chooser: {err}"))?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| "cannot capture chooser stdout".to_string())?;
        let stderr = child
            .stderr
            .take()
            .ok_or_else(|| "cannot capture chooser stderr".to_string())?;
        Ok(Self {
            child,
            stdout,
            stderr,
        })
    }

    async fn wait_for_exit_or_close(
        &mut self,
        close_stream: &mut MessageStream,
    ) -> Result<ChooserWaitOutcome, String> {
        loop {
            if let Some(status) = self
                .child
                .try_status()
                .map_err(|err| format!("cannot poll tensor-files chooser: {err}"))?
            {
                return Ok(ChooserWaitOutcome::Exited(status));
            }
            match future::poll_once(close_stream.next()).await {
                Some(close) => {
                    let status = kill_chooser_child(&mut self.child).await?;
                    return Ok(ChooserWaitOutcome::TerminatedByClose { status, close });
                }
                None => {
                    async_io::Timer::after(Duration::from_millis(10)).await;
                }
            }
        }
    }

    async fn collect(self, status: ExitStatus) -> Result<ChooserProcessOutput, String> {
        let Self {
            child: _,
            mut stdout,
            mut stderr,
        } = self;
        let (stdout, stderr) = future::try_zip(
            async {
                let mut output = Vec::new();
                stdout
                    .read_to_end(&mut output)
                    .await
                    .map_err(|err| format!("cannot read chooser stdout: {err}"))?;
                Ok::<_, String>(output)
            },
            async {
                let mut output = Vec::new();
                stderr
                    .read_to_end(&mut output)
                    .await
                    .map_err(|err| format!("cannot read chooser stderr: {err}"))?;
                Ok::<_, String>(output)
            },
        )
        .await?;
        Ok(ChooserProcessOutput {
            status,
            stdout,
            stderr,
        })
    }

    #[cfg(test)]
    async fn terminate_for_test(mut self) -> Result<ChooserProcessOutput, String> {
        let status = kill_chooser_child(&mut self.child).await?;
        self.collect(status).await
    }
}

async fn kill_chooser_child(child: &mut Child) -> Result<ExitStatus, String> {
    match child.kill() {
        Ok(()) => {}
        Err(err) if err.kind() == ErrorKind::InvalidInput => {}
        Err(err) => {
            return Err(format!(
                "cannot terminate tensor-files chooser after request Close: {err}"
            ));
        }
    }
    child
        .status()
        .await
        .map_err(|err| format!("cannot wait for terminated tensor-files chooser: {err}"))
}

async fn request_close_stream(
    connection: &Connection,
    handle: &OwnedObjectPath,
) -> Result<MessageStream, String> {
    let rule = request_close_match_rule(handle.as_str())?;
    MessageStream::for_match_rule(rule, connection, Some(1))
        .await
        .map_err(|err| format!("cannot subscribe to portal request Close signal: {err}"))
}

fn request_close_match_rule(handle: &str) -> Result<zbus::OwnedMatchRule, String> {
    Ok(MatchRule::builder()
        .msg_type(Type::Signal)
        .interface("org.freedesktop.impl.portal.Request")
        .map_err(|err| format!("cannot build portal request Close interface match: {err}"))?
        .member("Close")
        .map_err(|err| format!("cannot build portal request Close member match: {err}"))?
        .path(handle)
        .map_err(|err| format!("cannot build portal request Close path match: {err}"))?
        .build()
        .into())
}

fn chooser_command(program: PathBuf, args: Vec<String>) -> Command {
    let mut command = Command::new(program);
    command.args(args);
    command.stdin(Stdio::null());
    command.stderr(Stdio::piped());
    command.stdout(Stdio::piped());
    command.kill_on_drop(true);
    command
}

#[cfg(test)]
fn chooser_command_kills_on_drop(command: &Command) -> bool {
    // async-process does not expose get_kill_on_drop; keep the production path
    // configured above and assert the same policy from alternate Debug.
    format!("{command:#?}").contains("kill_on_drop: true")
}

fn chooser_failure_message(code: Option<i32>, stderr: &str) -> String {
    let status = code.map_or_else(
        || "terminated by signal".to_string(),
        |code| format!("exit code {code}"),
    );
    if stderr.is_empty() {
        format!("tensor-files chooser failed with {status}")
    } else {
        format!("tensor-files chooser failed with {status}: {stderr}")
    }
}

fn parse_chooser_output(stdout: &str) -> ChooserResult {
    let mut result = ChooserResult::default();
    for line in stdout.lines().filter(|line| !line.is_empty()) {
        if let Some(index) = line
            .strip_prefix("TENSOR_FILES_CHOOSER_FILTER\t")
            .and_then(|index| index.parse::<usize>().ok())
        {
            result.filter_index = Some(index);
        } else if let Some(choice) = line.strip_prefix("TENSOR_FILES_CHOOSER_CHOICE\t") {
            let mut parts = choice.splitn(2, '\t');
            if let (Some(id), Some(selected)) = (parts.next(), parts.next()) {
                result.choices.push((id.to_string(), selected.to_string()));
            }
        } else {
            result.paths.push(PathBuf::from(line));
        }
    }
    result
}

include!("tensor_files_xdp_filechooser/portal_options.rs");

#[cfg(test)]
#[path = "tensor_files_xdp_filechooser/tests.rs"]
mod tests;
