//! Rootless XWayland process and display-socket ownership.
//!
//! The display allocation and spawn sequence are derived from Smithay commit
//! c0aa71d. Smithay's copyright notice and MIT terms are in
//! `LICENSES/Smithay-MIT.txt`.

use std::{
    env,
    ffi::OsStr,
    fs::{File, OpenOptions},
    io::{Read, Write},
    os::{
        fd::{AsFd, AsRawFd, BorrowedFd, OwnedFd},
        unix::net::UnixStream,
    },
    process::{Child, Command, Stdio},
    sync::{Arc, Mutex},
    thread,
};

use rustix::{io::Errno, net::SocketAddrUnix};
use tracing::{debug, error, info, warn};
use wayland_server::{
    Client, DisplayHandle,
    backend::{ClientData, ClientId, DisconnectReason},
};

#[derive(Debug)]
pub(crate) struct XWayland {
    display_lock: X11Lock,
    display_fd: OwnedFd,
    x11_socket: Option<UnixStream>,
    display: DisplayHandle,
    client: Client,
}

impl XWayland {
    pub(crate) fn spawn(display: &DisplayHandle) -> std::io::Result<(Self, Client)> {
        let (x_wm_child, x_wm_parent) = UnixStream::pair()?;
        let (wl_child, wl_parent) = UnixStream::pair()?;
        let (display_lock, listen_sockets) = prepare_x11_sockets(None, true)?;
        let display_number = display_lock.display_number();
        let (displayfd_recv, displayfd_send) = rustix::pipe::pipe_with(
            rustix::pipe::PipeFlags::NONBLOCK | rustix::pipe::PipeFlags::CLOEXEC,
        )?;
        let wl_inherited = inheritable_duplicate(&wl_child)?;
        let wm_inherited = inheritable_duplicate(&x_wm_child)?;
        let displayfd_inherited = inheritable_duplicate(&displayfd_send)?;
        let listen_inherited = listen_sockets
            .iter()
            .map(inheritable_duplicate)
            .collect::<Result<Vec<_>, _>>()?;

        let mut command = Command::new("Xwayland");
        command
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .arg(format!(":{display_number}"))
            .arg("-verbose")
            .arg("-rootless")
            .arg("-terminate")
            .arg("-wm")
            .arg(wm_inherited.as_raw_fd().to_string())
            .arg("-displayfd")
            .arg(displayfd_inherited.as_raw_fd().to_string());
        for socket in &listen_inherited {
            command.arg("-listenfd").arg(socket.as_raw_fd().to_string());
        }

        command.env_clear();
        for (key, value) in env::vars_os() {
            if key == OsStr::new("PATH") || key == OsStr::new("XDG_RUNTIME_DIR") {
                command.env(key, value);
            }
        }
        command.env("WAYLAND_SOCKET", wl_inherited.as_raw_fd().to_string());

        info!(display_number, "spawning rootless XWayland");
        let child = command.spawn()?;
        drop((
            x_wm_child,
            wl_child,
            displayfd_send,
            listen_sockets,
            wl_inherited,
            wm_inherited,
            displayfd_inherited,
            listen_inherited,
        ));

        let data = Arc::new(XWaylandClientData {
            child: Mutex::new(Some(child)),
        });
        let mut display = display.clone();
        let client = display.insert_client(wl_parent, data)?;
        Ok((
            Self {
                display_lock,
                display_fd: displayfd_recv,
                x11_socket: Some(x_wm_parent),
                display,
                client: client.clone(),
            },
            client,
        ))
    }

    pub(crate) fn display_number(&self) -> u32 {
        self.display_lock.display_number()
    }

    pub(crate) fn completion_fd(&self) -> BorrowedFd<'_> {
        self.display_fd.as_fd()
    }

    pub(crate) fn take_socket(&mut self) -> std::io::Result<Option<UnixStream>> {
        if self.x11_socket.is_none() {
            return Ok(None);
        }
        let mut bytes = [0; 64];
        loop {
            match rustix::io::read(&self.display_fd, &mut bytes) {
                Ok(0) => return Ok(None),
                Ok(len) if bytes[..len].contains(&b'\n') => return Ok(self.x11_socket.take()),
                Ok(_) => {}
                Err(error) if error == Errno::AGAIN => return Ok(None),
                Err(error) => return Err(error.into()),
            }
        }
    }
}

impl Drop for XWayland {
    fn drop(&mut self) {
        self.display
            .backend_handle()
            .kill_client(self.client.id(), DisconnectReason::ConnectionClosed);
    }
}

