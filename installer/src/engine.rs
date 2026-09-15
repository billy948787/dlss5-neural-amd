//! The install engine, ported from `installer-x86/core.h`.
//!
//! This is step 1 of `docs/installer-merge.md`: the transactional install model moves here so both
//! the x86 and x64 routes can sit on one engine. Nothing calls it yet -- wiring the routes across is
//! step 2 -- so this module changes no behaviour on its own.
//!
//! **The manifest format is a compatibility surface, not an internal detail.** Installs performed by
//! `installer-x86` already exist on disk, and `decode` accepts a manifest only when re-encoding it
//! reproduces the file byte for byte. Any change to spacing, key order or punctuation in `encode`
//! makes every existing install unreadable, which reads as "modified or unsupported manifest" to the
//! user. The round-trip test against a literal captured from the C++ writer is what guards that.

#![allow(dead_code)] // Step 2 wires these in; step 1 only has to compile and pass its tests.

use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Component, Path, PathBuf};

pub const NOTICE: &str = "Install official ReShade Full Add-on Support for the translated API. D3D8 uses the pinned d3d8to9 compatibility layer and the native D3D9 frontend.";
pub const RUNTIME_SHA: &str = "ddd82d313aa74c2e7602d17dfb7e7cd90cca9bfc0306f581684d35d75d1b350b";
pub const WEIGHTS_SHA: &str = "6bf8dc931ef3ccffe18c82de26ab374156e7f19539ffcf8eabaa25dca5cf15ab";
pub const RESHADE_SHA: &str = "da430e0a9c6eecefa0d1b27d05e16c426fb5d04e808b194d914eaac4b31bc0f8";
pub const D3D8TO9_VERSION: &str = "v1.15.1";
pub const D3D8TO9_COMMIT: &str = "65870f2302e9c496cd6d873d6095961d5c777668";
pub const D3D8TO9_SHA: &str = "ab6bf7a9a9f4b3e66a75ca038d8d10289c88acbfe8d52c3b5a8a9a259cb26cd5";
pub const MANIFEST_NAME: &str = "dlss5-x86bridge.install.json";
pub const MANIFEST_NAME_X64: &str = "dlss5-neural.install.json";
pub const BACKUP_DIR: &str = ".dlss5-x86bridge-backups";

/// Which side of the bridge an install belongs to. This exists because the preset name alone is
/// ambiguous: "D3D11" is a valid preset on both routes, and without this a 64-bit install and a
/// 32-bit one would look interchangeable to `install`, which refuses only a *different* preset.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Route {
    X86,
    X64,
}

impl Route {
    /// x86 keeps the name `installer-x86` already wrote, so existing installs stay readable and the
    /// C++ tool still interoperates during the transition. x64 has no installs in the wild yet, so
    /// it gets the neutral name now, while renaming is still free.
    pub fn manifest_name(self) -> &'static str {
        match self {
            Route::X86 => MANIFEST_NAME,
            Route::X64 => MANIFEST_NAME_X64,
        }
    }
}

/// Every refusal carries the sentence the user sees. `core.h` threw `std::runtime_error`; the
/// message text is part of the contract and several are asserted by the tests.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Error(pub String);

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}
impl std::error::Error for Error {}

pub type Result<T> = std::result::Result<T, Error>;

pub fn require(condition: bool, why: impl Into<String>) -> Result<()> {
    if condition {
        Ok(())
    } else {
        Err(Error(why.into()))
    }
}

fn lower(s: &str) -> String {
    s.to_ascii_lowercase()
}

/// Also strips a UTF-8 byte order mark. ReShade writes its INI with one, which puts an invisible
/// character in front of the first `[SECTION]` header -- and a header that does not start with `[`
/// is not a header, so every key in the first section becomes unreachable. That is how Half-Life
/// 2's `[INSTALL] BasePath` read as absent and sent its install to the game root instead of `bin`.
/// Treating the mark as leading whitespace fixes reading without rewriting the file, so the mark
/// survives anything written back.
fn trim(s: &str) -> &str {
    s.trim_matches(|c| c == ' ' || c == '\t' || c == '\r' || c == '\n' || c == '\u{feff}')
}

pub fn read(path: &Path) -> Result<Vec<u8>> {
    std::fs::read(path).map_err(|e| Error(format!("Cannot read {}: {e}", path.display())))
}

pub fn write(path: &Path, bytes: &[u8]) -> Result<()> {
    std::fs::write(path, bytes).map_err(|e| Error(format!("Cannot write {}: {e}", path.display())))
}

