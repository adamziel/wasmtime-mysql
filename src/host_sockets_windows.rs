use std::sync::OnceLock;

use windows_sys::Win32::Networking::WinSock as ws;

use crate::guest_errno as errno;

use super::*;

const WASI_ALT_AF_INET: i32 = 1;
const WASI_ALT_SOCK_DGRAM: i32 = 5;
const WASI_ALT_SOCK_STREAM: i32 = 6;
const WASI_ALT_SOCK_CLOEXEC: i32 = 0x2000;
const WASI_ALT_SOCK_NONBLOCK: i32 = 0x4000;
const GUEST_AF_INET: i32 = 2;
const GUEST_AF_INET6: i32 = 10;
const GUEST_F_GETFL: i32 = 3;
const GUEST_F_SETFL: i32 = 4;
const GUEST_POLLIN: i16 = 0x0001;
const GUEST_POLLPRI: i16 = 0x0002;
const GUEST_POLLOUT: i16 = 0x0004;
const GUEST_POLLERR: i16 = 0x0008;
const GUEST_POLLHUP: i16 = 0x0010;
const GUEST_POLLNVAL: i16 = 0x0020;
const GUEST_SO_REUSEADDR: i32 = 2;
const GUEST_SO_ERROR: i32 = 4;
const GUEST_SO_KEEPALIVE: i32 = 9;
const GUEST_SO_RCVTIMEO: i32 = 20;
const GUEST_SO_SNDTIMEO: i32 = 21;
const GUEST_IP_TOS: i32 = 1;
const GUEST_IPV6_V6ONLY: i32 = 26;

#[derive(Clone, Copy)]
struct NormalizedSocketType {
    ty: ws::WINSOCK_SOCKET_TYPE,
    nonblocking: bool,
}

