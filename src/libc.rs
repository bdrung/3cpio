use std::ffi::CString;
use std::io::{Error, Result};

pub fn set_modified(path: &str, mtime: i64) -> Result<()> {
    let p = CString::new(path)?;
    let modified = libc::timespec {
        tv_sec: mtime,
        tv_nsec: 0,
    };
    // times contains the access time followed by modfied time
    let times = [modified, modified];
    let rc = unsafe {
        libc::utimensat(
            libc::AT_FDCWD,
            p.as_ptr(),
            times.as_ptr(),
            libc::AT_SYMLINK_NOFOLLOW,
        )
    };
    if rc != 0 {
        return Err(Error::last_os_error());
    };
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env::temp_dir;
    use std::fs::{self, create_dir};
    use std::path::PathBuf;
    use std::time::{Duration, SystemTime};

    fn make_temp_dir() -> Result<PathBuf> {
        let mut dir = temp_dir();
        let now = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap();
        dir.push(format!("3cpio-{:?}", now));
        create_dir(&dir)?;
        Ok(dir)
    }

    #[test]
    // Create a temporary directory and set the mtime 10 seconds earlier
    // than the current mtime of the directory.
    fn test_set_modified() {
        let dir: PathBuf = make_temp_dir().unwrap();
        let modified = dir.metadata().unwrap().modified().unwrap();
        let duration = modified.duration_since(SystemTime::UNIX_EPOCH).unwrap();
        let new_modified = SystemTime::UNIX_EPOCH
            .checked_add(Duration::new(duration.as_secs() - 10, 0))
            .unwrap();

        let mtime = new_modified.duration_since(SystemTime::UNIX_EPOCH).unwrap();
        let p = dir.clone().into_os_string().into_string().unwrap();
        set_modified(&p, mtime.as_secs().try_into().unwrap()).unwrap();

        assert_eq!(dir.metadata().unwrap().modified().unwrap(), new_modified);
        fs::remove_dir(dir).unwrap();
    }
}
