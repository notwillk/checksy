#[cfg(any(target_os = "linux", target_os = "macos"))]
use std::fs::{File, OpenOptions};

/// Serialize compiled-binary `check --fix` cases so parallel contract tests do
/// not contend on the production per-user semaphore they are exercising.
#[cfg(any(target_os = "linux", target_os = "macos"))]
pub fn provisioning_test_guard() -> File {
    let directory = std::path::Path::new(env!("CARGO_TARGET_TMPDIR"));
    std::fs::create_dir_all(directory).expect("create Cargo integration-test temp directory");
    let path = directory.join("checksy-provisioning-contract.lock");
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(&path)
        .unwrap_or_else(|error| panic!("open {}: {error}", path.display()));
    loop {
        match rustix::fs::flock(&file, rustix::fs::FlockOperation::LockExclusive) {
            Ok(()) => return file,
            Err(rustix::io::Errno::INTR) => {}
            Err(error) => panic!("lock {}: {error}", path.display()),
        }
    }
}

#[cfg(not(any(target_os = "linux", target_os = "macos")))]
pub fn provisioning_test_guard() {}