pub(crate) fn add_to_linker(linker: &mut Linker<AppState>) -> Result<()> {
    linker.func_wrap(
        MODULE_NAME,
        "socket",
        |mut caller: Caller<'_, AppState>, domain: i32, ty: i32, protocol: i32| -> i32 {
            if !caller.data().network_allowed {
                return neg_errno(errno::ENETDOWN);
            }
            if let Err(errno) = ensure_winsock() {
                return neg_errno(errno);
            }
            let socket_type = match normalize_socket_type(ty) {
                Ok(socket_type) => socket_type,
                Err(errno) => return neg_errno(errno),
            };
            let host_domain = normalize_socket_domain(domain);
            let raw_socket = unsafe { ws::socket(host_domain, socket_type.ty, protocol) };
            if raw_socket == ws::INVALID_SOCKET {
                return neg_last_socket_errno();
            }

            let status_flags = if socket_type.nonblocking {
                let mut enabled = 1_u32;
                if unsafe { ws::ioctlsocket(raw_socket, ws::FIONBIO, &mut enabled) }
                    == ws::SOCKET_ERROR
                {
                    unsafe {
                        ws::closesocket(raw_socket);
                    }
                    return neg_last_socket_errno();
                }
                WASI_ALT_SOCK_NONBLOCK
            } else {
                0
            };

            caller
                .data_mut()
                .sockets
                .insert(raw_socket, domain, host_domain, status_flags)
        },
    )?;

    linker.func_wrap(
        MODULE_NAME,
        "bind",
        |mut caller: Caller<'_, AppState>, fd: i32, addr_ptr: i32, addr_len: i32| -> i32 {
            let socket = match caller.data().sockets.get(fd) {
                Ok(socket) => socket,
                Err(errno) => return neg_errno(errno),
            };
            let mut addr = match read_guest(&mut caller, addr_ptr, addr_len) {
                Ok(addr) => addr,
                Err(errno) => return neg_errno(errno),
            };
            normalize_sockaddr_for_host(&mut addr, socket.host_domain);
            let rc = unsafe {
                ws::bind(
                    socket.raw_socket,
                    addr.as_ptr().cast::<ws::SOCKADDR>(),
                    addr.len() as i32,
                )
            };
            cvt_socket_i32(rc)
        },
    )?;

    linker.func_wrap(
        MODULE_NAME,
        "listen",
        |caller: Caller<'_, AppState>, fd: i32, backlog: i32| -> i32 {
            let socket = match caller.data().sockets.get(fd) {
                Ok(socket) => socket,
                Err(errno) => return neg_errno(errno),
            };
            cvt_socket_i32(unsafe { ws::listen(socket.raw_socket, backlog) })
        },
    )?;

    linker.func_wrap(
        MODULE_NAME,
        "accept",
        |mut caller: Caller<'_, AppState>, fd: i32, addr_ptr: i32, addr_len_ptr: i32| -> i32 {
            let socket = match caller.data().sockets.get(fd) {
                Ok(socket) => socket,
                Err(errno) => return neg_errno(errno),
            };
            let requested_len =
                match read_u32(&mut caller, addr_len_ptr).and_then(checked_sockaddr_len) {
                    Ok(len) => len,
                    Err(errno) => return neg_errno(errno),
                };
            let mut addr_len = requested_len as i32;
            let mut addr = vec![0_u8; requested_len];
            let accepted = unsafe {
                ws::accept(
                    socket.raw_socket,
                    addr.as_mut_ptr().cast::<ws::SOCKADDR>(),
                    &mut addr_len,
                )
            };
            if accepted == ws::INVALID_SOCKET {
                return neg_last_socket_errno();
            }
            if addr_len < 0 || addr_len as usize > addr.len() {
                unsafe {
                    ws::closesocket(accepted);
                }
                return neg_errno(errno::EIO);
            }
            denormalize_sockaddr_for_guest(&mut addr, socket.guest_domain);
            if let Err(errno) = write_guest(&mut caller, addr_ptr, &addr[..addr_len as usize]) {
                unsafe {
                    ws::closesocket(accepted);
                }
                return neg_errno(errno);
            }
            if let Err(errno) = write_guest_u32(&mut caller, addr_len_ptr, addr_len as u32) {
                unsafe {
                    ws::closesocket(accepted);
                }
                return neg_errno(errno);
            }
            caller.data_mut().sockets.insert(
                accepted,
                socket.guest_domain,
                socket.host_domain,
                socket.status_flags,
            )
        },
    )?;

    linker.func_wrap(
        MODULE_NAME,
        "connect",
        |mut caller: Caller<'_, AppState>, fd: i32, addr_ptr: i32, addr_len: i32| -> i32 {
            let socket = match caller.data().sockets.get(fd) {
                Ok(socket) => socket,
                Err(errno) => return neg_errno(errno),
            };
            let mut addr = match read_guest(&mut caller, addr_ptr, addr_len) {
                Ok(addr) => addr,
                Err(errno) => return neg_errno(errno),
            };
            normalize_sockaddr_for_host(&mut addr, socket.host_domain);
            cvt_socket_i32(unsafe {
                ws::connect(
                    socket.raw_socket,
                    addr.as_ptr().cast::<ws::SOCKADDR>(),
                    addr.len() as i32,
                )
            })
        },
    )?;

    linker.func_wrap(
        MODULE_NAME,
        "getsockname",
        |caller: Caller<'_, AppState>, fd: i32, addr_ptr: i32, addr_len_ptr: i32| -> i32 {
            socket_name(caller, fd, addr_ptr, addr_len_ptr, NameKind::Local)
        },
    )?;

    linker.func_wrap(
        MODULE_NAME,
        "getpeername",
        |caller: Caller<'_, AppState>, fd: i32, addr_ptr: i32, addr_len_ptr: i32| -> i32 {
            socket_name(caller, fd, addr_ptr, addr_len_ptr, NameKind::Peer)
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
            let socket = match caller.data().sockets.get(fd) {
                Ok(socket) => socket,
                Err(errno) => return neg_errno(errno),
            };
            let optval = match read_guest(&mut caller, optval_ptr, optlen) {
                Ok(optval) => optval,
                Err(errno) => return neg_errno(errno),
            };
            let (host_level, host_optname) = normalize_socket_option(level, optname);
            let optval = if is_guest_timeout(level, optname) {
                guest_timeout_to_windows(&optval)
            } else {
                optval
            };
            cvt_socket_i32(unsafe {
                ws::setsockopt(
                    socket.raw_socket,
                    host_level,
                    host_optname,
                    optval.as_ptr(),
                    optval.len() as i32,
                )
            })
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
            let socket = match caller.data().sockets.get(fd) {
                Ok(socket) => socket,
                Err(errno) => return neg_errno(errno),
            };
            let requested_len =
                match read_u32(&mut caller, optlen_ptr).and_then(checked_sockaddr_len) {
                    Ok(len) => len,
                    Err(errno) => return neg_errno(errno),
                };
            let (host_level, host_optname) = normalize_socket_option(level, optname);
            let timeout = is_guest_timeout(level, optname);
            let mut optval = vec![0_u8; if timeout { 4 } else { requested_len }];
            let mut optlen = optval.len() as i32;
            let rc = unsafe {
                ws::getsockopt(
                    socket.raw_socket,
                    host_level,
                    host_optname,
                    optval.as_mut_ptr(),
                    &mut optlen,
                )
            };
            if rc == ws::SOCKET_ERROR {
                return neg_last_socket_errno();
            }
            if optlen < 0 || optlen as usize > optval.len() {
                return neg_errno(errno::EIO);
            }
            let mut result = optval[..optlen as usize].to_vec();
            if timeout {
                result = windows_timeout_to_guest(&result, requested_len);
            } else if level == 1 && optname == GUEST_SO_ERROR && result.len() == 4 {
                let error = i32::from_ne_bytes(result[..4].try_into().unwrap());
                result.copy_from_slice(&socket_errno(error).to_ne_bytes());
            }
            if result.len() > requested_len {
                result.truncate(requested_len);
            }
            if let Err(errno) = write_guest(&mut caller, optval_ptr, &result) {
                return neg_errno(errno);
            }
            match write_guest_u32(&mut caller, optlen_ptr, result.len() as u32) {
                Ok(()) => 0,
                Err(errno) => neg_errno(errno),
            }
        },
    )?;

    linker.func_wrap(
        MODULE_NAME,
        "send",
        |mut caller: Caller<'_, AppState>, fd: i32, buf_ptr: i32, len: i32, flags: i32| -> i32 {
            let socket = match caller.data().sockets.get(fd) {
                Ok(socket) => socket,
                Err(errno) => return neg_errno(errno),
            };
            let buf = match read_guest(&mut caller, buf_ptr, len) {
                Ok(buf) => buf,
                Err(errno) => return neg_errno(errno),
            };
            cvt_socket_i32(unsafe {
                ws::send(
                    socket.raw_socket,
                    buf.as_ptr(),
                    buf.len() as i32,
                    normalize_message_flags(flags),
                )
            })
        },
    )?;

    linker.func_wrap(
        MODULE_NAME,
        "recv",
        |mut caller: Caller<'_, AppState>, fd: i32, buf_ptr: i32, len: i32, flags: i32| -> i32 {
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
                ws::recv(
                    socket.raw_socket,
                    buf.as_mut_ptr(),
                    buf.len() as i32,
                    normalize_message_flags(flags),
                )
            };
            if rc == ws::SOCKET_ERROR {
                return neg_last_socket_errno();
            }
            match write_guest(&mut caller, buf_ptr, &buf[..rc as usize]) {
                Ok(()) => rc,
                Err(errno) => neg_errno(errno),
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
            cvt_socket_i32(unsafe {
                ws::sendto(
                    socket.raw_socket,
                    buf.as_ptr(),
                    buf.len() as i32,
                    normalize_message_flags(flags),
                    addr.as_ptr().cast::<ws::SOCKADDR>(),
                    addr.len() as i32,
                )
            })
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
            let socket = match caller.data().sockets.get(fd) {
                Ok(socket) => socket,
                Err(errno) => return neg_errno(errno),
            };
            let len = match checked_len(len) {
                Ok(len) => len,
                Err(errno) => return neg_errno(errno),
            };
            let requested_addr_len =
                match read_u32(&mut caller, addr_len_ptr).and_then(checked_sockaddr_len) {
                    Ok(len) => len,
                    Err(errno) => return neg_errno(errno),
                };
            let mut buf = vec![0_u8; len];
            let mut addr = vec![0_u8; requested_addr_len];
            let mut addr_len = requested_addr_len as i32;
            let rc = unsafe {
                ws::recvfrom(
                    socket.raw_socket,
                    buf.as_mut_ptr(),
                    buf.len() as i32,
                    normalize_message_flags(flags),
                    addr.as_mut_ptr().cast::<ws::SOCKADDR>(),
                    &mut addr_len,
                )
            };
            if rc == ws::SOCKET_ERROR {
                return neg_last_socket_errno();
            }
            if addr_len < 0 || addr_len as usize > addr.len() {
                return neg_errno(errno::EIO);
            }
            if let Err(errno) = write_guest(&mut caller, buf_ptr, &buf[..rc as usize]) {
                return neg_errno(errno);
            }
            denormalize_sockaddr_for_guest(&mut addr, socket.guest_domain);
            if let Err(errno) = write_guest(&mut caller, addr_ptr, &addr[..addr_len as usize]) {
                return neg_errno(errno);
            }
            match write_guest_u32(&mut caller, addr_len_ptr, addr_len as u32) {
                Ok(()) => rc,
                Err(errno) => neg_errno(errno),
            }
        },
    )?;

    linker.func_wrap(
        MODULE_NAME,
        "shutdown",
        |caller: Caller<'_, AppState>, fd: i32, how: i32| -> i32 {
            let socket = match caller.data().sockets.get(fd) {
                Ok(socket) => socket,
                Err(errno) => return neg_errno(errno),
            };
            cvt_socket_i32(unsafe { ws::shutdown(socket.raw_socket, how) })
        },
    )?;

    linker.func_wrap(
        MODULE_NAME,
        "close",
        |mut caller: Caller<'_, AppState>, fd: i32| -> i32 {
            let socket = match caller.data_mut().sockets.remove(fd) {
                Ok(socket) => socket,
                Err(errno) => return neg_errno(errno),
            };
            cvt_socket_i32(unsafe { ws::closesocket(socket.raw_socket) })
        },
    )?;

    linker.func_wrap(
        MODULE_NAME,
        "fcntl",
        |caller: Caller<'_, AppState>, fd: i32, cmd: i32, arg: i32| -> i32 {
            let socket = match caller.data().sockets.get(fd) {
                Ok(socket) => socket,
                Err(errno) => return neg_errno(errno),
            };
            match cmd {
                GUEST_F_GETFL => socket.status_flags,
                GUEST_F_SETFL => {
                    let mut enabled = u32::from(arg != 0);
                    if unsafe { ws::ioctlsocket(socket.raw_socket, ws::FIONBIO, &mut enabled) }
                        == ws::SOCKET_ERROR
                    {
                        return neg_last_socket_errno();
                    }
                    match caller.data().sockets.set_status_flags(fd, arg) {
                        Ok(()) => 0,
                        Err(errno) => neg_errno(errno),
                    }
                }
                _ => neg_errno(errno::EINVAL),
            }
        },
    )?;

    linker.func_wrap(
        MODULE_NAME,
        "poll",
        |mut caller: Caller<'_, AppState>, fds_ptr: i32, nfds: i32, timeout: i32| -> i32 {
            let nfds = match checked_poll_count(nfds) {
                Ok(nfds) => nfds,
                Err(errno) => return neg_errno(errno),
            };
            let bytes = match read_guest(&mut caller, fds_ptr, (nfds * 8) as i32) {
                Ok(bytes) => bytes,
                Err(errno) => return neg_errno(errno),
            };
            let mut ignored = Vec::with_capacity(nfds);
            let mut host_fds = Vec::with_capacity(nfds);
            for chunk in bytes.chunks_exact(8) {
                let guest_fd = i32::from_le_bytes(chunk[0..4].try_into().unwrap());
                let events = i16::from_le_bytes(chunk[4..6].try_into().unwrap());
                if guest_fd < 0 {
                    ignored.push(true);
                    host_fds.push(ws::WSAPOLLFD {
                        fd: ws::INVALID_SOCKET,
                        events: 0,
                        revents: 0,
                    });
                    continue;
                }
                let socket = match caller.data().sockets.get(guest_fd) {
                    Ok(socket) => socket,
                    Err(errno) => return neg_errno(errno),
                };
                ignored.push(false);
                host_fds.push(ws::WSAPOLLFD {
                    fd: socket.raw_socket,
                    events: guest_poll_events_to_windows(events),
                    revents: 0,
                });
            }
            let rc = unsafe { ws::WSAPoll(host_fds.as_mut_ptr(), host_fds.len() as u32, timeout) };
            if rc == ws::SOCKET_ERROR {
                return neg_last_socket_errno();
            }
            for (index, pollfd) in host_fds.iter().enumerate() {
                let revents = if ignored[index] {
                    0
                } else {
                    windows_poll_events_to_guest(pollfd.revents)
                };
                if let Err(errno) = write_guest(
                    &mut caller,
                    fds_ptr + (index as i32 * 8) + 6,
                    &revents.to_le_bytes(),
                ) {
                    return neg_errno(errno);
                }
            }
            rc
        },
    )?;

    Ok(())
}

