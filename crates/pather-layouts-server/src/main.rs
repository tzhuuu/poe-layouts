#![allow(clippy::missing_errors_doc)]

use std::fs;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Command as ProcessCommand, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use pather_core::{
    parse_dgr_layout_graph, scrape_campaign_acts_one_to_five, summarize_layout_rooms,
    CampaignScrapeRequest, LayoutRoomSummary,
};
use pather_schema::{poe_layouts, root_layout_database};
use poe_content::{
    fetch_latest_patch_versions, BundleDecompressor, BundleSlice, BundleSliceOutput, CacheMode,
    DiskCache, PatchCdnSource, PoeGame,
};
use serde::{Deserialize, Serialize};

const DEFAULT_ADDR: &str = "127.0.0.1:5174";
const DEFAULT_DAT_SCHEMA_PATH: &str = "data/cache/dat-schema/_Core.gql";
const DEFAULT_CAMPAIGN_RAW_DIR: &str = ".poe-layouts/raw/campaign-acts-1-5";
const DEFAULT_LAYOUT_DB_PATH: &str = "app/public/data/layouts.bin";
const OOZ_BRIDGE_TIMEOUT: Duration = Duration::from_secs(120);

fn main() -> Result<(), WebError> {
    let config = ServerConfig::from_args(std::env::args().skip(1).collect());
    let listener = TcpListener::bind(&config.addr).map_err(|source| WebError::Bind {
        addr: config.addr.clone(),
        source,
    })?;
    println!("pather-layouts-server listening on http://{}", config.addr);
    for stream in listener.incoming() {
        match stream {
            Ok(stream) => {
                if let Err(error) = handle_connection(stream, &config) {
                    eprintln!("request failed: {error}");
                }
            }
            Err(error) => eprintln!("connection failed: {error}"),
        }
    }
    Ok(())
}

#[derive(Debug, Clone)]
struct ServerConfig {
    addr: String,
    workspace: Workspace,
    static_root: PathBuf,
}

impl ServerConfig {
    fn from_args(args: Vec<String>) -> Self {
        let workspace = Workspace::discover_current();
        let mut addr = DEFAULT_ADDR.to_owned();
        let mut static_root = workspace.root.join("app/dist");
        let mut index = 0;
        while index < args.len() {
            match args[index].as_str() {
                "--addr" => {
                    if let Some(value) = args.get(index + 1) {
                        addr.clone_from(value);
                        index += 1;
                    }
                }
                "--static-root" => {
                    if let Some(value) = args.get(index + 1) {
                        static_root = PathBuf::from(value);
                        index += 1;
                    }
                }
                _ => {}
            }
            index += 1;
        }
        Self {
            addr,
            workspace,
            static_root,
        }
    }
}

#[derive(Debug, Clone)]
struct Workspace {
    root: PathBuf,
    cache_root: PathBuf,
    dat_schema_path: PathBuf,
    campaign_raw_dir: PathBuf,
    layout_db_path: PathBuf,
    ooz_script: PathBuf,
}

impl Workspace {
    fn discover_current() -> Self {
        Self::from_root(workspace_dir())
    }

    fn from_root(root: impl Into<PathBuf>) -> Self {
        let root = root.into();
        Self {
            cache_root: root.join(".poe-layouts").join("cache"),
            dat_schema_path: root.join(DEFAULT_DAT_SCHEMA_PATH),
            campaign_raw_dir: root.join(DEFAULT_CAMPAIGN_RAW_DIR),
            layout_db_path: root.join(DEFAULT_LAYOUT_DB_PATH),
            ooz_script: root.join("scripts").join("ooz-decompress-bundle.mjs"),
            root,
        }
    }

    fn cache(&self) -> DiskCache {
        DiskCache::new(self.cache_root.clone())
    }

    fn campaign_files_dir(&self) -> PathBuf {
        self.campaign_raw_dir.join("files")
    }
}

fn workspace_dir() -> PathBuf {
    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    if cwd.ends_with("app") {
        return cwd.parent().map_or(cwd.clone(), PathBuf::from);
    }
    cwd
}

fn handle_connection(mut stream: TcpStream, config: &ServerConfig) -> Result<(), WebError> {
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .map_err(WebError::Io)?;
    let request = HttpRequest::read(&mut stream)?;
    let response = route(&request, config);
    response.write(&mut stream).map_err(WebError::Io)
}

fn route(request: &HttpRequest, config: &ServerConfig) -> HttpResponse {
    if request.method == "OPTIONS" {
        return HttpResponse::empty(204);
    }

    match (request.method.as_str(), request.path.as_str()) {
        ("GET", "/version") | ("GET", "/api/version") => json_response(version(config)),
        ("GET", "/zone_names") | ("GET", "/api/zone_names") => json_response(zone_names(config)),
        ("GET", "/data/layouts.bin") => file_response(&config.workspace.layout_db_path),
        ("GET", "/api/layout-graph") => json_response(layout_graph(config, request.query_params())),
        ("POST", "/api/layout-candidates") => json_response(layout_candidates(config, request.json_body())),
        ("POST", "/api/layout-rooms") => json_response(layout_rooms(config, request.json_body())),
        ("GET", "/api/room-variants") => json_response(room_variants(config, request.query_params())),
        ("GET", "/api/room-plan") => json_response(room_plan(config, request.query_params())),
        ("GET", "/api/layout-environments") => file_response(
            &config
                .workspace
                .campaign_raw_dir
                .join("layout-environments.json"),
        ),
        ("GET", "/api/raw-files") => json_response(raw_files(config, request.query_params())),
        ("GET", "/api/latest_patch_versions") => {
            json_response(fetch_latest_patch_versions().map_err(WebError::from))
        }
        ("POST", "/api/prefetch_bundles") => {
            json_response(prefetch_bundles(config, request.json_body()))
        }
        ("POST", "/api/clear_cache") => json_response(clear_cache(config, request.json_body())),
        ("POST", "/api/scrape_campaign_acts_1_5") => {
            json_response(scrape_campaign(config, request.json_body()))
        }
        _ if request.method == "GET" && request.path.starts_with("/layouts/") => {
            let zone_id = percent_decode(&request.path["/layouts/".len()..]);
            json_response(layout(config, &zone_id))
        }
        _ if request.method == "GET" && request.path.starts_with("/api/layouts/") => {
            let zone_id = percent_decode(&request.path["/api/layouts/".len()..]);
            json_response(layout(config, &zone_id))
        }
        _ if request.method == "GET" && request.path.starts_with("/raw-files/") => {
            let logical_path = percent_decode(&request.path["/raw-files/".len()..]);
            raw_file_response(config, &logical_path)
        }
        _ if request.path.starts_with("/api/") => json_response::<()>(Err(WebError::NotFound(
            format!("api route not found: {}", request.path),
        ))),
        _ if request.method == "GET" => static_response(request, config),
        _ => HttpResponse::text(405, "method not allowed"),
    }
}

