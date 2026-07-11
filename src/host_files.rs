use std::collections::HashMap;
use std::fs::{File, OpenOptions};
use std::io::{Seek, SeekFrom};
use std::ops::Range;
#[cfg(windows)]
use std::path::Path;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

#[cfg(unix)]
use std::os::unix::fs::FileExt;
#[cfg(unix)]
use std::os::unix::io::AsRawFd;
#[cfg(windows)]
use std::os::windows::ffi::OsStrExt;
#[cfg(windows)]
use std::os::windows::fs::FileExt;
#[cfg(windows)]
use std::os::windows::io::AsRawHandle;

use wasmtime::{Caller, Linker, Result};

use crate::{AppState, Cli, Preopen, guest_errno as errno};

const MODULE_NAME: &str = "waasmtime_mysql_files";
const GUEST_FD_BASE: i32 = 20_000;
const MAX_IO_LEN: usize = 16 * 1024 * 1024;
const MAX_PATH_LEN: usize = 16 * 1024;

const WASI_O_APPEND: i32 = 0x0001;
const WASI_O_CREAT: i32 = 0x0001 << 12;
const WASI_O_DIRECTORY: i32 = 0x0002 << 12;
const WASI_O_EXCL: i32 = 0x0004 << 12;
const WASI_O_TRUNC: i32 = 0x0008 << 12;
const WASI_O_RDONLY: i32 = 0x0400_0000;
const WASI_O_WRONLY: i32 = 0x1000_0000;

#[derive(Clone)]
pub(crate) struct HostFiles {
    inner: Arc<Mutex<HostFilesInner>>,
}

struct HostFilesInner {
    next_fd: i32,
    files: HashMap<i32, File>,
    #[cfg(windows)]
    lock_paths: HashMap<i32, PathBuf>,
    #[cfg(windows)]
    locks: HashMap<PathBuf, File>,
    preopens: Vec<PreopenMapping>,
}

struct PreopenMapping {
    guest: String,
    host: PathBuf,
}

impl HostFiles {
    pub(crate) fn new(cli: &Cli) -> Result<Self> {
        let mut preopens = Vec::new();

        if !cli.no_default_preopen {
            preopens.push(PreopenMapping {
                guest: normalize_guest_path(".").unwrap_or_else(|_| ".".to_owned()),
                host: std::env::current_dir()?,
            });
        }

        for Preopen { host, guest } in &cli.preopens {
            preopens.push(PreopenMapping {
                guest: normalize_guest_path(guest).map_err(io_error_from_errno)?,
                host: host.clone(),
            });
        }

        preopens.sort_by(|left, right| right.guest.len().cmp(&left.guest.len()));

        Ok(Self {
            inner: Arc::new(Mutex::new(HostFilesInner {
                next_fd: GUEST_FD_BASE,
                files: HashMap::new(),
                #[cfg(windows)]
                lock_paths: HashMap::new(),
                #[cfg(windows)]
                locks: HashMap::new(),
                preopens,
            })),
        })
    }

    fn open(&self, guest_path: &str, flags: i32, _mode: i32) -> i32 {
        let host_path = match self.resolve(guest_path) {
            Ok(path) => path,
            Err(errno) => return neg_errno(errno),
        };
        if flags & WASI_O_DIRECTORY != 0 {
            return neg_errno(errno::EISDIR);
        }

        let write = flags & WASI_O_WRONLY != 0;
        let read = flags & WASI_O_RDONLY != 0 || !write;
        let create = flags & WASI_O_CREAT != 0;
        let excl = flags & WASI_O_EXCL != 0;

        let mut options = OpenOptions::new();
        options.read(read).write(write);
        if flags & WASI_O_APPEND != 0 {
            options.append(true);
        }
        if create && excl {
            options.create_new(true);
        } else if create {
            options.create(true);
        }
        if flags & WASI_O_TRUNC != 0 {
            options.truncate(true);
        }

        let file = match options.open(&host_path) {
            Ok(file) => file,
            Err(err) => return neg_errno(io_errno(err)),
        };
        #[cfg(windows)]
        let lock_path = std::fs::canonicalize(&host_path).unwrap_or(host_path);

        let mut inner = self.inner.lock().unwrap();
        let fd = inner.next_fd;
        inner.next_fd = inner.next_fd.saturating_add(1);
        inner.files.insert(fd, file);
        #[cfg(windows)]
        inner.lock_paths.insert(fd, lock_path);
        fd
    }