pub fn sha(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hasher
        .finalize()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

pub fn hash_file(path: &Path) -> Result<String> {
    Ok(sha(&read(path)?))
}

fn hash_is(bytes: &[u8], expected: &str, what: &str) -> Result<()> {
    require(sha(bytes) == expected, format!("{what} SHA256 mismatch"))
}

// -- PE ---------------------------------------------------------------------------------------

fn u16_at(b: &[u8], p: usize) -> Result<u16> {
    require(p + 2 <= b.len(), "Truncated PE")?;
    Ok(u16::from(b[p]) | (u16::from(b[p + 1]) << 8))
}

fn u32_at(b: &[u8], p: usize) -> Result<u32> {
    Ok(u32::from(u16_at(b, p)?) | (u32::from(u16_at(b, p + 2)?) << 16))
}

pub const MACHINE_X86: u16 = 0x14c;
pub const MACHINE_X64: u16 = 0x8664;

/// Reads the PE machine type, rejecting anything that is not a well-formed PE32 or PE32+ image.
/// This is how bitness is decided; `docs/installer-merge.md` makes it a detection, never a question.
pub fn machine(b: &[u8]) -> Result<u16> {
    require(b.len() >= 64 && u16_at(b, 0)? == 0x5a4d, "Not a PE executable")?;
    let p = u32_at(b, 60)? as usize;
    require(
        p <= b.len() && b.len() - p >= 26 && u32_at(b, p)? == 0x4550,
        "Invalid PE header",
    )?;
    let m = u16_at(b, p + 4)?;
    let magic = u16_at(b, p + 24)?;
    require(
        (m == MACHINE_X86 && magic == 0x10b) || (m == MACHINE_X64 && magic == 0x20b),
        "Unsupported PE format",
    )?;
    Ok(m)
}

/// Read only enough of a file to answer the machine question. Scanning a game folder means opening
/// every executable in it, and the header sits in the first few hundred bytes.
pub fn machine_of_file(path: &Path) -> Option<u16> {
    use std::io::Read;
    let mut f = std::fs::File::open(path).ok()?;
    let mut head = vec![0u8; 64 * 1024];
    let read = f.read(&mut head).ok()?;
    head.truncate(read);
    machine(&head).ok()
}

/// Some maintained game wrappers deliberately forward Direct3D 8 to `d3d8R.dll`. Detect only an
/// explicit embedded sidecar name; otherwise fail closed rather than replacing an unknown wrapper.
pub fn advertises_d3d8_sidecar(b: &[u8]) -> bool {
    const MARKER: &[u8] = b"d3d8r.dll";
    let n = MARKER.len();
    (0..b.len().saturating_sub(n - 1)).any(|i| {
        let ascii = b[i..].len() >= n
            && (0..n).all(|j| b[i + j].to_ascii_lowercase() == MARKER[j]);
        let utf16 = b[i..].len() >= n * 2
            && (0..n).all(|j| b[i + j * 2].to_ascii_lowercase() == MARKER[j] && b[i + j * 2 + 1] == 0);
        ascii || utf16
    })
}

// -- INI --------------------------------------------------------------------------------------
// Preserve all unedited INI bytes, including comments and unrelated preferences.

pub fn get_ini(s: &str, section: &str, key: &str) -> String {
    let mut sec = String::new();
    for line in s.lines() {
        let t = trim(line);
        if t.len() > 1 && t.starts_with('[') && t.ends_with(']') {
            sec = lower(&t[1..t.len() - 1]);
        } else if sec == lower(section) {
            if let Some(eq) = t.find('=') {
                if lower(trim(&t[..eq])) == lower(key) {
                    return t[eq + 1..].to_string();
                }
            }
        }
    }
    String::new()
}

pub fn set_ini(s: &str, section: &str, key: &str, value: &str) -> String {
    let nl = if s.contains("\r\n") { "\r\n" } else { "\n" };
    let mut out = s.to_string();
    let mut sec = String::new();
    let mut start = 0usize;
    let mut insert: Option<usize> = None;
    let mut found = false;
    while start < out.len() {
        let end = match out[start..].find('\n') {
            Some(i) => start + i + 1,
            None => out.len(),
        };
        let t = trim(&out[start..end]).to_string();
        if t.len() > 1 && t.starts_with('[') && t.ends_with(']') {
            if found && sec == lower(section) {
                insert = Some(start);
                break;
            }
            sec = lower(&t[1..t.len() - 1]);
            if sec == lower(section) {
                found = true;
                insert = Some(end);
            }
        } else if sec == lower(section) {
            if let Some(eq) = t.find('=') {
                if lower(trim(&t[..eq])) == lower(key) {
                    out.replace_range(start..end, &format!("{key}={value}{nl}"));
                    return out;
                }
            }
            insert = Some(end);
        }
        start = end;
    }
    if found {
        let at = insert.unwrap_or(out.len());
        let lead = if at > 0 && out.as_bytes()[at - 1] != b'\n' { nl } else { "" };
        out.insert_str(at, &format!("{lead}{key}={value}{nl}"));
        return out;
    }
    if !out.is_empty() && !out.ends_with('\n') {
        out.push_str(nl);
    }
    format!("{out}{nl}[{section}]{nl}{key}={value}{nl}")
}

pub fn fresh_ini() -> String {
    "[dlss5]\r\n; x86 fresh-install overrides. All other values follow upstream defaults.\r\nScale=1.0\r\nColourStrength=0.25\r\nStructure=1\r\nSkin=1\r\nPasses=1\r\n".to_string()
}

fn dock_id_in(chunk: &str) -> Option<String> {
    let at = chunk.find("DockId=0x")?;
    let rest = &chunk[at + "DockId=".len()..];
    let end = rest[2..]
        .find(|c: char| !c.is_ascii_hexdigit())
        .map(|i| i + 2)
        .unwrap_or(rest.len());
    Some(rest[..end].to_string())
}

/// Dock the panel once, on a fresh layout only. A saved layout is authoritative even when the user
/// undocked the panel, so this never rebuilds or guesses a target.
pub fn first_dock(ini: &str, width: u32, height: u32) -> Result<String> {
    let mut ini = ini.to_string();
    let mut windows = get_ini(&ini, "OVERLAY", "Window");
    let panel = "[Window][DLSS Neural Rendering (AMD)]";
    if windows.contains(panel) {
        return Ok(ini);
    }
    let mut dock = String::new();
    if let Some(home) = windows.find("[Window][###home]") {
        let end = windows[home + 1..].find("[Window]").map(|i| home + 1 + i);
        let chunk = match end {
            Some(e) => &windows[home..e],
            None => &windows[home..],
        };
        if let Some(id) = dock_id_in(chunk) {
            dock = id;
        }
    }
    if dock.is_empty() {
        // Respect an existing layout without a docked Home. No destructive rebuild or guessed target.
        if !windows.is_empty() || !get_ini(&ini, "OVERLAY", "Docking").is_empty() {
            return Ok(ini);
        }
        require(
            width >= 320 && height >= 240,
            "Viewport size unavailable for fresh docking",
        )?;
        let left = width * 35 / 100;
        let layout = format!(
            "[Docking][Data],DockSpace   ID=0xB0DF600F Pos=0,,0 Size={width},,{height} Split=X,  DockNode  ID=0x00000001 Parent=0xB0DF600F SizeRef={left},,{height},  DockNode  ID=0x00000002 Parent=0xB0DF600F SizeRef={},,{height} CentralNode=1",
            width - left
        );
        ini = set_ini(&ini, "OVERLAY", "Docking", &layout);
        dock = "0x00000001".to_string();
        for (tab, title) in [
            "###home",
            "###addons",
            "###settings",
            "###statistics",
            "###log",
            "###about",
        ]
        .iter()
        .enumerate()
        {
            if !windows.is_empty() {
                windows.push(',');
            }
            windows.push_str(&format!("[Window][{title}],Collapsed=0,DockId={dock},,{tab}"));
        }
    }
    if !windows.is_empty() {
        windows.push(',');
    }
    windows.push_str(&format!("{panel},Collapsed=0,DockId={dock}"));
    Ok(set_ini(&ini, "OVERLAY", "Window", &windows))
}

// -- Ownership --------------------------------------------------------------------------------

/// The only filenames this installer will ever create, back up or remove. A manifest naming
/// anything else is rejected, which is what stops a tampered manifest from deleting arbitrary files.
pub fn allowed() -> &'static BTreeSet<&'static str> {
    use std::sync::OnceLock;
    static SET: OnceLock<BTreeSet<&'static str>> = OnceLock::new();
    SET.get_or_init(|| {
        [
            "dxgi.dll",
            "d3d8.dll",
            "d3d8R.dll",
            "d3d9.dll",
            "dgVoodoo.conf",
            "ReShade.ini",
            "dlss5-neural.ini",
            "dlss5-neural.addon32",
            "dlss5-neural.addon64",
            "dlss5-neural-host64.exe",
            "dlssnr_amd_pass1.dll",
            "dlssnr_on_amd_weights.bin",
        ]
        .into_iter()
        .collect()
    })
}

pub fn is_config(name: &str) -> bool {
    name == "ReShade.ini" || name == "dgVoodoo.conf" || name == "dlss5-neural.ini"
}

// -- Paths ------------------------------------------------------------------------------------

#[cfg(windows)]
const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x400;

/// Walk every prefix of the path and refuse links. A reparse point anywhere in the chain could put
/// a write outside the directory the user chose, so this is checked before each write, not once.
pub fn safe_path(p: &Path) -> Result<()> {
    let absolute = absolute(p);
    let mut walk = PathBuf::new();
    for part in absolute.components() {
        walk.push(part.as_os_str());
        let meta = match std::fs::symlink_metadata(&walk) {
            Ok(m) => m,
            Err(_) => continue, // Does not exist yet: nothing to impersonate.
        };
        require(
            !meta.file_type().is_symlink(),
            format!("Symbolic link refused: {}", walk.display()),
        )?;
        #[cfg(windows)]
        {
            use std::os::windows::fs::MetadataExt;
            require(
                meta.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT == 0,
                "Reparse path refused",
            )?;
        }
    }
    Ok(())
}

fn absolute(p: &Path) -> PathBuf {
    if p.is_absolute() {
        p.to_path_buf()
    } else {
        std::env::current_dir().unwrap_or_default().join(p)
    }
}

/// `std::filesystem::weakly_canonical` has no std equivalent: canonicalize the longest existing
/// prefix, then append what does not exist yet. The verbatim prefix Windows adds is stripped so the
/// results still compare and print like ordinary paths.
pub fn weakly_canonical(p: &Path) -> PathBuf {
    let absolute = absolute(p);
    let mut existing = absolute.as_path();
    let mut rest: Vec<&std::ffi::OsStr> = Vec::new();
    loop {
        if let Ok(c) = existing.canonicalize() {
            let mut out = strip_verbatim(&c);
            for part in rest.iter().rev() {
                out.push(part);
            }
            return normalise(&out);
        }
        match (existing.parent(), existing.file_name()) {
            (Some(parent), Some(name)) => {
                rest.push(name);
                existing = parent;
            }
            _ => return normalise(&absolute),
        }
    }
}

fn strip_verbatim(p: &Path) -> PathBuf {
    let s = p.to_string_lossy();
    match s.strip_prefix(r"\\?\") {
        Some(rest) => PathBuf::from(rest),
        None => p.to_path_buf(),
    }
}

/// Resolve `.` and `..` textually, without touching the filesystem.
fn normalise(p: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for part in p.components() {
        match part {
            Component::CurDir => {}
            Component::ParentDir => {
                out.pop();
            }
            other => out.push(other.as_os_str()),
        }
    }
    out
}

/// Where the add-on actually goes. ReShade's `[INSTALL] BasePath` is honoured, but only when it
/// resolves inside the selected game directory -- Half-Life 2 loads its proxy from `bin`, and a
/// BasePath pointing anywhere else is an escape, not a layout.
pub fn install_directory(target: &Path) -> Result<PathBuf> {
    // Either end of the pointer works: an executable, whose folder is the root, or the folder
    // itself. The x86 side has always named an executable because it needs the PE header; the x64
    // side has always named a folder. Accepting both is what lets one screen serve both.
    let absolute_target = absolute(target);
    let root = if absolute_target.is_dir() {
        weakly_canonical(&absolute_target)
    } else {
        weakly_canonical(absolute_target.parent().unwrap_or(Path::new(".")))
    };
    safe_path(&root)?;
    let redirect = root.join("ReShade.ini");
    safe_path(&redirect)?;
    if !redirect.exists() {
        return Ok(root);
    }
    let text = String::from_utf8_lossy(&read(&redirect)?).to_string();
    let configured = trim(&get_ini(&text, "INSTALL", "BasePath")).to_string();
    if configured.is_empty() {
        return Ok(root);
    }
    let candidate = {
        let c = PathBuf::from(&configured);
        if c.is_relative() {
            root.join(c)
        } else {
            c
        }
    };
    let candidate = weakly_canonical(&candidate);
    safe_path(&candidate)?;
    let inside = candidate.starts_with(&root) && candidate != root;
    require(
        inside,
        "ReShade BasePath must stay inside the selected game directory",
    )?;
    require(
        candidate.is_dir(),
        "ReShade BasePath is not an existing directory",
    )?;
    Ok(candidate)
}

// -- Manifest ---------------------------------------------------------------------------------

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Entry {
    pub name: String,
    pub hash: String,
    pub backup: String,
    pub backup_hash: String,
    pub owned: bool,
    pub configuration: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Manifest {
    pub preset: String,
    pub state: String,
    pub route: Route,
    pub entries: Vec<Entry>,
}

impl Manifest {
    pub fn new(preset: &str, route: Route) -> Self {
        Self {
            preset: preset.to_string(),
            state: "installed".to_string(),
            route,
            entries: Vec::new(),
        }
    }
}

/// Byte-for-byte the layout `installer-x86/core.h` writes. See the module comment: changing any
/// character here orphans every manifest already on disk.
pub fn encode(m: &Manifest) -> String {
    let mut o = String::new();
    o.push_str("{\n\"schema\":1,\n\"preset\":\"");
    o.push_str(&m.preset);
    o.push_str("\",\n\"state\":\"");
    o.push_str(&m.state);
    o.push_str("\",\n");
    // Emitted only for x64, so every x86 manifest already on disk still round-trips byte for byte.
    if m.route == Route::X64 {
        o.push_str("\"route\":\"x64\",\n");
    }
    o.push_str("\"bridge_protocol\":2,\n\"dgVoodoo\":\"none\",\n\"ReShade\":\"6.8.0.2156 Full Add-on Support\",\n\"files\":[\n");
    for (i, e) in m.entries.iter().enumerate() {
        o.push_str(&format!(
            "{{\"name\":\"{}\",\"sha256\":\"{}\",\"backup\":\"{}\",\"backup_sha256\":\"{}\",\"owned\":{},\"configuration\":{}}}{}\n",
            e.name,
            e.hash,
            e.backup,
            e.backup_hash,
            e.owned,
            e.configuration,
            if i + 1 == m.entries.len() { "" } else { "," }
        ));
    }
    o.push_str("]\n}\n");
    o
}

fn field<'a>(s: &'a str, key: &str) -> Option<&'a str> {
    let at = s.find(&format!("\"{key}\":\""))? + key.len() + 4;
    let rest = &s[at..];
    let end = rest.find('"')?;
    Some(&rest[..end])
}