fn json_response<T>(result: Result<T, WebError>) -> HttpResponse
where
    T: Serialize,
{
    match result.and_then(|value| {
        serde_json::to_vec(&value).map_err(|source| WebError::Json {
            context: "encode response".to_owned(),
            source,
        })
    }) {
        Ok(bytes) => HttpResponse::new(200, "application/json", bytes),
        Err(error) => {
            let status = match error {
                WebError::NotFound(_) => 404,
                WebError::BadRequest(_) | WebError::Json { .. } => 400,
                _ => 500,
            };
            let body = serde_json::json!({ "error": error.to_string() });
            HttpResponse::new(status, "application/json", body.to_string().into_bytes())
        }
    }
}

fn version(config: &ServerConfig) -> Result<VersionResponse, WebError> {
    let data = read_layout_data(config)?;
    Ok(VersionResponse {
        schema_version: data.schema_version,
        game_version: data.game_version,
        release_line: data.release_line,
        scope: data.scope,
    })
}

fn zone_names(config: &ServerConfig) -> Result<Vec<ZoneName>, WebError> {
    Ok(read_layout_data(config)?.zones)
}

fn layout(config: &ServerConfig, zone_id: &str) -> Result<ZoneLayout, WebError> {
    let data = read_layout_data(config)?;
    let zone = data
        .zones
        .into_iter()
        .find(|zone| zone.id == zone_id)
        .ok_or_else(|| WebError::NotFound(format!("zone not found: {zone_id}")))?;
    let source_prefix = format!("WorldAreas[{}]", zone.row_index);
    let terrain_files = data
        .terrain_files
        .into_iter()
        .filter(|file| file.source.contains(&source_prefix))
        .collect();
    Ok(ZoneLayout {
        zone,
        terrain_files,
        warnings: data.warnings,
    })
}

fn raw_files(
    config: &ServerConfig,
    query_params: Vec<(String, String)>,
) -> Result<RawFilesResponse, WebError> {
    let prefix = query_value(&query_params, "prefix")
        .as_deref()
        .map(normalize_logical_path)
        .unwrap_or_default();
    let recursive = query_value(&query_params, "recursive")
        .is_some_and(|value| matches!(value.as_str(), "1" | "true" | "yes"));
    let extension = query_value(&query_params, "extension").map(normalize_extension);
    let limit = query_value(&query_params, "limit")
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(200);
    let root = config.workspace.campaign_files_dir();
    let browse_root = raw_file_path(&root, &prefix)?;
    if !browse_root.exists() {
        return Err(WebError::NotFound(format!(
            "raw folder not found: {prefix}"
        )));
    }
    if !browse_root.is_dir() {
        return Err(WebError::BadRequest(format!(
            "raw prefix is not a folder: {prefix}"
        )));
    }

    let mut folders = Vec::new();
    let mut files = Vec::new();
    if recursive {
        collect_raw_files(&root, &browse_root, extension.as_deref(), limit, &mut files)?;
    } else {
        let entries = fs::read_dir(&browse_root).map_err(|source| WebError::File {
            path: browse_root.clone(),
            source,
        })?;
        for entry in entries {
            let entry = entry.map_err(|source| WebError::File {
                path: browse_root.clone(),
                source,
            })?;
            let path = entry.path();
            let logical_path = logical_path_from_raw_path(&root, &path)?;
            if path.is_dir() {
                folders.push(RawFolderEntry { logical_path });
            } else if extension_matches(&logical_path, extension.as_deref()) && files.len() < limit
            {
                files.push(raw_file_entry(&root, &path, logical_path)?);
            }
        }
    }
    folders.sort_by(|left, right| left.logical_path.cmp(&right.logical_path));
    files.sort_by(|left, right| left.logical_path.cmp(&right.logical_path));

    Ok(RawFilesResponse {
        prefix,
        recursive,
        folders,
        returned_files: files.len(),
        files,
    })
}

fn raw_file_response(config: &ServerConfig, logical_path: &str) -> HttpResponse {
    match raw_file_path(&config.workspace.campaign_files_dir(), logical_path) {
        Ok(path) => file_response(&path),
        Err(error) => json_response::<()>(Err(error)),
    }
}

#[derive(Serialize)]
struct LayoutGraphResponse {
    #[serde(flatten)]
    graph: pather_core::DgrLayoutGraph,
    node_bosses: Vec<pather_core::LayoutNodeBosses>,
}

