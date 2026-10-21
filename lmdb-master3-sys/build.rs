extern crate cc;

#[cfg(feature = "bindgen")]
extern crate bindgen;

#[cfg(feature = "bindgen")]
#[path = "bindgen.rs"]
mod generate;

use std::env;
use std::fs;
use std::path::Path;
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

/// Reserve the AEAD tag once per encrypted overflow allocation.
///
/// Cargo's synchronous build-script boundary creates a patched translation
/// unit in OUT_DIR; the tracked upstream submodule remains unchanged.
fn overflow_tag_source(lmdb: &Path) -> PathBuf {
    let mut source = fs::read_to_string(lmdb.join("mdb.c"))
        .expect("read LMDB source for encrypted overflow allocation repair");
    for (old, new) in [
        (
            "OVPAGES(data->mv_size, env->me_psize)",
            "OVPAGES(data->mv_size + env->me_esumsize, env->me_psize)",
        ),
        (
            "OVPAGES(data->mv_size, mc->mc_txn->mt_env->me_psize)",
            "OVPAGES(data->mv_size + mc->mc_txn->mt_env->me_esumsize, mc->mc_txn->mt_env->me_psize)",
        ),
    ] {
        assert_eq!(source.matches(old).count(), 1, "LMDB overflow allocation source changed: {old}");
        source = source.replace(old, new);
    }
    let output =
        PathBuf::from(env::var_os("OUT_DIR").expect("Cargo OUT_DIR")).join("mdb-overflow-tag.c");
    fs::write(&output, source).expect("write patched LMDB translation unit");
    output
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

    let target_os = env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    let macos_posix_mutex = target_os == "macos" && !cfg!(feature = "posix-sem");

    let mut builder = cc::Build::new();

    builder
        .define("MDB_IDL_LOGN", Some(MDB_IDL_LOGN.to_string().as_str()))
        .include(&lmdb)
        .file(overflow_tag_source(&lmdb))
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

    // On macOS, mdb.c defaults to POSIX *named* semaphores (`MDB_USE_POSIX_SEM`,
    // see mdb.c:184-186). Both kernel-namespace lock primitives the Apple branch
    // can reach — POSIX named sems and System V sems — LEAK across SIGKILL /
    // panic / sudden shutdown: the object lives in the kernel until explicitly
    // removed, so dev-iteration churn (one LMDB env per workspace/tempdir × many
    // killed processes) accumulates orphans without bound and eventually hits the
    // kernel ceiling (`kern.posix.sem.max=10000` / `kern.sysv.semmni`), after
    // which every `mdb_env_open` fails (ENOSPC, or EACCES on a cross-uid SysV
    // key collision).
    //
    // Instead, route macOS through `MDB_USE_POSIX_MUTEX`: the reader/writer locks
    // become pthread `PTHREAD_PROCESS_SHARED` mutexes embedded in the `lock.mdb`
    // mmap. They hold NO kernel-namespace resource, so they cannot leak or be
    // exhausted — the lock simply dies with the file. This is exactly the
    // primitive Linux already uses.
    //
    // Reaching that path on Apple needs a specific define combo (mdb.c branch
    // logic): `MDB_USE_ROBUST=0` makes the Apple branch skip POSIX-named sems by
    // *defined-ness* (mdb.c:184) while leaving `MDB_ROBUST_SUPPORTED` OFF by
    // *value* (mdb.c:374) — essential, because macOS defines `EOWNERDEAD` but not
    // `PTHREAD_MUTEX_ROBUST`, so the robust path would fail to compile.
    // `MDB_USE_POSIX_MUTEX=1` then skips the SysV branch (mdb.c:187).
    //
    // The mutexes are non-robust (macOS has no robust-mutex support), so a
    // process killed mid-write leaves the lock held; the next *exclusive* opener
    // reinitialises the lock region, which covers the normal restart and the
    // per-tempdir test cases. WWKG production is Linux (robust mutexes), so the
    // non-robust window only ever applies to the macOS dev host. The `posix-sem`
    // feature remains an explicit opt-in for the original POSIX-named-sem flavour.
    if macos_posix_mutex {
        builder.define("MDB_USE_ROBUST", "0");
        builder.define("MDB_USE_POSIX_MUTEX", "1");
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
