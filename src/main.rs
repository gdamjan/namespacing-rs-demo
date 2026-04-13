mod newmount;
mod rootfs;

use nix::sched::{CloneFlags, clone};
use nix::sys::signal::Signal;
use nix::sys::wait::waitpid;
use nix::unistd::sethostname;

fn main() {
    let pid = spawn_isolated().expect("clone failed");
    println!("child pid: {pid}");
    waitpid(pid, None).expect("waitpid failed");
}

fn spawn_isolated() -> nix::Result<nix::unistd::Pid> {
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
        isolated();
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

fn isolated() {
    println!("-- Namespaced Process --");
    let hostname = nix::unistd::gethostname().expect("gethostname failed");
    println!("hostname: {:?}", hostname);

    let pid = nix::unistd::getpid();
    println!("Pid: {pid}");

    inspect_system();
}

fn inspect_system() {
    let uid = std::fs::read_to_string("/proc/self/status")
        .expect("failed to read status")
        .lines()
        .filter(|l| l.starts_with("Uid:") || l.starts_with("Gid:"))
        .collect::<Vec<_>>()
        .join("\n");
    println!("{uid}");

    println!("mounts:");
    let mounts = std::fs::read_to_string("/proc/self/mounts").expect("failed to read mounts");
    for line in mounts.lines() {
        println!("  {line}");
    }

    println!("processes:");
    for entry in std::fs::read_dir("/proc").expect("failed to read /proc") {
        let entry = entry.expect("failed to read entry");
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if name.chars().all(|c| c.is_ascii_digit()) {
            let cmdline =
                std::fs::read_to_string(format!("/proc/{name}/cmdline")).unwrap_or_default();
            let cmd = cmdline.replace('\0', " ");
            println!("  pid {name}: {cmd}");
        }
    }

    println!("/");
    print_tree(std::path::Path::new("/"), "");
}

fn print_tree(dir: &std::path::Path, prefix: &str) {
    let mut entries: Vec<_> = match std::fs::read_dir(dir) {
        Ok(rd) => rd.filter_map(|e| e.ok()).collect(),
        Err(e) => {
            println!("{prefix}[unreadable: {e}]");
            return;
        }
    };
    entries.sort_by_key(|e| e.file_name());

    for (i, entry) in entries.iter().enumerate() {
        let is_last = i + 1 == entries.len();
        let connector = if is_last { "└── " } else { "├── " };
        let name = entry.file_name();
        let name = name.to_string_lossy();

        let ft = entry.file_type().ok();
        let is_dir = ft.as_ref().is_some_and(|ft| ft.is_dir());
        let is_link = ft.as_ref().is_some_and(|ft| ft.is_symlink());

        if is_link {
            let target = std::fs::read_link(entry.path())
                .map(|t| t.to_string_lossy().into_owned())
                .unwrap_or_default();
            println!("{prefix}{connector}{name} -> {target}");
        } else {
            println!("{prefix}{connector}{name}");
        }

        if is_dir {
            let child_prefix = if is_last { "    " } else { "│   " };
            print_tree(&entry.path(), &format!("{prefix}{child_prefix}"));
        }
    }
}