fn layout_graph(
    config: &ServerConfig,
    query_params: Vec<(String, String)>,
) -> Result<LayoutGraphResponse, WebError> {
    let logical_path = query_value(&query_params, "path")
        .ok_or_else(|| WebError::BadRequest("missing layout graph path".to_owned()))?;
    let mut graph = read_layout_graph(config, &logical_path)?;
    if let Some(zone_id) = query_value(&query_params, "zone") {
        let index_path = config
            .workspace
            .campaign_raw_dir
            .join("layout-transitions.json");
        if index_path.exists() {
            let bytes = fs::read(&index_path).map_err(|source| WebError::File {
                path: index_path.clone(),
                source,
            })?;
            let index: pather_core::LayoutTransitionIndex = serde_json::from_slice(&bytes)
                .map_err(|error| {
                    WebError::BadRequest(format!("invalid layout transition index: {error}"))
                })?;
            let data = read_layout_data(config)?;
            if data.game_version != index.patch_version {
                graph
                    .warnings
                    .push("Entrance index patch does not match the current scrape".to_owned());
            } else if let Some(layout) = index.layouts.into_iter().find(|layout| {
                layout.zone_id == zone_id && layout.logical_path == graph.logical_path
            }) {
                graph.node_transitions = layout.nodes;
                graph.warnings.extend(layout.warnings);
            }
        } else {
            graph
                .warnings
                .push("Entrance data has not been scraped for this cache".to_owned());
        }
    }
    let catalog = pather_core::inspect_layout_rooms(&config.workspace.campaign_files_dir(), &graph);
    let node_bosses = pather_core::resolve_room_bosses(&graph, &catalog);
    graph.warnings.extend(catalog.warnings.into_iter().map(|warning| format!("Boss coverage: {warning}")));
    Ok(LayoutGraphResponse { graph, node_bosses })
}

fn read_layout_graph(
    config: &ServerConfig,
    logical_path: &str,
) -> Result<pather_core::DgrLayoutGraph, WebError> {
    let logical_path = normalize_logical_path(logical_path);
    let is_layout_graph = logical_path.rsplit_once('.').is_some_and(|(_, extension)| {
        extension.eq_ignore_ascii_case("dgr") || extension.eq_ignore_ascii_case("tgr")
    });
    if !is_layout_graph {
        return Err(WebError::BadRequest(format!(
            "layout graph path must be a .dgr or .tgr file: {logical_path}"
        )));
    }
    let path = raw_file_path(&config.workspace.campaign_files_dir(), &logical_path)?;
    let bytes = fs::read(&path).map_err(|source| WebError::File {
        path: path.clone(),
        source,
    })?;
    parse_dgr_layout_graph(&logical_path, &bytes).map_err(WebError::from)
}

#[derive(Deserialize)]
struct LayoutRoomsRequest {
    paths: Vec<String>,
}

fn layout_candidates(
    config: &ServerConfig,
    request: Result<LayoutRoomsRequest, WebError>,
) -> Result<pather_core::LayoutCandidates, WebError> {
    let paths = request?.paths;
    if paths.len() > 512 {
        return Err(WebError::BadRequest("too many layout roots".to_owned()));
    }
    for path in &paths {
        raw_file_path(&config.workspace.campaign_files_dir(), path)?;
        if path.starts_with(['/', '\\']) || path.contains(':') ||
            !(path.to_ascii_lowercase().ends_with(".dgr") || path.to_ascii_lowercase().ends_with(".tgr")) {
            return Err(WebError::BadRequest(format!("invalid layout root: {path}")));
        }
    }
    Ok(pather_core::resolve_layout_candidates(&config.workspace.campaign_files_dir(), &paths))
}

fn room_variants(
    config: &ServerConfig,
    params: Vec<(String, String)>,
) -> Result<pather_core::RoomCatalog, WebError> {
    let path = query_value(&params, "layout")
        .ok_or_else(|| WebError::BadRequest("missing layout path".to_owned()))?;
    let graph = read_layout_graph(config, &path)?;
    Ok(pather_core::inspect_layout_rooms(&config.workspace.campaign_files_dir(), &graph))
}

#[cfg(test)]
mod layout_candidates_tests {
    use super::*;

    #[test]
    fn serves_candidates_and_rejects_unsafe_roots() {
        let dir = tempfile::tempdir().unwrap();
        let workspace = Workspace::from_root(dir.path());
        fs::create_dir_all(workspace.campaign_files_dir()).unwrap();
        let text = "version 19\nSize: 1 1\nNodes: 1\nEdges: 0\n\"\"\n\"\"\n\"\"\nDefault%: 0 0 0\n1 1 0 \"room\" I 0 100 0 N\n";
        fs::write(workspace.campaign_files_dir().join("room.dgr"), text.encode_utf16().flat_map(u16::to_le_bytes).collect::<Vec<_>>()).unwrap();
        let config = ServerConfig { addr: DEFAULT_ADDR.to_owned(), static_root: dir.path().join("dist"), workspace };
        for (paths, status) in [
            (vec!["room.dgr"], 200), (vec![], 200),
            (vec!["../room.dgr"], 400), (vec!["/room.dgr"], 400),
            (vec!["room.arm"], 400), (vec!["room.dgr"; 513], 400),
        ] {
            let request = HttpRequest { method: "POST".to_owned(), path: "/api/layout-candidates".to_owned(), query: String::new(), body: serde_json::to_vec(&serde_json::json!({"paths": paths})).unwrap() };
            let response = route(&request, &config);
            assert_eq!(response.status, status);
            assert_eq!(response.content_type, "application/json");
            let value: serde_json::Value = serde_json::from_slice(&response.body).unwrap();
            if status == 200 { assert!(value["candidates"].is_array()); }
            else { assert!(value["error"].is_string()); }
        }
    }
}

