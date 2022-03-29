use crate::client::ClientError;
use crate::event_loop::{EventLoopDispatcher, EventLoopError, EventLoopId};
use crate::state::State;
use crate::utils::errorfmt::ErrorFmt;
use std::rc::Rc;
use thiserror::Error;
use uapi::{c, format_ustr, Errno, OwnedFd, Ustring};

#[derive(Debug, Error)]
pub enum AcceptorError {
    #[error("XDG_RUNTIME_DIR is not set")]
    XrdNotSet,
    #[error("XDG_RUNTIME_DIR ({0:?}) is too long to form a unix socket address")]
    XrdTooLong(String),
    #[error("Could not create a wayland socket")]
    SocketFailed(#[source] crate::utils::oserror::OsError),
    #[error("Could not stat the existing socket")]
    SocketStat(#[source] crate::utils::oserror::OsError),
    #[error("Could not start listening for incoming connections")]
    ListenFailed(#[source] crate::utils::oserror::OsError),
    #[error("Could not open the lock file")]
    OpenLockFile(#[source] crate::utils::oserror::OsError),
    #[error("Could not lock the lock file")]
    LockLockFile(#[source] crate::utils::oserror::OsError),
    #[error("The wayland socket is in an error state")]
    ErrorEvent,
    #[error("Could not accept new connections")]
    AcceptFailed(#[source] crate::utils::oserror::OsError),
    #[error("Could not spawn an event handler for a new connection")]
    SpawnFailed(#[source] ClientError),
    #[error("Could not bind the socket to an address")]
    BindFailed(#[source] crate::utils::oserror::OsError),
    #[error("All wayland addresses in the range 0..1000 are already in use")]
    AddressesInUse,
    #[error("The event loop caused an error")]
    EventLoopError(#[from] EventLoopError),
}

pub struct Acceptor {
    id: EventLoopId,
    socket: AllocatedSocket,
    global: Rc<State>,
}

struct AllocatedSocket {
    // wayland-x
    name: Ustring,
    // /run/user/1000/wayland-x
    path: Ustring,
    fd: Rc<OwnedFd>,
    // /run/user/1000/wayland-x.lock
    lock_path: Ustring,
    _lock_fd: OwnedFd,
}

impl Drop for AllocatedSocket {
    fn drop(&mut self) {
        let _ = uapi::unlink(&self.path);
        let _ = uapi::unlink(&self.lock_path);
    }
}

fn bind_socket(fd: &Rc<OwnedFd>, xrd: &str, id: u32) -> Result<AllocatedSocket, AcceptorError> {
    let mut addr: c::sockaddr_un = uapi::pod_zeroed();
    addr.sun_family = c::AF_UNIX as _;
    let name = format_ustr!("wayland-{}", id);
    let path = format_ustr!("{}/{}", xrd, name.display());
    let lock_path = format_ustr!("{}.lock", path.display());
    if path.len() + 1 > addr.sun_path.len() {
        return Err(AcceptorError::XrdTooLong(xrd.to_string()));
    }
    let lock_fd = match uapi::open(&*lock_path, c::O_CREAT | c::O_CLOEXEC | c::O_RDWR, 0o644) {
        Ok(l) => l,
        Err(e) => return Err(AcceptorError::OpenLockFile(e.into())),
    };
    if let Err(e) = uapi::flock(lock_fd.raw(), c::LOCK_EX | c::LOCK_NB) {
        return Err(AcceptorError::LockLockFile(e.into()));
    }
    match uapi::lstat(&*path) {
        Ok(_) => {
            log::info!("Unlinking {}", path.display());
            let _ = uapi::unlink(&*path);
        }
        Err(Errno(c::ENOENT)) => {}
        Err(e) => return Err(AcceptorError::SocketStat(e.into())),
    }
    let sun_path = uapi::as_bytes_mut(&mut addr.sun_path[..]);
    sun_path[..path.len()].copy_from_slice(path.as_bytes());
    sun_path[path.len()] = 0;
    if let Err(e) = uapi::bind(fd.raw(), &addr) {
        return Err(AcceptorError::BindFailed(e.into()));
    }
    Ok(AllocatedSocket {
        name,
        path,
        fd: fd.clone(),
        lock_path,
        _lock_fd: lock_fd,
    })
}

fn allocate_socket() -> Result<AllocatedSocket, AcceptorError> {
    let xrd = match std::env::var("XDG_RUNTIME_DIR") {
        Ok(d) => d,
        Err(_) => return Err(AcceptorError::XrdNotSet),
    };
    let fd = match uapi::socket(
        c::AF_UNIX,
        c::SOCK_STREAM | c::SOCK_NONBLOCK | c::SOCK_CLOEXEC,
        0,
    ) {
        Ok(f) => Rc::new(f),
        Err(e) => return Err(AcceptorError::SocketFailed(e.into())),
    };
    for i in 1..1000 {
        match bind_socket(&fd, &xrd, i) {
            Ok(s) => return Ok(s),
            Err(e) => {
                log::warn!("Cannot use the wayland-{} socket: {}", i, ErrorFmt(e));
            }
        }
    }
    Err(AcceptorError::AddressesInUse)
}

impl Acceptor {
    pub fn install(global: &Rc<State>) -> Result<Ustring, AcceptorError> {
        let socket = allocate_socket()?;
        log::info!("bound to socket {}", socket.path.display());
        if let Err(e) = uapi::listen(socket.fd.raw(), 4096) {
            return Err(AcceptorError::ListenFailed(e.into()));
        }
        let id = global.el.id();
        let name = socket.name.to_owned();
        let acc = Rc::new(Acceptor {
            id,
            socket,
            global: global.clone(),
        });
        global
            .el
            .insert(id, Some(acc.socket.fd.raw()), c::EPOLLIN, acc)?;
        Ok(name)
    }
}

impl EventLoopDispatcher for Acceptor {
    fn dispatch(self: Rc<Self>, events: i32) -> Result<(), Box<dyn std::error::Error>> {
        if events & (c::EPOLLERR | c::EPOLLHUP) != 0 {
            return Err(Box::new(AcceptorError::ErrorEvent));
        }
        loop {
            let fd = match uapi::accept4(
                self.socket.fd.raw(),
                uapi::sockaddr_none_mut(),
                c::SOCK_NONBLOCK | c::SOCK_CLOEXEC,
            ) {
                Ok((fd, _)) => fd,
                Err(Errno(c::EAGAIN)) => break,
                Err(e) => return Err(Box::new(AcceptorError::AcceptFailed(e.into()))),
            };
            let id = self.global.clients.id();
            if let Err(e) = self.global.clients.spawn(id, &self.global, fd) {
                return Err(Box::new(AcceptorError::SpawnFailed(e)));
            }
        }
        Ok(())
    }
}

impl Drop for Acceptor {
    fn drop(&mut self) {
        let _ = self.global.el.remove(self.id);
    }
}