    fn close(&self, fd: i32) -> i32 {
        let mut inner = self.inner.lock().unwrap();
        if inner.files.remove(&fd).is_some() {
            #[cfg(windows)]
            inner.lock_paths.remove(&fd);
            0
        } else {
            neg_errno(errno::EBADF)
        }
    }

    fn pread(&self, fd: i32, buf: &mut [u8], offset: u64) -> i32 {
        #[cfg(unix)]
        {
            let inner = self.inner.lock().unwrap();
            let Some(file) = inner.files.get(&fd) else {
                return neg_errno(errno::EBADF);
            };
            match file.read_at(buf, offset) {
                Ok(n) => i32::try_from(n).unwrap_or_else(|_| neg_errno(errno::EOVERFLOW)),
                Err(err) => neg_errno(io_errno(err)),
            }
        }
        #[cfg(windows)]
        {
            let inner = self.inner.lock().unwrap();
            let Some(file) = inner.files.get(&fd) else {
                return neg_errno(errno::EBADF);
            };
            match file.seek_read(buf, offset) {
                Ok(n) => i32::try_from(n).unwrap_or_else(|_| neg_errno(errno::EOVERFLOW)),
                Err(err) => neg_errno(io_errno(err)),
            }
        }
        #[cfg(not(any(unix, windows)))]
        {
            let _ = (fd, buf, offset);
            neg_errno(errno::ENOSYS)
        }
    }

    fn pwrite(&self, fd: i32, buf: &[u8], offset: u64) -> i32 {
        #[cfg(unix)]
        {
            let inner = self.inner.lock().unwrap();
            let Some(file) = inner.files.get(&fd) else {
                return neg_errno(errno::EBADF);
            };
            match file.write_at(buf, offset) {
                Ok(n) => i32::try_from(n).unwrap_or_else(|_| neg_errno(errno::EOVERFLOW)),
                Err(err) => neg_errno(io_errno(err)),
            }
        }
        #[cfg(windows)]
        {
            let inner = self.inner.lock().unwrap();
            let Some(file) = inner.files.get(&fd) else {
                return neg_errno(errno::EBADF);
            };
            match file.seek_write(buf, offset) {
                Ok(n) => i32::try_from(n).unwrap_or_else(|_| neg_errno(errno::EOVERFLOW)),
                Err(err) => neg_errno(io_errno(err)),
            }
        }
        #[cfg(not(any(unix, windows)))]
        {
            let _ = (fd, buf, offset);
            neg_errno(errno::ENOSYS)
        }
    }

    fn seek(&self, fd: i32, offset: i64, whence: i32) -> i64 {
        let mut inner = self.inner.lock().unwrap();
        let Some(file) = inner.files.get_mut(&fd) else {
            return i64::from(neg_errno(errno::EBADF));
        };
        let seek_from = match whence {
            libc::SEEK_SET => SeekFrom::Start(match u64::try_from(offset) {
                Ok(offset) => offset,
                Err(_) => return i64::from(neg_errno(errno::EINVAL)),
            }),
            libc::SEEK_CUR => SeekFrom::Current(offset),
            libc::SEEK_END => SeekFrom::End(offset),
            _ => return i64::from(neg_errno(errno::EINVAL)),
        };
        match file.seek(seek_from) {
            Ok(pos) => {
                i64::try_from(pos).unwrap_or_else(|_| i64::from(neg_errno(errno::EOVERFLOW)))
            }
            Err(err) => i64::from(neg_errno(io_errno(err))),
        }
    }

    fn truncate(&self, fd: i32, size: u64) -> i32 {
        let inner = self.inner.lock().unwrap();
        let Some(file) = inner.files.get(&fd) else {
            return neg_errno(errno::EBADF);
        };
        match file.set_len(size) {
            Ok(()) => 0,
            Err(err) => neg_errno(io_errno(err)),
        }
    }

    fn sync(&self, fd: i32, data_only: bool) -> i32 {
        let inner = self.inner.lock().unwrap();
        let Some(file) = inner.files.get(&fd) else {
            return neg_errno(errno::EBADF);
        };
        let result = if data_only {
            file.sync_data()
        } else {
            file.sync_all()
        };
        match result {
            Ok(()) => 0,
            Err(err) => neg_errno(io_errno(err)),
        }
    }

