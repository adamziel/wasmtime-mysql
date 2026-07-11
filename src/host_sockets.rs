use std::collections::HashMap;
use std::ops::Range;
use std::sync::{Arc, Mutex};

#[cfg(unix)]
use std::os::fd::RawFd;
#[cfg(windows)]
use windows_sys::Win32::Networking::WinSock::SOCKET;

use wasmtime::{Caller, Linker, Result};

use crate::{AppState, guest_errno as errno};

#[cfg(windows)]
#[path = "host_sockets_windows.rs"]
mod windows;
#[cfg(windows)]
pub(crate) use windows::add_to_linker;

const MODULE_NAME: &str = "waasmtime_mysql_sockets";
const GUEST_FD_BASE: i32 = 10_000;
const MAX_IO_LEN: usize = 16 * 1024 * 1024;
const MAX_POLL_FDS: usize = 16_384;
const WASI_ALT_AF_INET: i32 = 1;
const WASI_ALT_SOCK_DGRAM: i32 = 5;
const WASI_ALT_SOCK_STREAM: i32 = 6;
const WASI_ALT_SOCK_CLOEXEC: i32 = 0x2000;
const WASI_ALT_SOCK_NONBLOCK: i32 = 0x4000;
const WASI_SOL_SOCKET: i32 = 0x7fff_ffff;
const WASI_F_GETFL: i32 = 3;
const WASI_F_SETFL: i32 = 4;
const WASI_O_NONBLOCK: i32 = 0x0004;
const WASI_SO_ERROR: i32 = 4;

#[derive(Clone)]
pub(crate) struct HostSockets {
    inner: Arc<Mutex<HostSocketsInner>>,
}

struct HostSocketsInner {
    next_fd: i32,
    #[cfg(any(unix, windows))]
    sockets: HashMap<i32, HostSocket>,
}

#[cfg(unix)]
#[derive(Clone, Copy)]
struct HostSocket {
    raw_fd: RawFd,
    guest_domain: i32,
    host_domain: i32,
}

#[cfg(windows)]
#[derive(Clone, Copy)]
struct HostSocket {
    raw_socket: SOCKET,
    guest_domain: i32,
    host_domain: i32,
    status_flags: i32,
}

#[cfg(unix)]
#[derive(Clone, Copy)]
struct NormalizedSocketType {
    ty: i32,
    close_on_exec: bool,
    nonblocking: bool,
}

impl HostSockets {
    pub(crate) fn new() -> Self {
        Self {
            inner: Arc::new(Mutex::new(HostSocketsInner {
                next_fd: GUEST_FD_BASE,
                #[cfg(any(unix, windows))]
                sockets: HashMap::new(),
            })),
        }
    }

    #[cfg(unix)]
    fn insert(&self, raw_fd: RawFd, guest_domain: i32, host_domain: i32) -> i32 {
        let mut inner = self.inner.lock().unwrap();
        let guest_fd = inner.next_fd;
        inner.next_fd = inner.next_fd.saturating_add(1);
        inner.sockets.insert(
            guest_fd,
            HostSocket {
                raw_fd,
                guest_domain,
                host_domain,
            },
        );
        guest_fd
    }

    #[cfg(unix)]
    fn get(&self, guest_fd: i32) -> std::result::Result<HostSocket, i32> {
        let inner = self.inner.lock().unwrap();
        inner.sockets.get(&guest_fd).copied().ok_or(errno::EBADF)
    }

    #[cfg(unix)]
    fn remove(&self, guest_fd: i32) -> std::result::Result<HostSocket, i32> {
        let mut inner = self.inner.lock().unwrap();
        inner.sockets.remove(&guest_fd).ok_or(errno::EBADF)
    }

    #[cfg(windows)]
    fn insert(
        &self,
        raw_socket: SOCKET,
        guest_domain: i32,
        host_domain: i32,
        status_flags: i32,
    ) -> i32 {
        let mut inner = self.inner.lock().unwrap();
        let guest_fd = inner.next_fd;
        inner.next_fd = inner.next_fd.saturating_add(1);
        inner.sockets.insert(
            guest_fd,
            HostSocket {
                raw_socket,
                guest_domain,
                host_domain,
                status_flags,
            },
        );
        guest_fd
    }

    #[cfg(windows)]
    fn get(&self, guest_fd: i32) -> std::result::Result<HostSocket, i32> {
        let inner = self.inner.lock().unwrap();
        inner.sockets.get(&guest_fd).copied().ok_or(errno::EBADF)
    }

