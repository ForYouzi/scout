use crate::Result;
use font_loader::system_fonts;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use tauri::{command, AppHandle, Manager, Runtime};
use ts_rs::TS;

#[derive(Default, Debug, Clone, Serialize, Deserialize, TS, PartialEq)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "gen_fonts.ts")]
pub struct Fonts {
    pub editor_fonts: Vec<String>,
    pub ui_fonts: Vec<String>,
}

#[command]
pub(crate) async fn list() -> Result<Fonts> {
    let mut ui_fonts = HashSet::new();
    let mut editor_fonts = HashSet::new();

    let mut property = system_fonts::FontPropertyBuilder::new().monospace().build();
    for font in &system_fonts::query_specific(&mut property) {
        editor_fonts.insert(font.to_string());
    }
    for font in &system_fonts::query_all() {
        if !editor_fonts.contains(font) {
            ui_fonts.insert(font.to_string());
        }
    }

    let mut ui_fonts: Vec<String> = ui_fonts.into_iter().collect();
    let mut editor_fonts: Vec<String> = editor_fonts.into_iter().collect();

    ui_fonts.sort();
    editor_fonts.sort();

    Ok(Fonts { ui_fonts, editor_fonts })
}

/// One file of an installed font family. `faces` lets the webview read it before returning it.
#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "gen_fonts.ts")]
pub struct FontFile {
    pub path: String,
    pub weight: u16,
    pub italic: bool,
    /// CSS font-stretch percentage.
    pub stretch: f32,
}