fn socket_name(
    mut caller: Caller<'_, AppState>,
    fd: i32,
    addr_ptr: i32,
    addr_len_ptr: i32,
    kind: NameKind,
) -> i32 {
    let socket = match caller.data().sockets.get(fd) {
        Ok(socket) => socket,
        Err(errno) => return neg_errno(errno),
    };
    let requested_len = match read_u32(&mut caller, addr_len_ptr).and_then(checked_sockaddr_len) {
        Ok(len) => len,
        Err(errno) => return neg_errno(errno),
    };
    let mut addr_len = requested_len as i32;
    let mut addr = vec![0_u8; requested_len];
    let rc = unsafe {
        match kind {
            NameKind::Local => ws::getsockname(
                socket.raw_socket,
                addr.as_mut_ptr().cast::<ws::SOCKADDR>(),
                &mut addr_len,
            ),
            NameKind::Peer => ws::getpeername(
                socket.raw_socket,
                addr.as_mut_ptr().cast::<ws::SOCKADDR>(),
                &mut addr_len,
            ),
        }
    };
    if rc == ws::SOCKET_ERROR {
        return neg_last_socket_errno();
    }
    if addr_len < 0 || addr_len as usize > addr.len() {
        return neg_errno(errno::EIO);
    }
    denormalize_sockaddr_for_guest(&mut addr, socket.guest_domain);
    if let Err(errno) = write_guest(&mut caller, addr_ptr, &addr[..addr_len as usize]) {
        return neg_errno(errno);
    }
    match write_guest_u32(&mut caller, addr_len_ptr, addr_len as u32) {
        Ok(()) => 0,
        Err(errno) => neg_errno(errno),
    }
}

