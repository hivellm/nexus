//! Regression coverage for `RecordStore::new_temporary()`'s self-cleaning
//! directory: the temp directory backing the store must be removed once
//! the store (and every clone sharing its mmaps) has been dropped, and
//! must NOT be removed while any clone is still alive.

use nexus_core::storage::RecordStore;

#[test]
fn temporary_store_directory_is_removed_when_store_drops() {
    let store = RecordStore::new_temporary().expect("failed to create temporary record store");
    let dir = store.path().to_path_buf();

    assert!(
        dir.exists(),
        "temp directory should exist immediately after creation"
    );

    drop(store);

    assert!(
        !dir.exists(),
        "temp directory should be removed once the (only) store handle drops"
    );
}

#[test]
fn temporary_store_directory_survives_until_last_clone_drops() {
    let store = RecordStore::new_temporary().expect("failed to create temporary record store");
    let dir = store.path().to_path_buf();
    assert!(dir.exists(), "temp directory should exist after creation");

    let clone = store.clone();
    assert!(
        dir.exists(),
        "cloning must not remove the directory (it shares the same mmaps)"
    );

    drop(store);
    assert!(
        dir.exists(),
        "dropping only the FIRST handle must not remove the directory while a clone \
         still holds a live reference to the shared mmaps"
    );

    drop(clone);
    assert!(
        !dir.exists(),
        "the directory must be removed once the LAST clone drops"
    );
}