    fn sync_parent(&self, guest_path: &str) -> i32 {
        #[cfg(unix)]
        {
            let parent = match self.resolve_parent(guest_path) {
                Ok(path) => path,
                Err(errno) => return neg_errno(errno),
            };
            match File::open(parent).and_then(|directory| directory.sync_all()) {
                Ok(()) => 0,
                Err(err) => neg_errno(io_errno(err)),
            }
        }
        #[cfg(windows)]
        {
            let parent = match self.resolve_parent(guest_path) {
                Ok(path) => path,
                Err(errno) => return neg_errno(errno),
            };
            match sync_directory(&parent) {
                Ok(()) => 0,
                Err(err) => neg_errno(io_errno(err)),
            }
        }
        #[cfg(not(any(unix, windows)))]
        {
            let _ = guest_path;
            neg_errno(errno::ENOSYS)
        }
    }

    fn lock_exclusive(&self, fd: i32) -> i32 {
        #[cfg(unix)]
        {
            let inner = self.inner.lock().unwrap();
            let Some(file) = inner.files.get(&fd) else {
                return neg_errno(errno::EBADF);
            };
            let mut lock = libc::flock {
                l_type: libc::F_WRLCK as _,
                l_whence: libc::SEEK_SET as _,
                l_start: 0,
                l_len: 0,
                l_pid: 0,
            };
            let rc = unsafe { libc::fcntl(file.as_raw_fd(), libc::F_SETLK, &mut lock) };
            if rc == 0 {
                0
            } else {
                neg_errno(io_errno(std::io::Error::last_os_error()))
            }
        }
        #[cfg(windows)]
        {
            use windows_sys::Win32::Storage::FileSystem::{
                LOCKFILE_EXCLUSIVE_LOCK, LOCKFILE_FAIL_IMMEDIATELY, LockFileEx,
            };
            use windows_sys::Win32::System::IO::OVERLAPPED;

            let mut inner = self.inner.lock().unwrap();
            let Some(lock_path) = inner.lock_paths.get(&fd).cloned() else {
                return neg_errno(errno::EBADF);
            };
            if inner.locks.contains_key(&lock_path) {
                return 0;
            }
            let Some(file) = inner.files.get(&fd) else {
                return neg_errno(errno::EBADF);
            };
            // Windows locks belong to a handle, while fcntl locks are process-wide.
            // Keep one duplicate handle per file so later opens in this guest process
            // observe the existing lock instead of conflicting with it.
            let lock_file = match file.try_clone() {
                Ok(file) => file,
                Err(err) => return neg_errno(io_errno(err)),
            };
            let mut overlapped = OVERLAPPED::default();
            let locked = unsafe {
                LockFileEx(
                    lock_file.as_raw_handle(),
                    LOCKFILE_EXCLUSIVE_LOCK | LOCKFILE_FAIL_IMMEDIATELY,
                    0,
                    u32::MAX,
                    u32::MAX,
                    &mut overlapped,
                )
            };
            if locked != 0 {
                inner.locks.insert(lock_path, lock_file);
                0
            } else {
                neg_errno(io_errno(std::io::Error::last_os_error()))
            }
        }
        #[cfg(not(any(unix, windows)))]
        {
            let _ = fd;
            neg_errno(errno::ENOSYS)
        }
    }

    fn resolve(&self, guest_path: &str) -> std::result::Result<PathBuf, i32> {
        let normalized = normalize_guest_path(guest_path)?;
        let inner = self.inner.lock().unwrap();

        for preopen in &inner.preopens {
            if preopen.guest == "." && !normalized.starts_with('/') {
                return Ok(join_suffix(&preopen.host, &normalized));
            }

            let suffix = if preopen.guest == "/" {
                normalized.strip_prefix('/').unwrap_or(&normalized)
            } else if normalized == preopen.guest {
                ""
            } else if normalized.starts_with(&preopen.guest)
                && normalized.as_bytes().get(preopen.guest.len()) == Some(&b'/')
            {
                &normalized[preopen.guest.len() + 1..]
            } else {
                continue;
            };
            return Ok(join_suffix(&preopen.host, suffix));
        }

        Err(errno::ENOTCAPABLE)
    }

    fn resolve_parent(&self, guest_path: &str) -> std::result::Result<PathBuf, i32> {
        let path = self.resolve(guest_path)?;
        let parent = path.parent().ok_or(errno::EINVAL)?;
        let inner = self.inner.lock().unwrap();
        if inner
            .preopens
            .iter()
            .any(|preopen| parent.starts_with(&preopen.host))
        {
            Ok(parent.to_path_buf())
        } else {
            Err(errno::ENOTCAPABLE)
        }
    }
}

