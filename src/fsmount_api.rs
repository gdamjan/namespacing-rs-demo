//! Safe wrappers around Linux's new mount API (Linux 5.2+):
//! fsopen(2), fsconfig(2), fsmount(2), move_mount(2).
//!
//! These replace the single mount(2) call with a multi-step fd-based workflow
//! that gives finer control over filesystem configuration and mount placement.

use nix::libc;
use std::ffi::CString;
use std::io;
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};

const FSOPEN_CLOEXEC: libc::c_uint = 0x0000_0001;
const FSCONFIG_SET_STRING: libc::c_uint = 1;
const FSCONFIG_CMD_CREATE: libc::c_uint = 6;
const FSMOUNT_CLOEXEC: libc::c_uint = 0x0000_0001;
const MOVE_MOUNT_F_EMPTY_PATH: libc::c_uint = 0x0000_0004;
#[allow(dead_code)]
const MOVE_MOUNT_BENEATH: libc::c_uint = 0x0000_0200;
const OPEN_TREE_CLOEXEC: libc::c_uint = libc::O_CLOEXEC as libc::c_uint;
const OPEN_TREE_NAMESPACE: libc::c_uint = 1 << 1;

/// Mount attribute flags for fsmount(2).
#[derive(Clone, Copy, Default)]
pub struct MountAttrs(u64);

impl MountAttrs {
    #[allow(dead_code)]
    pub const RDONLY: Self = Self(0x0000_0001);
    pub const NOSUID: Self = Self(0x0000_0002);
    pub const NODEV: Self = Self(0x0000_0004);
    pub const NOEXEC: Self = Self(0x0000_0008);

    pub const fn empty() -> Self {
        Self(0)
    }
}

impl std::ops::BitOr for MountAttrs {
    type Output = Self;
    fn bitor(self, rhs: Self) -> Self {
        Self(self.0 | rhs.0)
    }
}

/// Convert a raw syscall return value into a `Result`, mapping negative
/// values to the last OS error.
fn check(ret: libc::c_long) -> io::Result<libc::c_long> {
    if ret < 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(ret)
    }
}

/// Open a filesystem context for the given filesystem type.
/// Returns an `OwnedFd` that must be passed to [`fsconfig`] and then [`fsmount`].
pub fn fsopen(fstype: &str) -> io::Result<OwnedFd> {
    let fstype = CString::new(fstype).expect("fstype contains nul");
    let fd = check(unsafe { libc::syscall(libc::SYS_fsopen, fstype.as_ptr(), FSOPEN_CLOEXEC) })?;
    Ok(unsafe { OwnedFd::from_raw_fd(fd as i32) })
}

/// Finalize the filesystem configuration, creating the superblock.
pub fn fsconfig(fsfd: &OwnedFd) -> io::Result<()> {
    check(unsafe {
        libc::syscall(
            libc::SYS_fsconfig,
            fsfd.as_raw_fd(),
            FSCONFIG_CMD_CREATE,
            std::ptr::null::<libc::c_char>(),
            std::ptr::null::<libc::c_char>(),
            0,
        )
    })?;
    Ok(())
}

/// Set a string key=value parameter on a filesystem context.
pub fn fsconfig_set_string(fsfd: &OwnedFd, key: &str, value: &str) -> io::Result<()> {
    let key = CString::new(key).expect("key contains nul");
    let value = CString::new(value).expect("value contains nul");
    check(unsafe {
        libc::syscall(
            libc::SYS_fsconfig,
            fsfd.as_raw_fd(),
            FSCONFIG_SET_STRING,
            key.as_ptr(),
            value.as_ptr(),
            0,
        )
    })?;
    Ok(())
}

/// Create a detached mount from a configured filesystem context.
/// Returns an `OwnedFd` representing the mount that can be attached
/// with [`move_mount_at`].
pub fn fsmount(fsfd: &OwnedFd, attrs: MountAttrs) -> io::Result<OwnedFd> {
    let fd = check(unsafe {
        libc::syscall(
            libc::SYS_fsmount,
            fsfd.as_raw_fd(),
            FSMOUNT_CLOEXEC,
            attrs.0,
        )
    })?;
    Ok(unsafe { OwnedFd::from_raw_fd(fd as i32) })
}

/// Attach a detached mount onto a relative path under another mount fd.
/// This supports mounting onto detached mounts (detached-on-detached).
pub fn move_mount_at(mntfd: &OwnedFd, target_fd: &OwnedFd, target_path: &str) -> io::Result<()> {
    let target_path = CString::new(target_path).expect("target_path contains nul");
    check(unsafe {
        libc::syscall(
            libc::SYS_move_mount,
            mntfd.as_raw_fd(),
            c"".as_ptr(),
            target_fd.as_raw_fd(),
            target_path.as_ptr(),
            MOVE_MOUNT_F_EMPTY_PATH,
        )
    })?;
    Ok(())
}

/// Create a new mount namespace containing a recursive clone of the mount
/// tree at `path`. Returns an fd that can be passed to `setns(fd, CLONE_NEWNS)`
/// to enter it.
pub fn open_tree_namespace(path: &str) -> io::Result<OwnedFd> {
    let path = CString::new(path).expect("path contains nul");
    let fd = check(unsafe {
        libc::syscall(
            libc::SYS_open_tree,
            libc::AT_FDCWD,
            path.as_ptr(),
            OPEN_TREE_NAMESPACE | OPEN_TREE_CLOEXEC | libc::AT_RECURSIVE as libc::c_uint,
        )
    })?;
    Ok(unsafe { OwnedFd::from_raw_fd(fd as i32) })
}

/// Create a new mount namespace from a detached mount fd.
/// The mount becomes the root of the new namespace.
pub fn open_tree_namespace_fd(mntfd: &OwnedFd) -> io::Result<OwnedFd> {
    let fd = check(unsafe {
        libc::syscall(
            libc::SYS_open_tree,
            mntfd.as_raw_fd(),
            c"".as_ptr(),
            OPEN_TREE_NAMESPACE
                | OPEN_TREE_CLOEXEC
                | libc::AT_EMPTY_PATH as libc::c_uint
                | libc::AT_RECURSIVE as libc::c_uint,
        )
    })?;
    Ok(unsafe { OwnedFd::from_raw_fd(fd as i32) })
}

/// Create a directory relative to a mount fd.
pub fn mkdirat(dirfd: &OwnedFd, path: &str, mode: libc::mode_t) -> io::Result<()> {
    let path = CString::new(path).expect("path contains nul");
    let ret = unsafe { libc::mkdirat(dirfd.as_raw_fd(), path.as_ptr(), mode) };
    if ret < 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(())
    }
}