    #[cfg(windows)]
    fn remove(&self, guest_fd: i32) -> std::result::Result<HostSocket, i32> {
        let mut inner = self.inner.lock().unwrap();
        inner.sockets.remove(&guest_fd).ok_or(errno::EBADF)
    }

    #[cfg(windows)]
    fn set_status_flags(&self, guest_fd: i32, status_flags: i32) -> std::result::Result<(), i32> {
        let mut inner = self.inner.lock().unwrap();
        let socket = inner.sockets.get_mut(&guest_fd).ok_or(errno::EBADF)?;
        socket.status_flags = status_flags;
        Ok(())
    }
}

#[cfg(unix)]
impl Drop for HostSocketsInner {
    fn drop(&mut self) {
        for (_, socket) in self.sockets.drain() {
            unsafe {
                libc::close(socket.raw_fd);
            }
        }
    }
}

#[cfg(windows)]
impl Drop for HostSocketsInner {
    fn drop(&mut self) {
        for (_, socket) in self.sockets.drain() {
            unsafe {
                windows_sys::Win32::Networking::WinSock::closesocket(socket.raw_socket);
            }
        }
    }
}

#[cfg(not(any(unix, windows)))]
impl Drop for HostSocketsInner {
    fn drop(&mut self) {}
}

#[cfg(not(windows))]
pub(crate) fn add_to_linker(linker: &mut Linker<AppState>) -> Result<()> {
    linker.func_wrap(
        MODULE_NAME,
        "socket",
        |mut caller: Caller<'_, AppState>, domain: i32, ty: i32, protocol: i32| -> i32 {
            #[cfg(unix)]
            {
                if !caller.data().network_allowed {
                    return neg_errno(errno::ENETDOWN);
                }
                let socket_type = match normalize_socket_type(ty) {
                    Ok(socket_type) => socket_type,
                    Err(errno) => return neg_errno(errno),
                };
                let host_domain = normalize_socket_domain(domain);
                let raw_fd = unsafe { libc::socket(host_domain, socket_type.ty, protocol) };
                if raw_fd < 0 {
                    return neg_last_errno();
                }
                if let Err(errno) = configure_socket_type_flags(raw_fd, socket_type) {
                    unsafe {
                        libc::close(raw_fd);
                    }
                    return neg_errno(errno);
                }
                caller
                    .data_mut()
                    .sockets
                    .insert(raw_fd, domain, host_domain)
            }
            #[cfg(not(unix))]
            {
                let _ = (&mut caller, domain, ty, protocol);
                neg_errno(errno::ENOSYS)
            }
        },
    )?;

    linker.func_wrap(
        MODULE_NAME,
        "bind",
        |mut caller: Caller<'_, AppState>, fd: i32, addr_ptr: i32, addr_len: i32| -> i32 {
            #[cfg(unix)]
            {
                let socket = match caller.data().sockets.get(fd) {
                    Ok(socket) => socket,
                    Err(errno) => return neg_errno(errno),
                };
                let mut addr = match read_guest(&mut caller, addr_ptr, addr_len) {
                    Ok(addr) => addr,
                    Err(errno) => return neg_errno(errno),
                };
                normalize_sockaddr_for_host(&mut addr, socket.host_domain);
                cvt_i32(unsafe {
                    libc::bind(
                        socket.raw_fd,
                        addr.as_ptr().cast::<libc::sockaddr>(),
                        addr.len() as libc::socklen_t,
                    )
                })
            }
            #[cfg(not(unix))]
            {
                let _ = (&mut caller, fd, addr_ptr, addr_len);
                neg_errno(errno::ENOSYS)
            }
        },
    )?;

    linker.func_wrap(
        MODULE_NAME,
        "listen",
        |caller: Caller<'_, AppState>, fd: i32, backlog: i32| -> i32 {
            #[cfg(unix)]
            {
                let socket = match caller.data().sockets.get(fd) {
                    Ok(socket) => socket,
                    Err(errno) => return neg_errno(errno),
                };
                cvt_i32(unsafe { libc::listen(socket.raw_fd, backlog) })
            }
            #[cfg(not(unix))]
            {
                let _ = (caller, fd, backlog);
                neg_errno(errno::ENOSYS)
            }
        },
    )?;

    linker.func_wrap(
        MODULE_NAME,
        "accept",
        |mut caller: Caller<'_, AppState>, fd: i32, addr_ptr: i32, addr_len_ptr: i32| -> i32 {
            #[cfg(unix)]
            {
                let socket = match caller.data().sockets.get(fd) {
                    Ok(socket) => socket,
                    Err(errno) => return neg_errno(errno),
                };
                let mut addr_len = match read_u32(&mut caller, addr_len_ptr) {
                    Ok(len) => len as libc::socklen_t,
                    Err(errno) => return neg_errno(errno),
                };
                let mut addr = vec![0_u8; addr_len as usize];
                let accepted = unsafe {
                    libc::accept(
                        socket.raw_fd,
                        addr.as_mut_ptr().cast::<libc::sockaddr>(),
                        &mut addr_len,
                    )
                };
                if accepted < 0 {
                    return neg_last_errno();
                }
                denormalize_sockaddr_for_guest(&mut addr, socket.guest_domain);
                if let Err(errno) = write_guest(&mut caller, addr_ptr, &addr[..addr_len as usize]) {
                    unsafe {
                        libc::close(accepted);
                    }
                    return neg_errno(errno);
                }
                if let Err(errno) = write_u32(&mut caller, addr_len_ptr, addr_len as u32) {
                    unsafe {
                        libc::close(accepted);
                    }
                    return neg_errno(errno);
                }
                caller
                    .data_mut()
                    .sockets
                    .insert(accepted, socket.guest_domain, socket.host_domain)
            }
            #[cfg(not(unix))]
            {
                let _ = (&mut caller, fd, addr_ptr, addr_len_ptr);
                neg_errno(errno::ENOSYS)
            }
        },
    )?;

    linker.func_wrap(
        MODULE_NAME,
        "connect",
        |mut caller: Caller<'_, AppState>, fd: i32, addr_ptr: i32, addr_len: i32| -> i32 {
            #[cfg(unix)]
            {
                let socket = match caller.data().sockets.get(fd) {
                    Ok(socket) => socket,
                    Err(errno) => return neg_errno(errno),
                };
                let mut addr = match read_guest(&mut caller, addr_ptr, addr_len) {
                    Ok(addr) => addr,
                    Err(errno) => return neg_errno(errno),
                };
                normalize_sockaddr_for_host(&mut addr, socket.host_domain);
                cvt_i32(unsafe {
                    libc::connect(
                        socket.raw_fd,
                        addr.as_ptr().cast::<libc::sockaddr>(),
                        addr.len() as libc::socklen_t,
                    )
                })
            }
            #[cfg(not(unix))]
            {
                let _ = (&mut caller, fd, addr_ptr, addr_len);
                neg_errno(errno::ENOSYS)
            }
        },
    )?;

    linker.func_wrap(
        MODULE_NAME,
        "getsockname",
        |caller: Caller<'_, AppState>, fd: i32, addr_ptr: i32, addr_len_ptr: i32| -> i32 {
            sock_name(caller, fd, addr_ptr, addr_len_ptr, NameKind::Local)
        },
    )?;

    linker.func_wrap(
        MODULE_NAME,
        "getpeername",
        |caller: Caller<'_, AppState>, fd: i32, addr_ptr: i32, addr_len_ptr: i32| -> i32 {
            sock_name(caller, fd, addr_ptr, addr_len_ptr, NameKind::Peer)
        },
    )?;

    linker.func_wrap(
        MODULE_NAME,
        "setsockopt",
        |mut caller: Caller<'_, AppState>,
         fd: i32,
         level: i32,
         optname: i32,
         optval_ptr: i32,
         optlen: i32|
         -> i32 {
            #[cfg(unix)]
            {
                let socket = match caller.data().sockets.get(fd) {
                    Ok(socket) => socket,
                    Err(errno) => return neg_errno(errno),
                };
                let optval = match read_guest(&mut caller, optval_ptr, optlen) {
                    Ok(optval) => optval,
                    Err(errno) => return neg_errno(errno),
                };
                cvt_i32(unsafe {
                    libc::setsockopt(
                        socket.raw_fd,
                        normalize_socket_level(level),
                        optname,
                        optval.as_ptr().cast::<libc::c_void>(),
                        optval.len() as libc::socklen_t,
                    )
                })
            }
            #[cfg(not(unix))]
            {
                let _ = (&mut caller, fd, level, optname, optval_ptr, optlen);
                neg_errno(errno::ENOSYS)
            }
        },
    )?;

    linker.func_wrap(
        MODULE_NAME,
        "getsockopt",
        |mut caller: Caller<'_, AppState>,
         fd: i32,
         level: i32,
         optname: i32,
         optval_ptr: i32,
         optlen_ptr: i32|
         -> i32 {
            #[cfg(unix)]
            {
                let socket = match caller.data().sockets.get(fd) {
                    Ok(socket) => socket,
                    Err(errno) => return neg_errno(errno),
                };
                let mut optlen = match read_u32(&mut caller, optlen_ptr) {
                    Ok(len) => len as libc::socklen_t,
                    Err(errno) => return neg_errno(errno),
                };
                let mut optval = vec![0_u8; optlen as usize];
                let rc = unsafe {
                    libc::getsockopt(
                        socket.raw_fd,
                        normalize_socket_level(level),
                        optname,
                        optval.as_mut_ptr().cast::<libc::c_void>(),
                        &mut optlen,
                    )
                };
                if rc < 0 {
                    return neg_last_errno();
                }
                if is_guest_socket_level(level) && optname == WASI_SO_ERROR && optlen as usize >= 4
                {
                    let host_error = i32::from_ne_bytes(optval[..4].try_into().unwrap());
                    optval[..4].copy_from_slice(&errno::from_host_errno(host_error).to_ne_bytes());
                }
                if let Err(errno) = write_guest(&mut caller, optval_ptr, &optval[..optlen as usize])
                {
                    return neg_errno(errno);
                }
                match write_u32(&mut caller, optlen_ptr, optlen as u32) {
                    Ok(()) => 0,
                    Err(errno) => neg_errno(errno),
                }
            }
            #[cfg(not(unix))]
            {
                let _ = (&mut caller, fd, level, optname, optval_ptr, optlen_ptr);
                neg_errno(errno::ENOSYS)
            }
        },
    )?;

    linker.func_wrap(
        MODULE_NAME,
        "send",
        |mut caller: Caller<'_, AppState>, fd: i32, buf_ptr: i32, len: i32, flags: i32| -> i32 {
            #[cfg(unix)]
            {
                let socket = match caller.data().sockets.get(fd) {
                    Ok(socket) => socket,
                    Err(errno) => return neg_errno(errno),
                };
                let buf = match read_guest(&mut caller, buf_ptr, len) {
                    Ok(buf) => buf,
                    Err(errno) => return neg_errno(errno),
                };
                cvt_ssize(unsafe {
                    libc::send(
                        socket.raw_fd,
                        buf.as_ptr().cast::<libc::c_void>(),
                        buf.len(),
                        flags,
                    )
                })
            }
            #[cfg(not(unix))]
            {
                let _ = (&mut caller, fd, buf_ptr, len, flags);
                neg_errno(errno::ENOSYS)
            }
        },
    )?;

    linker.func_wrap(
        MODULE_NAME,
        "recv",
        |mut caller: Caller<'_, AppState>, fd: i32, buf_ptr: i32, len: i32, flags: i32| -> i32 {
            #[cfg(unix)]
            {
                let socket = match caller.data().sockets.get(fd) {
                    Ok(socket) => socket,
                    Err(errno) => return neg_errno(errno),
                };
                let len = match checked_len(len) {
                    Ok(len) => len,
                    Err(errno) => return neg_errno(errno),
                };
                let mut buf = vec![0_u8; len];
                let rc = unsafe {
                    libc::recv(
                        socket.raw_fd,
                        buf.as_mut_ptr().cast::<libc::c_void>(),
                        buf.len(),
                        flags,
                    )
                };
                if rc < 0 {
                    return neg_last_errno();
                }
                let rc = rc as usize;
                match write_guest(&mut caller, buf_ptr, &buf[..rc]) {
                    Ok(()) => rc as i32,
                    Err(errno) => neg_errno(errno),
                }
            }
            #[cfg(not(unix))]
            {
                let _ = (&mut caller, fd, buf_ptr, len, flags);
                neg_errno(errno::ENOSYS)
            }
        },
    )?;

    linker.func_wrap(
        MODULE_NAME,
        "sendto",
        |mut caller: Caller<'_, AppState>,
         fd: i32,
         buf_ptr: i32,
         len: i32,
         flags: i32,
         addr_ptr: i32,
         addr_len: i32|
         -> i32 {
            #[cfg(unix)]
            {
                let socket = match caller.data().sockets.get(fd) {
                    Ok(socket) => socket,
                    Err(errno) => return neg_errno(errno),
                };
                let buf = match read_guest(&mut caller, buf_ptr, len) {
                    Ok(buf) => buf,
                    Err(errno) => return neg_errno(errno),
                };
                let mut addr = match read_guest(&mut caller, addr_ptr, addr_len) {
                    Ok(addr) => addr,
                    Err(errno) => return neg_errno(errno),
                };
                normalize_sockaddr_for_host(&mut addr, socket.host_domain);
                cvt_ssize(unsafe {
                    libc::sendto(
                        socket.raw_fd,
                        buf.as_ptr().cast::<libc::c_void>(),
                        buf.len(),
                        flags,
                        addr.as_ptr().cast::<libc::sockaddr>(),
                        addr.len() as libc::socklen_t,
                    )
                })
            }
            #[cfg(not(unix))]
            {
                let _ = (&mut caller, fd, buf_ptr, len, flags, addr_ptr, addr_len);
                neg_errno(errno::ENOSYS)
            }
        },
    )?;

    linker.func_wrap(
        MODULE_NAME,
        "recvfrom",
        |mut caller: Caller<'_, AppState>,
         fd: i32,
         buf_ptr: i32,
         len: i32,
         flags: i32,
         addr_ptr: i32,
         addr_len_ptr: i32|
         -> i32 {
            #[cfg(unix)]
            {
                let socket = match caller.data().sockets.get(fd) {
                    Ok(socket) => socket,
                    Err(errno) => return neg_errno(errno),
                };
                let len = match checked_len(len) {
                    Ok(len) => len,
                    Err(errno) => return neg_errno(errno),
                };
                let mut addr_len = match read_u32(&mut caller, addr_len_ptr) {
                    Ok(len) => len as libc::socklen_t,
                    Err(errno) => return neg_errno(errno),
                };
                let mut buf = vec![0_u8; len];
                let mut addr = vec![0_u8; addr_len as usize];
                let rc = unsafe {
                    libc::recvfrom(
                        socket.raw_fd,
                        buf.as_mut_ptr().cast::<libc::c_void>(),
                        buf.len(),
                        flags,
                        addr.as_mut_ptr().cast::<libc::sockaddr>(),
                        &mut addr_len,
                    )
                };
                if rc < 0 {
                    return neg_last_errno();
                }
                let rc = rc as usize;
                if let Err(errno) = write_guest(&mut caller, buf_ptr, &buf[..rc]) {
                    return neg_errno(errno);
                }
                denormalize_sockaddr_for_guest(&mut addr, socket.guest_domain);
                if let Err(errno) = write_guest(&mut caller, addr_ptr, &addr[..addr_len as usize]) {
                    return neg_errno(errno);
                }
                match write_u32(&mut caller, addr_len_ptr, addr_len as u32) {
                    Ok(()) => rc as i32,
                    Err(errno) => neg_errno(errno),
                }
            }
            #[cfg(not(unix))]
            {
                let _ = (&mut caller, fd, buf_ptr, len, flags, addr_ptr, addr_len_ptr);
                neg_errno(errno::ENOSYS)
            }
        },
    )?;

    linker.func_wrap(
        MODULE_NAME,
        "shutdown",
        |caller: Caller<'_, AppState>, fd: i32, how: i32| -> i32 {
            #[cfg(unix)]
            {
                let socket = match caller.data().sockets.get(fd) {
                    Ok(socket) => socket,
                    Err(errno) => return neg_errno(errno),
                };
                cvt_i32(unsafe { libc::shutdown(socket.raw_fd, how) })
            }
            #[cfg(not(unix))]
            {
                let _ = (caller, fd, how);
                neg_errno(errno::ENOSYS)
            }
        },
    )?;

    linker.func_wrap(
        MODULE_NAME,
        "close",
        |mut caller: Caller<'_, AppState>, fd: i32| -> i32 {
            #[cfg(unix)]
            {
                let socket = match caller.data_mut().sockets.remove(fd) {
                    Ok(socket) => socket,
                    Err(errno) => return neg_errno(errno),
                };
                cvt_i32(unsafe { libc::close(socket.raw_fd) })
            }
            #[cfg(not(unix))]
            {
                let _ = (&mut caller, fd);
                neg_errno(errno::ENOSYS)
            }
        },
    )?;

    linker.func_wrap(
        MODULE_NAME,
        "fcntl",
        |caller: Caller<'_, AppState>, fd: i32, cmd: i32, arg: i32| -> i32 {
            #[cfg(unix)]
            {
                let socket = match caller.data().sockets.get(fd) {
                    Ok(socket) => socket,
                    Err(errno) => return neg_errno(errno),
                };
                socket_fcntl(socket.raw_fd, cmd, arg)
            }
            #[cfg(not(unix))]
            {
                let _ = (caller, fd, cmd, arg);
                neg_errno(errno::ENOSYS)
            }
        },
    )?;

    linker.func_wrap(
        MODULE_NAME,
        "poll",
        |mut caller: Caller<'_, AppState>, fds_ptr: i32, nfds: i32, timeout: i32| -> i32 {
            #[cfg(unix)]
            {
                let nfds = match checked_poll_count(nfds) {
                    Ok(nfds) => nfds,
                    Err(errno) => return neg_errno(errno),
                };
                let bytes = match read_guest(&mut caller, fds_ptr, (nfds * 8) as i32) {
                    Ok(bytes) => bytes,
                    Err(errno) => return neg_errno(errno),
                };
                let mut host_fds = Vec::with_capacity(nfds);
                for chunk in bytes.chunks_exact(8) {
                    let guest_fd = i32::from_le_bytes(chunk[0..4].try_into().unwrap());
                    let events = i16::from_le_bytes(chunk[4..6].try_into().unwrap());
                    let raw_fd = if guest_fd < 0 {
                        guest_fd
                    } else {
                        match caller.data().sockets.get(guest_fd) {
                            Ok(socket) => socket.raw_fd,
                            Err(errno) => return neg_errno(errno),
                        }
                    };
                    host_fds.push(libc::pollfd {
                        fd: raw_fd,
                        events,
                        revents: 0,
                    });
                }
                let rc = unsafe { libc::poll(host_fds.as_mut_ptr(), host_fds.len() as _, timeout) };
                if rc < 0 {
                    return neg_last_errno();
                }
                for (index, pollfd) in host_fds.iter().enumerate() {
                    let revents = pollfd.revents.to_le_bytes();
                    if let Err(errno) =
                        write_guest(&mut caller, fds_ptr + (index as i32 * 8) + 6, &revents)
                    {
                        return neg_errno(errno);
                    }
                }
                rc
            }
            #[cfg(not(unix))]
            {
                let _ = (&mut caller, fds_ptr, nfds, timeout);
                neg_errno(errno::ENOSYS)
            }
        },
    )?;

    Ok(())
}