fn ensure_winsock() -> std::result::Result<(), i32> {
    static WINSOCK: OnceLock<std::result::Result<(), i32>> = OnceLock::new();

    match *WINSOCK.get_or_init(|| {
        let mut data = ws::WSADATA::default();
        let rc = unsafe { ws::WSAStartup(0x0202, &mut data) };
        if rc == 0 {
            Ok(())
        } else {
            Err(socket_errno(rc))
        }
    }) {
        Ok(()) => Ok(()),
        Err(errno) => Err(errno),
    }
}

fn normalize_socket_type(ty: i32) -> std::result::Result<NormalizedSocketType, i32> {
    // The WASI build has used both POSIX and wasi-libc socket flag encodings.
    let flag_mask = 0x0000_0800 | 0x0008_0000 | WASI_ALT_SOCK_CLOEXEC | WASI_ALT_SOCK_NONBLOCK;
    let base = ty & !flag_mask;
    let flags = ty & flag_mask;
    let ty = match base {
        ws::SOCK_STREAM | WASI_ALT_SOCK_STREAM => ws::SOCK_STREAM,
        ws::SOCK_DGRAM | WASI_ALT_SOCK_DGRAM => ws::SOCK_DGRAM,
        _ => return Err(errno::ENOTSUP),
    };
    Ok(NormalizedSocketType {
        ty,
        nonblocking: flags & (0x0000_0800 | WASI_ALT_SOCK_NONBLOCK) != 0,
    })
}

