mod fsmount_api;
mod rootfs;
mod spawn;

use nix::sys::wait::waitpid;
use spawn::spawn;

fn main() {
    let pid = spawn(isolated).expect("clone failed");
    println!("child pid: {pid}");
    waitpid(pid, None).expect("waitpid failed");
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