enum NameKind {
    Local,
    Peer,
}

fn sock_name(
    mut caller: Caller<'_, AppState>,
    fd: i32,
    addr_ptr: i32,
    addr_len_ptr: i32,
    kind: NameKind,
) -> i32 {
    #[cfg(unix)]
    {
        let socket = match caller.data().sockets.get(fd) {
            Ok(socket) => socket,
            Err(errno) => return neg_errno(errno),
        };
        let mut addr_len = match read_u32(&mut caller, addr_len_ptr) {
            Ok(len) => len as libc::socklen_t,
            Err(errno) => return neg_errno(errno),
        };
        let mut addr = vec![0_u8; addr_len as usize];
        let rc = unsafe {
            match kind {
                NameKind::Local => libc::getsockname(
                    socket.raw_fd,
                    addr.as_mut_ptr().cast::<libc::sockaddr>(),
                    &mut addr_len,
                ),
                NameKind::Peer => libc::getpeername(
                    socket.raw_fd,
                    addr.as_mut_ptr().cast::<libc::sockaddr>(),
                    &mut addr_len,
                ),
            }
        };
        if rc < 0 {
            return neg_last_errno();
        }
        denormalize_sockaddr_for_guest(&mut addr, socket.guest_domain);
        if let Err(errno) = write_guest(&mut caller, addr_ptr, &addr[..addr_len as usize]) {
            return neg_errno(errno);
        }
        match write_u32(&mut caller, addr_len_ptr, addr_len as u32) {
            Ok(()) => 0,
            Err(errno) => neg_errno(errno),
        }
    }
    #[cfg(not(unix))]
    {
        let _ = (&mut caller, fd, addr_ptr, addr_len_ptr, kind);
        neg_errno(errno::ENOSYS)
    }
}

