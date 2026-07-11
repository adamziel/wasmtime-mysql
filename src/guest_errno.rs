//! Errno values exposed by the wasm32-wasip1-threads guest ABI.
//!
//! Host errno numbers are platform-specific. The guest is compiled with
//! wasi-libc, whose values are the WASI Preview 1 errno enumeration.

pub(crate) const E2BIG: i32 = 1;
pub(crate) const EACCES: i32 = 2;
pub(crate) const EADDRINUSE: i32 = 3;
pub(crate) const EADDRNOTAVAIL: i32 = 4;
pub(crate) const EAFNOSUPPORT: i32 = 5;
pub(crate) const EAGAIN: i32 = 6;
pub(crate) const EALREADY: i32 = 7;
pub(crate) const EBADF: i32 = 8;
pub(crate) const EBUSY: i32 = 10;
pub(crate) const ECONNABORTED: i32 = 13;
pub(crate) const ECONNREFUSED: i32 = 14;
pub(crate) const ECONNRESET: i32 = 15;
pub(crate) const EDESTADDRREQ: i32 = 17;
pub(crate) const EEXIST: i32 = 20;
pub(crate) const EFAULT: i32 = 21;
pub(crate) const EFBIG: i32 = 22;
pub(crate) const EHOSTUNREACH: i32 = 23;
pub(crate) const EINPROGRESS: i32 = 26;
pub(crate) const EINTR: i32 = 27;
pub(crate) const EINVAL: i32 = 28;
pub(crate) const EIO: i32 = 29;
pub(crate) const EISCONN: i32 = 30;
pub(crate) const EISDIR: i32 = 31;
pub(crate) const ELOOP: i32 = 32;
pub(crate) const EMFILE: i32 = 33;
pub(crate) const EMSGSIZE: i32 = 35;
pub(crate) const ENAMETOOLONG: i32 = 37;
pub(crate) const ENETDOWN: i32 = 38;
pub(crate) const ENETRESET: i32 = 39;
pub(crate) const ENETUNREACH: i32 = 40;
pub(crate) const ENFILE: i32 = 41;
pub(crate) const ENOBUFS: i32 = 42;
pub(crate) const ENODEV: i32 = 43;
pub(crate) const ENOENT: i32 = 44;
pub(crate) const ENOMEM: i32 = 48;
pub(crate) const ENOPROTOOPT: i32 = 50;
pub(crate) const ENOSPC: i32 = 51;
pub(crate) const ENOSYS: i32 = 52;
pub(crate) const ENOTCONN: i32 = 53;
pub(crate) const ENOTDIR: i32 = 54;
pub(crate) const ENOTEMPTY: i32 = 55;
pub(crate) const ENOTSOCK: i32 = 57;
pub(crate) const ENOTSUP: i32 = 58;
pub(crate) const EOPNOTSUPP: i32 = ENOTSUP;
pub(crate) const EOVERFLOW: i32 = 61;
pub(crate) const EPERM: i32 = 63;
pub(crate) const EPIPE: i32 = 64;
pub(crate) const EPROTONOSUPPORT: i32 = 66;
pub(crate) const EPROTOTYPE: i32 = 67;
pub(crate) const ERANGE: i32 = 68;
pub(crate) const EROFS: i32 = 69;
pub(crate) const ESPIPE: i32 = 70;
pub(crate) const ETIMEDOUT: i32 = 73;
pub(crate) const ENOTCAPABLE: i32 = 76;