fn is_hex(s: &str, len: usize) -> bool {
    s.len() == len && s.bytes().all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase())
}

/// Parsed structurally, then validated by re-encoding: a manifest is accepted only when `encode`
/// reproduces it exactly. That single check is what makes hand-editing detectable without having to
/// enumerate every way a file could be tampered with.
pub fn decode(s: &str) -> Result<Manifest> {
    require(s.contains("\"schema\":1,"), "Unknown manifest schema")?;
    let preset = field(s, "preset").unwrap_or_default().to_string();
    let route = if s.contains("\"route\":\"x64\",") { Route::X64 } else { Route::X86 };
    let known = match route {
        Route::X86 => matches!(preset.as_str(), "D3D11" | "D3D9" | "D3D8"),
        Route::X64 => matches!(
            preset.as_str(),
            "PCSX2" | "RPCS3" | "D3D11" | "D3D12" | "Vulkan"
        ),
    };
    require(known, "Bad manifest preset")?;
    let state = field(s, "state").unwrap_or_default().to_string();
    require(
        matches!(state.as_str(), "installed" | "installing"),
        "Bad manifest state",
    )?;
    let mut m = Manifest {
        preset,
        state,
        route,
        entries: Vec::new(),
    };
    let mut seen: BTreeSet<String> = BTreeSet::new();
    for row in s.split("{\"name\":\"").skip(1) {
        let row = match row.find('}') {
            Some(i) => &row[..=i],
            None => continue,
        };
        let row = format!("{{\"name\":\"{row}");
        let e = Entry {
            name: field(&row, "name").unwrap_or_default().to_string(),
            hash: field(&row, "sha256").unwrap_or_default().to_string(),
            backup: field(&row, "backup").unwrap_or_default().to_string(),
            backup_hash: field(&row, "backup_sha256").unwrap_or_default().to_string(),
            owned: row.contains("\"owned\":true"),
            configuration: row.contains("\"configuration\":true"),
        };
        require(is_hex(&e.hash, 64), "Unsafe/duplicate manifest entry")?;
        require(
            allowed().contains(e.name.as_str()) && seen.insert(e.name.clone()),
            "Unsafe/duplicate manifest entry",
        )?;
        require(
            e.configuration == is_config(&e.name),
            "Manifest config mismatch",
        )?;
        if !e.backup.is_empty() {
            let expected_prefix = format!("{BACKUP_DIR}/");
            let stamped = e
                .backup
                .strip_prefix(&expected_prefix)
                .and_then(|r| r.split_once('/'))
                .map(|(stamp, name)| {
                    !stamp.is_empty()
                        && stamp.bytes().all(|c| c.is_ascii_digit())
                        && name == e.name
                })
                .unwrap_or(false);
            require(
                stamped && is_hex(&e.backup_hash, 64) && e.owned,
                "Unsafe backup entry",
            )?;
        }
        m.entries.push(e);
    }
    let rows = s.matches("\"name\":").count();
    require(
        rows == m.entries.len() && rows <= allowed().len(),
        "Malformed manifest entries",
    )?;
    let canonical = encode(&m);
    let mut supported = canonical == s;
    // The original fork used dgVoodoo for both translated presets. Preserve its manifests for
    // uninstall/recovery, but never create another one or carry that wrapper into a new install.
    if !supported && (m.preset == "D3D8" || m.preset == "D3D9") {
        supported = canonical.replacen("\"dgVoodoo\":\"none\"", "\"dgVoodoo\":\"2.87.4\"", 1) == s;
    }
    require(supported, "Modified or unsupported install manifest")?;
    Ok(m)
}

/// The journal only means anything if it lands before the writes it describes, so the manifest is
/// written to a temporary file and moved over the old one through the filesystem's replace.
pub fn atomic_manifest(dir: &Path, m: &Manifest) -> Result<()> {
    let name = m.route.manifest_name();
    let final_path = dir.join(name);
    let tmp = dir.join(format!("{name}.tmp"));
    safe_path(&tmp)?;
    write(&tmp, encode(m).as_bytes())?;
    commit_rename(&tmp, &final_path)
}

#[cfg(windows)]
fn commit_rename(from: &Path, to: &Path) -> Result<()> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::{
        MoveFileExW, MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH,
    };
    let wide = |p: &Path| -> Vec<u16> {
        p.as_os_str().encode_wide().chain(std::iter::once(0)).collect()
    };
    let ok = unsafe {
        MoveFileExW(
            wide(from).as_ptr(),
            wide(to).as_ptr(),
            MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
        )
    };
    require(ok != 0, "Manifest commit failed")
}

#[cfg(not(windows))]
fn commit_rename(from: &Path, to: &Path) -> Result<()> {
    std::fs::rename(from, to).map_err(|e| Error(format!("Manifest commit failed: {e}")))
}

// -- Installer --------------------------------------------------------------------------------

pub struct Installer {
    pub release: PathBuf,
    pub width: u32,
    pub height: u32,
    pub log: Vec<String>,
}

impl Installer {
    pub fn new(release: impl Into<PathBuf>) -> Self {
        Self { release: release.into(), width: 1920, height: 1080, log: Vec::new() }
    }

    pub fn note(&mut self, s: impl Into<String>) {
        self.log.push(s.into());
    }

    fn payload(&self, name: &str, expected: &str) -> Result<Vec<u8>> {
        let b = read(&self.release.join("files").join(name))?;
        hash_is(&b, expected, name)?;
        Ok(b)
    }

    /// Reads the release's own checksum list so the bridge pair is pinned to the build it shipped
    /// with. Decision 1 of the merge plan: coupling is proved by hash, not by embedding bytes.
    fn bridge_sum(sums: &str, name: &str) -> Result<String> {
        for line in sums.lines() {
            if let Some((hash, file)) = line.split_once("  ") {
                if file.trim() == name && is_hex(&lower(hash), 64) {
                    return Ok(lower(hash));
                }
            }
        }
        Err(Error("Missing bridge release checksum".into()))
    }

