use std::io::Cursor;
use std::path::Path;

use insta::assert_yaml_snapshot;
use poe_content::ggpk::scan_record_headers;
use poe_content::{
    decompress_bundle, decompress_bundle_slice, fetch_latest_patch_versions, hydrate_index_bundle,
    murmur64a_lower, parse_bundle_header, parse_index_bundle, CacheManifest, CacheManifestEntry,
    CacheMode, DiskCache, PatchCdnSource, StoredBundleDecoder,
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
fn bundle_header_parse_is_stable() {
    let mut bytes = vec![0; 60];
    bytes[0..4].copy_from_slice(&65_536_u32.to_le_bytes());
    bytes[36..40].copy_from_slice(&2_u32.to_le_bytes());
    bytes[40..44].copy_from_slice(&32_768_u32.to_le_bytes());
    bytes.extend_from_slice(&101_u32.to_le_bytes());
    bytes.extend_from_slice(&202_u32.to_le_bytes());
    bytes.extend_from_slice(&[1, 2, 3]);

    let header = parse_bundle_header(&bytes).expect("parse synthetic bundle header");
    assert_yaml_snapshot!("bundle_header", header_for_snapshot(&header));
}

#[test]
fn stored_bundle_decompression_is_stable() {
    let bundle = stored_bundle(&[b"abcd".as_slice(), b"efgh".as_slice()], 4);
    let mut decoder = StoredBundleDecoder;
    let whole = decompress_bundle(&bundle, &mut decoder).expect("decompress stored bundle");

    let mut decoder = StoredBundleDecoder;
    let mut slice = vec![0; 5];
    decompress_bundle_slice(&bundle, 2, &mut slice, &mut decoder)
        .expect("decompress stored bundle slice");

    assert_yaml_snapshot!(
        "stored_bundle_decompression",
        serde_json::json!({
            "whole": String::from_utf8(whole).expect("test utf-8"),
            "slice_offset_2_len_5": String::from_utf8(slice).expect("test utf-8"),
        })
    );
}

#[test]
fn murmur64a_path_hashes_are_stable() {
    assert_yaml_snapshot!(
        "murmur64a_path_hashes",
        serde_json::json!({
            "metadata/worldareas.datc64": format!("{:016x}", murmur64a_lower("Metadata/WorldAreas.datc64")),
            "metadata/terrain/act1/area.tsi": format!("{:016x}", murmur64a_lower("Metadata/Terrain/Act1/Area.tsi")),
            "metadata/terrain/act1/area.dgr": format!("{:016x}", murmur64a_lower("Metadata/Terrain/Act1/Area.dgr")),
        })
    );
}

#[test]
fn decompressed_index_bundle_parse_and_lookup_are_stable() {
    let bytes = synthetic_index_bundle();
    let index = parse_index_bundle(&bytes).expect("parse synthetic decompressed index");
    let world_areas = index
        .file_location("Metadata/WorldAreas.datc64")
        .expect("lookup world areas")
        .expect("world areas should be present");
    let area_tsi = index
        .file_location("Metadata/Terrain/Act1/Area.tsi")
        .expect("lookup area tsi")
        .expect("area tsi should be present");
    let missing = index
        .file_location("Metadata/Terrain/Act1/Missing.tsi")
        .expect("lookup missing path");
    assert_yaml_snapshot!(
        "decompressed_index_bundle",
        serde_json::json!({
            "summary": index.summary(),
            "bundles": index.bundles,
            "files": index.files.iter().map(file_for_snapshot).collect::<Vec<_>>(),
            "directories": index.directories.iter().map(directory_for_snapshot).collect::<Vec<_>>(),
            "world_areas": world_areas,
            "area_tsi": area_tsi,
            "missing": missing,
        })
    );
}

#[test]
fn hydrated_index_bundle_double_unpack_is_stable() {
    let path_reps_bundle = stored_bundle(&[synthetic_path_reps().as_slice()], 1024);
    let index_payload = synthetic_index_bundle_with_path_reps(&path_reps_bundle);
    let outer_bundle = stored_bundle(&[index_payload.as_slice()], 65_536);
    let mut decoder = StoredBundleDecoder;
    let hydrated = hydrate_index_bundle(&outer_bundle, &mut decoder).expect("hydrate index bundle");
    let logical_paths = hydrated
        .logical_paths()
        .expect("unpack hydrated logical paths");
    let root_directories = poe_content::root_directories(&logical_paths);
    let world_areas = hydrated
        .index
        .file_location("Metadata/WorldAreas.datc64")
        .expect("lookup world areas")
        .expect("world areas should be present");

    assert_yaml_snapshot!(
        "hydrated_index_bundle",
        serde_json::json!({
            "summary": hydrated.index.summary(),
            "logical_paths": logical_paths,
            "root_directories": root_directories,
            "world_areas": world_areas,
        })
    );
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
fn cache_rejects_empty_keys() {
    let cache = DiskCache::new(".poe-layouts/cache");
    let error = cache
        .preview_clear_key("")
        .expect_err("empty cache key should not resolve to cache root");
    assert_yaml_snapshot!("empty_cache_key_error", error.to_string());
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
fn clear_release_line_cache_removes_only_that_namespace() {
    let temp = tempfile::tempdir().expect("temp cache dir");
    write_test_file(
        temp.path(),
        "poe1/3.29/patches/3.29.3.3/Bundles2/_.index.bin",
        b"index",
    );
    write_test_file(
        temp.path(),
        "poe1/3.29/patches/3.29.3.3/Bundles2/_.index.bin.json",
        b"{}",
    );
    write_test_file(
        temp.path(),
        "poe1/3.28/patches/3.28.2.1/Bundles2/_.index.bin",
        b"older",
    );

    let cache = DiskCache::new(temp.path());
    let preview = cache
        .preview_clear_key("poe1/3.29")
        .expect("preview release-line clear");
    assert_yaml_snapshot!(
        "preview_clear_release_line",
        report_for_snapshot(temp.path(), &preview)
    );

    let report = cache
        .clear_key("poe1/3.29")
        .expect("clear release-line cache");
    assert_yaml_snapshot!(
        "clear_release_line",
        report_for_snapshot(temp.path(), &report)
    );
    assert!(!temp.path().join("poe1/3.29").exists());
    assert!(temp.path().join("poe1/3.28").exists());
}

#[test]
fn clear_exact_patch_cache_keeps_release_line() {
    let temp = tempfile::tempdir().expect("temp cache dir");
    write_test_file(
        temp.path(),
        "poe1/3.29/patches/3.29.3.3/Bundles2/_.index.bin",
        b"index",
    );
    write_test_file(
        temp.path(),
        "poe1/3.29/patches/3.29.3.2/Bundles2/_.index.bin",
        b"previous",
    );

    let cache = DiskCache::new(temp.path());
    let report = cache
        .clear_key("poe1/3.29/patches/3.29.3.3")
        .expect("clear exact patch cache");
    assert_yaml_snapshot!(
        "clear_exact_patch",
        report_for_snapshot(temp.path(), &report)
    );
    assert!(!temp.path().join("poe1/3.29/patches/3.29.3.3").exists());
    assert!(temp.path().join("poe1/3.29/patches/3.29.3.2").exists());
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

fn write_test_file(root: &Path, key: &str, bytes: &[u8]) {
    let path = root.join(key);
    std::fs::create_dir_all(path.parent().expect("test file parent"))
        .expect("create test cache dirs");
    std::fs::write(path, bytes).expect("write test cache file");
}

fn report_for_snapshot(root: &Path, report: &poe_content::CacheClearReport) -> serde_json::Value {
    serde_json::json!({
        "key": report.key,
        "path": report.path.strip_prefix(root).expect("report under cache root").display().to_string(),
        "existed": report.existed,
        "removed": report.removed,
        "file_count": report.file_count,
        "directory_count": report.directory_count,
        "byte_len": report.byte_len,
    })
}

fn header_for_snapshot(header: &poe_content::BundleHeader) -> serde_json::Value {
    serde_json::json!({
        "decompressed_data_size": header.decompressed_data_size,
        "chunk_count": header.chunk_count,
        "compression_granularity": header.compression_granularity,
        "chunk_sizes": header.chunk_sizes,
        "payload_offset": header.payload_offset,
    })
}

fn synthetic_index_bundle() -> Vec<u8> {
    synthetic_index_bundle_with_path_reps(b"path-reps-placeholder")
}

fn synthetic_index_bundle_with_path_reps(path_reps: &[u8]) -> Vec<u8> {
    let mut bytes = Vec::new();
    push_i32(&mut bytes, 2);
    push_bundle(&mut bytes, "Art/Textures", 1_024);
    push_bundle(&mut bytes, "Metadata/Terrain", 2_048);

    push_i32(&mut bytes, 2);
    push_file(&mut bytes, "Metadata/WorldAreas.datc64", 0, 128, 42);
    push_file(&mut bytes, "Metadata/Terrain/Act1/Area.tsi", 1, 512, 77);

    push_i32(&mut bytes, 1);
    push_directory(&mut bytes, "metadata", 0, 0, 0);

    bytes.extend_from_slice(path_reps);
    bytes
}

fn push_bundle(bytes: &mut Vec<u8>, name: &str, decompressed_size: u32) {
    push_i32(
        bytes,
        i32::try_from(name.len()).expect("test bundle name length fits"),
    );
    bytes.extend_from_slice(name.as_bytes());
    bytes.extend_from_slice(&decompressed_size.to_le_bytes());
}

fn push_file(bytes: &mut Vec<u8>, path: &str, bundle_index: u32, offset: u32, size: u32) {
    bytes.extend_from_slice(&murmur64a_lower(path).to_le_bytes());
    bytes.extend_from_slice(&bundle_index.to_le_bytes());
    bytes.extend_from_slice(&offset.to_le_bytes());
    bytes.extend_from_slice(&size.to_le_bytes());
}

fn push_directory(
    bytes: &mut Vec<u8>,
    path: &str,
    path_reps_offset: u32,
    direct_size: u32,
    recursive_size: u32,
) {
    bytes.extend_from_slice(&murmur64a_lower(path).to_le_bytes());
    bytes.extend_from_slice(&path_reps_offset.to_le_bytes());
    bytes.extend_from_slice(&direct_size.to_le_bytes());
    bytes.extend_from_slice(&recursive_size.to_le_bytes());
}

fn push_i32(bytes: &mut Vec<u8>, value: i32) {
    bytes.extend_from_slice(&value.to_le_bytes());
}

fn synthetic_path_reps() -> Vec<u8> {
    let mut bytes = Vec::new();
    push_path_rep_toggle(&mut bytes);
    push_path_rep(&mut bytes, 0, "Metadata/");
    push_path_rep(&mut bytes, 0, "Data/");
    push_path_rep_toggle(&mut bytes);
    push_path_rep(&mut bytes, 1, "WorldAreas.datc64");
    push_path_rep(&mut bytes, 1, "Terrain/Act1/Area.tsi");
    push_path_rep(&mut bytes, 2, "BaseItemTypes.datc64");
    bytes
}

fn push_path_rep_toggle(bytes: &mut Vec<u8>) {
    push_i32(bytes, 0);
}

fn push_path_rep(bytes: &mut Vec<u8>, base_rep_index: i32, suffix: &str) {
    push_i32(bytes, base_rep_index + 1);
    bytes.extend_from_slice(suffix.as_bytes());
    bytes.push(0);
}

fn file_for_snapshot(entry: &poe_content::FileIndexEntry) -> serde_json::Value {
    serde_json::json!({
        "path_hash": format!("{:016x}", entry.path_hash),
        "bundle_index": entry.bundle_index,
        "offset": entry.offset,
        "size": entry.size,
    })
}

fn directory_for_snapshot(entry: &poe_content::DirectoryIndexEntry) -> serde_json::Value {
    serde_json::json!({
        "path_hash": format!("{:016x}", entry.path_hash),
        "path_reps_offset": entry.path_reps_offset,
        "direct_size": entry.direct_size,
        "recursive_size": entry.recursive_size,
    })
}

fn stored_bundle(chunks: &[&[u8]], compression_granularity: u32) -> Vec<u8> {
    let decompressed_size = chunks
        .iter()
        .map(|chunk| u32::try_from(chunk.len()).expect("test chunk len fits u32"))
        .sum::<u32>();
    let mut bytes = vec![0; 60];
    bytes[0..4].copy_from_slice(&decompressed_size.to_le_bytes());
    bytes[36..40].copy_from_slice(
        &u32::try_from(chunks.len())
            .expect("test chunk count fits u32")
            .to_le_bytes(),
    );
    bytes[40..44].copy_from_slice(&compression_granularity.to_le_bytes());
    for chunk in chunks {
        bytes.extend_from_slice(
            &u32::try_from(chunk.len())
                .expect("test chunk len fits u32")
                .to_le_bytes(),
        );
    }
    for chunk in chunks {
        bytes.extend_from_slice(chunk);
    }
    bytes
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