fn checked_range(ptr: i32, len: i32, memory_len: usize) -> std::result::Result<Range<usize>, i32> {
    let ptr = usize::try_from(ptr).map_err(|_| errno::EFAULT)?;
    let len = checked_len(len)?;
    let end = ptr.checked_add(len).ok_or(errno::EFAULT)?;
    if end > memory_len {
        return Err(errno::EFAULT);
    }
    Ok(ptr..end)
}

fn checked_len(len: i32) -> std::result::Result<usize, i32> {
    let len = usize::try_from(len).map_err(|_| errno::EINVAL)?;
    if len > MAX_IO_LEN {
        return Err(errno::EINVAL);
    }
    Ok(len)
}

fn checked_poll_count(nfds: i32) -> std::result::Result<usize, i32> {
    let nfds = usize::try_from(nfds).map_err(|_| errno::EINVAL)?;
    if nfds > MAX_POLL_FDS {
        return Err(errno::EINVAL);
    }
    Ok(nfds)
}

#[cfg(unix)]
fn normalize_socket_type(ty: i32) -> std::result::Result<NormalizedSocketType, i32> {
    let flag_mask = host_socket_flag_mask() | WASI_ALT_SOCK_CLOEXEC | WASI_ALT_SOCK_NONBLOCK;
    let base = ty & !flag_mask;
    let flags = ty & flag_mask;

    let ty = match base {
        libc::SOCK_STREAM | WASI_ALT_SOCK_STREAM => libc::SOCK_STREAM,
        libc::SOCK_DGRAM | WASI_ALT_SOCK_DGRAM => libc::SOCK_DGRAM,
        _ => return Err(errno::EOPNOTSUPP),
    };

    Ok(NormalizedSocketType {
        ty,
        close_on_exec: has_host_sock_cloexec(flags) || flags & WASI_ALT_SOCK_CLOEXEC != 0,
        nonblocking: has_host_sock_nonblock(flags) || flags & WASI_ALT_SOCK_NONBLOCK != 0,
    })
}