fn normalize_socket_domain(domain: i32) -> i32 {
    match domain {
        WASI_ALT_AF_INET | GUEST_AF_INET => ws::AF_INET as i32,
        GUEST_AF_INET6 | 23 => ws::AF_INET6 as i32,
        _ => domain,
    }
}

fn normalize_sockaddr_for_host(addr: &mut [u8], host_domain: i32) {
    write_sockaddr_family(addr, host_domain);
}

fn denormalize_sockaddr_for_guest(addr: &mut [u8], guest_domain: i32) {
    write_sockaddr_family(addr, guest_domain);
}

fn write_sockaddr_family(addr: &mut [u8], family: i32) {
    if addr.len() < 2 {
        return;
    }
    let Ok(family) = u16::try_from(family) else {
        return;
    };
    addr[..2].copy_from_slice(&family.to_ne_bytes());
}

fn normalize_socket_option(level: i32, optname: i32) -> (i32, i32) {
    match level {
        1 => {
            let optname = match optname {
                GUEST_SO_REUSEADDR => ws::SO_REUSEADDR,
                GUEST_SO_ERROR => ws::SO_ERROR,
                GUEST_SO_KEEPALIVE => ws::SO_KEEPALIVE,
                GUEST_SO_RCVTIMEO => ws::SO_RCVTIMEO,
                GUEST_SO_SNDTIMEO => ws::SO_SNDTIMEO,
                other => other,
            };
            (ws::SOL_SOCKET, optname)
        }
        0 => {
            let optname = if optname == GUEST_IP_TOS {
                ws::IP_TOS
            } else {
                optname
            };
            (0, optname)
        }
        41 => {
            let optname = if optname == GUEST_IPV6_V6ONLY {
                ws::IPV6_V6ONLY
            } else {
                optname
            };
            (41, optname)
        }
        _ => (level, optname),
    }
}

