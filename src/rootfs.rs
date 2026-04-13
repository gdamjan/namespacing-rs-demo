use nix::sched::CloneFlags;

use crate::newmount;

/// Assemble a container rootfs entirely as detached mounts.
/// Root(/) is a tmpfs, /proc is mounted, at the end that's the only filesystem
/// that the isolated child process can see.
pub fn setup() {
    // Enter a mount namespace we own so that the kernel may_mount() passes for
    // subsequent fsopen() calls.  AT_RECURSIVE is required so that proc
    // is already visible — the kernel's mount_too_revealing() check needs
    // it present when creating a new proc superblock in a user namespace.
    let bootstrap_ns = newmount::open_tree_namespace("/").expect("open_tree_namespace failed");
    nix::sched::setns(&bootstrap_ns, CloneFlags::CLONE_NEWNS).expect("setns failed");
    drop(bootstrap_ns);

    // Assemble the container rootfs entirely as detached mounts.
    let tmpfs_fd = {
        let fsfd = newmount::fsopen("tmpfs").expect("fsopen tmpfs failed");
        newmount::fsconfig(&fsfd).expect("fsconfig tmpfs failed");
        newmount::fsmount(&fsfd, newmount::MountAttrs::empty()).expect("fsmount tmpfs failed")
    };

    newmount::mkdirat(&tmpfs_fd, "proc", 0o755).expect("mkdirat proc failed");

    let proc_fd = {
        let fsfd = newmount::fsopen("proc").expect("fsopen proc failed");
        newmount::fsconfig_set_string(&fsfd, "subset", "pid").expect("fsconfig subset failed");
        newmount::fsconfig(&fsfd).expect("fsconfig proc failed");
        newmount::fsmount(
            &fsfd,
            newmount::MountAttrs::NOSUID
                | newmount::MountAttrs::NODEV
                | newmount::MountAttrs::NOEXEC,
        )
        .expect("fsmount proc failed")
    };

    // Mount proc onto the detached tmpfs (detached-on-detached).
    newmount::move_mount_at(&proc_fd, &tmpfs_fd, "proc").expect("move_mount proc failed");

    // Create the final mount namespace with our assembled rootfs as "/".
    // No pivot_root needed — open_tree(OPEN_TREE_NAMESPACE) on the
    // detached mount makes it the root of a brand-new namespace.
    let ns_fd = newmount::open_tree_namespace_fd(&tmpfs_fd).expect("open_tree_namespace_fd failed");
    nix::sched::setns(&ns_fd, CloneFlags::CLONE_NEWNS).expect("setns into rootfs failed");
    std::env::set_current_dir("/").expect("chdir failed");
}
