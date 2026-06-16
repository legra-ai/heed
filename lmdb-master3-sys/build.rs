extern crate cc;

#[cfg(feature = "bindgen")]
extern crate bindgen;

#[cfg(feature = "bindgen")]
#[path = "bindgen.rs"]
mod generate;

use std::env;
use std::path::PathBuf;

#[cfg(all(
    feature = "mdb_idl_logn_8",
    not(any(
        feature = "mdb_idl_logn_9",
        feature = "mdb_idl_logn_10",
        feature = "mdb_idl_logn_11",
        feature = "mdb_idl_logn_12",
        feature = "mdb_idl_logn_13",
        feature = "mdb_idl_logn_14",
        feature = "mdb_idl_logn_15",
        feature = "mdb_idl_logn_16"
    ))
))]
const MDB_IDL_LOGN: u8 = 8;
#[cfg(all(
    feature = "mdb_idl_logn_9",
    not(any(
        feature = "mdb_idl_logn_10",
        feature = "mdb_idl_logn_11",
        feature = "mdb_idl_logn_12",
        feature = "mdb_idl_logn_13",
        feature = "mdb_idl_logn_14",
        feature = "mdb_idl_logn_15",
        feature = "mdb_idl_logn_16"
    ))
))]
const MDB_IDL_LOGN: u8 = 9;
#[cfg(all(
    feature = "mdb_idl_logn_10",
    not(any(
        feature = "mdb_idl_logn_11",
        feature = "mdb_idl_logn_12",
        feature = "mdb_idl_logn_13",
        feature = "mdb_idl_logn_14",
        feature = "mdb_idl_logn_15",
        feature = "mdb_idl_logn_16"
    ))
))]
const MDB_IDL_LOGN: u8 = 10;
#[cfg(all(
    feature = "mdb_idl_logn_11",
    not(any(
        feature = "mdb_idl_logn_12",
        feature = "mdb_idl_logn_13",
        feature = "mdb_idl_logn_14",
        feature = "mdb_idl_logn_15",
        feature = "mdb_idl_logn_16"
    ))
))]
const MDB_IDL_LOGN: u8 = 11;
#[cfg(all(
    feature = "mdb_idl_logn_12",
    not(any(
        feature = "mdb_idl_logn_13",
        feature = "mdb_idl_logn_14",
        feature = "mdb_idl_logn_15",
        feature = "mdb_idl_logn_16"
    ))
))]
const MDB_IDL_LOGN: u8 = 12;
#[cfg(all(
    feature = "mdb_idl_logn_13",
    not(any(
        feature = "mdb_idl_logn_14",
        feature = "mdb_idl_logn_15",
        feature = "mdb_idl_logn_16"
    ))
))]
const MDB_IDL_LOGN: u8 = 13;
#[cfg(all(
    feature = "mdb_idl_logn_14",
    not(any(feature = "mdb_idl_logn_15", feature = "mdb_idl_logn_16"))
))]
const MDB_IDL_LOGN: u8 = 14;
#[cfg(all(feature = "mdb_idl_logn_15", not(any(feature = "mdb_idl_logn_16"))))]
const MDB_IDL_LOGN: u8 = 15;
#[cfg(any(
    feature = "mdb_idl_logn_16",
    not(any(
        feature = "mdb_idl_logn_8",
        feature = "mdb_idl_logn_9",
        feature = "mdb_idl_logn_10",
        feature = "mdb_idl_logn_11",
        feature = "mdb_idl_logn_12",
        feature = "mdb_idl_logn_13",
        feature = "mdb_idl_logn_14",
        feature = "mdb_idl_logn_15",
        feature = "mdb_idl_logn_16",
    ))
))]
const MDB_IDL_LOGN: u8 = 16;

macro_rules! warn {
    ($message:expr) => {
        println!("cargo:warning={}", $message);
    };
}