#[cfg(windows)]
fn sync_directory(path: &Path) -> std::io::Result<()> {
    use windows_sys::Win32::Foundation::{CloseHandle, GENERIC_WRITE, INVALID_HANDLE_VALUE};
    use windows_sys::Win32::Storage::FileSystem::{
        CreateFileW, FILE_FLAG_BACKUP_SEMANTICS, FILE_SHARE_DELETE, FILE_SHARE_READ,
        FILE_SHARE_WRITE, FlushFileBuffers, OPEN_EXISTING,
    };

    let mut wide_path: Vec<u16> = path.as_os_str().encode_wide().collect();
    wide_path.push(0);
    let handle = unsafe {
        CreateFileW(
            wide_path.as_ptr(),
            // FlushFileBuffers requires a handle with GENERIC_WRITE access.
            GENERIC_WRITE,
            FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
            std::ptr::null(),
            OPEN_EXISTING,
            FILE_FLAG_BACKUP_SEMANTICS,
            std::ptr::null_mut(),
        )
    };
    if handle == INVALID_HANDLE_VALUE {
        return Err(std::io::Error::last_os_error());
    }

    let result = if unsafe { FlushFileBuffers(handle) } != 0 {
        Ok(())
    } else {
        Err(std::io::Error::last_os_error())
    };
    unsafe {
        CloseHandle(handle);
    }
    result
}

pub(crate) fn add_to_linker(linker: &mut Linker<AppState>) -> Result<()> {
    linker.func_wrap(
        MODULE_NAME,
        "open",
        |mut caller: Caller<'_, AppState>, path_ptr: i32, flags: i32, mode: i32| -> i32 {
            let path = match read_cstr(&mut caller, path_ptr) {
                Ok(path) => path,
                Err(errno) => return neg_errno(errno),
            };
            caller.data().files.open(&path, flags, mode)
        },
    )?;

    linker.func_wrap(
        MODULE_NAME,
        "close",
        |caller: Caller<'_, AppState>, fd: i32| -> i32 { caller.data().files.close(fd) },
    )?;

    linker.func_wrap(
        MODULE_NAME,
        "pread",
        |mut caller: Caller<'_, AppState>, fd: i32, buf_ptr: i32, len: i32, offset: i64| -> i32 {
            let len = match checked_len(len) {
                Ok(len) => len,
                Err(errno) => return neg_errno(errno),
            };
            let mut buf = vec![0_u8; len];
            let rc = caller.data().files.pread(fd, &mut buf, offset as u64);
            if rc <= 0 {
                return rc;
            }
            match write_guest(&mut caller, buf_ptr, &buf[..rc as usize]) {
                Ok(()) => rc,
                Err(errno) => neg_errno(errno),
            }
        },
    )?;

    linker.func_wrap(
        MODULE_NAME,
        "pwrite",
        |mut caller: Caller<'_, AppState>, fd: i32, buf_ptr: i32, len: i32, offset: i64| -> i32 {
            let buf = match read_guest(&mut caller, buf_ptr, len) {
                Ok(buf) => buf,
                Err(errno) => return neg_errno(errno),
            };
            caller.data().files.pwrite(fd, &buf, offset as u64)
        },
    )?;

    linker.func_wrap(
        MODULE_NAME,
        "seek",
        |caller: Caller<'_, AppState>, fd: i32, offset: i64, whence: i32| -> i64 {
            caller.data().files.seek(fd, offset, whence)
        },
    )?;

    linker.func_wrap(
        MODULE_NAME,
        "truncate",
        |caller: Caller<'_, AppState>, fd: i32, size: i64| -> i32 {
            let size = match u64::try_from(size) {
                Ok(size) => size,
                Err(_) => return neg_errno(errno::EINVAL),
            };
            caller.data().files.truncate(fd, size)
        },
    )?;

    linker.func_wrap(
        MODULE_NAME,
        "sync",
        |caller: Caller<'_, AppState>, fd: i32, data_only: i32| -> i32 {
            caller.data().files.sync(fd, data_only != 0)
        },
    )?;

    linker.func_wrap(
        MODULE_NAME,
        "sync_parent",
        |mut caller: Caller<'_, AppState>, path_ptr: i32| -> i32 {
            let path = match read_cstr(&mut caller, path_ptr) {
                Ok(path) => path,
                Err(errno) => return neg_errno(errno),
            };
            caller.data().files.sync_parent(&path)
        },
    )?;

    linker.func_wrap(
        MODULE_NAME,
        "lock_exclusive",
        |caller: Caller<'_, AppState>, fd: i32| -> i32 { caller.data().files.lock_exclusive(fd) },
    )?;

    Ok(())
}