#[cfg(unix)]
fn configure_socket_type_flags(
    raw_fd: RawFd,
    socket_type: NormalizedSocketType,
) -> std::result::Result<(), i32> {
    if socket_type.close_on_exec {
        let flags = unsafe { libc::fcntl(raw_fd, libc::F_GETFD) };
        if flags < 0 {
            return Err(last_errno());
        }
        if unsafe { libc::fcntl(raw_fd, libc::F_SETFD, flags | libc::FD_CLOEXEC) } < 0 {
            return Err(last_errno());
        }
    }

    if socket_type.nonblocking {
        let flags = unsafe { libc::fcntl(raw_fd, libc::F_GETFL) };
        if flags < 0 {
            return Err(last_errno());
        }
        if unsafe { libc::fcntl(raw_fd, libc::F_SETFL, flags | libc::O_NONBLOCK) } < 0 {
            return Err(last_errno());
        }
    }

    Ok(())
}

#[cfg(unix)]
fn normalize_socket_domain(domain: i32) -> i32 {
    match domain {
        WASI_ALT_AF_INET => libc::AF_INET,
        libc::AF_INET => libc::AF_INET,
        libc::AF_INET6 => libc::AF_INET6,
        _ => domain,
    }
}

#[cfg(unix)]
fn normalize_socket_level(level: i32) -> i32 {
    if is_guest_socket_level(level) {
        libc::SOL_SOCKET
    } else {
        level
    }
}