fn room_plan(
    config: &ServerConfig,
    params: Vec<(String, String)>,
) -> Result<pather_core::RoomPlan, WebError> {
    let logical_path = query_value(&params, "path")
        .ok_or_else(|| WebError::BadRequest("missing room path".to_owned()))?;
    let logical_path = normalize_logical_path(&logical_path).to_ascii_lowercase();
    if !logical_path.ends_with(".arm") {
        return Err(WebError::BadRequest(
            "room path must be an .arm file".to_owned(),
        ));
    }
    let path = raw_file_path(&config.workspace.campaign_files_dir(), &logical_path)?;
    let bytes = fs::read(&path).map_err(|source| {
        if source.kind() == std::io::ErrorKind::NotFound {
            WebError::NotFound(format!("room not extracted: {logical_path}"))
        } else {
            WebError::File { path, source }
        }
    })?;
    pather_core::parse_arm_room_plan(&logical_path, &bytes).map_err(WebError::from)
}

#[cfg(test)]
mod room_plan_tests {
    use super::*;

    #[test]
    fn serves_room_plans_and_json_errors() {
        let dir = tempfile::tempdir().unwrap();
        let workspace = Workspace::from_root(dir.path());
        fs::create_dir_all(workspace.campaign_files_dir()).unwrap();
        let text = "version 30\n0\n1 1\n1 1\n\"room\"\n0 0\ns\n0 0\n0 0\n0 0\n0 0\n0\n0\n0\n0\n0\n0\ns\n0\n";
        fs::write(workspace.campaign_files_dir().join("room.arm"),
            text.encode_utf16().flat_map(u16::to_le_bytes).collect::<Vec<_>>()).unwrap();
        fs::write(workspace.campaign_files_dir().join("broken.arm"), [1]).unwrap();
        let config = ServerConfig { addr: DEFAULT_ADDR.to_owned(), static_root: dir.path().join("dist"), workspace };
        for (query, status) in [
            ("path=room.arm", 200), ("path=ROOM.ARM", 200),
            ("", 400), ("path=room.tgr", 400),
            ("path=..%2Froom.arm", 400), ("path=missing.arm", 404),
            ("path=broken.arm", 500),
        ] {
            let request = HttpRequest { method: "GET".to_owned(), path: "/api/room-plan".to_owned(), query: query.to_owned(), body: Vec::new() };
            let response = route(&request, &config);
            assert_eq!(response.status, status, "{query}");
            assert_eq!(response.content_type, "application/json");
            let value: serde_json::Value = serde_json::from_slice(&response.body).unwrap();
            if status == 200 {
                assert_eq!(value["width"], 1);
                assert_eq!(value["tiles"][0]["kind"], "s");
            } else {
                assert!(value["error"].is_string());
            }
        }
    }
}

#[derive(Serialize)]
struct LayoutRoomsResponse {
    requested_layout_count: usize,
    parsed_layout_count: usize,
    rooms: Vec<LayoutRoomSummary>,
    warnings: Vec<String>,
    room_catalogs: Vec<pather_core::RoomCatalog>,
}

fn layout_rooms(
    config: &ServerConfig,
    request: Result<LayoutRoomsRequest, WebError>,
) -> Result<LayoutRoomsResponse, WebError> {
    let mut paths = request?
        .paths
        .into_iter()
        .map(|path| normalize_logical_path(&path))
        .collect::<Vec<_>>();
    paths.sort();
    paths.dedup();
    let mut graphs = Vec::new();
    let mut warnings = Vec::new();
    for path in &paths {
        match read_layout_graph(config, path) {
            Ok(graph) => {
                warnings.extend(
                    graph
                        .warnings
                        .iter()
                        .map(|warning| format!("{path}: {warning}")),
                );
                graphs.push(graph);
            }
            Err(error) => warnings.push(format!("{path}: {error}")),
        }
    }
    let mut masters = std::collections::BTreeSet::new();
    let room_catalogs = graphs
        .iter()
        .filter(|graph| {
            let source = graph.logical_path.rsplit_once('/').map_or("", |(dir, _)| dir);
            masters.insert((source.to_owned(), graph.master_file.clone()))
        })
        .map(|graph| {
            pather_core::inspect_layout_rooms(&config.workspace.campaign_files_dir(), graph)
        })
        .collect();
    Ok(LayoutRoomsResponse {
        requested_layout_count: paths.len(),
        parsed_layout_count: graphs.len(),
        rooms: summarize_layout_rooms(&graphs),
        warnings,
        room_catalogs,
    })
}

fn prefetch_bundles(
    config: &ServerConfig,
    request: Result<PrefetchBundlesRequest, WebError>,
) -> Result<poe_content::CacheManifest, WebError> {
    let request = request?;
    let mut bundles = request
        .bundles
        .into_iter()
        .map(|bundle| bundle.trim().to_owned())
        .filter(|bundle| !bundle.is_empty())
        .collect::<Vec<_>>();
    if bundles.is_empty() {
        bundles.push("_.index.bin".to_owned());
    }
    bundles.sort();
    bundles.dedup();

    let source = PatchCdnSource::poe1(resolve_poe1_patch_version(request.patch_version)?);
    Ok(source.prefetch_bundles(
        &config.workspace.cache(),
        &bundles,
        if request.refresh {
            CacheMode::Refresh
        } else {
            CacheMode::Online
        },
    )?)
}

fn clear_cache(
    config: &ServerConfig,
    request: Result<ClearCacheRequest, WebError>,
) -> Result<poe_content::CacheClearReport, WebError> {
    let request = request?;
    let key = release_cache_key(request.release_line, request.patch_version)?;
    let cache = config.workspace.cache();
    Ok(if request.dry_run {
        cache.preview_clear_key(&key)?
    } else {
        cache.clear_key(&key)?
    })
}

