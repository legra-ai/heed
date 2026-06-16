//! Regression tests for the WWKG fork's System V semaphore patch (see
//! `build.rs::patch_mdb_sysv_sem`).
//!
//! On macOS the `MDB_USE_ROBUST` build flag routes LMDB locking through System V
//! semaphores. Upstream `mdb.c` creates each set with `ftok(lockpath, 'M')` —
//! which folds only the low 16 bits of the inode into the key, so distinct
//! databases collide — and with the env's own file mode (commonly 0600), so a
//! cross-uid collision fails the second opener with EACCES instead of sharing
//! the lock. The patch hashes the full lockfile path for a near-unique key and
//! creates the set 0666. These tests assert both properties through `ipcs`.
//!
//! macOS-only: the SysV path does not compile elsewhere.
#![cfg(target_os = "macos")]

use std::ffi::CString;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::ptr;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use lmdb_master3_sys::{MDB_env, mdb_env_close, mdb_env_create, mdb_env_open};

/// FNV-1a over the lockfile path, masked to a 31-bit System V key — the exact
/// derivation the patched `mdb.c` performs. Must stay in lockstep with the C.
fn expected_sem_key(lockfile_path: &str) -> i64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in lockfile_path.as_bytes() {
        h = (h ^ u64::from(*b)).wrapping_mul(0x0000_0100_0000_01b3);
    }
    let key = (h & 0x7fff_ffff) as i64;
    if key == 0 { 1 } else { key }
}

/// Mode field (e.g. `--ra-ra-ra-`) of the System V semaphore set whose key
/// matches `key`, or `None` if no such set currently exists.
fn sem_mode_for_key(key: i64) -> Option<String> {
    let out = Command::new("ipcs").arg("-s").output().expect("run ipcs -s");
    let text = String::from_utf8_lossy(&out.stdout);
    for line in text.lines() {
        let cols: Vec<&str> = line.split_whitespace().collect();
        // `T  ID  KEY  MODE  OWNER  GROUP`
        if cols.len() < 4 || cols[0] != "s" {
            continue;
        }
        let row_key = cols[2].strip_prefix("0x").and_then(|h| i64::from_str_radix(h, 16).ok());
        if row_key == Some(key) {
            return Some(cols[3].to_owned());
        }
    }
    None
}

/// A freshly created, uniquely named directory under the temp dir. Unique names
/// keep each env's hashed sem key disjoint from every other process on the box.
fn unique_env_dir(tag: &str) -> PathBuf {
    static COUNTER: AtomicU32 = AtomicU32::new(0);
    let nanos = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!(
        "wwkg-sysv-sem-{tag}-{}-{nanos}-{n}",
        std::process::id()
    ));
    std::fs::create_dir_all(&dir).expect("create env dir");
    dir
}

/// Opens an LMDB env at `dir` (default subdir mode) requesting file mode 0600,
/// to prove the patch forces 0666 on the semaphore regardless of the env mode.
/// Returns the live env pointer; the caller must `mdb_env_close` it.
fn open_env(dir: &Path) -> *mut MDB_env {
    let cpath = CString::new(dir.to_str().unwrap()).unwrap();
    unsafe {
        let mut env: *mut MDB_env = ptr::null_mut();
        let rc = mdb_env_create(&mut env);
        assert_eq!(rc, 0, "mdb_env_create failed: {rc}");
        let rc = mdb_env_open(env, cpath.as_ptr(), 0, 0o600 as libc::mode_t);
        assert_eq!(rc, 0, "mdb_env_open({}) failed: {rc}", dir.display());
        env
    }
}

#[test]
fn sysv_sem_is_world_writable_and_collision_free() {
    let dir_a = unique_env_dir("a");
    let dir_b = unique_env_dir("b");

    let env_a = open_env(&dir_a);
    let env_b = open_env(&dir_b);

    let key_a = expected_sem_key(&format!("{}/lock.mdb", dir_a.display()));
    let key_b = expected_sem_key(&format!("{}/lock.mdb", dir_b.display()));

    // Collision-free: distinct lockfile paths hash to distinct keys (ftok's
    // 16-bit inode fold would routinely collide here).
    assert_ne!(key_a, key_b, "two distinct envs must derive distinct sem keys");

    // World-writable (0666): a SysV writer lock must be shareable across uids,
    // not the env's private 0600 — that's the EACCES fix. ipcs renders sem
    // perms with `r`/`a` (alter) bits, so 0666 shows as `--ra-ra-ra-`; the old
    // buggy 0600 would show as `--ra-------`.
    let mode_a = sem_mode_for_key(key_a)
        .unwrap_or_else(|| panic!("no SysV sem found for env A (key {key_a:#x})"));
    let mode_b = sem_mode_for_key(key_b)
        .unwrap_or_else(|| panic!("no SysV sem found for env B (key {key_b:#x})"));

    assert_eq!(mode_a, "--ra-ra-ra-", "env A semaphore must be world-writable (0666)");
    assert_eq!(mode_b, "--ra-ra-ra-", "env B semaphore must be world-writable (0666)");

    // Closing the last user IPC_RMID-removes the set (the graceful-exit path).
    unsafe {
        mdb_env_close(env_a);
        mdb_env_close(env_b);
    }
    let _ = std::fs::remove_dir_all(&dir_a);
    let _ = std::fs::remove_dir_all(&dir_b);
}