#[cfg(unix)]
fn is_guest_socket_level(level: i32) -> bool {
    level == 1 || level == WASI_SOL_SOCKET
}

#[cfg(unix)]
fn socket_fcntl(raw_fd: RawFd, cmd: i32, arg: i32) -> i32 {
    match cmd {
        WASI_F_GETFL => {
            let flags = unsafe { libc::fcntl(raw_fd, libc::F_GETFL) };
            if flags < 0 {
                return neg_last_errno();
            }
            if flags & libc::O_NONBLOCK != 0 {
                WASI_O_NONBLOCK
            } else {
                0
            }
        }
        WASI_F_SETFL => {
            let flags = unsafe { libc::fcntl(raw_fd, libc::F_GETFL) };
            if flags < 0 {
                return neg_last_errno();
            }
            let flags = if arg & WASI_O_NONBLOCK != 0 {
                flags | libc::O_NONBLOCK
            } else {
                flags & !libc::O_NONBLOCK
            };
            cvt_i32(unsafe { libc::fcntl(raw_fd, libc::F_SETFL, flags) })
        }
        _ => neg_errno(errno::EINVAL),
    }
}

#[cfg(any(target_os = "android", target_os = "linux"))]
fn host_socket_flag_mask() -> i32 {
    libc::SOCK_CLOEXEC | libc::SOCK_NONBLOCK
}

