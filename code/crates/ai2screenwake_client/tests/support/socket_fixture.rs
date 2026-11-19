// socket_fixture.rs Copyright (c) 2026:AndrewC. All Rights Reserved. Do Not Distribute.
//! Private runtime fixture data; authored source always stays in its assigned worktree.

use std::{
    fs,
    os::unix::{
        fs::{MetadataExt, PermissionsExt},
        net::SocketAddr,
    },
    path::{Path, PathBuf},
};

/// Prefer this compiled checkout's target; only overlong socket paths use a short owned root.
pub fn owner_directory(socket_suffix: &str) -> tempfile::TempDir {
    let project = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../..")
        .canonicalize()
        .unwrap();
    let target = project.join("target");
    fs::create_dir_all(&target).unwrap();
    let target = target.canonicalize().unwrap();
    assert!(
        target.starts_with(&project),
        "fixture target escaped its worktree"
    );
    owner_directory_in(&target, socket_suffix)
}

/// Select by the full projected endpoint before creating a fixture, then verify actual ownership.
pub fn owner_directory_in(preferred: &Path, socket_suffix: &str) -> tempfile::TempDir {
    let suffix = Path::new(socket_suffix);
    assert!(
        !suffix.is_absolute()
            && suffix
                .components()
                .all(|part| matches!(part, std::path::Component::Normal(_)))
    );
    let fits = SocketAddr::from_pathname(preferred.join("uXXXXXX").join(suffix)).is_ok();
    let parent = if fits {
        preferred.to_path_buf()
    } else {
        PathBuf::from("/tmp")
    };
    fs::create_dir_all(&parent).unwrap();
    let parent = parent.canonicalize().unwrap();
    let directory = tempfile::Builder::new()
        .prefix("u")
        .tempdir_in(&parent)
        .unwrap();
    fs::set_permissions(directory.path(), fs::Permissions::from_mode(0o700)).unwrap();
    let metadata = fs::symlink_metadata(directory.path()).unwrap();
    assert!(metadata.is_dir() && !metadata.file_type().is_symlink());
    assert_eq!(metadata.permissions().mode() & 0o777, 0o700);
    // SAFETY: geteuid has no arguments or failure sentinel.
    assert_eq!(metadata.uid(), unsafe { libc::geteuid() });
    assert_eq!(directory.path().parent(), Some(parent.as_path()));
    let socket = directory.path().join(suffix);
    assert!(
        SocketAddr::from_pathname(&socket).is_ok(),
        "owned fixture exceeds native socket capacity: {}",
        socket.display()
    );
    println!(
        "SOCKET_FIXTURE policy={} preferred={} root={} socket_bytes={} inherited_temp={}",
        if fits { "worktree" } else { "short-owned" },
        preferred.display(),
        directory.path().display(),
        socket.as_os_str().as_encoded_bytes().len(),
        std::env::temp_dir().display()
    );
    directory
}
// end socket_fixture.rs
