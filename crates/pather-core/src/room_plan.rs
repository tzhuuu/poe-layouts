use serde::Serialize;

use crate::{decode_utf16le_text, tokenize_dgr_line, CampaignScrapeError};

const TILE_SIZE: usize = 24;
const MAX_CELLS: usize = 65_536;

#[derive(Debug, Clone, Serialize)]
pub struct RoomPlan {
    pub logical_path: String,
    pub label: String,
    pub version: usize,
    pub width: usize,
    pub height: usize,
    pub tile_size: usize,
    pub assets: Vec<String>,
    pub tiles: Vec<RoomTile>,
    pub markers: Vec<RoomMarker>,
    pub objects: Vec<RoomObject>,
}

#[derive(Debug, Clone, Serialize)]
pub struct RoomTile {
    pub x: usize,
    pub y: usize,
    pub kind: String,
    pub width: usize,
    pub height: usize,
    pub edges: [usize; 4],
    pub ground: [usize; 4],
    pub elevations: [i32; 4],
    pub feature: usize,
    pub origin: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct RoomMarker {
    pub x: f64,
    pub y: f64,
    pub rotation: f64,
    pub kind: String,
    pub tag: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct RoomObject {
    pub x: f64,
    pub y: f64,
    pub art: String,
    pub entity: String,
}

pub fn parse_arm_room_plan(path: &str, bytes: &[u8]) -> Result<RoomPlan, CampaignScrapeError> {
    let text = decode_utf16le_text(path, bytes)?;
    parse_room_plan(path, &text).map_err(|message| CampaignScrapeError::ArmParse {
        path: path.to_owned(),
        message,
    })
}

fn parse_room_plan(path: &str, text: &str) -> Result<RoomPlan, String> {
    let mut reader = RoomReader::new(text);
    let version = reader
        .line()?
        .trim_start_matches('\u{feff}')
        .strip_prefix("version ")
        .ok_or("missing ARM version")?
        .parse::<usize>()
        .map_err(|_| "invalid ARM version")?;
    let section_count = match version {
        14..=19 => 9,
        20..=24 => 10,
        26..=28 => 5,
        29..=32 | 35..=36 => 6,
        _ => return Err(format!("unsupported ARM version {version}")),
    };
    let asset_count = reader.count()?;
    let assets = (0..asset_count)
        .map(|_| reader.quoted())
        .collect::<Result<Vec<_>, _>>()?;
    let base_size = integers(reader.line()?)?;
    let header = integers(reader.line()?)?;
    if header.len() != 2 {
        return Err("expected ARM header pair".to_owned());
    }
    let label = reader.quoted()?;
    if version == 22 && reader.peek().is_some_and(|line| line.is_empty()) {
        reader.line()?;
    }
    reader.line()?;
    let root_tokens = tokenize_dgr_line(reader.line()?);
    let mut root_cursor = 0;
    let mut root = parse_tile(&root_tokens, &mut root_cursor, version, 0, 0)?;
    if root_cursor != root_tokens.len() {
        return Err("expected a root tile key".to_owned());
    }
    if root.kind != "k" {
        root.width = *base_size.first().ok_or("missing room width")?;
        root.height = *base_size.get(1).ok_or("missing room height")?;
    }
    let cells = root
        .width
        .checked_mul(root.height)
        .ok_or("room size overflow")?;
    if cells == 0 || cells > MAX_CELLS {
        return Err("room grid exceeds supported bounds".to_owned());
    }
    let edge_rows = header[0]
        .checked_add(header[1])
        .and_then(|sum| sum.checked_mul(2))
        .filter(|count| *count <= MAX_CELLS)
        .ok_or("invalid edge row count")?;
    for _ in 0..edge_rows {
        reader.line()?;
    }
    let mut markers = Vec::new();
    for section in 0..section_count {
        let kind = marker_kind(version, section);
        for row in reader.section(version)? {
            let tokens = tokenize_dgr_line(row);
            if tokens.len() != 4 {
                return Err(format!("unsupported marker row in section {section}"));
            }
            markers.push(RoomMarker {
                x: coordinate(&tokens[0])?,
                y: coordinate(&tokens[1])?,
                rotation: coordinate(&tokens[2])?,
                kind: kind.to_owned(),
                tag: tokens[3].clone(),
            });
        }
    }
    if version >= 35 {
        reader.quoted()?;
    }
    let mut tiles = Vec::new();
    for y in 0..root.height {
        let tokens = tokenize_dgr_line(reader.line()?);
        let mut cursor = 0;
        for x in 0..root.width {
            let tile = parse_tile(&tokens, &mut cursor, version, x, y)?;
            if tile.kind != "n" {
                tiles.push(tile);
            }
        }
        if cursor != tokens.len() {
            return Err(format!("extra tile tokens in row {y}"));
        }
    }
    let mut objects = Vec::new();
    if reader.peek().is_some() {
        for row in reader.section(version)? {
            let tokens = tokenize_dgr_line(row);
            let paths = tokens
                .iter()
                .filter(|token| token.to_ascii_lowercase().starts_with("metadata/"))
                .collect::<Vec<_>>();
            if paths.len() != 2 || tokens.len() < 4 {
                return Err("unsupported placed object row".to_owned());
            }
            objects.push(RoomObject {
                x: coordinate(&tokens[0])?,
                y: coordinate(&tokens[1])?,
                art: paths[0].clone(),
                entity: paths[1].clone(),
            });
        }
    }
    for tile in &tiles {
        if tile
            .edges
            .iter()
            .chain(&tile.ground)
            .any(|index| *index > assets.len())
        {
            return Err("tile references an invalid asset index".to_owned());
        }
    }
    Ok(RoomPlan {
        logical_path: path.to_owned(),
        label,
        version,
        width: root.width,
        height: root.height,
        tile_size: TILE_SIZE,
        assets,
        tiles,
        markers,
        objects,
    })
}

fn parse_tile(
    tokens: &[String],
    cursor: &mut usize,
    version: usize,
    x: usize,
    y: usize,
) -> Result<RoomTile, String> {
    let kind = tokens.get(*cursor).ok_or("missing tile key")?.clone();
    *cursor += 1;
    let mut tile = RoomTile {
        x,
        y,
        kind: kind.clone(),
        width: 1,
        height: 1,
        edges: [0; 4],
        ground: [0; 4],
        elevations: [0; 4],
        feature: 0,
        origin: 0,
    };
    match kind.as_str() {
        "k" => {
            let field_count = if version > 18 { 24 } else { 23 };
            let fields = tokens
                .get(*cursor..*cursor + field_count)
                .ok_or("truncated tile key")?
                .iter()
                .map(|token| {
                    token
                        .parse::<i32>()
                        .map_err(|_| "invalid tile key integer".to_owned())
                })
                .collect::<Result<Vec<_>, _>>()?;
            *cursor += field_count;
            tile.width = positive_size(fields[0])?;
            tile.height = positive_size(fields[1])?;
            for index in 0..4 {
                tile.edges[index] =
                    usize::try_from(fields[2 + index]).map_err(|_| "negative edge index")?;
                tile.ground[index] =
                    usize::try_from(fields[14 + index]).map_err(|_| "negative ground index")?;
                tile.elevations[index] = fields[18 + index];
            }
            tile.feature = usize::try_from(fields[22]).map_err(|_| "negative feature index")?;
            if version > 18 {
                tile.origin = usize::try_from(fields[23]).map_err(|_| "negative tile origin")?;
            }
        }
        "f" => {
            tile.feature = tokens
                .get(*cursor)
                .ok_or("missing tile feature")?
                .parse()
                .map_err(|_| "invalid tile feature")?;
            *cursor += 1;
        }
        "s" | "n" | "o" => {}
        _ => return Err(format!("unknown tile key {kind}")),
    }
    Ok(tile)
}

fn positive_size(value: i32) -> Result<usize, String> {
    usize::try_from(value)
        .ok()
        .filter(|value| *value > 0 && *value <= MAX_CELLS)
        .ok_or_else(|| "invalid tile size".to_owned())
}

fn integers(line: &str) -> Result<Vec<usize>, String> {
    line.split_whitespace()
        .map(|token| {
            token
                .parse()
                .map_err(|_| "invalid header integer".to_owned())
        })
        .collect()
}

fn coordinate(token: &str) -> Result<f64, String> {
    token
        .parse::<f64>()
        .ok()
        .filter(|value| value.is_finite() && value.abs() <= 1_000_000.0)
        .ok_or_else(|| "invalid ARM coordinate".to_owned())
}

fn marker_kind(version: usize, section: usize) -> &'static str {
    match section {
        0 => "spawn",
        1 => "chest",
        3 if version >= 20 => "spawner",
        4 if version >= 26 => "entrance",
        5 if version >= 29 => "object",
        8 if version < 20 => "entrance",
        9 if (20..=24).contains(&version) => "entrance",
        _ => "marker",
    }
}

struct RoomReader<'a> {
    lines: Vec<&'a str>,
    cursor: usize,
}