fn scrape_campaign(
    config: &ServerConfig,
    request: Result<ScrapeCampaignRequest, WebError>,
) -> Result<ScrapeCampaignSummary, WebError> {
    let request = request?;
    let source = PatchCdnSource::poe1(resolve_poe1_patch_version(request.patch_version)?);
    let mut decompressor = NodeOozBridge {
        node: PathBuf::from("node"),
        script: config.workspace.ooz_script.clone(),
    };
    let output = scrape_campaign_acts_one_to_five(
        &CampaignScrapeRequest {
            source,
            cache: config.workspace.cache(),
            mode: scrape_cache_mode(request.offline, request.refresh),
            schema_path: config.workspace.dat_schema_path.clone(),
            out_dir: config.workspace.campaign_raw_dir.clone(),
            layout_db_out: Some(config.workspace.layout_db_path.clone()),
            raw_folder_prefixes: Vec::new(),
        },
        &mut decompressor,
    )?;
    let manifest = output.manifest;
    Ok(ScrapeCampaignSummary {
        scope: manifest.scope,
        patch_version: manifest.patch_version,
        release_line: manifest.release_line,
        manifest_path: output.manifest_path,
        layout_db_path: output.layout_db_path,
        selected_areas: manifest.selected_areas.len(),
        candidate_files: manifest.candidate_files.len(),
        terrain_folders: manifest.terrain_folders.len(),
        extracted_files: manifest.extracted_files.len(),
        folder_files: manifest.folder_files.len(),
        missing_files: manifest.missing_files.len(),
        warnings: manifest.warnings,
    })
}

fn read_layout_data(config: &ServerConfig) -> Result<LayoutData, WebError> {
    let bytes = fs::read(&config.workspace.layout_db_path).map_err(|source| WebError::File {
        path: config.workspace.layout_db_path.clone(),
        source,
    })?;
    let database = root_layout_database(&bytes)?;
    Ok(LayoutData {
        schema_version: database.schema_version().unwrap_or("unknown").to_owned(),
        game_version: database.game_version().unwrap_or("unknown").to_owned(),
        release_line: database.release_line().unwrap_or("unknown").to_owned(),
        scope: database.scope().unwrap_or("unknown").to_owned(),
        zones: read_zones(&database),
        terrain_files: read_terrain_files(&database),
        warnings: read_string_vector(database.warnings()),
    })
}

fn read_zones(database: &poe_layouts::LayoutDatabase<'_>) -> Vec<ZoneName> {
    database.zones().map_or_else(Vec::new, |zones| {
        zones
            .iter()
            .map(|zone| ZoneName {
                row_index: zone.row_index(),
                id: zone.id().unwrap_or_default().to_owned(),
                name: zone.name().unwrap_or_default().to_owned(),
                act: zone.act(),
                area_level: zone.area_level(),
                is_town: zone.is_town(),
                topology_indices: read_u32_vector(zone.topology_indices()),
                tsi_file: zone.tsi_file().map(str::to_owned),
            })
            .collect()
    })
}

fn read_terrain_files(database: &poe_layouts::LayoutDatabase<'_>) -> Vec<TerrainFile> {
    database.terrain_files().map_or_else(Vec::new, |files| {
        files
            .iter()
            .map(|file| TerrainFile {
                logical_path: file.logical_path().unwrap_or_default().to_owned(),
                source: file.source().unwrap_or_default().to_owned(),
                kind: terrain_file_kind(file.kind()),
                status: terrain_file_status(file.status()),
                reason: file.reason().map(str::to_owned),
            })
            .collect()
    })
}

fn read_u32_vector(vector: Option<flatbuffers::Vector<'_, u32>>) -> Vec<u32> {
    vector.map_or_else(Vec::new, |items| items.iter().collect())
}

fn read_string_vector(
    vector: Option<flatbuffers::Vector<'_, flatbuffers::ForwardsUOffset<&str>>>,
) -> Vec<String> {
    vector.map_or_else(Vec::new, |items| items.iter().map(str::to_owned).collect())
}

fn terrain_file_kind(kind: poe_layouts::TerrainFileKind) -> &'static str {
    match kind {
        poe_layouts::TerrainFileKind::Graph => "graph",
        poe_layouts::TerrainFileKind::Tsi => "tsi",
        poe_layouts::TerrainFileKind::DgrVariant => "dgr_variant",
        poe_layouts::TerrainFileKind::ArmVariant => "arm_variant",
        _ => "unknown",
    }
}

fn terrain_file_status(status: poe_layouts::TerrainFileStatus) -> &'static str {
    match status {
        poe_layouts::TerrainFileStatus::Extracted => "extracted",
        poe_layouts::TerrainFileStatus::Missing => "missing",
        _ => "candidate",
    }
}

fn static_response(request: &HttpRequest, config: &ServerConfig) -> HttpResponse {
    let path = static_path(&config.static_root, &request.path);
    if path.is_dir() {
        return file_response(&path.join("index.html"));
    }
    if path.exists() {
        return file_response(&path);
    }
    file_response(&config.static_root.join("index.html"))
}

fn static_path(root: &Path, request_path: &str) -> PathBuf {
    let trimmed = request_path.trim_start_matches('/');
    let mut path = root.to_path_buf();
    for part in trimmed.split('/') {
        if part.is_empty() || part == "." || part == ".." {
            continue;
        }
        path.push(percent_decode(part));
    }
    path
}

fn query_value(params: &[(String, String)], name: &str) -> Option<String> {
    params
        .iter()
        .find_map(|(key, value)| (key == name).then(|| value.clone()))
}