fn main() {
    #[cfg(feature = "bindgen")]
    generate::generate();

    println!("cargo::rerun-if-changed=lmdb");

    let mut lmdb = PathBuf::from(&env::var("CARGO_MANIFEST_DIR").unwrap());
    lmdb.push("lmdb");
    lmdb.push("libraries");
    lmdb.push("liblmdb");

    if cfg!(feature = "fuzzer") && cfg!(feature = "fuzzer-no-link") {
        warn!("Features `fuzzer` and `fuzzer-no-link` are mutually exclusive.");
        warn!("Building with `-fsanitize=fuzzer`.");
    }

    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());
    let target_os = env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();

    // macOS uses System V semaphores (via `MDB_USE_ROBUST` below) unless the
    // caller explicitly opted into POSIX named semaphores. The SysV creation
    // path in the pinned `mdb.c` submodule has two defects (collision-prone
    // `ftok` keys and owner-only 0600 perms) that surface as intermittent
    // EACCES on `semget` across users; patch them when that path is live.
    let use_sysv_sem = target_os == "macos" && !cfg!(feature = "posix-sem");
    let mdb_c = if use_sysv_sem {
        patch_mdb_sysv_sem(&lmdb.join("mdb.c"), &out_dir.join("mdb.c"))
    } else {
        lmdb.join("mdb.c")
    };

    let mut builder = cc::Build::new();

    builder
        .define("MDB_IDL_LOGN", Some(MDB_IDL_LOGN.to_string().as_str()))
        // The patched `mdb.c` lives in OUT_DIR, so its quoted includes of
        // `lmdb.h` / `midl.h` need the original source dir on the search path.
        .include(&lmdb)
        .file(&mdb_c)
        .file(lmdb.join("midl.c"))
        // https://github.com/mozilla/lmdb/blob/b7df2cac50fb41e8bd16aab4cc5fd167be9e032a/libraries/liblmdb/Makefile#L23
        .flag_if_supported("-Wno-unused-parameter")
        .flag_if_supported("-Wbad-function-cast")
        .flag_if_supported("-Wuninitialized");

    // On Windows, link explicitly advapi32 for security functions
    #[cfg(target_os = "windows")]
    println!("cargo:rustc-link-lib=advapi32");

    if cfg!(feature = "posix-sem") {
        builder.define("MDB_USE_POSIX_SEM", None);
    }

    // On macOS, mdb.c defaults to POSIX named semaphores
    // (`MDB_USE_POSIX_SEM`), which leak across SIGKILL / panic /
    // sudden shutdown — every named semaphore lives in the
    // kernel until explicitly `sem_unlink`'d or the system reboots,
    // and the kernel-wide ceiling (`kern.posix.sem.max = 10000`)
    // is reached quickly under dev-iteration churn. Defining
    // `MDB_USE_ROBUST` skips the Apple branch that picks
    // `MDB_USE_POSIX_SEM` (see mdb.c:184-186) and falls through to
    // the BSD-family default of `MDB_USE_SYSV_SEM`, which has
    // 87 381 slots (`kern.sysv.semmni`) and is auto-cleaned by
    // `IPC_RMID` from `mdb_env_close` (with `ipcs` / `ipcrm` for
    // manual audit). Linux is unaffected — it uses
    // `MDB_USE_POSIX_MUTEX` and never enters either of these
    // branches. The `posix-sem` feature is left as an explicit
    // opt-in so anyone who deliberately needs POSIX-named sems can
    // still get them. The SysV creation path is additionally patched in OUT_DIR
    // (see `patch_mdb_sysv_sem`) to fix `ftok` key collisions and 0600 perms.
    if use_sysv_sem {
        builder.define("MDB_USE_ROBUST", "1");
    }

    if cfg!(feature = "use-valgrind") {
        builder.define("USE_VALGRIND", None);
    }

    if cfg!(feature = "asan") {
        builder.flag("-fsanitize=address");
    }

    if cfg!(feature = "fuzzer") {
        builder.flag("-fsanitize=fuzzer");
    } else if cfg!(feature = "fuzzer-no-link") {
        builder.flag("-fsanitize=fuzzer-no-link");
    }

    if cfg!(feature = "longer-keys") {
        builder.define("MDB_MAXKEYSIZE", "0");
    }

    if !cfg!(debug_assertions) {
        builder.define("NDEBUG", None);
    }

    builder.compile("liblmdb.a")
}