#[cfg(not(any(target_os = "android", target_os = "linux")))]
fn host_socket_flag_mask() -> i32 {
    0
}

#[cfg(any(target_os = "android", target_os = "linux"))]
fn has_host_sock_cloexec(flags: i32) -> bool {
    flags & libc::SOCK_CLOEXEC != 0
}

#[cfg(not(any(target_os = "android", target_os = "linux")))]
fn has_host_sock_cloexec(_flags: i32) -> bool {
    false
}

#[cfg(any(target_os = "android", target_os = "linux"))]
fn has_host_sock_nonblock(flags: i32) -> bool {
    flags & libc::SOCK_NONBLOCK != 0
}

#[cfg(not(any(target_os = "android", target_os = "linux")))]
fn has_host_sock_nonblock(_flags: i32) -> bool {
    false
}

#[cfg(unix)]
fn normalize_sockaddr_for_host(addr: &mut [u8], host_domain: i32) {
    write_sockaddr_family(addr, host_domain);
}

#[cfg(unix)]
fn denormalize_sockaddr_for_guest(addr: &mut [u8], guest_domain: i32) {
    write_sockaddr_family(addr, guest_domain);
}

#[cfg(unix)]
fn write_sockaddr_family(addr: &mut [u8], family: i32) {
    if addr.len() < 2 {
        return;
    }
    let Ok(family) = u16::try_from(family) else {
        return;
    };
    addr[..2].copy_from_slice(&family.to_ne_bytes());
}