fn is_guest_timeout(level: i32, optname: i32) -> bool {
    level == 1 && matches!(optname, GUEST_SO_RCVTIMEO | GUEST_SO_SNDTIMEO)
}

fn guest_timeout_to_windows(guest: &[u8]) -> Vec<u8> {
    if guest.len() < 16 {
        return guest.to_vec();
    }
    let seconds = i64::from_le_bytes(guest[0..8].try_into().unwrap());
    let micros = i64::from_le_bytes(guest[8..16].try_into().unwrap());
    let millis = seconds
        .saturating_mul(1_000)
        .saturating_add(micros.saturating_add(999).div_euclid(1_000))
        .clamp(0, i64::from(u32::MAX)) as u32;
    millis.to_ne_bytes().to_vec()
}

fn windows_timeout_to_guest(windows: &[u8], requested_len: usize) -> Vec<u8> {
    if requested_len < 16 || windows.len() < 4 {
        return windows.to_vec();
    }
    let millis = u32::from_ne_bytes(windows[0..4].try_into().unwrap()) as i64;
    let mut guest = Vec::with_capacity(16);
    guest.extend_from_slice(&(millis / 1_000).to_le_bytes());
    guest.extend_from_slice(&((millis % 1_000) * 1_000).to_le_bytes());
    guest
}

fn normalize_message_flags(flags: i32) -> i32 {
    // MSG_PEEK is shared by the guest ABI and WinSock. MSG_NOSIGNAL and
    // MSG_DONTWAIT are either unnecessary or handled by socket mode on Windows.
    flags & 0x0002
}

fn guest_poll_events_to_windows(events: i16) -> i16 {
    let mut normalized = 0;
    if events & GUEST_POLLIN != 0 {
        normalized |= ws::POLLIN;
    }
    if events & GUEST_POLLPRI != 0 {
        normalized |= ws::POLLPRI;
    }
    if events & GUEST_POLLOUT != 0 {
        normalized |= ws::POLLOUT;
    }
    normalized
}

