# rust namespace demo

Demoing the latest Linux namespacing and mount APIs to run a "container".
Tested on Linux v7.0 that implements `OPEN_TREE_NAMESPACE`.

Don't implement fallbacks for older kernels or libc.


## rootfs creation logic

No need for pivot_root or umount2. The rootfs::setup() flow:

 1. Bootstrap ns — open_tree_namespace("/") + setns gives us a mount namespace we own (needed for may_mount() in fsopen)
 2. Assemble detached — create detached tmpfs via fsmount, mkdirat("proc") on it, create detached proc, then move_mount_at proc onto 
tmpfs (detached-on-detached)
 3. Final ns — open_tree_namespace_fd(tmpfs_fd) makes the assembled rootfs the root of a brand-new namespace
 4. Enter — setns + chdir("/") — done

The rootfs is fully assembled before entering it. No pivot_root, nor umount needed.

## References:

- https://man7.org/linux/man-pages/man2/open_tree.2.html
- [OPEN_TREE_NAMESPACE](https://patchew.org/linux/20260206-vfs-v70-7df0b750d594@brauner/)
- https://man7.org/linux/man-pages/man2/fsmount.2.html
- https://man7.org/linux/man-pages/man7/mount_namespaces.7.html
- https://man7.org/linux/man-pages/man7/user_namespaces.7.html
- https://man7.org/linux/man-pages/man2/move_mount.2.html
- https://man7.org/linux/man-pages/man2/pivot_root.2.html
- https://man7.org/linux/man-pages/man2/clone.2.html
