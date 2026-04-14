use nix::sched::{CloneFlags, clone};
use nix::sys::signal::Signal;
use nix::unistd::sethostname;

use crate::rootfs;

pub fn spawn<F>(func: F) -> nix::Result<nix::unistd::Pid>
where
    F: Fn(),
{
    let mut stack = [0u8; 1024 * 1024];
    let uid = nix::unistd::getuid();
    let gid = nix::unistd::getgid();

    let flags = CloneFlags::CLONE_NEWCGROUP
        | CloneFlags::CLONE_NEWUTS
        | CloneFlags::CLONE_NEWIPC
        | CloneFlags::CLONE_NEWUSER
        | CloneFlags::CLONE_NEWPID
        | CloneFlags::CLONE_NEWNET;

    let cb = Box::new(move || -> isize {
        setup_id_maps(uid, gid);
        rootfs::setup();
        sethostname("container").expect("failed to setup container hostname");
        func();
        0
    });

    unsafe { clone(cb, &mut stack, flags, Some(Signal::SIGCHLD as i32)) }
}

/// Map UID/GID 0 inside the user namespace to the caller's real UID/GID on
/// the host. This gives the namespaced process root-like capabilities without
/// requiring actual root privileges. `setgroups` must be denied before writing
/// `gid_map` when running unprivileged.
fn setup_id_maps(uid: nix::unistd::Uid, gid: nix::unistd::Gid) {
    std::fs::write("/proc/self/uid_map", format!("0 {uid} 1\n")).expect("failed to write uid_map");
    std::fs::write("/proc/self/setgroups", "deny\n").expect("failed to write setgroups");
    std::fs::write("/proc/self/gid_map", format!("0 {gid} 1\n")).expect("failed to write gid_map");
}