    /// Everything that would be written, with nothing written. Every payload hash, the PE machine
    /// type and the chaining rule are decided here, so a refusal happens before any file moves.
    pub fn plan(&self, target: &Path, preset: &str) -> Result<BTreeMap<String, Vec<u8>>> {
        require(
            matches!(preset, "D3D11" | "D3D9" | "D3D8"),
            "Unsupported x86 preset",
        )?;
        safe_path(target)?;
        require(
            machine(&read(target)?)? == MACHINE_X86,
            "Target must be PE32/x86; x64 targets are not supported",
        )?;
        let dir = install_directory(target)?;
        let mut p: BTreeMap<String, Vec<u8>> = BTreeMap::new();

        // Say what the folder is, not that a file could not be read. Getting field 1 wrong is the
        // ordinary mistake here, and "Cannot read ...\payload.sha256" tells nobody what to do.
        let manifest = self.release.join("payload.sha256");
        require(
            manifest.is_file(),
            format!(
                "{} does not look like the unpacked download: it has no payload.sha256 beside a files folder. Point field 1 at the folder you unzipped.",
                self.release.display()
            ),
        )?;
        let sums = String::from_utf8_lossy(&read(&manifest)?).to_string();
        for name in ["dlss5-neural.addon32", "dlss5-neural-host64.exe"] {
            let bytes = self.payload(name, &Self::bridge_sum(&sums, name)?)?;
            let want = if name.contains("addon32") { MACHINE_X86 } else { MACHINE_X64 };
            require(machine(&bytes)? == want, "Wrong bridge architecture")?;
            p.insert(name.to_string(), bytes);
        }
        p.insert("dlssnr_amd_pass1.dll".into(), self.payload("dlssnr_amd_pass1.dll", RUNTIME_SHA)?);
        p.insert("dlssnr_on_amd_weights.bin".into(), self.payload("dlssnr_on_amd_weights.bin", WEIGHTS_SHA)?);

        if preset == "D3D8" {
            let translator = self.payload("d3d8to9.dll", D3D8TO9_SHA)?;
            require(machine(&translator)? == MACHINE_X86, "d3d8to9 must be x86")?;
            let mut name = "d3d8.dll".to_string();
            let existing = dir.join(&name);
            safe_path(&existing)?;
            if existing.exists() && hash_file(&existing)? != D3D8TO9_SHA {
                require(
                    advertises_d3d8_sidecar(&read(&existing)?),
                    "Existing d3d8.dll does not advertise d3d8R.dll chaining; preserved",
                )?;
                name = "d3d8R.dll".to_string();
            }
            p.insert(name, translator);
        }

        let reshade_name = if preset == "D3D11" { "dxgi.dll" } else { "d3d9.dll" };
        let reshade = if self.release.join("files/dxgi.dll").exists() {
            self.payload("dxgi.dll", RESHADE_SHA)?
        } else {
            let existing = dir.join(reshade_name);
            require(
                existing.exists(),
                format!(
                    "ReShade is not installed for this API: there is no {reshade_name} in the game folder. Install ReShade 6.8.0.2156 with full add-on support, 32-bit, against the game's own executable and pick the API it uses."
                ),
            )?;
            let b = read(&existing)?;
            // Having *a* ReShade is not the same as having the one this was tested against, and the
            // difference is invisible unless it is said out loud.
            require(
                sha(&b) == RESHADE_SHA,
                format!(
                    "The {reshade_name} already in the game folder is a different build from the one this was tested with. It has to be ReShade 6.8.0.2156 with full add-on support, 32-bit -- a newer version is refused too, not just an older one."
                ),
            )?;
            b
        };
        require(machine(&reshade)? == MACHINE_X86, "ReShade must be x86")?;
        p.insert(reshade_name.to_string(), reshade);

        let tuning = dir.join("dlss5-neural.ini");
        safe_path(&tuning)?;
        if !tuning.exists() {
            p.insert("dlss5-neural.ini".into(), fresh_ini().into_bytes());
        }
        let ini = dir.join("ReShade.ini");
        safe_path(&ini)?;
        let before = if ini.exists() {
            String::from_utf8_lossy(&read(&ini)?).to_string()
        } else {
            String::new()
        };
        let after = first_dock(&before, self.width, self.height)?;
        if before != after {
            p.insert("ReShade.ini".into(), after.into_bytes());
        }
        Ok(p)
    }

    /// Plan, then apply. The transactional half lives in [`apply`] so the x64 route can reuse it.
    pub fn install(&mut self, target: &Path, preset: &str) -> Result<()> {
        let dir = install_directory(target)?;
        safe_path(&dir)?;
        let desired = self.plan(&absolute(target), preset)?;
        apply(&dir, preset, Route::X86, &desired, &mut self.log)?;
        let how = if preset == "D3D8" {
            format!("d3d8to9 {D3D8TO9_VERSION} -> native D3D9 frontend")
        } else {
            "native frontend".to_string()
        };
        self.note(format!("Installed {preset} x86; {how}; same-frame protocol v2"));
        Ok(())
    }

    pub fn uninstall(&mut self, directory: &Path, remove_configs: bool) -> Result<()> {
        let mut log = std::mem::take(&mut self.log);
        let outcome = uninstall(directory, Route::X86, remove_configs, &mut log);
        self.log = log;
        outcome
    }
}

// -- Environment guard ---------------------------------------------------------------------------

/// Can this folder be written to at all? Program Files without elevation is the usual answer.
pub fn folder_is_writable(dir: &Path) -> bool {
    let probe = dir.join(".dlss5-installer-write-probe");
    match std::fs::write(&probe, b"") {
        Ok(()) => {
            let _ = std::fs::remove_file(&probe);
            true
        }
        Err(_) => false,
    }
}

/// A file that exists but cannot be opened for writing is held by something -- on Windows that is
/// nearly always the game still running, which is the single most common way an install fails.
pub fn is_locked(path: &Path) -> bool {
    path.is_file() && std::fs::OpenOptions::new().write(true).open(path).is_err()
}

#[cfg(windows)]
pub fn free_bytes(dir: &Path) -> Option<u64> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::GetDiskFreeSpaceExW;
    let mut wide: Vec<u16> = dir.as_os_str().encode_wide().collect();
    wide.push(0);
    let mut free = 0u64;
    let ok = unsafe {
        GetDiskFreeSpaceExW(wide.as_ptr(), &mut free, std::ptr::null_mut(), std::ptr::null_mut())
    };
    (ok != 0).then_some(free)
}

#[cfg(not(windows))]
pub fn free_bytes(_dir: &Path) -> Option<u64> {
    None
}

/// Same file, same bytes? Only the length is compared, which is what keeps the guard cheap.
pub fn size_of(path: &Path) -> Option<u64> {
    std::fs::metadata(path).ok().map(|m| m.len())
}

/// Everything about the folder that would make the transaction fail halfway, checked before the
/// journal is written so a refusal costs nothing and says why.
///
/// This runs inside [`apply`], which is what folds it in front of both routes at once rather than
/// leaving it as advice on one screen. The common case by far is the third one: the game is still
/// open, and the old failure for that was an access-denied error from somewhere inside a copy.
pub fn guard(dir: &Path, desired: &BTreeMap<String, Vec<u8>>) -> Result<()> {
    require(
        dir.is_dir(),
        format!("{} is not a folder.", dir.display()),
    )?;
    require(
        folder_is_writable(dir),
        "That folder cannot be written to. It is either read-only or somewhere that needs \
         administrator rights -- run this installer as administrator, or move the game."
            .to_string(),
    )?;

    let held: Vec<&str> = desired
        .keys()
        .filter(|n| is_locked(&dir.join(n)))
        .map(|s| s.as_str())
        .collect();
    require(
        held.is_empty(),
        format!(
            "{} {} open by another program. The game or emulator is almost certainly still \
             running -- close it and this line goes away.",
            held.join(", "),
            if held.len() == 1 { "is" } else { "are" }
        ),
    )?;

    // Size is a cheap stand-in for "already the file we want": hashing every payload here would
    // read the 147 MB of weights twice, once to decide and once to install. A file whose size
    // already matches needs no new room; anything else needs its own bytes plus a backup of
    // whatever it displaces.
    let mut need = 0u64;
    for (name, data) in desired {
        let existing = size_of(&dir.join(name));
        if existing != Some(data.len() as u64) {
            need += data.len() as u64 + existing.unwrap_or(0);
        }
    }
    if let Some(free) = free_bytes(dir) {
        require(
            need == 0 || free >= need,
            format!(
                "Not enough room: {} MB free, and this needs {} MB including the backup of what it \
                 replaces.",
                free / 1_048_576,
                need / 1_048_576
            ),
        )?;
    }
    Ok(())
}

