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

    let mut builder = cc::Build::new();

    builder
        .define("MDB_IDL_LOGN", Some(MDB_IDL_LOGN.to_string().as_str()))
        .file(lmdb.join("mdb.c"))
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
    // still get them.
    let target_os = env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    if target_os == "macos" && !cfg!(feature = "posix-sem") {
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