fn raw_file_path(root: &Path, logical_path: &str) -> Result<PathBuf, WebError> {
    let logical_path = normalize_logical_path(logical_path);
    let mut path = root.to_path_buf();
    for part in logical_path.split('/') {
        if part.is_empty() {
            continue;
        }
        if part == "." || part == ".." {
            return Err(WebError::BadRequest(format!(
                "unsafe raw file path: {logical_path}"
            )));
        }
        path.push(part);
    }
    Ok(path)
}

fn collect_raw_files(
    root: &Path,
    dir: &Path,
    extension: Option<&str>,
    limit: usize,
    files: &mut Vec<RawFileEntry>,
) -> Result<(), WebError> {
    if files.len() >= limit {
        return Ok(());
    }
    let entries = fs::read_dir(dir).map_err(|source| WebError::File {
        path: dir.to_path_buf(),
        source,
    })?;
    for entry in entries {
        let entry = entry.map_err(|source| WebError::File {
            path: dir.to_path_buf(),
            source,
        })?;
        let path = entry.path();
        if path.is_dir() {
            collect_raw_files(root, &path, extension, limit, files)?;
        } else {
            let logical_path = logical_path_from_raw_path(root, &path)?;
            if extension_matches(&logical_path, extension) && files.len() < limit {
                files.push(raw_file_entry(root, &path, logical_path)?);
            }
        }
        if files.len() >= limit {
            break;
        }
    }
    Ok(())
}

fn raw_file_entry(
    root: &Path,
    path: &Path,
    logical_path: String,
) -> Result<RawFileEntry, WebError> {
    let metadata = fs::metadata(path).map_err(|source| WebError::File {
        path: path.to_path_buf(),
        source,
    })?;
    Ok(RawFileEntry {
        logical_path: logical_path.clone(),
        byte_len: metadata.len(),
        url: format!("/raw-files/{}", encode_path(&logical_path)),
        path: path
            .strip_prefix(root)
            .unwrap_or(path)
            .display()
            .to_string(),
    })
}

fn logical_path_from_raw_path(root: &Path, path: &Path) -> Result<String, WebError> {
    let relative = path
        .strip_prefix(root)
        .map_err(|_| WebError::BadRequest(format!("raw path outside root: {}", path.display())))?;
    Ok(relative
        .components()
        .filter_map(|component| match component {
            std::path::Component::Normal(part) => part.to_str().map(str::to_owned),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("/"))
}

fn normalize_logical_path(path: &str) -> String {
    path.replace('\\', "/")
        .trim_matches('/')
        .split('/')
        .filter(|part| !part.is_empty() && *part != ".")
        .collect::<Vec<_>>()
        .join("/")
}

fn normalize_extension(extension: String) -> String {
    extension
        .trim()
        .trim_start_matches('.')
        .to_ascii_lowercase()
}

fn extension_matches(logical_path: &str, extension: Option<&str>) -> bool {
    let Some(extension) = extension else {
        return true;
    };
    if extension.is_empty() {
        return true;
    }
    logical_path
        .rsplit_once('.')
        .is_some_and(|(_, path_extension)| path_extension.eq_ignore_ascii_case(extension))
}

fn encode_path(path: &str) -> String {
    let mut encoded = String::new();
    for byte in path.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'/' | b'-' | b'_' | b'.' | b'~' => {
                encoded.push(char::from(byte));
            }
            _ => encoded.push_str(&format!("%{byte:02X}")),
        }
    }
    encoded
}

fn file_response(path: &Path) -> HttpResponse {
    match fs::read(path) {
        Ok(bytes) => HttpResponse::new(200, content_type(path), bytes),
        Err(_) => HttpResponse::text(404, "not found"),
    }
}

fn content_type(path: &Path) -> &'static str {
    match path.extension().and_then(|extension| extension.to_str()) {
        Some("css") => "text/css; charset=utf-8",
        Some("html") => "text/html; charset=utf-8",
        Some("js") => "text/javascript; charset=utf-8",
        Some("json") => "application/json",
        Some("png") => "image/png",
        Some("svg") => "image/svg+xml",
        Some("wasm") => "application/wasm",
        _ => "application/octet-stream",
    }
}

fn percent_decode(value: &str) -> String {
    let bytes = value.as_bytes();
    let mut output = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' && index + 2 < bytes.len() {
            if let Ok(hex) = std::str::from_utf8(&bytes[index + 1..index + 3]) {
                if let Ok(byte) = u8::from_str_radix(hex, 16) {
                    output.push(byte);
                    index += 3;
                    continue;
                }
            }
        }
        output.push(bytes[index]);
        index += 1;
    }
    String::from_utf8_lossy(&output).into_owned()
}

fn percent_decode_form(value: &str) -> String {
    percent_decode(&value.replace('+', " "))
}

#[derive(Debug)]
struct HttpRequest {
    method: String,
    path: String,
    query: String,
    body: Vec<u8>,
}

