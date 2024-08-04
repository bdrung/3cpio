use std::ffi::CString;
use std::io::{Error, ErrorKind, Result};

/// Get password file entry and return user name.
///
/// This function wraps the standard C library function getpwuid().
/// The getpwuid() function returns a pointer to a structure containing the
/// broken-out fields of the record in the password database (e.g., the local
/// password file /etc/passwd, NIS, and LDAP) that matches the user ID uid.
pub fn getpwuid_name(uid: u32) -> Result<Option<String>> {
    let mut pwd: libc::passwd = unsafe { std::mem::zeroed() };
    let mut buf = [0u8; 2048];
    let mut result = std::ptr::null_mut::<libc::passwd>();
    let rc = unsafe {
        libc::getpwuid_r(
            uid,
            &mut pwd,
            buf.as_mut_ptr() as *mut libc::c_char,
            buf.len(),
            &mut result,
        )
    };
    if rc != 0 {
        return Err(Error::last_os_error());
    }
    if result.is_null() {
        return Ok(None);
    }
    let name = unsafe { core::ffi::CStr::from_ptr(pwd.pw_name) };
    Ok(Some(name.to_string_lossy().to_string()))
}

/// Get group file entry and return group name.
///
/// This function wraps the standard C library function getgrgid().
/// The getgrgid() function returns a pointer to a structure containing the
/// broken-out fields of the record in the group database (e.g., the local
/// group file /etc/group, NIS, and LDAP) that matches the group ID gid.
pub fn getgrgid_name(gid: u32) -> Result<Option<String>> {
    let mut group: libc::group = unsafe { std::mem::zeroed() };
    let mut buf = [0u8; 2048];
    let mut result = std::ptr::null_mut::<libc::group>();
    let rc = unsafe {
        libc::getgrgid_r(
            gid,
            &mut group,
            buf.as_mut_ptr() as *mut libc::c_char,
            buf.len(),
            &mut result,
        )
    };
    if rc != 0 {
        return Err(Error::last_os_error());
    }
    if result.is_null() {
        return Ok(None);
    }
    let name = unsafe { core::ffi::CStr::from_ptr(group.gr_name) };
    Ok(Some(name.to_string_lossy().to_string()))
}

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

fn strftime(format: &str, tm: &libc::tm) -> Result<String> {
    let f = CString::new(format)?;
    let mut s = [0u8; 19];
    let length =
        unsafe { libc::strftime(s.as_mut_ptr() as *mut libc::c_char, s.len(), f.as_ptr(), tm) };
    if length == 0 {
        return Err(Error::new(ErrorKind::Other, "strftime returned 0"));
    }
    Ok(String::from_utf8_lossy(&s[..length]).to_string())
}

pub fn strftime_local(format: &str, timestamp: u32) -> Result<String> {
    let mut tm: libc::tm = unsafe { std::mem::zeroed() };
    let rc = unsafe { libc::localtime_r(&timestamp.into(), &mut tm) };
    if rc.is_null() {
        return Err(Error::last_os_error());
    };
    strftime(format, &tm)
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

    extern "C" {
        fn tzset();
    }

    #[test]
    fn test_getpwuid_name_root() {
        let got = getpwuid_name(0).unwrap();
        assert_eq!(got, Some("root".to_string()));
    }

    #[test]
    fn test_getpwuid_name_non_existing() {
        // Assume that this UID is not in /etc/passwd (nobody is 65534)
        let got = getpwuid_name(65520).unwrap();
        assert_eq!(got, None);
    }

    #[test]
    fn test_getgrgid_name_root() {
        let got = getgrgid_name(0).unwrap();
        assert_eq!(got, Some("root".to_string()));
    }

    #[test]
    fn test_getgrgid_name_non_existing() {
        // Assume that this GID is not in /etc/passwd (nogroup is 65534)
        let got = getgrgid_name(65520).unwrap();
        assert_eq!(got, None);
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

    #[test]
    fn test_strftime_local_year() {
        let time = strftime_local("%b %e  %Y", 2278410030).unwrap();
        assert_eq!(time, "Mar 14  2042");
    }

    #[test]
    fn test_strftime_local_hour() {
        std::env::set_var("TZ", "UTC");
        unsafe { tzset() };
        let time = strftime_local("%b %e %H:%M", 1720735264).unwrap();
        assert_eq!(time, "Jul 11 22:01");
    }
}