/// The transactional half of an install, shared by both routes.
///
/// `desired` is whatever the route's own planner decided to write; this function is deliberately
/// ignorant of what those files mean. It records ownership, backs up anything it is about to
/// displace, writes the journal *before* touching a target, and restores everything it moved if any
/// write fails. Splitting it out is what lets the x64 route gain backups and a manifest without
/// inheriting the x86 payload rules.
pub fn apply(
    dir: &Path,
    preset: &str,
    route: Route,
    desired: &BTreeMap<String, Vec<u8>>,
    log: &mut Vec<String>,
) -> Result<()> {
    guard(dir, desired)?;
    let manifest_path = dir.join(route.manifest_name());
    safe_path(&manifest_path)?;

    let mut m = Manifest::new(preset, route);
    if manifest_path.exists() {
        m = decode(&String::from_utf8_lossy(&read(&manifest_path)?))?;
        require(
            m.state == "installed",
            "Interrupted transaction: run uninstall/recovery before reinstall",
        )?;
        require(
            m.preset == preset,
            "Uninstall previous preset before changing API",
        )?;
        require(
            m.route == route,
            "That folder already has an install for the other architecture; uninstall it first",
        )?;
    }

    struct Change {
        name: String,
        before: Vec<u8>,
        after: Vec<u8>,
        existed: bool,
    }
    let mut changes: Vec<Change> = Vec::new();
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos().to_string())
        .unwrap_or_else(|_| "0".into());

    for (name, data) in desired {
        require(
            allowed().contains(name.as_str()),
            format!("Refusing to install an unmanaged filename: {name}"),
        )?;
        let dst = dir.join(name);
        safe_path(&dst)?;
        let exists = dst.exists();
        let wanted = sha(data);
        let old = if exists { hash_file(&dst)? } else { String::new() };
        let index = m.entries.iter().position(|x| &x.name == name);

        if exists && old == wanted {
            log.push(format!("IDENTICAL: {name}"));
            if index.is_none() {
                m.entries.push(Entry {
                    name: name.clone(),
                    hash: wanted,
                    backup: String::new(),
                    backup_hash: String::new(),
                    owned: false,
                    configuration: is_config(name),
                });
            }
            continue;
        }
        if let Some(i) = index {
            if exists && old != m.entries[i].hash {
                if is_config(name) {
                    log.push(format!("PRESERVED user-modified config: {name}"));
                    continue;
                }
                return Err(Error(format!("File changed since install; preserved: {name}")));
            }
        }
        let before = if exists { read(&dst)? } else { Vec::new() };
        match index {
            None => {
                let mut item = Entry {
                    name: name.clone(),
                    hash: wanted,
                    backup: String::new(),
                    backup_hash: String::new(),
                    owned: true,
                    configuration: is_config(name),
                };
                if exists {
                    item.backup = format!("{BACKUP_DIR}/{stamp}/{name}");
                    item.backup_hash = old.clone();
                    let bp = dir.join(&item.backup);
                    safe_path(&bp)?;
                    make_parent(&bp)?;
                    write(&bp, &before)?;
                    hash_is(&read(&bp)?, &old, "Backup")?;
                    log.push(format!("EXTERNAL backed up: {name}"));
                } else {
                    log.push(format!("CREATE: {name}"));
                }
                m.entries.push(item);
            }
            Some(i) => {
                if !m.entries[i].owned && exists {
                    let backup = format!("{BACKUP_DIR}/{stamp}/{name}");
                    let bp = dir.join(&backup);
                    safe_path(&bp)?;
                    make_parent(&bp)?;
                    write(&bp, &before)?;
                    hash_is(&read(&bp)?, &old, "Upgrade backup")?;
                    m.entries[i].backup = backup;
                    m.entries[i].backup_hash = old.clone();
                }
                m.entries[i].hash = wanted;
                m.entries[i].owned = true;
            }
        }
        changes.push(Change {
            name: name.clone(),
            before,
            after: data.clone(),
            existed: exists,
        });
    }

    // Journal precedes target writes. Uninstall can recover interrupted installs using hashes.
    let had_manifest = manifest_path.exists();
    let old_manifest = if had_manifest {
        read(&manifest_path)?
    } else {
        Vec::new()
    };
    m.state = "installing".into();
    atomic_manifest(dir, &m)?;

    let mut applied: std::result::Result<(), Error> = Ok(());
    for c in &changes {
        if let Err(e) = write(&dir.join(&c.name), &c.after) {
            applied = Err(e);
            break;
        }
    }
    if applied.is_ok() {
        m.state = "installed".into();
        applied = atomic_manifest(dir, &m);
    }
    if let Err(e) = applied {
        for c in changes.iter().rev() {
            if c.existed {
                let _ = write(&dir.join(&c.name), &c.before);
            } else {
                let _ = std::fs::remove_file(dir.join(&c.name));
            }
        }
        if had_manifest {
            let _ = write(&manifest_path, &old_manifest);
        } else {
            let _ = std::fs::remove_file(&manifest_path);
        }
        return Err(e);
    }
    Ok(())
}

/// Undo an install using its own manifest. Files the installer did not own are left alone, files
/// the user changed afterwards are kept with a warning, and anything displaced at install time is
/// put back from its backup.
pub fn uninstall(
    dir: &Path,
    route: Route,
    remove_configs: bool,
    log: &mut Vec<String>,
) -> Result<()> {
    let dir = absolute(dir);
    safe_path(&dir)?;
    let manifest_path = dir.join(route.manifest_name());
    safe_path(&manifest_path)?;
    require(manifest_path.exists(), "No install manifest")?;
    let mut m = decode(&String::from_utf8_lossy(&read(&manifest_path)?))?;
    let mut keep: Vec<Entry> = Vec::new();

    for e in &m.entries {
        let dst = dir.join(&e.name);
        safe_path(&dst)?;
        if !e.owned {
            log.push(format!("PRESERVED pre-existing identical file: {}", e.name));
            continue;
        }
        if !e.backup.is_empty() {
            safe_path(&dir.join(&e.backup))?;
            hash_is(&read(&dir.join(&e.backup))?, &e.backup_hash, "Original backup")?;
        }
        if dst.exists() && hash_file(&dst)? != e.hash {
            if m.state == "installing" && !e.backup.is_empty() && hash_file(&dst)? == e.backup_hash {
                let _ = std::fs::remove_file(dir.join(&e.backup));
                continue;
            }
            log.push(format!(
                "WARNING modified after install; retained with backup: {}",
                e.name
            ));
            keep.push(e.clone());
            continue;
        }
        if !dst.exists() && e.backup.is_empty() {
            continue;
        }
        if e.configuration && e.backup.is_empty() && !remove_configs {
            log.push(format!("PRESERVED personal/default configuration: {}", e.name));
            keep.push(e.clone());
            continue;
        }
        if !e.backup.is_empty() {
            let restored = read(&dir.join(&e.backup))?;
            write(&dst, &restored)?;
            let _ = std::fs::remove_file(dir.join(&e.backup));
            log.push(format!("RESTORED: {}", e.name));
        } else {
            let _ = std::fs::remove_file(&dst);
            log.push(format!("REMOVED: {}", e.name));
        }
    }
    if keep.is_empty() {
        let _ = std::fs::remove_file(&manifest_path);
    } else {
        m.entries = keep;
        m.state = "installed".into();
        atomic_manifest(&dir, &m)?;
    }
    log.push("Uninstall complete; retained files/backups are listed above.".into());
    Ok(())
}

