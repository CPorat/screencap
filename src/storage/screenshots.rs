use std::{
    ffi::{CString, OsStr},
    fs::File,
    io::{Error, ErrorKind, Read},
    os::{
        fd::{AsRawFd, FromRawFd, RawFd},
        unix::ffi::OsStrExt,
    },
    path::{Component, Path, PathBuf},
};

pub fn sanitize_relative_screenshot_path(raw: &str) -> Option<PathBuf> {
    sanitize_relative_screenshot_path_buf(Path::new(raw))
}

pub fn sanitize_relative_screenshot_path_buf(path: &Path) -> Option<PathBuf> {
    let mut sanitized = PathBuf::new();
    for component in path.components() {
        match component {
            Component::Normal(part) => sanitized.push(part),
            _ => return None,
        }
    }

    if sanitized.as_os_str().is_empty() {
        return None;
    }

    Some(sanitized)
}

pub fn relative_screenshot_path(root: &Path, screenshot_path: &str) -> Option<PathBuf> {
    let path = Path::new(screenshot_path);
    let relative_path = if path.is_absolute() {
        path.strip_prefix(root).ok()?
    } else {
        path
    };

    sanitize_relative_screenshot_path_buf(relative_path)
}

pub fn read_screenshot_file(root: &Path, relative_path: &Path) -> std::io::Result<Vec<u8>> {
    fn cstring_from_os_str(value: &OsStr) -> std::io::Result<CString> {
        CString::new(value.as_bytes())
            .map_err(|_| Error::new(ErrorKind::InvalidInput, "path contains NUL byte"))
    }

    fn file_from_fd(fd: RawFd) -> std::io::Result<File> {
        if fd == -1 {
            Err(Error::last_os_error())
        } else {
            Ok(unsafe { File::from_raw_fd(fd) })
        }
    }

    fn open_path(path: &Path, flags: i32) -> std::io::Result<File> {
        let path = cstring_from_os_str(path.as_os_str())?;
        let fd = unsafe { libc::open(path.as_ptr(), flags) };
        file_from_fd(fd)
    }

    fn open_at(directory: &File, name: &OsStr, flags: i32) -> std::io::Result<File> {
        let name = cstring_from_os_str(name)?;
        let fd = unsafe { libc::openat(directory.as_raw_fd(), name.as_ptr(), flags) };
        file_from_fd(fd)
    }

    let mut current = open_path(root, libc::O_RDONLY | libc::O_CLOEXEC | libc::O_DIRECTORY)?;
    let mut components = relative_path.components().peekable();
    while let Some(component) = components.next() {
        let Component::Normal(name) = component else {
            return Err(Error::new(
                ErrorKind::InvalidInput,
                "invalid screenshot path",
            ));
        };

        let is_last = components.peek().is_none();
        let flags = if is_last {
            libc::O_RDONLY | libc::O_CLOEXEC | libc::O_NOFOLLOW | libc::O_NONBLOCK
        } else {
            libc::O_RDONLY | libc::O_CLOEXEC | libc::O_DIRECTORY | libc::O_NOFOLLOW
        };
        let next = open_at(&current, name, flags)?;

        if is_last {
            if !next.metadata()?.is_file() {
                return Err(Error::from(ErrorKind::NotFound));
            }

            let mut bytes = Vec::new();
            let mut next = next;
            next.read_to_end(&mut bytes)?;
            return Ok(bytes);
        }

        current = next;
    }

    Err(Error::from(ErrorKind::NotFound))
}
