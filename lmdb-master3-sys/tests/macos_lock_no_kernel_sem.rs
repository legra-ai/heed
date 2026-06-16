//! Regression test for the WWKG fork's macOS locking choice (see `build.rs`).
//!
//! macOS LMDB must NOT use a kernel-namespace lock primitive (POSIX *named*
//! semaphores or System V semaphores): both leak across SIGKILL / panic — the
//! object lives in the kernel until explicitly removed — so dev-iteration churn
//! (one env per workspace/tempdir × many killed processes) accumulates orphans
//! without bound and eventually exhausts the kernel ceiling, after which every
//! `mdb_env_open` fails. The fork routes macOS through `MDB_USE_POSIX_MUTEX`, a
//! pthread mutex embedded in the `lock.mdb` mmap that holds no kernel resource
//! and dies with the file.
//!
//! This test guards against a silent revert to the System V path: opening an env
//! must create no SysV semaphore set at the key that path would have used.
//!
//! macOS-only: the relevant lock-implementation branches are macOS-specific.
#![cfg(target_os = "macos")]

use std::ffi::CString;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::ptr;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use lmdb_master3_sys::{
    MDB_env, MDB_txn, MDB_val, mdb_dbi_open, mdb_env_close, mdb_env_create, mdb_env_open,
    mdb_get, mdb_put, mdb_txn_begin, mdb_txn_commit,
};

/// The System V key the (now-removed) SysV path would derive for `lockfile_path`
/// — FNV-1a of the full path, masked to 31 bits. Used here only to prove that no
/// such semaphore exists, i.e. that the SysV path is not compiled.
fn sysv_key_for_lockfile(lockfile_path: &str) -> i64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in lockfile_path.as_bytes() {
        h = (h ^ u64::from(*b)).wrapping_mul(0x0000_0100_0000_01b3);
    }
    let key = (h & 0x7fff_ffff) as i64;
    if key == 0 { 1 } else { key }
}

/// Whether a System V semaphore set with key `key` currently exists.
fn sysv_sem_exists(key: i64) -> bool {
    let out = Command::new("ipcs").arg("-s").output().expect("run ipcs -s");
    let text = String::from_utf8_lossy(&out.stdout);
    text.lines().any(|line| {
        let cols: Vec<&str> = line.split_whitespace().collect();
        // `T  ID  KEY  MODE  OWNER  GROUP`
        cols.len() >= 3
            && cols[0] == "s"
            && cols[2].strip_prefix("0x").and_then(|h| i64::from_str_radix(h, 16).ok())
                == Some(key)
    })
}

/// A freshly created, uniquely named directory under the temp dir.
fn unique_env_dir(tag: &str) -> PathBuf {
    static COUNTER: AtomicU32 = AtomicU32::new(0);
    let nanos = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!(
        "wwkg-lmdb-lock-{tag}-{}-{nanos}-{n}",
        std::process::id()
    ));
    std::fs::create_dir_all(&dir).expect("create env dir");
    dir
}

/// Opens an LMDB env at `dir` (default subdir mode). Returns the live env
/// pointer; the caller must `mdb_env_close` it.
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
fn env_open_creates_no_kernel_semaphore() {
    let dir = unique_env_dir("a");
    let key = sysv_key_for_lockfile(&format!("{}/lock.mdb", dir.display()));

    // The unique path means no other process could already hold this key.
    assert!(!sysv_sem_exists(key), "stale sem at unique key before open — impossible");

    let env = open_env(&dir);

    // Under MDB_USE_POSIX_MUTEX the lock lives in lock.mdb; no SysV sem is
    // created. A revert to the System V path would create a set at exactly this
    // key and fail the assertion.
    assert!(
        !sysv_sem_exists(key),
        "opening an env created a System V semaphore (key {key:#x}) — macOS LMDB \
         must use MDB_USE_POSIX_MUTEX, not a leaking kernel-namespace lock"
    );

    unsafe { mdb_env_close(env) };
    let _ = std::fs::remove_dir_all(&dir);
}

/// Exercises the file-embedded pthread mutex through real write/read
/// transactions: a write txn must take and release the writer lock, and the
/// committed value must survive a close + reopen. This proves the
/// `MDB_USE_POSIX_MUTEX` locking path is functional at runtime, not merely that
/// the env opens.
#[test]
fn posix_mutex_write_read_round_trips() {
    let dir = unique_env_dir("rw");
    let key = b"k";
    let val = b"v42";

    unsafe {
        // Write and commit.
        let env = open_env(&dir);
        let mut txn: *mut MDB_txn = ptr::null_mut();
        assert_eq!(mdb_txn_begin(env, ptr::null_mut(), 0, &mut txn), 0, "begin write txn");
        let mut dbi: lmdb_master3_sys::MDB_dbi = 0;
        assert_eq!(mdb_dbi_open(txn, ptr::null(), 0, &mut dbi), 0, "open dbi");
        let mut k = MDB_val { mv_size: key.len(), mv_data: key.as_ptr() as *mut _ };
        let mut d = MDB_val { mv_size: val.len(), mv_data: val.as_ptr() as *mut _ };
        assert_eq!(mdb_put(txn, dbi, &mut k, &mut d, 0), 0, "put");
        assert_eq!(mdb_txn_commit(txn), 0, "commit");
        mdb_env_close(env);

        // Reopen and read back.
        let env = open_env(&dir);
        let mut rtxn: *mut MDB_txn = ptr::null_mut();
        const MDB_RDONLY: libc::c_uint = 0x20000;
        assert_eq!(mdb_txn_begin(env, ptr::null_mut(), MDB_RDONLY, &mut rtxn), 0, "begin read txn");
        let mut dbi: lmdb_master3_sys::MDB_dbi = 0;
        assert_eq!(mdb_dbi_open(rtxn, ptr::null(), 0, &mut dbi), 0, "open dbi (read)");
        let mut k = MDB_val { mv_size: key.len(), mv_data: key.as_ptr() as *mut _ };
        let mut out = MDB_val { mv_size: 0, mv_data: ptr::null_mut() };
        assert_eq!(mdb_get(rtxn, dbi, &mut k, &mut out), 0, "get");
        let got = std::slice::from_raw_parts(out.mv_data as *const u8, out.mv_size);
        assert_eq!(got, val, "read-back value must match committed value");
        mdb_env_close(env);
    }

    let _ = std::fs::remove_dir_all(&dir);
}
