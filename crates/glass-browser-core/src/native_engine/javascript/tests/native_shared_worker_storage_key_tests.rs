use super::{NativeOrigin, NativeSharedWorkerStorageKey};
use url::Url;

#[test]
fn tuple_origins_share_storage_key_across_documents() {
    let origin = NativeOrigin::from_url(&Url::parse("https://example.test/one").unwrap()).unwrap();
    assert_eq!(
        NativeSharedWorkerStorageKey::for_document(&origin, "target-a", "frame-a", 1),
        NativeSharedWorkerStorageKey::for_document(&origin, "target-b", "frame-b", 7),
    );
}

#[test]
fn opaque_origins_remain_document_scoped() {
    let origin = NativeOrigin::Opaque;
    let first_document =
        NativeSharedWorkerStorageKey::for_document(&origin, "target-a", "frame-a", 1);
    assert_eq!(
        first_document,
        NativeSharedWorkerStorageKey::for_document(&origin, "target-a", "frame-a", 1),
    );
    assert_ne!(
        first_document,
        NativeSharedWorkerStorageKey::for_document(&origin, "target-a", "frame-a", 2),
    );
    assert_ne!(
        first_document,
        NativeSharedWorkerStorageKey::for_document(&origin, "target-b", "frame-a", 1),
    );
}
