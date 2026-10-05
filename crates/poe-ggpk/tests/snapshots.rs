use std::io::Cursor;

use insta::assert_yaml_snapshot;
use poe_ggpk::ggpk::scan_record_headers;
use poe_ggpk::{
    fetch_latest_patch_versions, CacheManifest, CacheManifestEntry, CacheMode, DiskCache,
    PatchCdnSource,
};

#[test]
fn patch_cdn_source_paths_are_stable() {
    let source = PatchCdnSource::poe1("3.29.3.3");
    assert_yaml_snapshot!(
        "patch_cdn_source_paths",
        serde_json::json!({
            "bundle_url": source.bundle_url("_.index.bin"),
            "cache_key": source.cache_key("_.index.bin"),
            "cache_namespace": source.cache_namespace(),
            "release_line": source.release_line(),
            "nested_cache_key": source.cache_key("Metadata/Terrain/Act1/Area.tsi"),
        })
    );
}

#[test]
fn synthetic_ggpk_record_scan_is_stable() {
    let mut bytes = Vec::new();
    bytes.extend_from_slice(&12_u64.to_le_bytes());
    bytes.extend_from_slice(b"GGPK");
    bytes.extend_from_slice(&16_u64.to_le_bytes());
    bytes.extend_from_slice(b"FREE");
    bytes.extend_from_slice(&[1, 2, 3, 4]);

    let records = scan_record_headers(&mut Cursor::new(bytes))
        .expect("scan synthetic GGPK records")
        .into_iter()
        .map(|record| {
            serde_json::json!({
                "offset": record.offset,
                "length": record.length,
                "tag": record.tag.to_string(),
            })
        })
        .collect::<Vec<_>>();

    assert_yaml_snapshot!("synthetic_ggpk_record_scan", records);
}

#[test]
fn cache_rejects_unsafe_keys() {
    let cache = DiskCache::new(".poe-layouts/cache");
    let error = cache
        .fetch_url("../escape.bin", "https://patch.poecdn.com/example")
        .expect_err("unsafe cache key should fail before network access");
    assert_yaml_snapshot!("unsafe_cache_key_error", error.to_string());
}

#[test]
fn offline_cache_miss_is_stable() {
    let temp = tempfile::tempdir().expect("temp cache dir");
    let cache = DiskCache::new(temp.path());
    let error = cache
        .fetch_url_with_mode(
            "poe1/3.29/patches/3.29.3.3/Bundles2/_.index.bin",
            "https://patch.poecdn.com/3.29.3.3/Bundles2/_.index.bin",
            CacheMode::Offline,
        )
        .expect_err("offline mode should fail when bytes are missing");
    assert_yaml_snapshot!("offline_cache_miss_error", error.to_string());
}

#[test]
fn manifest_verification_reports_missing_entries() {
    let temp = tempfile::tempdir().expect("temp cache dir");
    let cache = DiskCache::new(temp.path());
    let manifest = CacheManifest {
        schema_version: 1,
        entries: vec![CacheManifestEntry {
            key: "poe1/3.29/patches/3.29.3.3/Bundles2/_.index.bin".to_owned(),
            url: "https://patch.poecdn.com/3.29.3.3/Bundles2/_.index.bin".to_owned(),
            byte_len: 20_398_470,
            blake3: "97b94b289597b782e34c2b0c0e5325d65b6711db97b337061a7e2426df990a58".to_owned(),
        }],
        namespace: "poe1/3.29".to_owned(),
    };
    let verification = cache
        .verify_manifest(&manifest)
        .expect("verify missing cache manifest");
    assert_yaml_snapshot!("manifest_verification_missing", verification);
}

#[test]
#[ignore = "hits live patch CDN; run with POE_LAYOUTS_PATCH_VERSION or let it query poe-versions.obsoleet.org"]
fn live_poe1_index_snapshot() {
    let patch_version = std::env::var("POE_LAYOUTS_PATCH_VERSION").unwrap_or_else(|_| {
        fetch_latest_patch_versions()
            .expect("fetch latest patch versions")
            .poe
    });
    let temp = tempfile::tempdir().expect("temp cache dir");
    let cache = DiskCache::new(temp.path());
    let snapshot = PatchCdnSource::poe1(patch_version)
        .snapshot_index(&cache, CacheMode::Online)
        .expect("fetch live PoE1 patch index");
    assert_yaml_snapshot!("live_poe1_index", snapshot);
}