#[cfg(unix)]
pub(crate) fn from_host_errno(code: i32) -> i32 {
    if code == libc::E2BIG {
        E2BIG
    } else if code == libc::EACCES {
        EACCES
    } else if code == libc::EADDRINUSE {
        EADDRINUSE
    } else if code == libc::EADDRNOTAVAIL {
        EADDRNOTAVAIL
    } else if code == libc::EAFNOSUPPORT {
        EAFNOSUPPORT
    } else if code == libc::EAGAIN || code == libc::EWOULDBLOCK {
        EAGAIN
    } else if code == libc::EALREADY {
        EALREADY
    } else if code == libc::EBADF {
        EBADF
    } else if code == libc::EBUSY {
        EBUSY
    } else if code == libc::ECONNABORTED {
        ECONNABORTED
    } else if code == libc::ECONNREFUSED {
        ECONNREFUSED
    } else if code == libc::ECONNRESET {
        ECONNRESET
    } else if code == libc::EDESTADDRREQ {
        EDESTADDRREQ
    } else if code == libc::EEXIST {
        EEXIST
    } else if code == libc::EFAULT {
        EFAULT
    } else if code == libc::EFBIG {
        EFBIG
    } else if code == libc::EHOSTUNREACH {
        EHOSTUNREACH
    } else if code == libc::EINPROGRESS {
        EINPROGRESS
    } else if code == libc::EINTR {
        EINTR
    } else if code == libc::EINVAL {
        EINVAL
    } else if code == libc::EISCONN {
        EISCONN
    } else if code == libc::EISDIR {
        EISDIR
    } else if code == libc::ELOOP {
        ELOOP
    } else if code == libc::EMFILE {
        EMFILE
    } else if code == libc::EMSGSIZE {
        EMSGSIZE
    } else if code == libc::ENAMETOOLONG {
        ENAMETOOLONG
    } else if code == libc::ENETDOWN {
        ENETDOWN
    } else if code == libc::ENETRESET {
        ENETRESET
    } else if code == libc::ENETUNREACH {
        ENETUNREACH
    } else if code == libc::ENFILE {
        ENFILE
    } else if code == libc::ENOBUFS {
        ENOBUFS
    } else if code == libc::ENODEV {
        ENODEV
    } else if code == libc::ENOENT {
        ENOENT
    } else if code == libc::ENOMEM {
        ENOMEM
    } else if code == libc::ENOPROTOOPT {
        ENOPROTOOPT
    } else if code == libc::ENOSPC {
        ENOSPC
    } else if code == libc::ENOSYS {
        ENOSYS
    } else if code == libc::ENOTCONN {
        ENOTCONN
    } else if code == libc::ENOTDIR {
        ENOTDIR
    } else if code == libc::ENOTEMPTY {
        ENOTEMPTY
    } else if code == libc::ENOTSOCK {
        ENOTSOCK
    } else if code == libc::EOPNOTSUPP || code == libc::ENOTSUP {
        ENOTSUP
    } else if code == libc::EOVERFLOW {
        EOVERFLOW
    } else if code == libc::EPERM {
        EPERM
    } else if code == libc::EPIPE {
        EPIPE
    } else if code == libc::EPROTONOSUPPORT {
        EPROTONOSUPPORT
    } else if code == libc::EPROTOTYPE {
        EPROTOTYPE
    } else if code == libc::ERANGE {
        ERANGE
    } else if code == libc::EROFS {
        EROFS
    } else if code == libc::ESPIPE {
        ESPIPE
    } else if code == libc::ETIMEDOUT {
        ETIMEDOUT
    } else {
        EIO
    }
}

#[cfg(windows)]
pub(crate) fn from_windows_error(code: i32) -> i32 {
    match code {
        2 | 3 => ENOENT,
        5 => EACCES,
        6 => EBADF,
        8 | 14 => ENOMEM,
        19 => EROFS,
        32 => EBUSY,
        33 => EAGAIN,
        80 | 183 => EEXIST,
        87 => EINVAL,
        112 => ENOSPC,
        145 => ENOTEMPTY,
        267 => ENOTDIR,
        995 => EINTR,
        _ => EIO,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(unix)]
    #[test]
    fn maps_native_errno_to_the_guest_abi() {
        assert_eq!(from_host_errno(libc::EAGAIN), EAGAIN);
        assert_eq!(from_host_errno(libc::EACCES), EACCES);
        assert_eq!(from_host_errno(libc::ENOENT), ENOENT);
    }
}