fn read_guest(
    caller: &mut Caller<'_, AppState>,
    ptr: i32,
    len: i32,
) -> std::result::Result<Vec<u8>, i32> {
    let export = caller.get_export("memory").ok_or(errno::EFAULT)?;

    if let Some(mem) = export.clone().into_memory() {
        let data = mem.data(&mut *caller);
        let range = checked_range(ptr, len, data.len())?;
        return Ok(data[range].to_vec());
    }

    if let Some(mem) = export.into_shared_memory() {
        let data = mem.data();
        let range = checked_range(ptr, len, data.len())?;
        let mut bytes = Vec::with_capacity(range.len());
        for cell in &data[range] {
            bytes.push(unsafe { *cell.get() });
        }
        return Ok(bytes);
    }

    Err(errno::EFAULT)
}

fn write_guest(
    caller: &mut Caller<'_, AppState>,
    ptr: i32,
    bytes: &[u8],
) -> std::result::Result<(), i32> {
    let export = caller.get_export("memory").ok_or(errno::EFAULT)?;

    if let Some(mem) = export.clone().into_memory() {
        let data = mem.data_mut(&mut *caller);
        let range = checked_range(
            ptr,
            i32::try_from(bytes.len()).map_err(|_| errno::EINVAL)?,
            data.len(),
        )?;
        data[range].copy_from_slice(bytes);
        return Ok(());
    }

    if let Some(mem) = export.into_shared_memory() {
        let data = mem.data();
        let range = checked_range(
            ptr,
            i32::try_from(bytes.len()).map_err(|_| errno::EINVAL)?,
            data.len(),
        )?;
        for (cell, byte) in data[range].iter().zip(bytes) {
            unsafe {
                *cell.get() = *byte;
            }
        }
        return Ok(());
    }

    Err(errno::EFAULT)
}

fn read_u32(caller: &mut Caller<'_, AppState>, ptr: i32) -> std::result::Result<u32, i32> {
    let bytes = read_guest(caller, ptr, 4)?;
    Ok(u32::from_le_bytes(bytes.try_into().unwrap()))
}

fn write_u32(
    caller: &mut Caller<'_, AppState>,
    ptr: i32,
    value: u32,
) -> std::result::Result<(), i32> {
    write_guest(caller, ptr, &value.to_le_bytes())
}

#[cfg(unix)]
fn cvt_i32(rc: i32) -> i32 {
    if rc < 0 { neg_last_errno() } else { rc }
}

#[cfg(unix)]
fn cvt_ssize(rc: libc::ssize_t) -> i32 {
    if rc < 0 {
        return neg_last_errno();
    }
    i32::try_from(rc).unwrap_or_else(|_| neg_errno(errno::EOVERFLOW))
}

#[cfg(unix)]
fn neg_last_errno() -> i32 {
    neg_errno(last_errno())
}

#[cfg(unix)]
fn last_errno() -> i32 {
    match std::io::Error::last_os_error().raw_os_error() {
        Some(code) => errno::from_host_errno(code),
        None => errno::EIO,
    }
}

fn neg_errno(errno: i32) -> i32 {
    -errno
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(unix)]
    #[test]
    fn translates_wasi_sol_socket_to_the_host_value() {
        assert_eq!(normalize_socket_level(WASI_SOL_SOCKET), libc::SOL_SOCKET);
    }
}