fn normalize_guest_path(path: &str) -> std::result::Result<String, i32> {
    if path.is_empty() {
        return Err(errno::ENOENT);
    }

    let absolute = path.starts_with('/');
    let mut parts = Vec::new();
    for part in path.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                if parts.pop().is_none() {
                    return Err(errno::ENOTCAPABLE);
                }
            }
            part => parts.push(part),
        }
    }

    let joined = parts.join("/");
    if absolute {
        if joined.is_empty() {
            Ok("/".to_owned())
        } else {
            Ok(format!("/{joined}"))
        }
    } else if joined.is_empty() {
        Ok(".".to_owned())
    } else {
        Ok(joined)
    }
}

fn join_suffix(root: &PathBuf, suffix: &str) -> PathBuf {
    let mut path = root.clone();
    for component in suffix.split('/') {
        if !component.is_empty() && component != "." {
            path.push(component);
        }
    }
    path
}

fn read_cstr(caller: &mut Caller<'_, AppState>, ptr: i32) -> std::result::Result<String, i32> {
    let start = usize::try_from(ptr).map_err(|_| errno::EFAULT)?;
    let export = caller.get_export("memory").ok_or(errno::EFAULT)?;

    if let Some(mem) = export.clone().into_memory() {
        let data = mem.data(&mut *caller);
        if start >= data.len() {
            return Err(errno::EFAULT);
        }
        let max_end = start.saturating_add(MAX_PATH_LEN).min(data.len());
        let Some(end) = data[start..max_end].iter().position(|byte| *byte == 0) else {
            return Err(errno::ENAMETOOLONG);
        };
        return std::str::from_utf8(&data[start..start + end])
            .map(str::to_owned)
            .map_err(|_| errno::EINVAL);
    }

    if let Some(mem) = export.into_shared_memory() {
        let data = mem.data();
        if start >= data.len() {
            return Err(errno::EFAULT);
        }
        let max_end = start.saturating_add(MAX_PATH_LEN).min(data.len());
        let mut bytes = Vec::new();
        for cell in &data[start..max_end] {
            let byte = unsafe { *cell.get() };
            if byte == 0 {
                return String::from_utf8(bytes).map_err(|_| errno::EINVAL);
            }
            bytes.push(byte);
        }
        return Err(errno::ENAMETOOLONG);
    }

    Err(errno::EFAULT)
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

fn io_error_from_errno(errno: i32) -> std::io::Error {
    std::io::Error::from_raw_os_error(errno)
}

fn io_errno(err: std::io::Error) -> i32 {
    let Some(code) = err.raw_os_error() else {
        return errno::EIO;
    };
    #[cfg(unix)]
    {
        return errno::from_host_errno(code);
    }
    #[cfg(windows)]
    {
        return errno::from_windows_error(code);
    }
    #[cfg(not(any(unix, windows)))]
    {
        let _ = code;
        errno::EIO
    }
}

fn neg_errno(errno: i32) -> i32 {
    -errno
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn syncs_a_directory_handle() {
        let path = std::env::temp_dir().join(format!(
            "wasmtime-mysql-sync-directory-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir(&path).unwrap();
        let result = sync_directory(&path);
        std::fs::remove_dir(&path).unwrap();
        result.unwrap();
    }

    #[test]
    fn treats_locks_from_multiple_guest_handles_as_process_wide() {
        let path = std::env::temp_dir().join(format!(
            "wasmtime-mysql-lock-file-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let first = OpenOptions::new()
            .create_new(true)
            .read(true)
            .write(true)
            .open(&path)
            .unwrap();
        let second = OpenOptions::new()
            .read(true)
            .write(true)
            .open(&path)
            .unwrap();
        let lock_path = std::fs::canonicalize(&path).unwrap();
        let files = HostFiles {
            inner: Arc::new(Mutex::new(HostFilesInner {
                next_fd: GUEST_FD_BASE + 2,
                files: HashMap::from([(GUEST_FD_BASE, first), (GUEST_FD_BASE + 1, second)]),
                lock_paths: HashMap::from([
                    (GUEST_FD_BASE, lock_path.clone()),
                    (GUEST_FD_BASE + 1, lock_path),
                ]),
                locks: HashMap::new(),
                preopens: Vec::new(),
            })),
        };

        assert_eq!(files.lock_exclusive(GUEST_FD_BASE), 0);
        assert_eq!(files.lock_exclusive(GUEST_FD_BASE + 1), 0);
        drop(files);
        std::fs::remove_file(&path).unwrap();
    }
}
