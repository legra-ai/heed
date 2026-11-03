//! Exercise LMDB's page-layout boundary with independently padded keys and values.

use std::ffi::{CStr, CString};
use std::path::PathBuf;
use std::ptr;
use std::slice;
use std::time::{SystemTime, UNIX_EPOCH};

use lmdb_master3_sys::*;

fn check(code: i32) {
    assert_eq!(code, 0, "{}", unsafe {
        CStr::from_ptr(mdb_strerror(code)).to_string_lossy()
    });
}

struct Environment {
    raw: *mut MDB_env,
    path: PathBuf,
}

impl Environment {
    fn new(page_size: i32) -> Self {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!("lmdb-split-{}-{nonce}", std::process::id()));
        // This regression directly exercises the synchronous native LMDB I/O boundary.
        std::fs::create_dir(&path).unwrap();
        let name = CString::new(path.to_str().unwrap()).unwrap();
        let mut raw = ptr::null_mut();
        unsafe {
            check(mdb_env_create(&mut raw));
            check(mdb_env_set_mapsize(raw, 256 * 1024 * 1024));
            check(mdb_env_set_pagesize(raw, page_size));
            check(mdb_env_open(raw, name.as_ptr(), MDB_NOSYNC, 0o600));
        }
        Self { raw, path }
    }
}

impl Drop for Environment {
    fn drop(&mut self) {
        unsafe { mdb_env_close(self.raw) };
        std::fs::remove_dir_all(&self.path).unwrap();
    }
}

#[test]
fn odd_keys_and_values_fit_after_page_splits() {
    for page_size in [4096, 16384] {
        let env = Environment::new(page_size);
        let mut transaction = ptr::null_mut();
        let mut database = 0;
        unsafe {
            check(mdb_txn_begin(env.raw, ptr::null_mut(), 0, &mut transaction));
            check(mdb_dbi_open(
                transaction,
                ptr::null(),
                MDB_CREATE,
                &mut database,
            ));
            check(mdb_txn_commit(transaction));
        }
        let mut random = 0x9182_7364_u32;
        for ordinal in 0..3000_u32 {
            random = random.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            let identity = ordinal.wrapping_mul(65_521) % 65_536;
            let mut key_bytes = [b'x'; 53];
            key_bytes[51..].copy_from_slice(&(identity as u16).to_be_bytes());
            random = random.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            let size = 1 + 2 * ((random >> 16) as usize % (page_size as usize / 4 - 54));
            // bounded: one payload smaller than half a page, across 3000 fixture keys.
            let mut payload = vec![ordinal as u8; size];
            let mut key = MDB_val {
                mv_size: key_bytes.len(),
                mv_data: key_bytes.as_mut_ptr().cast(),
            };
            let mut value = MDB_val {
                mv_size: payload.len(),
                mv_data: payload.as_mut_ptr().cast(),
            };
            unsafe {
                check(mdb_txn_begin(env.raw, ptr::null_mut(), 0, &mut transaction));
                let code = mdb_put(transaction, database, &mut key, &mut value, 0);
                if code != 0 {
                    mdb_txn_abort(transaction);
                    panic!(
                        "page_size={page_size} ordinal={ordinal} size={size}: {}",
                        CStr::from_ptr(mdb_strerror(code)).to_string_lossy()
                    );
                }
                check(mdb_txn_commit(transaction));
                check(mdb_txn_begin(
                    env.raw,
                    ptr::null_mut(),
                    MDB_RDONLY,
                    &mut transaction,
                ));
                check(mdb_get(transaction, database, &mut key, &mut value));
                assert_eq!(
                    slice::from_raw_parts(value.mv_data.cast::<u8>(), value.mv_size),
                    payload
                );
                mdb_txn_abort(transaction);
            }
        }
    }
}