impl<'a> RoomReader<'a> {
    fn new(text: &'a str) -> Self {
        Self {
            lines: text.lines().map(str::trim).collect(),
            cursor: 0,
        }
    }

    fn peek(&self) -> Option<&'a str> {
        self.lines.get(self.cursor).copied()
    }

    fn line(&mut self) -> Result<&'a str, String> {
        let line = self
            .peek()
            .ok_or_else(|| format!("truncated ARM at line {}", self.cursor + 1))?;
        self.cursor += 1;
        Ok(line)
    }

    fn count(&mut self) -> Result<usize, String> {
        self.line()?
            .parse::<usize>()
            .ok()
            .filter(|count| *count <= MAX_CELLS)
            .ok_or_else(|| "invalid ARM section count".to_owned())
    }

    fn quoted(&mut self) -> Result<String, String> {
        let line = self.line()?;
        if !line.starts_with('"') || !line.ends_with('"') {
            return Err("expected quoted ARM string".to_owned());
        }
        let tokens = tokenize_dgr_line(line);
        if tokens.len() != 1 {
            return Err("expected single ARM string".to_owned());
        }
        Ok(tokens[0].clone())
    }

    fn section(&mut self, version: usize) -> Result<Vec<&'a str>, String> {
        if version <= 31 {
            let count = self.count()?;
            return (0..count).map(|_| self.line()).collect();
        }
        let mut rows = Vec::new();
        loop {
            let line = self.line()?;
            if line == "-1" {
                return Ok(rows);
            }
            rows.push(line);
            if rows.len() > MAX_CELLS {
                return Err("ARM section exceeds supported bounds".to_owned());
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(version: usize, width: usize, height: usize) -> String {
        let mut fields = vec![0; if version > 18 { 24 } else { 23 }];
        fields[0] = width;
        fields[1] = height;
        fields[2] = 1;
        fields[14] = 2;
        format!(
            "k {}",
            fields
                .iter()
                .map(usize::to_string)
                .collect::<Vec<_>>()
                .join(" ")
        )
    }

    fn fixture(version: usize) -> String {
        let sections = if version <= 19 {
            9
        } else if version <= 24 {
            10
        } else if version <= 28 {
            5
        } else {
            6
        };
        let empty = if version <= 31 { "0\n" } else { "-1\n" };
        let spawn = if version <= 31 {
            "1\n12 24 0.5 \"mapboss\"\n"
        } else {
            "12 24 0.5 \"mapboss\"\n-1\n"
        };
        let object = "24 36 0 0 0 0 1 0 0 1 \"Metadata/Art/fire.ao\" \"Metadata/MiscellaneousObjects/Doodad\"\n";
        format!("version {version}\n2\n\"wall.et\"\n\"floor.gt\"\n1 0\n1 1\n\"camp\"\n1 0\n{}\n0 0\n0 0\n0 0\n0 0\n{spawn}{}{}{} n\ns f 2\n{}{}",
            key(version, 2, 2), empty.repeat(sections - 1), if version >= 35 { "\"\"\n" } else { "" },
            key(version, 2, 1), if version <= 31 { "1\n" } else { "" },
            format!("{object}{}", if version > 31 { "-1\n" } else { "" }))
    }

    #[test]
    fn parses_tiles_and_real_positions_across_supported_versions() {
        for version in [14, 18, 22, 26, 30, 31, 32, 35, 36] {
            let plan = parse_room_plan("fixture.arm", &fixture(version)).unwrap();
            assert_eq!((plan.width, plan.height), (2, 2));
            assert_eq!(plan.tile_size, 24);
            assert_eq!(plan.tiles.len(), 3);
            assert_eq!(plan.tiles[0].width, 2);
            assert_eq!(plan.tiles[0].ground, [2, 0, 0, 0]);
            assert_eq!(plan.tiles[2].feature, 2);
            assert_eq!(plan.markers[0].tag, "mapboss");
            assert_eq!((plan.markers[0].x, plan.markers[0].y), (12.0, 24.0));
            assert_eq!(plan.objects[0].art, "Metadata/Art/fire.ao");
        }
    }

    #[test]
    fn reads_non_key_roots_and_legacy_blank_label_separator() {
        let text = fixture(30)
            .replace("1 0\n1 1", "2 2\n1 1")
            .replace(&key(30, 2, 2), "s");
        let plan = parse_room_plan("simple.arm", &text).unwrap();
        assert_eq!((plan.width, plan.height), (2, 2));
        let legacy = fixture(22).replace("\"camp\"\n1 0", "\"camp\"\n\n1 0");
        assert!(parse_room_plan("legacy.arm", &legacy).is_ok());
    }

    #[test]
    fn preserves_key_spans_without_treating_them_as_anchored_rectangles() {
        let text = fixture(30).replace(
            &format!("{} n", key(30, 2, 1)),
            &format!("n {}", key(30, 3, 2)),
        );
        let plan = parse_room_plan("anchor.arm", &text).unwrap();
        assert_eq!((plan.tiles[0].x, plan.tiles[0].y), (1, 0));
        assert_eq!((plan.tiles[0].width, plan.tiles[0].height), (3, 2));
    }

    #[test]
    fn rejects_malformed_and_unsupported_data() {
        let valid = fixture(30);
        for text in [
            valid.replace("version 30", "version 99"),
            valid.replace("12 24 0.5", "NaN 24 0.5"),
            valid.replace("s f 2", "s z"),
            valid.replace("k 2 1 1", "k 2 1 999"),
            valid.replace("k 2 2", "k 0 2"),
            valid.lines().take(8).collect::<Vec<_>>().join("\n"),
        ] {
            assert!(parse_room_plan("bad.arm", &text).is_err());
        }
        assert!(parse_arm_room_plan("bad.arm", &[1]).is_err());
    }

    #[test]
    #[ignore = "requires the local Acts 1-5 raw cache"]
    fn validates_local_room_corpus() {
        fn visit(path: &std::path::Path, counts: &mut (usize, usize)) {
            for entry in std::fs::read_dir(path).unwrap() {
                let path = entry.unwrap().path();
                if path.is_dir() {
                    visit(&path, counts);
                } else if path.extension().is_some_and(|extension| extension == "arm") {
                    counts.0 += 1;
                    if let Err(error) =
                        parse_arm_room_plan(&path.to_string_lossy(), &std::fs::read(&path).unwrap())
                    {
                        counts.1 += 1;
                        eprintln!("{error}");
                    }
                }
            }
        }
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../.poe-layouts/raw/campaign-acts-1-5/files");
        let mut counts = (0, 0);
        visit(&root, &mut counts);
        eprintln!("ARM corpus: {} files, {} failures", counts.0, counts.1);
        assert!(counts.0 > 0);
        assert_eq!(counts.1, 0);
    }
}