#[derive(Debug)]
pub(crate) struct XWaylandClientData {
    child: Mutex<Option<Child>>,
}

impl ClientData for XWaylandClientData {
    fn disconnected(&self, _client: ClientId, reason: DisconnectReason) {
        if let DisconnectReason::ProtocolError(error) = reason {
            error!(%error, "XWayland Wayland client disconnected with a protocol error");
        }
        let Some(mut child) = self.child.lock().unwrap().take() else {
            return;
        };
        thread::spawn(move || match child.wait() {
            Ok(status) if !status.success() => {
                error!(%status, "XWayland terminated unsuccessfully")
            }
            Err(error) => error!(%error, "failed to wait for XWayland"),
            _ => {}
        });
    }
}

#[derive(Debug)]
struct X11Lock {
    display: u32,
}

impl X11Lock {
    fn grab(display: u32) -> Result<Self, ()> {
        let path = format!("/tmp/.X{display}-lock");
        match OpenOptions::new().write(true).create_new(true).open(&path) {
            Ok(mut file) => {
                writeln!(
                    file,
                    "{:>10}",
                    rustix::process::Pid::as_raw(Some(rustix::process::getpid()))
                )
                .map_err(|_| ())?;
                Ok(Self { display })
            }
            Err(_) => {
                let mut pid_bytes = [0_u8; 11];
                File::open(&path)
                    .and_then(|mut file| file.read_exact(&mut pid_bytes))
                    .map_err(|_| ())?;
                let pid = std::str::from_utf8(&pid_bytes)
                    .ok()
                    .and_then(|value| value.trim().parse::<i32>().ok())
                    .and_then(rustix::process::Pid::from_raw)
                    .ok_or(())?;
                if rustix::process::test_kill_process(pid) == Err(Errno::SRCH) {
                    std::fs::remove_file(&path).map_err(|_| ())?;
                    return Self::grab(display);
                }
                Err(())
            }
        }
    }

    const fn display_number(&self) -> u32 {
        self.display
    }
}

impl Drop for X11Lock {
    fn drop(&mut self) {
        let socket = format!("/tmp/.X11-unix/X{}", self.display);
        let lock = format!("/tmp/.X{}-lock", self.display);
        for path in [socket, lock] {
            if let Err(error) = std::fs::remove_file(&path)
                && error.kind() != std::io::ErrorKind::NotFound
            {
                warn!(%error, %path, "failed to remove X11 display artifact");
            }
        }
    }
}

fn prepare_x11_sockets(
    requested: Option<u32>,
    abstract_socket: bool,
) -> std::io::Result<(X11Lock, Vec<UnixStream>)> {
    let displays = requested.map_or(0..=32, |display| display..=display);
    for display in displays {
        let Ok(lock) = X11Lock::grab(display) else {
            continue;
        };
        match open_x11_sockets(display, abstract_socket) {
            Ok(sockets) => return Ok((lock, sockets)),
            Err(error) => {
                debug!(
                    x11_display = lock.display_number(),
                    %error,
                    "failed to bind X11 display sockets"
                );
            }
        }
    }
    Err(std::io::Error::new(
        std::io::ErrorKind::AddrInUse,
        "no free X11 display in :0 through :32",
    ))
}

fn open_x11_sockets(display: u32, abstract_socket: bool) -> std::io::Result<Vec<UnixStream>> {
    let path = format!("/tmp/.X11-unix/X{display}");
    let _ = std::fs::remove_file(&path);
    let mut sockets = vec![open_socket(SocketAddrUnix::new(path.as_bytes())?)?];
    #[cfg(target_os = "linux")]
    if abstract_socket {
        sockets.push(open_socket(SocketAddrUnix::new_abstract_name(
            path.as_bytes(),
        )?)?);
    }
    #[cfg(not(target_os = "linux"))]
    let _ = abstract_socket;
    Ok(sockets)
}

fn open_socket(address: SocketAddrUnix) -> std::io::Result<UnixStream> {
    let socket = rustix::net::socket_with(
        rustix::net::AddressFamily::UNIX,
        rustix::net::SocketType::STREAM,
        rustix::net::SocketFlags::CLOEXEC,
        None,
    )?;
    rustix::net::bind(&socket, &address)?;
    rustix::net::listen(&socket, 1)?;
    Ok(UnixStream::from(socket))
}

fn inheritable_duplicate(fd: &impl AsFd) -> std::io::Result<OwnedFd> {
    let duplicate = rustix::io::fcntl_dupfd_cloexec(fd, 3)?;
    rustix::io::fcntl_setfd(&duplicate, rustix::io::FdFlags::empty())?;
    Ok(duplicate)
}
