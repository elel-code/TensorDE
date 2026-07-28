use std::collections::{BTreeSet, HashMap};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, Sender};
use std::time::{Duration, Instant};

use crate::windowing::EventLoopProxy;
use fika_core::is_network_path;
use notify::{Event as NotifyEvent, RecursiveMode as NotifyRecursiveMode};

use crate::ShellScene;
use crate::ui::pane::ShellPaneId;

#[path = "directory_watch/event.rs"]
mod event;

use event::{shell_directory_watch_event_mutates, shell_directory_watch_event_touches_path};

pub(crate) struct ShellDirectoryWatcherRuntime {
    tx: Sender<notify::Result<NotifyEvent>>,
    rx: Receiver<notify::Result<NotifyEvent>>,
    proxy: EventLoopProxy,
    watchers: HashMap<ShellPaneId, ShellDirectoryWatcher>,
    failed_paths: HashMap<ShellPaneId, PathBuf>,
    pending_reload_paths: BTreeSet<PathBuf>,
    reload_deadline: Option<Instant>,
}

struct ShellDirectoryWatcher {
    path: PathBuf,
    _watcher: notify::RecommendedWatcher,
}

impl ShellDirectoryWatcherRuntime {
    const DEBOUNCE: Duration = Duration::from_millis(250);

    pub(crate) fn new(proxy: EventLoopProxy) -> Self {
        let (tx, rx) = mpsc::channel();
        Self {
            tx,
            rx,
            proxy,
            watchers: HashMap::new(),
            failed_paths: HashMap::new(),
            pending_reload_paths: BTreeSet::new(),
            reload_deadline: None,
        }
    }

    pub(crate) fn sync_with_scene(&mut self, scene: &ShellScene) {
        for pane in ShellPaneId::ALL {
            let target_path = scene
                .pane_state(pane)
                .filter(|state| !is_network_path(&state.path))
                .map(|state| state.path.clone());
            let Some(path) = target_path else {
                self.watchers.remove(&pane);
                self.failed_paths.remove(&pane);
                continue;
            };

            if self
                .watchers
                .get(&pane)
                .is_some_and(|watcher| watcher.path == path)
            {
                self.failed_paths.remove(&pane);
                continue;
            }
            if self
                .failed_paths
                .get(&pane)
                .is_some_and(|failed_path| *failed_path == path)
            {
                continue;
            }

            self.watchers.remove(&pane);
            match self.start_watching(&path) {
                Ok(watcher) => {
                    self.failed_paths.remove(&pane);
                    self.watchers.insert(pane, watcher);
                    fika_log!(
                        "[fika-wgpu] directory-watch-start pane={} path={}",
                        pane.as_str(),
                        path.display()
                    );
                }
                Err(error) => {
                    self.failed_paths.insert(pane, path.clone());
                    fika_log!(
                        "[fika-wgpu] directory-watch-start-error pane={} path={} error={error}",
                        pane.as_str(),
                        path.display()
                    );
                }
            }
        }
    }

    fn start_watching(&self, path: &Path) -> Result<ShellDirectoryWatcher, String> {
        let tx = self.tx.clone();
        let proxy = self.proxy.clone();
        let mut watcher = notify::recommended_watcher(move |event: notify::Result<NotifyEvent>| {
            if tx.send(event).is_ok() {
                proxy.wake_up();
            }
        })
        .map_err(|error| error.to_string())?;
        notify::Watcher::watch(&mut watcher, path, NotifyRecursiveMode::NonRecursive)
            .map_err(|error| error.to_string())?;
        Ok(ShellDirectoryWatcher {
            path: path.to_path_buf(),
            _watcher: watcher,
        })
    }

    pub(crate) fn drain_events(&mut self, scene: &ShellScene) {
        let mut should_debounce = false;
        while let Ok(event) = self.rx.try_recv() {
            match event {
                Ok(event) if shell_directory_watch_event_mutates(&event.kind) => {
                    should_debounce |= self.schedule_event_reload(scene, &event);
                }
                Ok(_) => {}
                Err(error) => {
                    fika_log!("[fika-wgpu] directory-watch-error {error}");
                    should_debounce |= self.schedule_all_watched_reloads();
                }
            }
        }
        if should_debounce {
            self.reload_deadline = Some(Instant::now() + Self::DEBOUNCE);
        }
    }

    fn schedule_event_reload(&mut self, scene: &ShellScene, event: &NotifyEvent) -> bool {
        let mut changed = false;
        for pane in ShellPaneId::ALL {
            let Some(state) = scene.pane_state(pane) else {
                continue;
            };
            if is_network_path(&state.path) {
                continue;
            }
            if shell_directory_watch_event_touches_path(event, &state.path) {
                changed |= self.pending_reload_paths.insert(state.path.clone());
            }
        }
        changed
    }

    fn schedule_all_watched_reloads(&mut self) -> bool {
        let mut changed = false;
        for path in self
            .watchers
            .values()
            .map(|watcher| watcher.path.clone())
            .collect::<Vec<_>>()
        {
            changed |= self.pending_reload_paths.insert(path);
        }
        changed
    }

    pub(crate) fn defer_reload_paths(&mut self, paths: Vec<PathBuf>) {
        if paths.is_empty() {
            return;
        }
        for path in paths {
            self.pending_reload_paths.insert(path);
        }
        self.reload_deadline = Some(Instant::now() + Self::DEBOUNCE);
    }

    pub(crate) fn take_due_reload_paths(&mut self, now: Instant) -> Vec<PathBuf> {
        let Some(deadline) = self.reload_deadline else {
            return Vec::new();
        };
        if now < deadline {
            return Vec::new();
        }
        self.reload_deadline = None;
        std::mem::take(&mut self.pending_reload_paths)
            .into_iter()
            .collect()
    }

    pub(crate) fn next_reload_deadline(&self) -> Option<Instant> {
        (!self.pending_reload_paths.is_empty())
            .then_some(self.reload_deadline)
            .flatten()
    }
}