fn windows_poll_events_to_guest(events: i16) -> i16 {
    let mut normalized = 0;
    if events & ws::POLLIN != 0 {
        normalized |= GUEST_POLLIN;
    }
    if events & ws::POLLPRI != 0 {
        normalized |= GUEST_POLLPRI;
    }
    if events & ws::POLLOUT != 0 {
        normalized |= GUEST_POLLOUT;
    }
    if events & ws::POLLERR != 0 {
        normalized |= GUEST_POLLERR;
    }
    if events & ws::POLLHUP != 0 {
        normalized |= GUEST_POLLHUP;
    }
    if events & ws::POLLNVAL != 0 {
        normalized |= GUEST_POLLNVAL;
    }
    normalized
}

fn checked_sockaddr_len(value: u32) -> std::result::Result<usize, i32> {
    let value = i32::try_from(value).map_err(|_| errno::EINVAL)?;
    checked_len(value)
}

fn write_guest_u32(
    caller: &mut Caller<'_, AppState>,
    ptr: i32,
    value: u32,
) -> std::result::Result<(), i32> {
    write_guest(caller, ptr, &value.to_le_bytes())
}

fn cvt_socket_i32(rc: i32) -> i32 {
    if rc == ws::SOCKET_ERROR {
        neg_last_socket_errno()
    } else {
        rc
    }
}

fn neg_last_socket_errno() -> i32 {
    neg_errno(socket_errno(unsafe { ws::WSAGetLastError() }))
}

fn socket_errno(code: i32) -> i32 {
    match code {
        0 => 0,
        ws::WSAEINTR => errno::EINTR,
        ws::WSAEBADF => errno::EBADF,
        ws::WSAEACCES => errno::EACCES,
        ws::WSAEFAULT => errno::EFAULT,
        ws::WSAEINVAL => errno::EINVAL,
        ws::WSAEMFILE => errno::EMFILE,
        ws::WSAEWOULDBLOCK => errno::EAGAIN,
        ws::WSAEINPROGRESS => errno::EINPROGRESS,
        ws::WSAEALREADY => errno::EALREADY,
        ws::WSAENOTSOCK => errno::ENOTSOCK,
        ws::WSAEDESTADDRREQ => errno::EDESTADDRREQ,
        ws::WSAEMSGSIZE => errno::EMSGSIZE,
        ws::WSAEPROTOTYPE => errno::EPROTOTYPE,
        ws::WSAENOPROTOOPT => errno::ENOPROTOOPT,
        ws::WSAEPROTONOSUPPORT => errno::EPROTONOSUPPORT,
        ws::WSAESOCKTNOSUPPORT => errno::ENOTSUP,
        ws::WSAEOPNOTSUPP => errno::ENOTSUP,
        ws::WSAEPFNOSUPPORT => errno::EAFNOSUPPORT,
        ws::WSAEAFNOSUPPORT => errno::EAFNOSUPPORT,
        ws::WSAEADDRINUSE => errno::EADDRINUSE,
        ws::WSAEADDRNOTAVAIL => errno::EADDRNOTAVAIL,
        ws::WSAENETDOWN => errno::ENETDOWN,
        ws::WSAENETUNREACH => errno::ENETUNREACH,
        ws::WSAENETRESET => errno::ENETRESET,
        ws::WSAECONNABORTED => errno::ECONNABORTED,
        ws::WSAECONNRESET => errno::ECONNRESET,
        ws::WSAENOBUFS => errno::ENOBUFS,
        ws::WSAEISCONN => errno::EISCONN,
        ws::WSAENOTCONN => errno::ENOTCONN,
        ws::WSAESHUTDOWN => errno::ENOTCONN,
        ws::WSAETIMEDOUT => errno::ETIMEDOUT,
        ws::WSAECONNREFUSED => errno::ECONNREFUSED,
        ws::WSAEHOSTDOWN => errno::EHOSTUNREACH,
        ws::WSAEHOSTUNREACH => errno::EHOSTUNREACH,
        _ => errno::EIO,
    }
}