/// Patch the System V semaphore path of the vendored `mdb.c` and emit the
/// result to `dst`, returning that path for the `cc` build to compile.
///
/// `mdb.c` ships in a pinned git submodule, so its `mdb_env_setup_locks` SysV
/// branch can't be edited in place without dirtying the submodule. Two upstream
/// defects make `semget` fail intermittently with EACCES on macOS:
///
///  * **Key collisions.** Upstream derives the IPC key with
///    `ftok(lockpath, 'M')`, which folds only the low 16 bits of the inode into
///    the key. Distinct databases collide routinely — a collision either
///    cross-wires two unrelated envs onto one semaphore set or, across users,
///    fails the second opener with EACCES because the colliding set is private
///    to another uid. We hash the full lockfile path for a near-unique 31-bit
///    key instead.
///  * **Owner-only perms.** Upstream creates the set with the env's file mode
///    (commonly 0600). A SysV writer lock is legitimately shared — including by
///    several uids that open the same DB — so 0600 turns every cross-uid attach
///    into EACCES. We create it 0666; the lockfile and data-file perms still
///    gate who may `open` the env.
///
/// Panics if the upstream anchor text is absent (e.g. a future submodule bump
/// reworked the block) so the buggy `ftok`/0600 path can never ship silently.
fn patch_mdb_sysv_sem(src: &PathBuf, dst: &PathBuf) -> PathBuf {
    const ANCHOR: &str = "\t\tkey_t key = ftok(fname->mn_val, 'M'); /* fname is lockfile path now */\n\t\tif (key == -1)\n\t\t\tgoto fail_errno;\n\t\tsemid = semget(key, 2, (mode & 0777) | IPC_CREAT);";
    const REPLACEMENT: &str = "\t\t/* WWKG fork patch (applied by build.rs): ftok() folds only the\n\t\t * low 16 bits of the inode into the IPC key, so distinct DBs\n\t\t * collide — a collision cross-wires two envs onto one semaphore\n\t\t * set, or (across uids) fails the second opener with EACCES when\n\t\t * the set is private to another user. Derive the key from an\n\t\t * FNV-1a hash of the full lockfile path (mdb_hash is gated behind\n\t\t * MDB_USE_HASH, which this build does not enable) for a\n\t\t * near-unique 31-bit value instead. */\n\t\tunsigned long long wwkg_h = 0xcbf29ce484222325ULL;\n\t\tconst unsigned char *wwkg_p = (const unsigned char *)fname->mn_val;\n\t\tkey_t key;\n\t\twhile (*wwkg_p)\n\t\t\twwkg_h = (wwkg_h ^ *wwkg_p++) * 0x100000001b3ULL;\n\t\tkey = (key_t)(wwkg_h & 0x7fffffff);\n\t\tif (key == IPC_PRIVATE)\n\t\t\tkey = 1;\t/* 0 is IPC_PRIVATE; nudge to a stable nonzero key */\n\t\t/* 0666, not the env's mode (commonly 0600): a SysV writer lock is\n\t\t * legitimately shared, including by several uids opening the same\n\t\t * DB, so private perms would turn every cross-uid attach into\n\t\t * EACCES. The lockfile and data-file perms still gate open(). */\n\t\tsemid = semget(key, 2, 0666 | IPC_CREAT);";

    let original = std::fs::read_to_string(src).expect("read vendored mdb.c");
    assert!(
        original.contains(ANCHOR),
        "mdb.c SysV-sem anchor not found; the vendored submodule changed — \
         re-verify the ftok/0600 fix in build.rs against the new mdb.c"
    );
    std::fs::write(dst, original.replace(ANCHOR, REPLACEMENT)).expect("write patched mdb.c");
    dst.clone()
}