impl HttpRequest {
    fn read(stream: &mut TcpStream) -> Result<Self, WebError> {
        let mut bytes = Vec::new();
        let mut buffer = [0; 4096];
        let header_end = loop {
            let read = stream.read(&mut buffer).map_err(WebError::Io)?;
            if read == 0 {
                return Err(WebError::BadRequest("empty request".to_owned()));
            }
            bytes.extend_from_slice(&buffer[..read]);
            if let Some(index) = find_header_end(&bytes) {
                break index;
            }
            if bytes.len() > 1024 * 1024 {
                return Err(WebError::BadRequest("request header too large".to_owned()));
            }
        };

        let headers = std::str::from_utf8(&bytes[..header_end])
            .map_err(|_| WebError::BadRequest("request header is not utf-8".to_owned()))?;
        let mut lines = headers.lines();
        let request_line = lines
            .next()
            .ok_or_else(|| WebError::BadRequest("missing request line".to_owned()))?;
        let mut request_parts = request_line.split_whitespace();
        let method = request_parts
            .next()
            .ok_or_else(|| WebError::BadRequest("missing method".to_owned()))?
            .to_owned();
        let raw_path = request_parts
            .next()
            .ok_or_else(|| WebError::BadRequest("missing path".to_owned()))?;
        let (path, query) = raw_path
            .split_once('?')
            .map_or((raw_path, ""), |(path, query)| (path, query));
        let path = path.to_owned();
        let query = query.to_owned();
        let content_length = lines
            .find_map(|line| {
                let (name, value) = line.split_once(':')?;
                name.eq_ignore_ascii_case("content-length")
                    .then(|| value.trim().parse::<usize>().ok())
                    .flatten()
            })
            .unwrap_or(0);

        let body_start = header_end + 4;
        let mut body = bytes[body_start..].to_vec();
        while body.len() < content_length {
            let read = stream.read(&mut buffer).map_err(WebError::Io)?;
            if read == 0 {
                break;
            }
            body.extend_from_slice(&buffer[..read]);
        }
        body.truncate(content_length);

        Ok(Self {
            method,
            path,
            query,
            body,
        })
    }

    fn json_body<T>(&self) -> Result<T, WebError>
    where
        T: for<'de> Deserialize<'de>,
    {
        serde_json::from_slice(&self.body).map_err(|source| WebError::Json {
            context: "decode request".to_owned(),
            source,
        })
    }

    fn query_params(&self) -> Vec<(String, String)> {
        self.query
            .split('&')
            .filter(|part| !part.is_empty())
            .map(|part| {
                let (name, value) = part.split_once('=').unwrap_or((part, ""));
                (percent_decode_form(name), percent_decode_form(value))
            })
            .collect()
    }
}

fn find_header_end(bytes: &[u8]) -> Option<usize> {
    bytes.windows(4).position(|window| window == b"\r\n\r\n")
}

struct HttpResponse {
    status: u16,
    content_type: &'static str,
    body: Vec<u8>,
}

impl HttpResponse {
    fn new(status: u16, content_type: &'static str, body: Vec<u8>) -> Self {
        Self {
            status,
            content_type,
            body,
        }
    }

    fn text(status: u16, text: &str) -> Self {
        Self::new(
            status,
            "text/plain; charset=utf-8",
            text.as_bytes().to_vec(),
        )
    }

    fn empty(status: u16) -> Self {
        Self::new(status, "text/plain; charset=utf-8", Vec::new())
    }

    fn write(&self, stream: &mut TcpStream) -> std::io::Result<()> {
        write!(
            stream,
            "HTTP/1.1 {} {}\r\nContent-Type: {}\r\nContent-Length: {}\r\nAccess-Control-Allow-Origin: *\r\nAccess-Control-Allow-Headers: content-type\r\nAccess-Control-Allow-Methods: GET, POST, OPTIONS\r\nConnection: close\r\n\r\n",
            self.status,
            status_text(self.status),
            self.content_type,
            self.body.len()
        )?;
        stream.write_all(&self.body)
    }
}