/// A font the Windows registry lists as installed: the value name and the file it points to.
#[derive(Debug, Clone)]
pub(crate) struct RegistryFont {
    pub name: String,
    pub path: PathBuf,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct FontInfo {
    pub weight: u16,
    pub italic: bool,
    pub stretch: f32,
    pub variable: bool,
}

/// The files of the installed font `name` (a family name, or a full name such as
/// "Rec Mono St.Helens Bold" that the font list offers on Windows), each one allowed in the asset
/// protocol scope. Returns [] unless every installed face of that family can be served as a file,
/// so the webview never gets only part of a family.
#[command]
pub(crate) async fn faces<R: Runtime>(app: AppHandle<R>, name: String) -> Result<Vec<FontFile>> {
    let scope = app.asset_protocol_scope();
    tauri::async_runtime::spawn_blocking(move || {
        faces_from(installed_fonts, &name, |path| Ok(scope.allow_file(path)?))
    })
    .await?
}

/// Runs `scan` on every call, with no cache, so a font installed while Scout runs is seen.
pub(crate) fn faces_from(
    scan: impl FnOnce() -> (fontdb::Database, Vec<RegistryFont>),
    name: &str,
    allow: impl FnMut(&Path) -> Result<()>,
) -> Result<Vec<FontFile>> {
    let (db, registry) = scan();
    resolve_faces(&db, &registry, name, allow)
}

/// Every installed font, scanned fresh so a font installed while Scout runs is seen.
/// Only Windows is supported: elsewhere the database stays empty and `faces` returns [].
fn installed_fonts() -> (fontdb::Database, Vec<RegistryFont>) {
    let mut db = fontdb::Database::new();
    if cfg!(windows) {
        db.load_system_fonts();
    }
    // fontdb scans fixed font folders; the registry also lists fonts installed elsewhere.
    // shortcut: fonts activated without a registry entry (AddFontResource by another app) are not
    // seen, so such a family can come back incomplete; upgrade if that is ever reported.
    let registry = registry_fonts();
    load_registry_fonts(&mut db, &registry);
    (db, registry)
}

/// Machine-wide (HKLM) and per-user (HKCU) installed fonts.
#[cfg(windows)]
fn registry_fonts() -> Vec<RegistryFont> {
    use winreg::{HKCU, HKLM};

    let fonts_dir =
        PathBuf::from(std::env::var_os("WINDIR").unwrap_or_else(|| "C:\\Windows".into()))
            .join("Fonts");
    [HKLM, HKCU]
        .into_iter()
        .filter_map(|root| {
            root.open_subkey("SOFTWARE\\Microsoft\\Windows NT\\CurrentVersion\\Fonts").ok()
        })
        .flat_map(|key| read_font_key(&key, &fonts_dir))
        .collect()
}

/// The fonts one registry Fonts key lists; relative file names live in `fonts_dir`.
#[cfg(windows)]
fn read_font_key(key: &winreg::RegKey, fonts_dir: &Path) -> Vec<RegistryFont> {
    key.enum_values()
        .flatten()
        .filter_map(|(name, _)| {
            let value: String = key.get_value(&name).ok()?;
            let path = PathBuf::from(value);
            let path = if path.is_absolute() { path } else { fonts_dir.join(path) };
            Some(RegistryFont { name, path })
        })
        .collect()
}

#[cfg(not(windows))]
fn registry_fonts() -> Vec<RegistryFont> {
    Vec::new()
}

/// Loads the registry-listed font files the folder scan did not find.
pub(crate) fn load_registry_fonts(db: &mut fontdb::Database, registry: &[RegistryFont]) {
    let mut known: HashSet<String> = db.faces().filter_map(face_path).map(path_key).collect();
    for font in registry {
        if known.insert(path_key(&font.path)) {
            // A file that yields no face is caught by `select_files`, which then returns [].
            let _ = db.load_font_file(&font.path);
        }
    }
}

/// Picks the files of `name` and only then passes each one to `allow`, so a name that is unknown,
/// path-shaped or incomplete never widens what the webview can read.
pub(crate) fn resolve_faces(
    db: &fontdb::Database,
    registry: &[RegistryFont],
    name: &str,
    mut allow: impl FnMut(&Path) -> Result<()>,
) -> Result<Vec<FontFile>> {
    let files = select_files(db, registry, name);
    for file in &files {
        allow(Path::new(&file.path))?;
    }
    Ok(files)
}

fn select_files(db: &fontdb::Database, registry: &[RegistryFont], name: &str) -> Vec<FontFile> {
    let Some(family) = family_of(db, name) else {
        return Vec::new();
    };
    let family_key = family.to_lowercase();

    if !registry_complete(db, registry, &family_key) {
        return Vec::new();
    }

    // Every face must be a static font file of its own; copies share a PostScript name.
    let mut by_postscript: HashMap<&str, Vec<(PathBuf, FontInfo)>> = HashMap::new();
    for face in db.faces().filter(|face| has_family(face, &family_key)) {
        let Some(path) = face_path(face) else {
            return Vec::new();
        };
        if face.index != 0 || !is_font_file(path) {
            return Vec::new();
        }
        let Some(info) = db.with_face_data(face.id, font_info).flatten() else {
            return Vec::new();
        };
        if info.variable {
            return Vec::new();
        }
        by_postscript
            .entry(face.post_script_name.as_str())
            .or_default()
            .push((path.to_path_buf(), info));
    }

    // Of several copies, Windows uses the one its registry lists; anything else is a guess.
    let listed: HashSet<String> = registry.iter().map(|font| path_key(&font.path)).collect();
    let mut files = Vec::new();
    for copies in by_postscript.into_values() {
        let chosen = if copies.len() == 1 {
            copies.into_iter().next()
        } else {
            let mut listed_copies =
                copies.into_iter().filter(|(path, _)| listed.contains(&path_key(path)));
            match (listed_copies.next(), listed_copies.next()) {
                (Some(copy), None) => Some(copy),
                _ => None,
            }
        };
        let Some((path, info)) = chosen else {
            return Vec::new();
        };
        let Some(path) = path.to_str() else {
            return Vec::new();
        };
        files.push(FontFile {
            path: path.to_string(),
            weight: info.weight,
            italic: info.italic,
            stretch: info.stretch,
        });
    }

    // Two faces with the same descriptors can't both be registered faithfully.
    let mut slots = HashSet::new();
    if !files.iter().all(|file| slots.insert((file.weight, file.italic, file.stretch.to_bits()))) {
        return Vec::new();
    }
    files.sort_by(|a, b| a.path.cmp(&b.path));
    files
}

/// The family `name` belongs to: a family of that name, or the family of a face whose full name
/// it is. Full names are only checked for families the name starts with.
fn family_of(db: &fontdb::Database, name: &str) -> Option<String> {
    let wanted = name.to_lowercase();
    // A name with a path separator is never a font this command serves, whatever is installed.
    if wanted.is_empty() || name.contains(['/', '\\']) {
        return None;
    }
    for face in db.faces() {
        if let Some((family, _)) =
            face.families.iter().find(|(family, _)| family.to_lowercase() == wanted)
        {
            return Some(family.clone());
        }
    }
    for face in db.faces() {
        let Some((family, _)) =
            face.families.iter().find(|(family, _)| wanted.starts_with(&family.to_lowercase()))
        else {
            continue;
        };
        let full_names = db.with_face_data(face.id, full_names).unwrap_or_default();
        if full_names.iter().any(|full| full.to_lowercase() == wanted) {
            return Some(family.clone());
        }
    }
    None
}

fn has_family(face: &fontdb::FaceInfo, family_key: &str) -> bool {
    face.families.iter().any(|(family, _)| family.to_lowercase() == family_key)
}

/// Whether no registry font that may hold one of the family's styles is only partly loaded. A file
/// that loaded fewer faces than its header lists may be missing one of the family's styles when
/// the file already gave a face of the family, its registry name mentions one of the family's
/// names (fontdb's, or the legacy and typographic names its faces declare), or one of its faces
/// declares such a name. Registering the rest of the family would then leave that style out.
/// shortcut: a face that didn't load and whose name table can't be read either is only caught by
/// the registry name; upgrade to DirectWrite's family grouping if that is ever reported.
fn registry_complete(db: &fontdb::Database, registry: &[RegistryFont], family_key: &str) -> bool {
    let mut names: HashSet<String> = HashSet::from([family_key.to_string()]);
    let mut family_files = HashSet::new();
    // Per file: how many faces loaded, and the path fontdb read them from.
    let mut loaded: HashMap<String, (usize, PathBuf)> = HashMap::new();
    for face in db.faces() {
        let Some(path) = face_path(face) else {
            continue;
        };
        let key = path_key(path);
        loaded.entry(key.clone()).or_insert_with(|| (0, path.to_path_buf())).0 += 1;
        if has_family(face, family_key) {
            family_files.insert(key);
            for name in db.with_face_data(face.id, family_names).unwrap_or_default() {
                names.insert(name.to_lowercase());
            }
        }
    }

    registry.iter().all(|font| {
        let key = path_key(&font.path);
        let (count, path) = match loaded.get(&key) {
            Some((count, path)) => (*count, path.as_path()),
            None => (0, font.path.as_path()),
        };
        if faces_in_file(path) == Some(count) {
            return true;
        }
        let entry = font.name.to_lowercase();
        let relevant = family_files.contains(&key)
            || names.iter().any(|name| entry.contains(name.as_str()))
            || declared_families(path).iter().any(|name| names.contains(&name.to_lowercase()));
        !relevant
    })
}

/// Every family name (IDs 1 and 16) any face in the file declares, read without fontdb.
fn declared_families(path: &Path) -> Vec<String> {
    let Ok(data) = std::fs::read(path) else {
        return Vec::new();
    };
    let faces = ttf_parser::fonts_in_collection(&data).unwrap_or(1);
    (0..faces).flat_map(|index| family_names(&data, index)).collect()
}

/// How many faces a font file holds, from its header; None when the file can't be opened.
fn faces_in_file(path: &Path) -> Option<usize> {
    use std::io::Read;

    let mut file = std::fs::File::open(path).ok()?;
    let mut header = [0u8; 12];
    match file.read_exact(&mut header) {
        Ok(()) if &header[0..4] == b"ttcf" => {
            Some(u32::from_be_bytes([header[8], header[9], header[10], header[11]]) as usize)
        }
        _ => Some(1),
    }
}

/// The family names a face declares: name IDs 1 (legacy family) and 16 (typographic family).
fn family_names(data: &[u8], index: u32) -> Vec<String> {
    names_with_ids(
        data,
        index,
        &[
            ttf_parser::name_id::FAMILY,
            ttf_parser::name_id::TYPOGRAPHIC_FAMILY,
        ],
    )
}

fn full_names(data: &[u8], index: u32) -> Vec<String> {
    names_with_ids(data, index, &[ttf_parser::name_id::FULL_NAME])
}

fn names_with_ids(data: &[u8], index: u32, ids: &[u16]) -> Vec<String> {
    let Ok(face) = ttf_parser::RawFace::parse(data, index) else {
        return Vec::new();
    };
    let Some(table) =
        face.table(ttf_parser::Tag::from_bytes(b"name")).and_then(ttf_parser::name::Table::parse)
    else {
        return Vec::new();
    };
    table
        .names
        .into_iter()
        .filter(|name| ids.contains(&name.name_id))
        .filter_map(|name| decode_name(&name))
        .collect()
}

/// A name record's text. The Unicode platform and Windows encodings 0 (symbol), 1 (BMP) and
/// 10 (full repertoire) are all UTF-16BE; ttf-parser's `to_string` skips encoding 10.
fn decode_name(name: &ttf_parser::name::Name) -> Option<String> {
    let utf16 = match name.platform_id {
        ttf_parser::PlatformId::Unicode => true,
        ttf_parser::PlatformId::Windows => matches!(name.encoding_id, 0 | 1 | 10),
        _ => false,
    };
    let (pairs, rest) = name.name.as_chunks::<2>();
    if !utf16 || !rest.is_empty() {
        return None;
    }
    let units: Vec<u16> = pairs.iter().map(|pair| u16::from_be_bytes(*pair)).collect();
    String::from_utf16(&units).ok()
}

/// Weight, style and width from the OS/2 table. Italic comes from fsSelection only: some upright
/// faces (the Rec fonts) carry a slant angle in `post`, which fontdb would read as italic.
pub(crate) fn font_info(data: &[u8], index: u32) -> Option<FontInfo> {
    let face = ttf_parser::RawFace::parse(data, index).ok()?;
    let os2 = ttf_parser::os2::Table::parse(face.table(ttf_parser::Tag::from_bytes(b"OS/2"))?)?;
    Some(FontInfo {
        weight: os2.weight().to_number(),
        italic: os2.style() == ttf_parser::Style::Italic,
        stretch: css_stretch(os2.width().to_number()),
        variable: face.table(ttf_parser::Tag::from_bytes(b"fvar")).is_some(),
    })
}

/// OS/2 usWidthClass to the CSS font-stretch percentage.
fn css_stretch(width_class: u16) -> f32 {
    match width_class {
        1 => 50.0,
        2 => 62.5,
        3 => 75.0,
        4 => 87.5,
        6 => 112.5,
        7 => 125.0,
        8 => 150.0,
        9 => 200.0,
        _ => 100.0,
    }
}

fn face_path(face: &fontdb::FaceInfo) -> Option<&Path> {
    match &face.source {
        fontdb::Source::File(path) | fontdb::Source::SharedFile(path, _) => Some(path),
        fontdb::Source::Binary(_) => None,
    }
}

/// Windows paths compare without case or separator differences.
fn path_key(path: &Path) -> String {
    path.to_string_lossy().replace('/', "\\").to_lowercase()
}

fn is_font_file(path: &Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| ext.eq_ignore_ascii_case("ttf") || ext.eq_ignore_ascii_case("otf"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::{Path, PathBuf};
    use tauri::Manager;

    /// A minimal TrueType font: OS/2, name and post tables (plus fvar when variable).
    struct TestFont {
        family: String,
        typographic: Option<String>,
        full: String,
        postscript: String,
        weight: u16,
        width: u16,
        italic: bool,
        variable: bool,
        /// Windows encoding ID of the full-name record (1 = BMP, 10 = full repertoire).
        full_encoding: u16,
        os2: bool,
        /// fontdb skips a face with no PostScript name record.
        postscript_record: bool,
    }

    impl TestFont {
        fn new(family: &str, style: &str, weight: u16, italic: bool) -> Self {
            let full =
                if style == "Regular" { family.to_string() } else { format!("{family} {style}") };
            Self {
                family: family.into(),
                typographic: None,
                full,
                postscript: format!("{}-{style}", family.replace(' ', "")),
                weight,
                width: 5,
                italic,
                variable: false,
                full_encoding: 1,
                os2: true,
                postscript_record: true,
            }
        }

        fn postscript(mut self, postscript: &str) -> Self {
            self.postscript = postscript.into();
            self
        }

        fn width(mut self, width: u16) -> Self {
            self.width = width;
            self
        }

        fn variable(mut self) -> Self {
            self.variable = true;
            self
        }

        fn typographic(mut self, family: &str) -> Self {
            self.typographic = Some(family.into());
            self
        }

        fn full(mut self, full: &str) -> Self {
            self.full = full.into();
            self
        }

        fn full_encoding(mut self, encoding: u16) -> Self {
            self.full_encoding = encoding;
            self
        }

        fn without_os2(mut self) -> Self {
            self.os2 = false;
            self
        }

        fn without_postscript(mut self) -> Self {
            self.postscript_record = false;
            self
        }

        fn bytes(&self) -> Vec<u8> {
            self.bytes_at(0)
        }

        /// The font's bytes when it starts `base` bytes into a file (table offsets are absolute).
        fn bytes_at(&self, base: usize) -> Vec<u8> {
            let mut os2 = vec![0u8; 96];
            os2[0..2].copy_from_slice(&4u16.to_be_bytes());
            os2[4..6].copy_from_slice(&self.weight.to_be_bytes());
            os2[6..8].copy_from_slice(&self.width.to_be_bytes());
            let selection: u16 = if self.italic { 0x0001 } else { 0x0040 };
            os2[62..64].copy_from_slice(&selection.to_be_bytes());

            // Upright faces carry a slant angle too, as the Rec fonts do; italic must come from OS/2.
            let mut post = vec![0u8; 32];
            post[0..4].copy_from_slice(&0x0003_0000u32.to_be_bytes());
            post[4..8].copy_from_slice(&(-3i32 * 65536).to_be_bytes());

            let subfamily = if self.italic { "Italic" } else { "Regular" };
            let mut names: Vec<(u16, u16, &str)> = vec![
                (1, 1, self.family.as_str()),
                (1, 2, subfamily),
                (self.full_encoding, 4, self.full.as_str()),
            ];
            if self.postscript_record {
                names.push((1, 6, self.postscript.as_str()));
            }
            if let Some(typographic) = &self.typographic {
                names.push((1, 16, typographic.as_str()));
            }
            // Name records are sorted by platform, encoding, language, then name ID.
            names.sort_by_key(|(encoding, id, _)| (*encoding, *id));
            let mut records = Vec::new();
            let mut strings = Vec::new();
            for (encoding, id, text) in &names {
                let encoded: Vec<u8> = text.encode_utf16().flat_map(u16::to_be_bytes).collect();
                for value in [
                    3u16,
                    *encoding,
                    0x0409,
                    *id,
                    encoded.len() as u16,
                    strings.len() as u16,
                ] {
                    records.extend_from_slice(&value.to_be_bytes());
                }
                strings.extend_from_slice(&encoded);
            }
            let mut name = Vec::new();
            for value in [0u16, names.len() as u16, (6 + records.len()) as u16] {
                name.extend_from_slice(&value.to_be_bytes());
            }
            name.extend_from_slice(&records);
            name.extend_from_slice(&strings);

            // Table records must be sorted by tag.
            let mut tables: Vec<(&[u8; 4], Vec<u8>)> = Vec::new();
            if self.os2 {
                tables.push((b"OS/2", os2));
            }
            if self.variable {
                tables.push((b"fvar", vec![0u8; 16]));
            }
            tables.push((b"name", name));
            tables.push((b"post", post));

            let header_len = 12 + 16 * tables.len();
            let mut font = Vec::new();
            font.extend_from_slice(&0x0001_0000u32.to_be_bytes());
            for value in [tables.len() as u16, 0, 0, 0] {
                font.extend_from_slice(&value.to_be_bytes());
            }
            let mut body = Vec::new();
            for (tag, data) in &tables {
                font.extend_from_slice(*tag);
                font.extend_from_slice(&0u32.to_be_bytes());
                font.extend_from_slice(&((base + header_len + body.len()) as u32).to_be_bytes());
                font.extend_from_slice(&(data.len() as u32).to_be_bytes());
                body.extend_from_slice(data);
                while body.len() % 4 != 0 {
                    body.push(0);
                }
            }
            font.extend_from_slice(&body);
            font
        }
    }

    /// A TrueType collection of the given faces; `None` is a face that does not parse.
    fn collection(faces: &[Option<&TestFont>]) -> Vec<u8> {
        let header_len = 12 + 4 * faces.len();
        let mut offsets = Vec::new();
        let mut body = Vec::new();
        for face in faces {
            let offset = header_len + body.len();
            offsets.push(offset as u32);
            match face {
                Some(font) => body.extend_from_slice(&font.bytes_at(offset)),
                None => body.extend_from_slice(&[0xff; 16]),
            }
            while body.len() % 4 != 0 {
                body.push(0);
            }
        }
        let mut file = Vec::new();
        file.extend_from_slice(b"ttcf");
        file.extend_from_slice(&0x0001_0000u32.to_be_bytes());
        file.extend_from_slice(&(faces.len() as u32).to_be_bytes());
        for offset in offsets {
            file.extend_from_slice(&offset.to_be_bytes());
        }
        file.extend_from_slice(&body);
        file
    }

    /// A scratch folder whose path has spaces and a non-ASCII letter, as user font folders can.
    struct TempDir(PathBuf);

    impl TempDir {
        fn new(tag: &str) -> Self {
            let dir =
                std::env::temp_dir().join(format!("yaak fonts ë {tag} {}", std::process::id()));
            let _ = std::fs::remove_dir_all(&dir);
            std::fs::create_dir_all(&dir).unwrap();
            Self(dir)
        }

        fn write(&self, name: &str, bytes: &[u8]) -> PathBuf {
            let path = self.0.join(name);
            std::fs::write(&path, bytes).unwrap();
            path
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn database(paths: &[&Path]) -> fontdb::Database {
        let mut db = fontdb::Database::new();
        for path in paths {
            db.load_font_file(path).unwrap();
        }
        db
    }

    /// Resolves `name`, recording every path passed to `allow`.
    fn resolve(
        db: &fontdb::Database,
        registry: &[RegistryFont],
        name: &str,
    ) -> (Vec<FontFile>, Vec<PathBuf>) {
        let mut allowed = Vec::new();
        let files = resolve_faces(db, registry, name, |path| {
            allowed.push(path.to_path_buf());
            Ok(())
        })
        .unwrap();
        (files, allowed)
    }

    fn paths(files: &[FontFile]) -> Vec<PathBuf> {
        files.iter().map(|file| PathBuf::from(&file.path)).collect()
    }

    fn mono_family(dir: &TempDir) -> Vec<PathBuf> {
        vec![
            dir.write(
                "Mono-Regular.ttf",
                &TestFont::new("Example Mono", "Regular", 400, false).bytes(),
            ),
            dir.write("Mono-Bold.ttf", &TestFont::new("Example Mono", "Bold", 600, false).bytes()),
            dir.write(
                "Mono-Italic.ttf",
                &TestFont::new("Example Mono", "Italic", 400, true).bytes(),
            ),
        ]
    }

    fn registry_font(name: &str, path: &Path) -> RegistryFont {
        RegistryFont { name: name.into(), path: path.to_path_buf() }
    }

    #[test]
    fn font_info_reads_weight_style_and_width_from_os2() {
        let regular = TestFont::new("Example Mono", "Regular", 400, false).bytes();
        assert_eq!(
            font_info(&regular, 0),
            Some(FontInfo { weight: 400, italic: false, stretch: 100.0, variable: false })
        );

        let italic = TestFont::new("Example Mono", "Italic", 400, true).width(3).bytes();
        assert_eq!(
            font_info(&italic, 0),
            Some(FontInfo { weight: 400, italic: true, stretch: 75.0, variable: false })
        );

        let variable = TestFont::new("Example Mono", "Regular", 400, false).variable().bytes();
        assert_eq!(font_info(&variable, 0).map(|info| info.variable), Some(true));

        assert_eq!(font_info(b"not a font", 0), None);
    }

    #[test]
    fn resolves_a_complete_family_by_family_or_full_name() {
        let dir = TempDir::new("complete");
        let files = mono_family(&dir);
        let other =
            dir.write("Other.ttf", &TestFont::new("Other Sans", "Regular", 400, false).bytes());
        let mut all: Vec<&Path> = files.iter().map(PathBuf::as_path).collect();
        all.push(&other);
        let db = database(&all);

        let mut expected = files.clone();
        expected.sort();
        for name in [
            "Example Mono",
            "example mono",
            "Example Mono Bold",
            "Example Mono Italic",
        ] {
            let (found, allowed) = resolve(&db, &[], name);
            assert_eq!(paths(&found), expected, "{name}");
            assert_eq!(allowed, expected, "{name}");
        }

        let (found, _) = resolve(&db, &[], "Example Mono");
        let bold = found.iter().find(|file| file.weight == 600).unwrap();
        assert!(!bold.italic);
        assert_eq!(bold.stretch, 100.0);
        assert_eq!(found.iter().filter(|file| file.italic).count(), 1);
    }

    #[test]
    fn authorizes_exactly_the_returned_files_in_the_asset_scope() {
        let dir = TempDir::new("scope");
        let files = mono_family(&dir);
        let sibling = dir.write("notes.txt", b"not a font");
        let other =
            dir.write("Other.ttf", &TestFont::new("Other Sans", "Regular", 400, false).bytes());
        let mut all: Vec<&Path> = files.iter().map(PathBuf::as_path).collect();
        all.push(&other);
        let db = database(&all);

        let app = tauri::test::mock_app();
        let scope = app.asset_protocol_scope();
        let found =
            resolve_faces(&db, &[], "Example Mono", |path| Ok(scope.allow_file(path)?)).unwrap();

        assert_eq!(found.len(), 3);
        for file in &files {
            assert!(scope.is_allowed(file), "{}", file.display());
        }
        assert!(!scope.is_allowed(&dir.0));
        assert!(!scope.is_allowed(&sibling));
        assert!(!scope.is_allowed(&other));
    }

    #[test]
    fn unknown_and_path_shaped_names_authorize_nothing() {
        let dir = TempDir::new("unknown");
        let files = mono_family(&dir);
        let all: Vec<&Path> = files.iter().map(PathBuf::as_path).collect();
        let db = database(&all);

        let dir_name = dir.0.to_string_lossy().to_string();
        let file_name = files[0].to_string_lossy().to_string();
        for name in [
            "Nope Sans",
            "",
            "*",
            "..\\Example Mono",
            dir_name.as_str(),
            file_name.as_str(),
        ] {
            let (found, allowed) = resolve(&db, &[], name);
            assert!(found.is_empty(), "{name}");
            assert!(allowed.is_empty(), "{name}");
        }
    }

    #[test]
    fn a_full_name_that_does_not_start_with_its_family_is_not_resolved() {
        let dir = TempDir::new("nonprefix");
        let path = dir.write(
            "Narrow.ttf",
            &TestFont::new("Example Narrow", "Bold", 700, false)
                .typographic("Example Collection")
                .full("Example Narrow Bold")
                .bytes(),
        );
        let db = database(&[&path]);

        assert!(resolve(&db, &[], "Example Narrow Bold").0.is_empty());
        assert_eq!(resolve(&db, &[], "Example Collection").0.len(), 1);
    }

    #[test]
    fn duplicate_copies_resolve_to_the_registry_listed_file() {
        let dir = TempDir::new("duplicates");
        let regular = TestFont::new("Example Mono", "Regular", 400, false).bytes();
        let a = dir.write("Mono-Regular.ttf", &regular);
        let b = dir.write("Mono-Regular_0.ttf", &regular);
        let db = database(&[&a, &b]);

        let listed_a = [registry_font("Example Mono (TrueType)", &a)];
        let listed_b = [registry_font("Example Mono (TrueType)", &b)];
        assert_eq!(paths(&resolve(&db, &listed_a, "Example Mono").0), vec![a.clone()]);
        assert_eq!(paths(&resolve(&db, &listed_b, "Example Mono").0), vec![b.clone()]);

        // Neither or both listed: there is no telling which copy Windows uses.
        assert!(resolve(&db, &[], "Example Mono").0.is_empty());
        let both = [
            registry_font("Example Mono (TrueType)", &a),
            registry_font("Example Mono #2 (TrueType)", &b),
        ];
        assert!(resolve(&db, &both, "Example Mono").0.is_empty());
    }

    #[test]
    fn faces_that_would_share_a_slot_make_the_family_ambiguous() {
        let dir = TempDir::new("ambiguous");
        let a = dir.write("A.ttf", &TestFont::new("Example Mono", "Regular", 400, false).bytes());
        let b = dir.write(
            "B.ttf",
            &TestFont::new("Example Mono", "Regular", 400, false)
                .postscript("ExampleMono-Book")
                .bytes(),
        );

        let (found, allowed) = resolve(&database(&[&a, &b]), &[], "Example Mono");
        assert!(found.is_empty());
        assert!(allowed.is_empty());
    }

    #[test]
    fn condensed_and_normal_faces_keep_their_own_stretch() {
        let dir = TempDir::new("widths");
        let a = dir
            .write("Regular.ttf", &TestFont::new("Example Serif", "Regular", 400, false).bytes());
        let b = dir.write(
            "Condensed.ttf",
            &TestFont::new("Example Serif", "Condensed", 400, false).width(3).bytes(),
        );

        let found = resolve(&database(&[&a, &b]), &[], "Example Serif").0;
        let mut stretches: Vec<f32> = found.iter().map(|file| file.stretch).collect();
        stretches.sort_by(f32::total_cmp);
        assert_eq!(stretches, vec![75.0, 100.0]);
    }

    #[test]
    fn one_unusable_face_leaves_the_whole_family_alone() {
        let dir = TempDir::new("unusable");
        let regular =
            dir.write("Regular.ttf", &TestFont::new("Example Mono", "Regular", 400, false).bytes());
        let bold = TestFont::new("Example Mono", "Bold", 700, false);

        let variable = dir.write(
            "Bold-VF.ttf",
            &TestFont::new("Example Mono", "Bold", 700, false).variable().bytes(),
        );
        let (found, allowed) = resolve(&database(&[&regular, &variable]), &[], "Example Mono");
        assert!(found.is_empty() && allowed.is_empty(), "variable member");

        let collection = dir.write("Bold.ttc", &bold.bytes());
        assert!(
            resolve(&database(&[&regular, &collection]), &[], "Example Mono").0.is_empty(),
            "ttc member"
        );

        let mut db = database(&[&regular]);
        db.load_font_data(bold.bytes());
        assert!(resolve(&db, &[], "Example Mono").0.is_empty(), "in-memory member");

        let second = dir.write("Bold.ttf", &bold.bytes());
        let mut db = database(&[&regular]);
        db.push_face_info(fontdb::FaceInfo {
            id: fontdb::ID::dummy(),
            source: fontdb::Source::File(second),
            index: 1,
            families: vec![("Example Mono".into(), fontdb::Language::English_UnitedStates)],
            post_script_name: "ExampleMono-Bold".into(),
            style: fontdb::Style::Normal,
            weight: fontdb::Weight(700),
            stretch: fontdb::Stretch::Normal,
            monospaced: false,
        });
        assert!(resolve(&db, &[], "Example Mono").0.is_empty(), "collection index 1");
    }

    #[test]
    fn registry_fonts_outside_the_scanned_folders_are_loaded_once() {
        let dir = TempDir::new("registry");
        let regular =
            dir.write("Regular.ttf", &TestFont::new("Example Mono", "Regular", 400, false).bytes());
        let elsewhere = TempDir::new("registry elsewhere");
        let bold =
            elsewhere.write("Bold.ttf", &TestFont::new("Example Mono", "Bold", 700, false).bytes());

        let mut db = database(&[&regular]);
        let registry = [
            // Registry paths differ in case from the scan's.
            registry_font(
                "Example Mono (TrueType)",
                &PathBuf::from(regular.to_string_lossy().to_uppercase()),
            ),
            registry_font("Example Mono Bold (TrueType)", &bold),
        ];
        load_registry_fonts(&mut db, &registry);

        assert_eq!(db.len(), 2);
        assert_eq!(resolve(&db, &registry, "Example Mono").0.len(), 2);
    }

    #[test]
    fn a_registry_listed_face_that_did_not_load_leaves_the_family_alone() {
        let dir = TempDir::new("registry broken");
        let regular =
            dir.write("Regular.ttf", &TestFont::new("Example Mono", "Regular", 400, false).bytes());
        let broken = dir.write("Bold.ttf", b"not a font");

        let mut db = database(&[&regular]);
        let registry = [
            registry_font("Example Mono (TrueType)", &regular),
            registry_font("Example Mono Bold (TrueType)", &broken),
        ];
        load_registry_fonts(&mut db, &registry);

        let (found, allowed) = resolve(&db, &registry, "Example Mono");
        assert!(found.is_empty());
        assert!(allowed.is_empty());
    }

    #[test]
    fn a_full_name_in_the_full_unicode_encoding_resolves() {
        let dir = TempDir::new("encoding10");
        let regular =
            dir.write("Regular.ttf", &TestFont::new("Example Mono", "Regular", 400, false).bytes());
        let bold = dir.write(
            "Bold.ttf",
            &TestFont::new("Example Mono", "Bold", 600, false).full_encoding(10).bytes(),
        );
        let db = database(&[&regular, &bold]);

        let mut expected = vec![bold.clone(), regular.clone()];
        expected.sort();
        assert_eq!(paths(&resolve(&db, &[], "Example Mono").0), expected);
        assert_eq!(paths(&resolve(&db, &[], "Example Mono Bold").0), expected);
    }

    #[test]
    fn a_family_named_like_a_path_is_never_resolved() {
        let dir = TempDir::new("path family");
        let a =
            dir.write("A.ttf", &TestFont::new("..\\Example Mono", "Regular", 400, false).bytes());
        let b = dir.write("B.ttf", &TestFont::new("Example/Mono", "Regular", 400, false).bytes());
        let db = database(&[&a, &b]);

        for name in ["..\\Example Mono", "Example/Mono"] {
            let (found, allowed) = resolve(&db, &[], name);
            assert!(found.is_empty(), "{name}");
            assert!(allowed.is_empty(), "{name}");
        }
    }

    #[test]
    fn a_face_without_an_os2_table_leaves_the_family_alone() {
        let dir = TempDir::new("no os2");
        let regular =
            dir.write("Regular.ttf", &TestFont::new("Example Mono", "Regular", 400, false).bytes());
        let bold = dir.write(
            "Bold.ttf",
            &TestFont::new("Example Mono", "Bold", 600, false).without_os2().bytes(),
        );
        let db = database(&[&regular, &bold]);
        assert_eq!(db.len(), 2, "fontdb keeps a face that has no OS/2 table");

        let (found, allowed) = resolve(&db, &[], "Example Mono");
        assert!(found.is_empty());
        assert!(allowed.is_empty());
    }

    #[test]
    fn every_call_scans_again_so_a_newly_installed_font_is_found() {
        let dir = TempDir::new("fresh scan");
        let scans = std::cell::Cell::new(0);
        let scan = || {
            scans.set(scans.get() + 1);
            let mut db = fontdb::Database::new();
            db.load_fonts_dir(&dir.0);
            (db, Vec::new())
        };

        assert!(faces_from(scan, "Example Mono", |_| Ok(())).unwrap().is_empty());
        dir.write(
            "Mono-Regular.ttf",
            &TestFont::new("Example Mono", "Regular", 400, false).bytes(),
        );
        assert_eq!(faces_from(scan, "Example Mono", |_| Ok(())).unwrap().len(), 1);
        assert_eq!(scans.get(), 2);
    }

    #[cfg(windows)]
    #[test]
    fn reads_registry_fonts_with_relative_names_under_the_fonts_folder() {
        use winreg::HKCU;

        let subkey = format!(r"Software\Scout yaak-fonts test {}", std::process::id());
        let (key, _) = HKCU.create_subkey(&subkey).unwrap();
        key.set_value("Example Mono (TrueType)", &"ExampleMono.ttf").unwrap();
        key.set_value("Example Mono Bold (TrueType)", &r"D:\Elsewhere\ExampleMono-Bold.ttf")
            .unwrap();

        let fonts = read_font_key(&key, Path::new(r"C:\Windows\Fonts"));
        HKCU.delete_subkey_all(&subkey).unwrap();

        let mut found: Vec<(String, PathBuf)> =
            fonts.into_iter().map(|font| (font.name, font.path)).collect();
        found.sort();
        assert_eq!(
            found,
            vec![
                (
                    "Example Mono (TrueType)".into(),
                    PathBuf::from(r"C:\Windows\Fonts\ExampleMono.ttf")
                ),
                (
                    "Example Mono Bold (TrueType)".into(),
                    PathBuf::from(r"D:\Elsewhere\ExampleMono-Bold.ttf")
                ),
            ]
        );
    }

    #[cfg(windows)]
    #[test]
    fn registry_fonts_include_the_machine_wide_fonts() {
        let fonts = registry_fonts();
        let arial = fonts
            .iter()
            .find(|font| font.name.eq_ignore_ascii_case("Arial (TrueType)"))
            .expect("every Windows install registers Arial in HKLM");
        assert!(arial.path.is_absolute());
        assert!(path_key(&arial.path).ends_with(r"\fonts\arial.ttf"), "{}", arial.path.display());
    }

    #[cfg(not(windows))]
    #[test]
    fn other_platforms_find_nothing_and_authorize_nothing() {
        let (db, registry) = installed_fonts();
        assert_eq!(db.len(), 0);
        assert!(registry.is_empty());

        let mut allowed = 0;
        let found = faces_from(installed_fonts, "Arial", |_| {
            allowed += 1;
            Ok(())
        })
        .unwrap();
        assert!(found.is_empty());
        assert_eq!(allowed, 0);
    }

    #[test]
    fn a_registry_entry_under_the_legacy_family_name_still_counts() {
        let dir = TempDir::new("legacy registry name");
        let regular = dir.write(
            "Narrow.ttf",
            &TestFont::new("Example Narrow", "Regular", 400, false)
                .typographic("Example Collection")
                .bytes(),
        );
        let broken = dir.write("Narrow-Bold.ttf", b"not a font");
        let mut db = database(&[&regular]);
        let registry = [
            registry_font("Example Narrow (TrueType)", &regular),
            registry_font("Example Narrow Bold (TrueType)", &broken),
        ];
        load_registry_fonts(&mut db, &registry);

        let (found, allowed) = resolve(&db, &registry, "Example Collection");
        assert!(found.is_empty());
        assert!(allowed.is_empty());
    }

    #[test]
    fn a_registry_file_that_loaded_only_some_faces_leaves_the_family_alone() {
        let dir = TempDir::new("partial collection");
        let regular = dir.write(
            "Mono-Regular.ttf",
            &TestFont::new("Example Mono", "Regular", 400, false).bytes(),
        );
        let shared = dir.write(
            "Mono-Bold-Other.ttc",
            &collection(&[
                Some(&TestFont::new("Other Sans", "Regular", 400, false)),
                None,
            ]),
        );
        let mut db = database(&[&regular]);
        let registry = [
            registry_font("Example Mono (TrueType)", &regular),
            registry_font("Example Mono Bold & Other Sans (TrueType)", &shared),
        ];
        load_registry_fonts(&mut db, &registry);
        assert_eq!(db.len(), 2, "the parseable face of the collection loads");

        let (found, allowed) = resolve(&db, &registry, "Example Mono");
        assert!(found.is_empty());
        assert!(allowed.is_empty());
    }

    #[test]
    fn a_partly_loaded_registry_file_counts_when_its_name_lists_the_family_later() {
        let dir = TempDir::new("partial collection reordered");
        let regular = dir.write(
            "Mono-Regular.ttf",
            &TestFont::new("Example Mono", "Regular", 400, false).bytes(),
        );
        let shared = dir.write(
            "Other-Mono-Bold.ttc",
            &collection(&[
                Some(&TestFont::new("Other Sans", "Regular", 400, false)),
                None,
            ]),
        );
        let mut db = database(&[&regular]);
        let registry = [
            registry_font("Example Mono (TrueType)", &regular),
            registry_font("Other Sans & Example Mono Bold (TrueType)", &shared),
        ];
        load_registry_fonts(&mut db, &registry);

        let (found, allowed) = resolve(&db, &registry, "Example Mono");
        assert!(found.is_empty());
        assert!(allowed.is_empty());
    }

    #[test]
    fn a_face_that_did_not_load_counts_when_its_own_names_declare_the_family() {
        let dir = TempDir::new("partial collection unnamed");
        let regular = dir.write(
            "Mono-Regular.ttf",
            &TestFont::new("Example Mono", "Regular", 400, false).bytes(),
        );
        // fontdb skips the bold face (no PostScript name), and the registry name doesn't say Mono.
        let shared = dir.write(
            "Shared.ttc",
            &collection(&[
                Some(&TestFont::new("Other Sans", "Regular", 400, false)),
                Some(&TestFont::new("Example Mono", "Bold", 700, false).without_postscript()),
            ]),
        );
        let mut db = database(&[&regular]);
        let registry = [
            registry_font("Example Mono (TrueType)", &regular),
            registry_font("Shared Collection (TrueType)", &shared),
        ];
        load_registry_fonts(&mut db, &registry);
        assert_eq!(db.len(), 2, "fontdb loads only the named face of the collection");

        let (found, allowed) = resolve(&db, &registry, "Example Mono");
        assert!(found.is_empty());
        assert!(allowed.is_empty());
    }
}