fn make_parent(p: &Path) -> Result<()> {
    if let Some(parent) = p.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| Error(format!("Cannot create {}: {e}", parent.display())))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Minimal well-formed PE header, the same shape `installer-x86/tests.cpp` builds.
    fn pe(x64: bool) -> Vec<u8> {
        let mut b = vec![0u8; 512];
        b[0] = 0x4d;
        b[1] = 0x5a;
        b[60] = 128;
        b[128] = 0x50;
        b[129] = 0x45;
        let machine: u16 = if x64 { MACHINE_X64 } else { MACHINE_X86 };
        b[132] = (machine & 0xff) as u8;
        b[133] = (machine >> 8) as u8;
        let magic: u16 = if x64 { 0x20b } else { 0x10b };
        b[152] = (magic & 0xff) as u8;
        b[153] = (magic >> 8) as u8;
        b
    }

    fn temp(tag: &str) -> PathBuf {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("engine-{tag}-{nanos}"));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// Long messages are wrapped across source lines with a trailing backslash. Lose it and the
    /// source indentation is baked into the sentence, which reads as a run of spaces mid-line on
    /// screen. It has happened twice, so this reads the file rather than trusting review.
    #[test]
    fn no_message_in_this_file_carries_its_own_source_indentation() {
        let source = include_str!("engine.rs");
        let mut offenders = Vec::new();
        for (n, line) in source.lines().enumerate() {
            let trimmed = line.trim_start();
            // Only string literals that start mid-line, i.e. continuations of a wrapped message.
            if !trimmed.starts_with('"') || trimmed.starts_with("\"\"") {
                continue;
            }
            if let Some(rest) = trimmed.strip_prefix('"') {
                // Four or more, not two: the docking layout ReShade writes uses runs of two and
                // three spaces on purpose, while lost indentation is a whole source indent.
                if rest.contains("    ") && !rest.contains("\n") {
                    offenders.push((n + 1, line.trim().to_string()));
                }
            }
        }
        assert!(
            offenders.is_empty(),
            "a message lost its line continuation and kept the indentation: {offenders:?}"
        );
    }

    #[test]
    fn sha256_known_vector() {
        assert_eq!(
            sha(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    #[test]
    fn pe_machine_is_read_and_malformed_images_refused() {
        assert_eq!(machine(&pe(false)).unwrap(), MACHINE_X86);
        assert_eq!(machine(&pe(true)).unwrap(), MACHINE_X64);
        assert!(machine(b"not a pe").is_err());
        let mut truncated = pe(false);
        truncated.truncate(100);
        assert!(machine(&truncated).is_err());
    }

    #[test]
    fn d3d8_sidecar_marker_detected_in_both_encodings() {
        assert!(advertises_d3d8_sidecar(b"prefix D3D8R.DLL suffix"));
        let wide: Vec<u8> = "d3d8R.dll".bytes().flat_map(|c| [c, 0]).collect();
        assert!(advertises_d3d8_sidecar(&wide));
        // An unknown wrapper is never assumed chainable: replacing it would break the game.
        assert!(!advertises_d3d8_sidecar(b"ordinary d3d8.dll wrapper"));
    }

    #[test]
    fn ini_edits_preserve_every_other_byte() {
        let before = "[INPUT]\nKeyOverlay=36,0,0,0\n";
        let after = set_ini(before, "OVERLAY", "Window", "x");
        assert_eq!(get_ini(&after, "INPUT", "KeyOverlay"), "36,0,0,0");
        assert_eq!(get_ini(&after, "OVERLAY", "Window"), "x");
        // Rewriting an existing key replaces that line and nothing else.
        let again = set_ini(&after, "INPUT", "KeyOverlay", "9,0,0,0");
        assert_eq!(get_ini(&again, "INPUT", "KeyOverlay"), "9,0,0,0");
        assert_eq!(get_ini(&again, "OVERLAY", "Window"), "x");
    }

    #[test]
    fn an_ini_written_with_a_byte_order_mark_is_still_readable() {
        // ReShade writes one. Without this the first section is invisible and every key in it
        // reads as absent, which fails silently: nothing errors, the value is just never found.
        let with_bom = "\u{feff}[INSTALL]\r\nBasePath=bin\r\n";
        assert_eq!(get_ini(with_bom, "INSTALL", "BasePath"), "bin");
        // And editing it keeps the mark, rather than quietly changing the file's encoding.
        let edited = set_ini(with_bom, "INSTALL", "BasePath", "other");
        assert!(edited.starts_with('\u{feff}'), "the mark must survive a write");
        assert_eq!(get_ini(&edited, "INSTALL", "BasePath"), "other");
    }

    #[test]
    fn fresh_docking_follows_the_supplied_viewport() {
        for (w, h) in [(1280u32, 720u32), (2560, 1440)] {
            let s = first_dock("[INPUT]\nKeyOverlay=36,0,0,0\n", w, h).unwrap();
            assert!(s.contains(&format!("Size={w},,{h}")));
            assert_eq!(get_ini(&s, "INPUT", "KeyOverlay"), "36,0,0,0");
        }
    }

    #[test]
    fn a_saved_panel_layout_is_never_redocked() {
        let saved = "[OVERLAY]\nWindow=[Window][DLSS Neural Rendering (AMD)],Collapsed=0\n";
        assert_eq!(first_dock(saved, 1920, 1080).unwrap(), saved);
    }

    #[test]
    fn an_existing_layout_without_a_docked_home_is_left_alone() {
        let other = "[OVERLAY]\nWindow=[Window][###something],Collapsed=0\n";
        assert_eq!(first_dock(other, 1920, 1080).unwrap(), other);
    }

    #[test]
    fn an_existing_home_dock_id_is_reused() {
        let home = "[OVERLAY]\nWindow=[Window][###home],Collapsed=0,DockId=0x0000ABCD,,0\n";
        let s = first_dock(home, 1920, 1080).unwrap();
        assert!(s.contains("[Window][DLSS Neural Rendering (AMD)],Collapsed=0,DockId=0x0000ABCD"));
        assert!(!s.contains("DockSpace"), "no layout rebuild when Home is already docked");
    }

    /// The exact bytes `installer-x86` writes. This is the compatibility guard described at the top
    /// of the module: if `encode` drifts, every install already on disk stops being readable.
    fn captured_manifest() -> String {
        concat!(
            "{\n\"schema\":1,\n\"preset\":\"D3D8\",\n\"state\":\"installed\",\n",
            "\"bridge_protocol\":2,\n\"dgVoodoo\":\"none\",\n",
            "\"ReShade\":\"6.8.0.2156 Full Add-on Support\",\n\"files\":[\n",
            "{\"name\":\"d3d8R.dll\",\"sha256\":\"ab6bf7a9a9f4b3e66a75ca038d8d10289c88acbfe8d52c3b5a8a9a259cb26cd5\",",
            "\"backup\":\".dlss5-x86bridge-backups/17894153360915702/d3d8R.dll\",",
            "\"backup_sha256\":\"ee9b4916304592a31f0882f339bcbeac7133439a297fbd5274e503c0147d209e\",",
            "\"owned\":true,\"configuration\":false},\n",
            "{\"name\":\"d3d9.dll\",\"sha256\":\"da430e0a9c6eecefa0d1b27d05e16c426fb5d04e808b194d914eaac4b31bc0f8\",",
            "\"backup\":\"\",\"backup_sha256\":\"\",\"owned\":false,\"configuration\":false}\n",
            "]\n}\n"
        )
        .to_string()
    }

    #[test]
    fn manifest_written_by_the_cpp_installer_is_still_readable() {
        let text = captured_manifest();
        let m = decode(&text).expect("a manifest from installer-x86 must decode");
        assert_eq!(m.preset, "D3D8");
        assert_eq!(m.state, "installed");
        assert_eq!(m.entries.len(), 2);
        assert!(m.entries[0].owned && !m.entries[1].owned);
        assert_eq!(m.entries[0].backup_hash.len(), 64);
        // The round trip is the contract: re-encoding must reproduce the file byte for byte.
        assert_eq!(encode(&m), text);
    }

    #[test]
    fn a_hand_edited_manifest_is_rejected() {
        // A name outside allowed() is what stops a forged manifest from deleting arbitrary files.
        let renamed = captured_manifest().replace("d3d9.dll", "evil.dll");
        assert!(decode(&renamed).is_err());
        // Two rows for one file.
        let duplicated = captured_manifest().replace("d3d8R.dll", "d3d9.dll");
        assert!(decode(&duplicated).is_err());
        // The configuration flag has to agree with the filename.
        let flag = captured_manifest().replace(
            "\"owned\":false,\"configuration\":false",
            "\"owned\":false,\"configuration\":true",
        );
        assert!(decode(&flag).is_err());
        // Any reformatting at all breaks the byte-for-byte round trip.
        let spaced = captured_manifest().replace("\"schema\":1,", "\"schema\": 1,");
        assert!(decode(&spaced).is_err());
    }

    /// Recorded because a test asserting the opposite failed, and checking `installer-x86/core.h`
    /// showed the C++ behaves the same way: `owned` and the recorded hash are re-encoded faithfully,
    /// so editing either still round-trips. This is the edge of what the manifest check proves.
    ///
    /// The blast radius is bounded elsewhere rather than here. A forged entry can only name one of
    /// the eleven files in `allowed()`, and uninstall re-hashes the target before acting, so the
    /// worst a flipped `owned` achieves is removing a file that would otherwise be preserved.
    /// Tightening it is a change to the format, so it belongs to the merge, not to this port.
    #[test]
    fn the_round_trip_check_does_not_constrain_owned_or_the_recorded_hash() {
        let flipped = captured_manifest().replace(
            "\"owned\":false,\"configuration\":false",
            "\"owned\":true,\"configuration\":false",
        );
        assert!(decode(&flipped).is_ok(), "ported faithfully from core.h");
    }

    #[test]
    fn a_legacy_dgvoodoo_manifest_stays_uninstallable() {
        // The original fork wrote this marker. It must still decode so those installs can be undone.
        let legacy = captured_manifest().replace("\"dgVoodoo\":\"none\"", "\"dgVoodoo\":\"2.87.4\"");
        assert_eq!(decode(&legacy).unwrap().preset, "D3D8");
    }

    #[test]
    fn a_backup_path_outside_the_backup_directory_is_refused() {
        let escape = captured_manifest().replace(
            ".dlss5-x86bridge-backups/17894153360915702/d3d8R.dll",
            "../../elsewhere/d3d8R.dll",
        );
        assert!(decode(&escape).is_err());
    }

    #[test]
    fn reshade_basepath_is_followed_into_the_game_directory_and_no_further() {
        let root = temp("basepath");
        std::fs::create_dir_all(root.join("bin")).unwrap();
        let exe = root.join("game.exe");
        write(&exe, &pe(false)).unwrap();

        write(&root.join("ReShade.ini"), b"[INSTALL]\nBasePath=bin\n").unwrap();
        assert_eq!(
            install_directory(&exe).unwrap(),
            weakly_canonical(&root.join("bin"))
        );

        write(&root.join("ReShade.ini"), b"[INSTALL]\nBasePath=..\n").unwrap();
        assert!(
            install_directory(&exe).is_err(),
            "a BasePath escaping the game directory must be refused"
        );

        std::fs::remove_file(root.join("ReShade.ini")).unwrap();
        assert_eq!(install_directory(&exe).unwrap(), weakly_canonical(&root));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn plan_refuses_an_x64_target_and_an_unsupported_preset() {
        let root = temp("refuse");
        let x64 = root.join("x64.exe");
        write(&x64, &pe(true)).unwrap();
        let app = Installer::new(root.join("release"));
        let err = app.plan(&x64, "D3D11").unwrap_err().0;
        assert!(err.contains("PE32/x86"), "got: {err}");

        let x86 = root.join("x86.exe");
        write(&x86, &pe(false)).unwrap();
        assert_eq!(
            app.plan(&x86, "D3D12").unwrap_err().0,
            "Unsupported x86 preset"
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn uninstall_without_a_manifest_refuses() {
        let root = temp("nomanifest");
        let mut app = Installer::new(root.join("release"));
        assert_eq!(
            app.uninstall(&root, false).unwrap_err().0,
            "No install manifest"
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn an_x86_manifest_is_still_byte_identical_now_that_routes_exist() {
        // The route marker must not leak into x86 output, or every existing install breaks.
        let text = captured_manifest();
        let m = decode(&text).unwrap();
        assert_eq!(m.route, Route::X86);
        assert_eq!(encode(&m), text);
        assert!(!text.contains("\"route\""));
    }

    #[test]
    fn an_x64_manifest_round_trips_and_names_its_route() {
        let mut m = Manifest::new("Vulkan", Route::X64);
        m.entries.push(Entry {
            name: "dlss5-neural.addon64".into(),
            hash: "a".repeat(64),
            backup: String::new(),
            backup_hash: String::new(),
            owned: true,
            configuration: false,
        });
        let text = encode(&m);
        assert!(text.contains("\"route\":\"x64\","));
        assert_eq!(decode(&text).unwrap(), m);
        // x64 presets are only valid on the x64 route, and vice versa.
        assert!(decode(&text.replace("\"preset\":\"Vulkan\"", "\"preset\":\"D3D8\"")).is_err());
    }

    #[test]
    fn the_two_routes_write_different_manifest_files() {
        assert_eq!(Route::X86.manifest_name(), MANIFEST_NAME);
        assert_eq!(Route::X64.manifest_name(), MANIFEST_NAME_X64);
        assert_ne!(Route::X86.manifest_name(), Route::X64.manifest_name());
    }

    fn set_readonly(path: &Path, on: bool) {
        let mut perms = std::fs::metadata(path).unwrap().permissions();
        #[allow(clippy::permissions_set_readonly_false)]
        perms.set_readonly(on);
        std::fs::set_permissions(path, perms).unwrap();
    }

    #[test]
    fn a_file_another_program_is_holding_stops_the_transaction_before_it_starts() {
        let root = temp("guard-locked");
        let target = root.join("dlss5-neural.addon64");
        write(&target, b"in use").unwrap();
        set_readonly(&target, true);

        let mut files: BTreeMap<String, Vec<u8>> = BTreeMap::new();
        files.insert("dlss5-neural.addon64".into(), b"the new one".to_vec());
        let mut log = Vec::new();
        let err = apply(&root, "D3D11", Route::X64, &files, &mut log)
            .unwrap_err()
            .0;
        assert!(err.contains("open by another program"), "got: {err}");

        // The journal must not exist: the guard runs before anything is recorded or written.
        assert!(!root.join(Route::X64.manifest_name()).exists());
        assert!(!root.join(BACKUP_DIR).exists());
        set_readonly(&target, false);
        assert_eq!(read(&target).unwrap(), b"in use", "the held file is untouched");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_folder_that_is_not_there_is_named_rather_than_failing_midway() {
        let root = temp("guard-missing").join("no-such-subfolder");
        let mut files: BTreeMap<String, Vec<u8>> = BTreeMap::new();
        files.insert("dlss5-neural.addon64".into(), b"x".to_vec());
        let mut log = Vec::new();
        let err = apply(&root, "D3D11", Route::X64, &files, &mut log)
            .unwrap_err()
            .0;
        assert!(err.contains("is not a folder"), "got: {err}");
    }

    #[test]
    fn the_guard_counts_the_backup_as_well_as_the_replacement() {
        let root = temp("guard-space");
        // A file that will be displaced: the transaction needs room for the new bytes and for the
        // copy of the old ones, which is what the panel's old check did not account for.
        write(&root.join("dlss5-neural.addon64"), &vec![0u8; 2048]).unwrap();
        let mut files: BTreeMap<String, Vec<u8>> = BTreeMap::new();
        files.insert("dlss5-neural.addon64".into(), vec![1u8; 4096]);
        // Nothing here is short of disk, so this must pass; the arithmetic is asserted by the
        // message when it does not, and by this test not regressing into a false refusal.
        let mut log = Vec::new();
        assert!(apply(&root, "D3D11", Route::X64, &files, &mut log).is_ok());
        assert_eq!(read(&root.join("dlss5-neural.addon64")).unwrap().len(), 4096);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn apply_refuses_a_filename_it_does_not_manage() {
        let root = temp("unmanaged");
        let mut files: BTreeMap<String, Vec<u8>> = BTreeMap::new();
        files.insert("something-else.dll".into(), b"x".to_vec());
        let mut log = Vec::new();
        let err = apply(&root, "D3D11", Route::X64, &files, &mut log).unwrap_err().0;
        assert!(err.contains("unmanaged filename"), "got: {err}");
        let _ = std::fs::remove_dir_all(&root);
    }

    // --- Parity with installer-x86/tests.cpp -------------------------------------------------
    // These were the behaviours the C++ fixture asserted and this suite did not, checked one by one
    // before that fixture was retired rather than trusting that two test counts meant the same
    // coverage.

    #[test]
    fn a_fresh_install_writes_the_x86_tuning_defaults() {
        let ini = fresh_ini();
        assert_eq!(get_ini(&ini, "dlss5", "ColourStrength"), "0.25");
        assert_eq!(get_ini(&ini, "dlss5", "Scale"), "1.0");
        assert_eq!(get_ini(&ini, "dlss5", "Passes"), "1");
        assert!(ini.contains("\r\n"), "the ini is written with CRLF like the rest of them");
    }

    fn release_fixture() -> Option<PathBuf> {
        let raw = std::env::var("DLSS5_TEST_RELEASE_DIR").ok()?;
        let path = PathBuf::from(raw);
        path.join("payload.sha256").is_file().then_some(path)
    }

    fn x86_game(tag: &str) -> PathBuf {
        let root = temp(tag);
        let mut pe = vec![0u8; 512];
        pe[0] = 0x4d;
        pe[1] = 0x5a;
        pe[60] = 128;
        pe[128] = 0x50;
        pe[129] = 0x45;
        pe[132] = 0x4c;
        pe[133] = 0x01;
        pe[152] = 0x0b;
        pe[153] = 0x01;
        write(&root.join("game.exe"), &pe).unwrap();
        root
    }

    #[test]
    fn the_proxy_that_gets_installed_follows_the_api() {
        let Some(release) = release_fixture() else {
            eprintln!("skipped: set DLSS5_TEST_RELEASE_DIR");
            return;
        };
        for (preset, wanted, unwanted) in
            [("D3D11", "dxgi.dll", "d3d9.dll"), ("D3D9", "d3d9.dll", "dxgi.dll")]
        {
            let game = x86_game(&format!("parity-proxy-{preset}"));
            let mut app = Installer::new(release.clone());
            app.install(&game.join("game.exe"), preset).unwrap();

            assert!(game.join(wanted).is_file(), "{preset} must install {wanted}");
            assert!(!game.join(unwanted).exists(), "{preset} must not install {unwanted}");
            // No translation wrapper on a native route, and dgVoodoo is gone for good.
            assert!(!game.join("d3d8.dll").exists());
            assert!(!game.join("dgVoodoo.conf").exists());
            assert_eq!(
                hash_file(&game.join(wanted)).unwrap(),
                RESHADE_SHA,
                "the pinned ReShade build is what lands"
            );
            let _ = std::fs::remove_dir_all(&game);
        }
    }

    #[test]
    fn reinstalling_is_idempotent_and_never_rewrites_the_users_tuning() {
        let Some(release) = release_fixture() else {
            eprintln!("skipped: set DLSS5_TEST_RELEASE_DIR");
            return;
        };
        let game = x86_game("parity-reinstall");
        let exe = game.join("game.exe");
        let mut app = Installer::new(release.clone());
        app.install(&exe, "D3D11").unwrap();

        let manifest_before = read(&game.join(MANIFEST_NAME)).unwrap();
        let mut again = Installer::new(release.clone());
        again.install(&exe, "D3D11").unwrap();
        assert_eq!(
            read(&game.join(MANIFEST_NAME)).unwrap(),
            manifest_before,
            "a reinstall of the same content must not churn the manifest"
        );

        // Tuning the person changed afterwards is theirs.
        let tuned = b"[dlss5]\r\nColourStrength=0.65\r\nScale=0.75\r\n".to_vec();
        write(&game.join("dlss5-neural.ini"), &tuned).unwrap();
        let mut third = Installer::new(release.clone());
        third.install(&exe, "D3D11").unwrap();
        assert_eq!(
            read(&game.join("dlss5-neural.ini")).unwrap(),
            tuned,
            "reinstall preserves user tuning byte for byte"
        );

        // And uninstall keeps it, because it is not ours to take.
        let mut remover = Installer::new(release);
        remover.uninstall(&game, false).unwrap();
        assert_eq!(read(&game.join("dlss5-neural.ini")).unwrap(), tuned);
        let _ = std::fs::remove_dir_all(&game);
    }

    #[test]
    fn a_pre_existing_tuning_file_is_never_recreated_or_touched() {
        let Some(release) = release_fixture() else {
            eprintln!("skipped: set DLSS5_TEST_RELEASE_DIR");
            return;
        };
        let game = x86_game("parity-tuning");
        let mine = b"[dlss5]\r\nStartOn=1\r\n".to_vec();
        write(&game.join("dlss5-neural.ini"), &mine).unwrap();

        let mut app = Installer::new(release);
        app.install(&game.join("game.exe"), "D3D11").unwrap();
        assert_eq!(
            read(&game.join("dlss5-neural.ini")).unwrap(),
            mine,
            "an ini that was already there is left exactly as it was"
        );
        let _ = std::fs::remove_dir_all(&game);
    }

    #[test]
    fn a_file_replaced_after_installing_is_kept_rather_than_removed() {
        let Some(release) = release_fixture() else {
            eprintln!("skipped: set DLSS5_TEST_RELEASE_DIR");
            return;
        };
        let game = x86_game("parity-replaced");
        let mut app = Installer::new(release.clone());
        app.install(&game.join("game.exe"), "D3D11").unwrap();

        write(&game.join("dxgi.dll"), b"a build of my own").unwrap();
        let mut remover = Installer::new(release);
        remover.uninstall(&game, false).unwrap();
        assert_eq!(
            read(&game.join("dxgi.dll")).unwrap(),
            b"a build of my own",
            "uninstall must not delete something swapped in after the install"
        );
        let _ = std::fs::remove_dir_all(&game);
    }

    #[test]
    fn a_payload_that_fails_its_hash_is_refused_before_anything_is_written() {
        let Some(release) = release_fixture() else {
            eprintln!("skipped: set DLSS5_TEST_RELEASE_DIR");
            return;
        };
        for corrupt in ["dlssnr_amd_pass1.dll", "dlssnr_on_amd_weights.bin", "dxgi.dll"] {
            let fake = temp(&format!("parity-corrupt-{corrupt}"));
            std::fs::create_dir_all(fake.join("files")).unwrap();
            write(
                &fake.join("payload.sha256"),
                &read(&release.join("payload.sha256")).unwrap(),
            )
            .unwrap();
            for entry in std::fs::read_dir(release.join("files")).unwrap().flatten() {
                let name = entry.file_name();
                let to = fake.join("files").join(&name);
                if name.to_string_lossy() == corrupt {
                    write(&to, b"not the real payload").unwrap();
                } else {
                    std::fs::copy(entry.path(), &to).unwrap();
                }
            }

            let game = x86_game(&format!("parity-corrupt-game-{corrupt}"));
            let app = Installer::new(fake.clone());
            let err = app.plan(&game.join("game.exe"), "D3D11").unwrap_err().0;
            assert!(
                err.contains("SHA256 mismatch"),
                "{corrupt} should have been refused by hash, got: {err}"
            );
            assert!(
                !game.join(MANIFEST_NAME).exists(),
                "a refused payload must not leave a journal"
            );
            let _ = std::fs::remove_dir_all(&fake);
            let _ = std::fs::remove_dir_all(&game);
        }
    }

    #[test]
    fn the_d3d8_route_fails_closed_when_the_pinned_translator_is_absent() {
        let Some(release) = release_fixture() else {
            eprintln!("skipped: set DLSS5_TEST_RELEASE_DIR");
            return;
        };
        let without = temp("parity-no-translator");
        std::fs::create_dir_all(without.join("files")).unwrap();
        write(
            &without.join("payload.sha256"),
            &read(&release.join("payload.sha256")).unwrap(),
        )
        .unwrap();
        for entry in std::fs::read_dir(release.join("files")).unwrap().flatten() {
            if entry.file_name().to_string_lossy() == "d3d8to9.dll" {
                continue;
            }
            std::fs::copy(entry.path(), without.join("files").join(entry.file_name())).unwrap();
        }

        let game = x86_game("parity-no-translator-game");
        let app = Installer::new(without.clone());
        assert!(
            app.plan(&game.join("game.exe"), "D3D8").is_err(),
            "D3D8 must fail closed without its pinned sidecar rather than improvising"
        );
        let _ = std::fs::remove_dir_all(&without);
        let _ = std::fs::remove_dir_all(&game);
    }

    #[test]
    fn a_legacy_d3d9_manifest_also_stays_uninstallable() {
        let legacy = captured_manifest()
            .replace("\"preset\":\"D3D8\"", "\"preset\":\"D3D9\"")
            .replace("\"dgVoodoo\":\"none\"", "\"dgVoodoo\":\"2.87.4\"");
        assert_eq!(decode(&legacy).unwrap().preset, "D3D9");
    }

    #[test]
    fn atomic_manifest_round_trips_through_the_filesystem() {
        let root = temp("atomic");
        let mut m = Manifest::new("D3D9", Route::X86);
        m.entries.push(Entry {
            name: "d3d9.dll".into(),
            hash: RESHADE_SHA.into(),
            backup: String::new(),
            backup_hash: String::new(),
            owned: true,
            configuration: false,
        });
        atomic_manifest(&root, &m).unwrap();
        let text = String::from_utf8(read(&root.join(MANIFEST_NAME)).unwrap()).unwrap();
        assert_eq!(decode(&text).unwrap(), m);
        assert!(
            !root.join(format!("{MANIFEST_NAME}.tmp")).exists(),
            "the temporary journal file must not survive the commit"
        );
        let _ = std::fs::remove_dir_all(&root);
    }

}