fn status_text(status: u16) -> &'static str {
    match status {
        200 => "OK",
        204 => "No Content",
        400 => "Bad Request",
        404 => "Not Found",
        405 => "Method Not Allowed",
        _ => "Internal Server Error",
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct VersionResponse {
    schema_version: String,
    game_version: String,
    release_line: String,
    scope: String,
}

#[derive(Debug)]
struct LayoutData {
    schema_version: String,
    game_version: String,
    release_line: String,
    scope: String,
    zones: Vec<ZoneName>,
    terrain_files: Vec<TerrainFile>,
    warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct ZoneName {
    row_index: u32,
    id: String,
    name: String,
    act: i32,
    area_level: i32,
    is_town: bool,
    topology_indices: Vec<u32>,
    tsi_file: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct TerrainFile {
    logical_path: String,
    source: String,
    kind: &'static str,
    status: &'static str,
    reason: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ZoneLayout {
    zone: ZoneName,
    terrain_files: Vec<TerrainFile>,
    warnings: Vec<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct RawFilesResponse {
    prefix: String,
    recursive: bool,
    folders: Vec<RawFolderEntry>,
    returned_files: usize,
    files: Vec<RawFileEntry>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct RawFolderEntry {
    logical_path: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct RawFileEntry {
    logical_path: String,
    byte_len: u64,
    url: String,
    path: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ScrapeCampaignSummary {
    scope: String,
    patch_version: String,
    release_line: String,
    manifest_path: PathBuf,
    layout_db_path: Option<PathBuf>,
    selected_areas: usize,
    candidate_files: usize,
    terrain_folders: usize,
    extracted_files: usize,
    folder_files: usize,
    missing_files: usize,
    warnings: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PrefetchBundlesRequest {
    patch_version: Option<String>,
    bundles: Vec<String>,
    refresh: bool,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ClearCacheRequest {
    release_line: Option<String>,
    patch_version: Option<String>,
    dry_run: bool,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ScrapeCampaignRequest {
    patch_version: Option<String>,
    refresh: bool,
    offline: bool,
}

#[derive(Debug, thiserror::Error)]
enum WebError {
    #[error("failed to bind {addr}: {source}")]
    Bind {
        addr: String,
        #[source]
        source: std::io::Error,
    },
    #[error("{0}")]
    BadRequest(String),
    #[error(transparent)]
    Cache(#[from] poe_content::CacheError),
    #[error(transparent)]
    CampaignScrape(#[from] pather_core::CampaignScrapeError),
    #[error("file error for {path}: {source}")]
    File {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("json error while {context}: {source}")]
    Json {
        context: String,
        #[source]
        source: serde_json::Error,
    },
    #[error("{0}")]
    NotFound(String),
    #[error(transparent)]
    PatchCdn(#[from] poe_content::PatchCdnError),
    #[error(transparent)]
    Schema(#[from] pather_schema::SchemaError),
}

#[derive(Debug, Clone)]
struct NodeOozBridge {
    node: PathBuf,
    script: PathBuf,
}

#[derive(Debug)]
struct NodeOozBridgeError(String);

impl std::fmt::Display for NodeOozBridgeError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl std::error::Error for NodeOozBridgeError {}

impl BundleDecompressor for NodeOozBridge {
    type Error = NodeOozBridgeError;

    fn decompress_bundle(
        &mut self,
        input: &Path,
        output: &Path,
        slice: Option<BundleSlice>,
    ) -> Result<(), Self::Error> {
        run_ooz_bridge(&self.node, &self.script, input, output, slice).map_err(NodeOozBridgeError)
    }

    fn decompress_bundle_slices(
        &mut self,
        input: &Path,
        slices: &[BundleSliceOutput],
    ) -> Result<(), Self::Error> {
        run_ooz_bridge_batch(&self.node, &self.script, input, slices).map_err(NodeOozBridgeError)
    }
}

fn run_ooz_bridge_batch(
    node: &Path,
    script: &Path,
    input: &Path,
    slices: &[BundleSliceOutput],
) -> Result<(), String> {
    let mut manifest = tempfile::NamedTempFile::new()
        .map_err(|source| format!("create temporary ooz batch manifest: {source}"))?;
    serde_json::to_writer(
        &mut manifest,
        &serde_json::json!({
            "slices": slices.iter().map(|slice| serde_json::json!({
                "offset": slice.slice.offset,
                "size": slice.slice.size,
                "outputPath": slice.output_path,
            })).collect::<Vec<_>>(),
        }),
    )
    .map_err(|source| format!("write temporary ooz batch manifest: {source}"))?;
    manifest
        .flush()
        .map_err(|source| format!("flush temporary ooz batch manifest: {source}"))?;
    let mut command = ProcessCommand::new(node);
    command
        .arg(script)
        .arg("--batch")
        .arg(input)
        .arg(manifest.path());
    run_ooz_command(node, script, command)
}

fn resolve_poe1_patch_version(patch_version: Option<String>) -> Result<String, WebError> {
    Ok(match patch_version {
        Some(patch_version) if !patch_version.trim().is_empty() => patch_version,
        _ => fetch_latest_patch_versions()?.poe1_source().patch_version,
    })
}

fn release_cache_key(
    release_line: Option<String>,
    patch_version: Option<String>,
) -> Result<String, WebError> {
    if let Some(patch_version) = patch_version.filter(|value| !value.trim().is_empty()) {
        let source = PatchCdnSource::poe1(patch_version);
        return Ok(format!(
            "{}/patches/{}",
            source.cache_namespace(),
            source.patch_version
        ));
    }

    let release_line = match release_line {
        Some(release_line) if !release_line.trim().is_empty() => release_line,
        _ => fetch_latest_patch_versions()?.poe1_source().release_line(),
    };
    Ok(PatchCdnSource::release_cache_namespace(
        PoeGame::Poe1,
        release_line.trim(),
    ))
}

fn scrape_cache_mode(offline: bool, refresh: bool) -> CacheMode {
    if offline {
        CacheMode::Offline
    } else if refresh {
        CacheMode::Refresh
    } else {
        CacheMode::Online
    }
}

fn run_ooz_bridge(
    node: &Path,
    script: &Path,
    input: &Path,
    output: &Path,
    slice: Option<BundleSlice>,
) -> Result<(), String> {
    let mut command = ProcessCommand::new(node);
    command.arg(script).arg(input).arg(output);
    if let Some(slice) = slice {
        command
            .arg(slice.offset.to_string())
            .arg(slice.size.to_string());
    }
    run_ooz_command(node, script, command)
}

fn run_ooz_command(node: &Path, script: &Path, mut command: ProcessCommand) -> Result<(), String> {
    command.stdout(Stdio::piped()).stderr(Stdio::piped());
    let mut child = command
        .spawn()
        .map_err(|source| format!("run {} {}: {source}", node.display(), script.display()))?;
    let started = Instant::now();
    loop {
        let status = child.try_wait().map_err(|source| {
            format!("wait for {} {}: {source}", node.display(), script.display())
        })?;
        if status.is_some() {
            break;
        }
        if started.elapsed() > OOZ_BRIDGE_TIMEOUT {
            let _ = child.kill();
            let output = child.wait_with_output().map_err(|source| {
                format!(
                    "collect timed-out {} {} output: {source}",
                    node.display(),
                    script.display()
                )
            })?;
            return Err(format!(
                "timed out after {}s\nstdout:\n{}\nstderr:\n{}",
                OOZ_BRIDGE_TIMEOUT.as_secs(),
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            ));
        }
        thread::sleep(Duration::from_millis(100));
    }
    let output_status = child.wait_with_output().map_err(|source| {
        format!(
            "collect {} {} output: {source}",
            node.display(),
            script.display()
        )
    })?;
    if !output_status.status.success() {
        return Err(format!(
            "status {}\nstdout:\n{}\nstderr:\n{}",
            output_status.status,
            String::from_utf8_lossy(&output_status.stdout),
            String::from_utf8_lossy(&output_status.stderr)
        ));
    }
    Ok(())
}
