//! What the installer actually does to a folder. No terminal, no drawing -- so the whole of it
//! can be reasoned about, and tested, without a TUI in the way.

use sha2::{Digest, Sha256};
use std::fmt::Write as _;
use std::fs;
use crate::engine;
pub use crate::engine::Route;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// The add-on, built into this executable. One file to hand out, and the installer can never
/// install an add-on from a different release than the one it was built beside.
pub const ADDON: &[u8] = include_bytes!("../../build/dlss5-neural.addon64");
pub const ADDON_NAME: &str = "dlss5-neural.addon64";
#[cfg(test)]
const ADDON32: &str = "dlss5-neural.addon32";
#[cfg(test)]
const HOST64: &str = "dlss5-neural-host64.exe";

pub const RUNTIME_NAME: &str = "dlssnr_amd_pass1.dll";
pub const WEIGHTS_NAME: &str = "dlssnr_on_amd_weights.bin";

/// The add-on hashes the runtime at load and refuses anything else, because the integration is
/// fixed offsets into one specific binary. Checking here as well means the installer can say so
/// in words, instead of leaving the add-on to fail later with the game already open.
pub const RUNTIME_SHA: &str = "ddd82d313aa74c2e7602d17dfb7e7cd90cca9bfc0306f581684d35d75d1b350b";
pub const WEIGHTS_SHA: &str = "6bf8dc931ef3ccffe18c82de26ab374156e7f19539ffcf8eabaa25dca5cf15ab";

/// Known-bad: the runtime this release replaced. Recognised by name so the message can be
/// "you have the old one" instead of "this file is wrong".
pub const RUNTIME_SHA_0214: &str =
    "e145ff963b1ef614000000000000000000000000000000000000000000000000";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Preset {
    Pcsx2,
    Rpcs3,
    Dx11,
    Dx12,
    Vulkan,
    X86Dx11,
    X86Dx9,
    X86Dx8,
}

impl Preset {
    pub const ALL: [Preset; 8] = [
        Preset::Pcsx2,
        Preset::Rpcs3,
        Preset::Dx11,
        Preset::Dx12,
        Preset::Vulkan,
        Preset::X86Dx11,
        Preset::X86Dx9,
        Preset::X86Dx8,
    ];

    const X64: [Preset; 5] =
        [Preset::Pcsx2, Preset::Rpcs3, Preset::Dx11, Preset::Dx12, Preset::Vulkan];
    const X86: [Preset; 3] = [Preset::X86Dx11, Preset::X86Dx9, Preset::X86Dx8];

    pub fn route(self) -> Route {
        match self {
            Preset::X86Dx11 | Preset::X86Dx9 | Preset::X86Dx8 => Route::X86,
            _ => Route::X64,
        }
    }

    /// What the target row offers. Five of the ten API-by-bitness combinations do not exist, and
    /// the detected width rules out the rest, so a person is never shown a choice that cannot work.
    /// When nothing could be detected the whole list stays available rather than guessing.
    pub fn offered(detected: &Detected) -> &'static [Preset] {
        match detected.route() {
            Some(Route::X64) => &Self::X64,
            Some(Route::X86) => &Self::X86,
            None => &Self::ALL,
        }
    }

    /// The string recorded in the install manifest. Kept separate from `label`, which is prose
    /// that can be reworded, while this one has to keep matching manifests already on disk.
    pub fn manifest_preset(self) -> &'static str {
        match self {
            Preset::Pcsx2 => "PCSX2",
            Preset::Rpcs3 => "RPCS3",
            Preset::Dx11 => "D3D11",
            Preset::Dx12 => "D3D12",
            Preset::Vulkan => "Vulkan",
            Preset::X86Dx11 => "D3D11",
            Preset::X86Dx9 => "D3D9",
            Preset::X86Dx8 => "D3D8",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Preset::Pcsx2 => "PCSX2",
            Preset::Rpcs3 => "RPCS3",
            Preset::Dx11 => "D3D11 game",
            Preset::Dx12 => "D3D12 game",
            Preset::Vulkan => "Vulkan game",
            Preset::X86Dx11 => "D3D11 game, 32-bit",
            Preset::X86Dx9 => "D3D9 game, 32-bit",
            Preset::X86Dx8 => "D3D8 game, 32-bit",
        }
    }

    /// Vulkan is not a proxy DLL: ReShade loads as a global layer and the add-on sits beside the
    /// executable all the same. Both the emulator on Vulkan and a native Vulkan game are checked
    /// the same way, which is why this is a question and not four copies of one branch.
    fn is_vulkan(self) -> bool {
        matches!(self, Preset::Rpcs3 | Preset::Vulkan)
    }

    /// "Game" is wrong for an emulator, and the people most likely to get the folder wrong are
    /// exactly the emulator users -- the files go beside the emulator, not beside the ROM.
    pub fn folder_label(self) -> &'static str {
        match self {
            Preset::Pcsx2 | Preset::Rpcs3 => " Emulator folder or executable ",
            _ => " Game folder or executable ",
        }
    }

    /// An executable whose presence says the folder is the right one. Absent means "warn", never
    /// "refuse": there is no whitelist anywhere in this project and there is not going to be one
    /// here either.
    fn expected_exe(self) -> Option<&'static str> {
        match self {
            Preset::Pcsx2 => Some("pcsx2-qt.exe"),
            Preset::Rpcs3 => Some("rpcs3.exe"),
            _ => None,
        }
    }

    pub fn note(self) -> &'static str {
        match self {
            Preset::Pcsx2 => {
                "Set the renderer to Direct3D 11 -- it is the only one where the emulator's depth \
                 reaches the network. Watch for a per-game override: it beats the global setting \
                 silently, and it is the most common way this looks broken when it is not."
            }
            Preset::Rpcs3 => {
                "EXPERIMENTAL. ReShade on Vulkan is a global layer, not a proxy DLL: run the \
                 ReShade installer against rpcs3.exe and pick Vulkan, or nothing will load. The \
                 network gets colour and estimated motion only -- there is no depth on Vulkan."
            }
            Preset::Dx11 => {
                "The best case. D3D11 is the only route where the game's own depth and motion \
                 vectors reach the network."
            }
            Preset::Dx12 => {
                "The degraded case. On D3D12 an add-on is shown nothing but the swapchain, so the \
                 network gets colour and guesses at the rest. It works; expect less from it."
            }
            Preset::Vulkan => {
                "EXPERIMENTAL. ReShade on Vulkan is a global layer, not a proxy DLL: run its \
                 installer against the game's own .exe and pick Vulkan, or nothing loads. The \
                 game also has to import vkCreateDevice statically -- one that resolves Vulkan \
                 through vkGetInstanceProcAddr cannot be hooked, and the add-on stands down \
                 rather than guess. No depth on Vulkan either way: colour and estimated motion."
            }
            Preset::X86Dx11 => {
                "EXPERIMENTAL. A 32-bit game cannot load the 64-bit runtime, so the add-on runs as \
                 a pair: a 32-bit frontend inside the game and a 64-bit helper beside it, sharing \
                 frames on the same adapter. Install ReShade with full add-on support as the \
                 32-bit dxgi.dll."
            }
            Preset::X86Dx9 => {
                "EXPERIMENTAL. The same 32-bit pair as D3D11, reached through a private D3D9/D3D11 \
                 stage. D3D9Ex shares GPU textures; plain D3D9 falls back to a CPU round trip that \
                 costs a fixed few milliseconds every frame, no matter how far the scale is turned \
                 down. Install ReShade as the 32-bit d3d9.dll."
            }
            Preset::X86Dx8 => {
                "EXPERIMENTAL. D3D8 is translated to D3D9 by the pinned d3d8to9 build and then \
                 takes the D3D9 route above; there is no second renderer here. A game that already \
                 ships its own d3d8.dll wrapper keeps it, and the translator is installed beside \
                 it as d3d8R.dll. Install ReShade as the 32-bit d3d9.dll."
            }
        }
    }
}

/// A ReShade proxy, by the name it has to be loaded under.
const PROXIES: [&str; 4] = ["d3d11.dll", "dxgi.dll", "d3d12.dll", "opengl32.dll"];

/// The sizes that go with the two hashes above. Hashing 147 MB on every keystroke is not an
/// option, but comparing a length is free, and a wrong length is a wrong file -- which is the
/// whole of what the pre-flight needs to say before anything is copied.
const RUNTIME_SIZE: u64 = 7_248_384;
const WEIGHTS_SIZE: u64 = 147_689_451;

/// Files an older layout left behind. One copy of the runtime per pass, which did not fit in
/// VRAM and has not been used for two releases.
fn dead_files() -> Vec<String> {
    (2..=10).map(|n| format!("dlssnr_amd_pass{n}.dll")).collect()
}

pub struct Report {
    pub lines: Vec<(Level, String)>,
    pub failed: bool,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Level {
    Ok,
    Warn,
    Err,
    Info,
}

impl Report {
    pub fn new() -> Self {
        Report { lines: Vec::new(), failed: false }
    }
    fn ok(&mut self, s: impl Into<String>) {
        self.lines.push((Level::Ok, s.into()));
    }
    fn info(&mut self, s: impl Into<String>) {
        self.lines.push((Level::Info, s.into()));
    }
    fn warn(&mut self, s: impl Into<String>) {
        self.lines.push((Level::Warn, s.into()));
    }
    fn err(&mut self, s: impl Into<String>) {
        self.lines.push((Level::Err, s.into()));
        self.failed = true;
    }

    /// The report as a file, for the user to hand over when something went wrong. Everything the
    /// installer saw is in here; there is nothing it knows that this does not say.
    pub fn to_log(&self, header: &str) -> String {
        let mut out = String::new();
        let _ = writeln!(out, "dlss5-neural-amd installer log");
        let _ = writeln!(out, "{header}");
        let _ = writeln!(out, "{}", "-".repeat(70));
        for (level, line) in &self.lines {
            let tag = match level {
                Level::Ok => "ok  ",
                Level::Warn => "warn",
                Level::Err => "ERR ",
                Level::Info => "    ",
            };
            let _ = writeln!(out, "{tag} {line}");
        }
        out
    }
}

#[cfg_attr(not(test), allow(dead_code))]
fn sha256(path: &Path) -> std::io::Result<String> {
    let bytes = fs::read(path)?;
    let mut hasher = Sha256::new();
    hasher.update(&bytes);
    Ok(format!("{:x}", hasher.finalize()))
}

/// Accepts either the folder holding the two files or one of the files themselves, because both
/// are things a person reasonably pastes.
/// The path exactly as typed, file or folder. `resolve_source` deliberately turns a dropped
/// executable into its folder, which is right for a flow that installs into one and wrong for
/// anything that has to read the PE header of the executable itself.
fn resolve_target(raw: &str) -> PathBuf {
    PathBuf::from(raw.trim().trim_matches('"'))
}

/// Field 1 takes either shape of folder, so one field means one thing on both routes.
///
/// A release folder keeps its payloads in `files\`, and it already carries the runtime and the
/// weights, so it serves the x64 route too. A folder holding just the two unzipped files is what
/// this screen has always asked for and still works. Whichever was given, this is where the x64
/// route reads its payloads from.
pub fn payload_dir(source: &Path) -> PathBuf {
    let nested = source.join("files");
    if nested.join(RUNTIME_NAME).is_file() || nested.join(WEIGHTS_NAME).is_file() {
        nested
    } else {
        source.to_path_buf()
    }
}

fn resolve_source(raw: &str) -> PathBuf {
    let p = PathBuf::from(raw.trim().trim_matches('"'));
    if p.is_file() {
        p.parent().map(Path::to_path_buf).unwrap_or(p)
    } else {
        p
    }
}

fn check_reshade(dir: &Path, preset: Preset, report: &mut Report) {
    let found: Vec<&str> = PROXIES.iter().copied().filter(|n| dir.join(n).is_file()).collect();

    if preset.is_vulkan() {
        if found.is_empty() {
            report.info(format!(
                "No ReShade proxy DLL here, which is correct for Vulkan: ReShade loads as a \
                 global layer instead. Make sure you ran its installer against {} and picked \
                 Vulkan.",
                preset.expected_exe().unwrap_or("the game's own .exe")
            ));
        } else {
            report.warn(format!(
                "Found {} here. On Vulkan ReShade loads as a global layer, and a proxy DLL as \
                 well means two ReShade instances in one process. Remove it if Vulkan is what \
                 you run.",
                found.join(", ")
            ));
        }
        return;
    }

    match found.len() {
        0 => report.warn(
            "No ReShade proxy DLL found here (d3d11.dll, dxgi.dll, d3d12.dll). The add-on cannot \
             load without ReShade, and it has to be the build with full add-on support. Files \
             were still copied, so installing ReShade afterwards is enough.",
        ),
        1 => report.ok(format!("ReShade found: {}", found[0])),
        _ => report.warn(format!(
            "More than one ReShade proxy here ({}). Only one is loaded, and which one depends on \
             the game. Keep the one that matches the renderer.",
            found.join(", ")
        )),
    }
}

fn check_exe(dir: &Path, preset: Preset, report: &mut Report) {
    if let Some(exe) = preset.expected_exe() {
        if dir.join(exe).is_file() {
            report.ok(format!("{exe} is here, so this is the right folder."));
        } else {
            report.warn(format!(
                "{exe} is not in this folder. That is only a warning -- nothing checks which \
                 program it is -- but it is usually a sign the path is wrong."
            ));
        }
    }
}

/// Verify a payload in the folder the user pointed at and hand back its bytes. This is the half of
/// the old `copy_verified` that decides whether a file is acceptable; whether it then gets written
/// is [`engine::apply`]'s decision, because that is what records ownership and takes the backup.
fn verified_payload(
    src_dir: &Path,
    name: &str,
    want_sha: &str,
    report: &mut Report,
) -> Option<Vec<u8>> {
    let src = src_dir.join(name);
    if !src.is_file() {
        report.err(format!("{name} is not in the runtime folder you gave."));
        return None;
    }
    let bytes = match fs::read(&src) {
        Ok(b) => b,
        Err(e) => {
            report.err(format!("could not read {name}: {e}"));
            return None;
        }
    };
    let got = engine::sha(&bytes);
    if got != want_sha {
        if name == RUNTIME_NAME && got.starts_with(&RUNTIME_SHA_0214[..16]) {
            report.err(format!(
                "{name} is the old v0.2.14 runtime. This release requires v0.2.17 and the add-on \
                 refuses anything else. Get the current one from the files channel."
            ));
        } else {
            report.err(format!(
                "{name} does not match the expected SHA-256.\n      expected {want_sha}\n      \
                 got      {got}\n      The add-on hashes the runtime at load and will refuse it."
            ));
        }
        return None;
    }
    Some(bytes)
}

/// The engine speaks the x86 installer's vocabulary. Until step 4 unifies the wording, translate it
/// into the sentences this screen has always printed, so the terminal output does not change shape
/// underneath people who are following the README.
fn narrate(line: &str, report: &mut Report) {
    if let Some(name) = line.strip_prefix("IDENTICAL: ") {
        report.ok(format!("{name} already correct, left alone."));
    } else if let Some(name) = line.strip_prefix("CREATE: ") {
        report.ok(format!("{name} copied and verified."));
    } else if let Some(name) = line.strip_prefix("EXTERNAL backed up: ") {
        report.ok(format!("{name} replaced; the previous file was backed up."));
    } else if let Some(name) = line.strip_prefix("RESTORED: ") {
        report.ok(format!("restored {name} from its backup"));
    } else if let Some(name) = line.strip_prefix("REMOVED: ") {
        report.ok(format!("removed {name}"));
    } else if let Some(rest) = line.strip_prefix("WARNING ") {
        report.warn(rest.to_string());
    } else {
        // PRESERVED lines and anything the engine adds later read fine as they are.
        report.info(line.to_string());
    }
}

fn sweep_dead(dir: &Path, report: &mut Report) {
    let mut removed = Vec::new();
    for name in dead_files() {
        let p = dir.join(&name);
        if p.is_file() {
            match fs::remove_file(&p) {
                Ok(()) => removed.push(name),
                Err(e) => report.warn(format!("could not remove {name}: {e}")),
            }
        }
    }
    if !removed.is_empty() {
        report.ok(format!(
            "removed {} unused file(s) from the old per-pass layout: {}",
            removed.len(),
            removed.join(", ")
        ));
    }
}

// ---------------------------------------------------------------------------------------------
// Pre-flight: everything that can be known before a single byte is written.
//
// All of it is cheap enough to redo on every keystroke -- metadata, one open(), one free-space
// call -- so the screen can answer "will this work?" while the path is still being pasted,
// instead of after 147 MB have been copied into a folder that was read-only.


/// ReShade writes `DisabledAddons=` into its own ini the first time anyone unticks an add-on, and
/// from then on it never loads it again and says nothing anywhere. It is the one failure in this
/// project that looks exactly like a broken install, so it is worth a line of its own.
fn check_disabled_addons(dir: &Path, report: &mut Report) {
    let ini = dir.join("ReShade.ini");
    let Ok(text) = fs::read_to_string(&ini) else { return };
    for line in text.lines() {
        let line = line.trim();
        if let Some(list) = line.strip_prefix("DisabledAddons=") {
            if list.contains(ADDON_NAME) || list.to_lowercase().contains("dlss5") {
                report.err(
                    "ReShade.ini has this add-on in DisabledAddons=. ReShade writes that line if \
                     the add-on is ever unticked, and then it never loads it again, with no error \
                     anywhere. Clear that line before blaming the install.",
                );
            } else if !list.is_empty() {
                report.info(format!("ReShade.ini disables other add-ons: {list}"));
            }
        }
    }
}

/// What is known before F5, from whatever is filled in so far. Never writes anything except one
/// zero-byte probe it removes again.
pub fn preflight(game_dir: &str, runtime_dir: &str, preset: Preset) -> Report {
    let mut report = Report::new();
    let dir = resolve_source(game_dir);
    let src = resolve_source(runtime_dir);

    // --- the two files, which is where someone starts -------------------------------------
    if src.as_os_str().is_empty() {
        report.info("Waiting for field 1: the folder with the runtime and the weights.");
    } else if !src.is_dir() {
        report.err(format!("Field 1: {} is not a folder.", src.display()));
    } else {
        let mut all_there = true;
        let payloads = payload_dir(&src);
        for (name, want) in [(RUNTIME_NAME, RUNTIME_SIZE), (WEIGHTS_NAME, WEIGHTS_SIZE)] {
            match engine::size_of(&payloads.join(name)) {
                None => {
                    report.err(format!("{name} is not in that folder."));
                    all_there = false;
                }
                Some(got) if got != want => {
                    report.err(format!(
                        "{name} is {got} bytes, and this release expects {want}. That is a \
                         different build, and the add-on refuses anything but the one it was \
                         compiled against.",
                    ));
                    all_there = false;
                }
                Some(_) => {}
            }
        }
        if all_there {
            report.ok("Both files are there and the right size. F5 verifies the SHA-256 too.");
        }
    }

    // --- the target ------------------------------------------------------------------------
    if dir.as_os_str().is_empty() {
        report.info(format!(
            "Waiting for field 2: the {}.",
            preset.folder_label().trim().to_lowercase()
        ));
        return report;
    }
    if !dir.is_dir() {
        report.err(format!("Field 2: {} is not a folder.", dir.display()));
        return report;
    }

    if !engine::folder_is_writable(&dir) {
        report.err(
            "That folder cannot be written to. It is either read-only or somewhere that needs \
             administrator rights -- run this installer as administrator, or move the game.",
        );
    }

    // Anything already there and held open will fail the copy, so name the files rather than let
    // fs::copy come back with "Acesso negado" halfway through.
    let held: Vec<&str> = [ADDON_NAME, RUNTIME_NAME, WEIGHTS_NAME]
        .into_iter()
        .filter(|n| engine::is_locked(&dir.join(n)))
        .collect();
    if !held.is_empty() {
        report.err(format!(
            "{} {} open by another program. The game or emulator is almost certainly still \
             running -- close it and this line goes away.",
            held.join(", "),
            if held.len() == 1 { "is" } else { "are" }
        ));
    }

    // --- room for the weights ---------------------------------------------------------------
    let mut need = 0u64;
    for (name, size) in
        [(ADDON_NAME, ADDON.len() as u64), (RUNTIME_NAME, RUNTIME_SIZE), (WEIGHTS_NAME, WEIGHTS_SIZE)]
    {
        if engine::size_of(&dir.join(name)) != Some(size) {
            need += size;
        }
    }
    if let Some(free) = engine::free_bytes(&dir) {
        if need > 0 && free < need {
            report.err(format!(
                "Not enough room: {} MB free, and this needs {} MB. The weights alone are {} MB.",
                free / 1_048_576,
                need / 1_048_576,
                WEIGHTS_SIZE / 1_048_576
            ));
        }
    }

    check_exe(&dir, preset, &mut report);
    check_reshade(&dir, preset, &mut report);
    check_disabled_addons(&dir, &mut report);

    let dead: Vec<String> =
        dead_files().into_iter().filter(|n| dir.join(n).is_file()).collect();
    if !dead.is_empty() {
        report.info(format!(
            "{} file(s) from the old per-pass layout are here and will be removed: {}",
            dead.len(),
            dead.join(", ")
        ));
    }

    if !report.failed {
        report.ok("Nothing in the way. F5 installs.");
    }
    report
}

/// The 32-bit route reads the PE header to be certain, so it needs the executable and not just the
/// folder. When a folder was given and exactly one 32-bit executable is in it, that is unambiguous
/// and gets used; anything else is a question only the person can answer.
fn x86_target(game_dir: &str, report: &mut Report) -> Option<PathBuf> {
    let path = resolve_target(game_dir);
    if path.is_file() {
        return Some(path);
    }
    if !path.is_dir() {
        report.err(format!("{} is not a folder.", path.display()));
        return None;
    }
    let mut found: Vec<PathBuf> = Vec::new();
    if let Ok(entries) = fs::read_dir(&path) {
        for entry in entries.flatten() {
            let p = entry.path();
            let is_exe = p
                .extension()
                .and_then(|e| e.to_str())
                .map(|e| e.eq_ignore_ascii_case("exe"))
                == Some(true);
            if p.is_file()
                && is_exe
                && engine::machine_of_file(&p) == Some(engine::MACHINE_X86)
            {
                found.push(p);
            }
        }
    }
    match found.len() {
        1 => Some(found.remove(0)),
        0 => {
            report.err(
                "No 32-bit executable in that folder. The bridge route needs the game's own .exe: \
                 point field 2 straight at it.",
            );
            None
        }
        _ => {
            let names: Vec<String> = found
                .iter()
                .map(|p| p.file_name().unwrap_or_default().to_string_lossy().to_string())
                .collect();
            report.err(format!(
                "More than one 32-bit executable here ({}). Point field 2 at the one the game \
                 actually runs, rather than at the folder.",
                names.join(", ")
            ));
            None
        }
    }
}

fn install_x86(game_dir: &str, release_dir: &str, preset: Preset) -> Report {
    let mut report = Report::new();
    let Some(target) = x86_target(game_dir, &mut report) else {
        return report;
    };
    let release = resolve_source(release_dir);
    if release.as_os_str().is_empty() {
        report.err(
            "Field 1 has to be the folder you unzipped the x86 release into -- the one holding \
             files\\ and payload.sha256. The bridge ships as separate files, so nothing can be \
             installed without it.",
        );
        return report;
    }
    if !release.join("payload.sha256").is_file() {
        report.err(format!(
            "{} does not look like the x86 release: payload.sha256 is not in it.",
            release.display()
        ));
        return report;
    }

    report.info(format!("target: {}", target.display()));
    report.info(format!("preset: {}", preset.label()));

    let mut app = engine::Installer::new(release);
    let outcome = app.install(&target, preset.manifest_preset());
    for line in &app.log {
        narrate(line, &mut report);
    }
    match outcome {
        Ok(()) => {
            report.info(preset.note());
            report.info(
                "It starts switched off. Open the overlay with Home, or press Ctrl+End. StartOn=1 \
                 in dlss5-neural.ini makes it come up enabled.",
            );
        }
        Err(e) => report.err(format!(
            "{e}. Nothing was left half-written: the install rolled itself back."
        )),
    }
    report
}

fn uninstall_x86(game_dir: &str) -> Report {
    let mut report = Report::new();
    let path = resolve_target(game_dir);
    if path.as_os_str().is_empty() {
        report.err("No game folder given.");
        return report;
    }
    // Uninstall works off the manifest, so the folder is enough -- but accept an executable too,
    // because that is what the same field held during the install.
    let dir = if path.is_file() {
        match engine::install_directory(&path) {
            Ok(d) => d,
            Err(e) => {
                report.err(e.0);
                return report;
            }
        }
    } else {
        path
    };
    report.info(format!("target: {}", dir.display()));

    let mut log = Vec::new();
    match engine::uninstall(&dir, Route::X86, false, &mut log) {
        Ok(()) => {
            for line in &log {
                narrate(line, &mut report);
            }
        }
        Err(e) => report.err(format!("{e}")),
    }
    report.info("ReShade itself was left alone. Use its own installer to remove it.");
    report
}

pub fn install(game_dir: &str, runtime_dir: &str, preset: Preset) -> Report {
    if preset.route() == Route::X86 {
        return install_x86(game_dir, runtime_dir, preset);
    }
    let mut report = Report::new();
    let dir = resolve_source(game_dir);
    let src = resolve_source(runtime_dir);

    if dir.as_os_str().is_empty() {
        report.err("No game folder given.");
        return report;
    }
    if !dir.is_dir() {
        report.err(format!("{} is not a folder.", dir.display()));
        return report;
    }
    report.info(format!("target: {}", dir.display()));
    report.info(format!("preset: {}", preset.label()));

    check_exe(&dir, preset, &mut report);
    check_reshade(&dir, preset, &mut report);

    // The add-on is always part of the plan. The runtime and the weights join it only when the
    // folder holding them was given and every byte checked out.
    let mut files: BTreeMap<String, Vec<u8>> = BTreeMap::new();
    files.insert(ADDON_NAME.to_string(), ADDON.to_vec());

    if src.as_os_str().is_empty() {
        report.warn(
            "No runtime folder given, so the runtime and weights were not installed. The add-on \
             does nothing without them. Re-run with the folder you unzipped them into.",
        );
    } else if !src.is_dir() {
        report.err(format!("{} is not a folder.", src.display()));
    } else {
        let payloads = payload_dir(&src);
        for (name, want) in [(RUNTIME_NAME, RUNTIME_SHA), (WEIGHTS_NAME, WEIGHTS_SHA)] {
            if let Some(bytes) = verified_payload(&payloads, name, want, &mut report) {
                files.insert(name.to_string(), bytes);
            }
        }
    }

    // A refused payload now stops the whole install rather than leaving the add-on behind on its
    // own. The transaction is all-or-nothing, which is the point of routing through the engine.
    if report.failed {
        report.info("Nothing was written: fix the problem above and run it again.");
        return report;
    }

    let mut log = Vec::new();
    match engine::apply(
        &dir,
        preset.manifest_preset(),
        Route::X64,
        &files,
        &mut log,
    ) {
        Ok(()) => {
            for line in &log {
                narrate(line, &mut report);
            }
        }
        Err(e) => {
            for line in &log {
                narrate(line, &mut report);
            }
            report.err(format!(
                "{e}. Nothing was left half-written: the install rolled itself back."
            ));
            return report;
        }
    }

    sweep_dead(&dir, &mut report);

    if !report.failed {
        report.info(preset.note());
        report.info(
            "It starts switched off. Open the overlay with Home, or press Ctrl+End. StartOn=1 in \
             dlss5-neural.ini makes it come up enabled.",
        );
    }
    report
}

pub fn uninstall(game_dir: &str, preset: Preset) -> Report {
    if preset.route() == Route::X86 {
        return uninstall_x86(game_dir);
    }
    let mut report = Report::new();
    let dir = resolve_source(game_dir);
    if dir.as_os_str().is_empty() {
        report.err("No game folder given.");
        return report;
    }
    if !dir.is_dir() {
        report.err(format!("{} is not a folder.", dir.display()));
        return report;
    }
    report.info(format!("target: {}", dir.display()));

    let mut gone = 0usize;

    // An install written by this version has a manifest, so it knows what it owned, what it
    // displaced and what the user has changed since. Installs from before the manifest existed have
    // none, and the name sweep below is the only way to take those back.
    let manifest = dir.join(Route::X64.manifest_name());
    if manifest.is_file() {
        let mut log = Vec::new();
        match engine::uninstall(&dir, Route::X64, false, &mut log) {
            Ok(()) => {
                for line in &log {
                    narrate(line, &mut report);
                }
                gone += 1;
            }
            Err(e) => report.err(format!("could not undo the recorded install: {e}")),
        }
    } else {
        // Everything the add-on installs. The ini is deliberately not in this list.
        let mut names: Vec<String> =
            vec![ADDON_NAME.into(), RUNTIME_NAME.into(), WEIGHTS_NAME.into()];
        names.extend(dead_files());
        for name in &names {
            let p = dir.join(name);
            if p.is_file() {
                match fs::remove_file(&p) {
                    Ok(()) => {
                        gone += 1;
                        report.ok(format!("removed {name}"));
                    }
                    Err(e) => report.err(format!("could not remove {name}: {e}")),
                }
            }
        }
    }

    // Written by the add-on itself at run time, so they are never in a manifest and are swept the
    // same way whichever branch ran above.
    for name in ["dlss5-pass1.dll", "dlss5-neural.log", "dlssnr_on_amd.log", "dlssnr_on_amd.ini"] {
        let p = dir.join(name);
        if p.is_file() {
            match fs::remove_file(&p) {
                Ok(()) => {
                    gone += 1;
                    report.ok(format!("removed {name}"));
                }
                Err(e) => report.err(format!("could not remove {name}: {e}")),
            }
        }
    }
    for folder in ["dlss5-runtime", "dlss5-captures"] {
        let p = dir.join(folder);
        if p.is_dir() {
            match fs::remove_dir_all(&p) {
                Ok(()) => {
                    gone += 1;
                    report.ok(format!("removed {folder}\\"));
                }
                Err(e) => report.err(format!("could not remove {folder}: {e}")),
            }
        }
    }

    if gone == 0 {
        report.warn("Nothing of ours was in that folder.");
    }
    if dir.join("dlss5-neural.ini").is_file() {
        report.info(
            "dlss5-neural.ini was left in place: it is your tuning, not ours. Delete it by hand \
             if you want a clean slate.",
        );
    }
    report.info("ReShade itself was left alone. Use its own installer to remove it.");
    report
}

// The TUI cannot be exercised from a script, so what is checked here is everything underneath
// it: what lands in a folder, what is refused, and what uninstall takes back out. The real
// weights are 147 MB, so the tests use stand-ins and assert on the paths that do not need the
// genuine bytes -- a wrong hash, a missing file, the dead-file sweep, the round trip.
/// What the target says about which route applies.
///
/// `docs/installer-merge.md` calls for bitness to be detected rather than asked, and it is -- but
/// implementing it turned up a case the plan did not: the x64 screen has always taken a *folder*,
/// and a folder can hold a 32-bit launcher next to a 64-bit game. So this detects when the answer
/// is unambiguous and says so when it is not, rather than picking one and being confidently wrong.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Detected {
    /// Every executable found agrees.
    Route(engine::Route, String),
    /// Executables of both widths are present; the person has to say which one they run.
    Mixed(String),
    /// Nothing to read: an empty field, a folder with no executables, or a path that is not there.
    Unknown,
}

impl Detected {
    pub fn route(&self) -> Option<engine::Route> {
        match self {
            Detected::Route(r, _) => Some(*r),
            _ => None,
        }
    }

    /// The line the screen shows under the target field.
    pub fn line(&self) -> Option<&str> {
        match self {
            Detected::Route(_, why) | Detected::Mixed(why) => Some(why),
            Detected::Unknown => None,
        }
    }
}

fn route_of(machine: u16) -> Option<engine::Route> {
    match machine {
        engine::MACHINE_X86 => Some(Route::X86),
        engine::MACHINE_X64 => Some(Route::X64),
        _ => None,
    }
}

/// Read the target -- an executable, or the executables sitting in a folder -- and decide.
pub fn detect(target: &str) -> Detected {
    let path = resolve_target(target);
    if path.as_os_str().is_empty() {
        return Detected::Unknown;
    }

    if path.is_file() {
        return match engine::machine_of_file(&path).and_then(route_of) {
            Some(Route::X86) => Detected::Route(
                Route::X86,
                format!(
                    "{} is a 32-bit executable, so this is the bridge route.",
                    name_of(&path)
                ),
            ),
            Some(Route::X64) => Detected::Route(
                Route::X64,
                format!("{} is a 64-bit executable.", name_of(&path)),
            ),
            None => Detected::Unknown,
        };
    }
    if !path.is_dir() {
        return Detected::Unknown;
    }

    let mut x86: Vec<String> = Vec::new();
    let mut x64: Vec<String> = Vec::new();
    let Ok(entries) = fs::read_dir(&path) else {
        return Detected::Unknown;
    };
    for entry in entries.flatten() {
        let p = entry.path();
        if !p.is_file() {
            continue;
        }
        if p.extension().and_then(|e| e.to_str()).map(|e| e.eq_ignore_ascii_case("exe")) != Some(true)
        {
            continue;
        }
        match engine::machine_of_file(&p).and_then(route_of) {
            Some(Route::X86) => x86.push(name_of(&p)),
            Some(Route::X64) => x64.push(name_of(&p)),
            None => {}
        }
    }

    match (x86.is_empty(), x64.is_empty()) {
        (true, true) => Detected::Unknown,
        (false, true) => Detected::Route(
            Route::X86,
            format!(
                "{} here {} 32-bit, so this is the bridge route.",
                joined(&x86),
                if x86.len() == 1 { "is" } else { "are" }
            ),
        ),
        (true, false) => Detected::Route(
            Route::X64,
            format!(
                "{} here {} 64-bit.",
                joined(&x64),
                if x64.len() == 1 { "is" } else { "are" }
            ),
        ),
        (false, false) => Detected::Mixed(format!(
            "Both widths are here: {} is 32-bit and {} is 64-bit. A 32-bit launcher beside a \
             64-bit game is normal -- pick the one the game actually runs as.",
            joined(&x86),
            joined(&x64)
        )),
    }
}

fn name_of(p: &Path) -> String {
    p.file_name().unwrap_or_default().to_string_lossy().to_string()
}

/// Three names at most: the point is to show the evidence, not to list a folder.
fn joined(names: &[String]) -> String {
    let shown: Vec<&str> = names.iter().take(3).map(|s| s.as_str()).collect();
    if names.len() > shown.len() {
        format!("{} and {} more", shown.join(", "), names.len() - shown.len())
    } else {
        shown.join(", ")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("dlss5-installer-test-{name}"));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn has_err(report: &Report, needle: &str) -> bool {
        report.lines.iter().any(|(l, t)| *l == Level::Err && t.contains(needle))
    }
    fn has_any(report: &Report, needle: &str) -> bool {
        report.lines.iter().any(|(_, t)| t.contains(needle))
    }

    #[test]
    fn addon_is_embedded_and_looks_like_a_dll() {
        assert!(ADDON.len() > 100_000, "add-on looks too small: {}", ADDON.len());
        assert_eq!(&ADDON[..2], b"MZ", "embedded add-on is not a PE image");
    }

    #[test]
    fn install_writes_the_addon_even_with_no_runtime_folder() {
        let game = temp("addon-only");
        let report = install(game.to_str().unwrap(), "", Preset::Dx11);
        assert!(!report.failed, "should not fail without a runtime folder");
        assert!(game.join(ADDON_NAME).is_file());
        assert_eq!(fs::read(game.join(ADDON_NAME)).unwrap().len(), ADDON.len());
        assert!(has_any(&report, "does nothing without them"));
    }

    #[test]
    fn a_runtime_with_the_wrong_hash_is_refused_and_not_copied() {
        let game = temp("bad-hash-game");
        let src = temp("bad-hash-src");
        fs::write(src.join(RUNTIME_NAME), b"not the runtime").unwrap();
        fs::write(src.join(WEIGHTS_NAME), b"not the weights").unwrap();

        let report = install(game.to_str().unwrap(), src.to_str().unwrap(), Preset::Dx11);
        assert!(report.failed);
        assert!(has_err(&report, "does not match the expected SHA-256"));
        assert!(!game.join(RUNTIME_NAME).exists(), "a rejected file must not be copied");
        assert!(!game.join(WEIGHTS_NAME).exists());
    }

    #[test]
    fn a_missing_runtime_file_is_named() {
        let game = temp("missing-game");
        let src = temp("missing-src");
        let report = install(game.to_str().unwrap(), src.to_str().unwrap(), Preset::Dx11);
        assert!(report.failed);
        assert!(has_err(&report, RUNTIME_NAME));
        assert!(has_err(&report, WEIGHTS_NAME));
    }

    #[test]
    fn the_old_per_pass_files_are_swept() {
        let game = temp("sweep");
        for n in 2..=10 {
            fs::write(game.join(format!("dlssnr_amd_pass{n}.dll")), b"x").unwrap();
        }
        let report = install(game.to_str().unwrap(), "", Preset::Dx11);
        assert!(has_any(&report, "old per-pass layout"));
        for n in 2..=10 {
            assert!(!game.join(format!("dlssnr_amd_pass{n}.dll")).exists(), "pass{n} survived");
        }
    }

    fn pe_bytes(x64: bool) -> Vec<u8> {
        let mut b = vec![0u8; 512];
        b[0] = 0x4d;
        b[1] = 0x5a;
        b[60] = 128;
        b[128] = 0x50;
        b[129] = 0x45;
        let machine: u16 = if x64 { 0x8664 } else { 0x14c };
        b[132] = (machine & 0xff) as u8;
        b[133] = (machine >> 8) as u8;
        let magic: u16 = if x64 { 0x20b } else { 0x10b };
        b[152] = (magic & 0xff) as u8;
        b[153] = (magic >> 8) as u8;
        b
    }

    /// The bridge route end to end against the real release, including the case Silent Hill 3
    /// actually is: a game that already ships its own d3d8.dll wrapper, which must survive.
    ///
    /// Set DLSS5_TEST_RELEASE_DIR to the release folder (the one holding files\ and
    /// payload.sha256). Skipped otherwise, because the payloads are private and 147 MB.
    #[test]
    fn a_real_x86_round_trip_through_the_release_folder() {
        let Ok(release) = std::env::var("DLSS5_TEST_RELEASE_DIR") else {
            eprintln!("skipped: set DLSS5_TEST_RELEASE_DIR to the x86 release folder");
            return;
        };
        let has_d3d8to9 = PathBuf::from(&release).join("files/d3d8to9.dll").is_file();

        let game = temp("x86-real");
        let exe = game.join("game.exe");
        fs::write(&exe, pe_bytes(false)).unwrap();
        // A maintained wrapper that forwards to d3d8R.dll, the shape the PC Fix has.
        let wrapper = {
            let mut b = pe_bytes(false);
            b.extend_from_slice(b"d3d8R.dll");
            b
        };
        if has_d3d8to9 {
            fs::write(game.join("d3d8.dll"), &wrapper).unwrap();
        }
        // ReShade has to already be there, since the public release carries no proxy.
        let reshade = PathBuf::from(&release).join("files/dxgi.dll");
        let preset = if has_d3d8to9 { Preset::X86Dx8 } else { Preset::X86Dx11 };
        if !reshade.is_file() && preset == Preset::X86Dx11 {
            eprintln!("skipped: no ReShade sidecar in the release folder");
            return;
        }

        let report = install(exe.to_str().unwrap(), &release, preset);
        assert!(!report.failed, "{}", report.to_log("x86 real"));

        assert!(game.join(ADDON32).is_file(), "the 32-bit frontend was not installed");
        assert!(game.join(HOST64).is_file(), "the 64-bit helper was not installed");
        let manifest = game.join(engine::MANIFEST_NAME);
        assert!(manifest.is_file(), "the bridge route must journal what it did");
        let m = engine::decode(&String::from_utf8(fs::read(&manifest).unwrap()).unwrap()).unwrap();
        assert_eq!(m.route, engine::Route::X86);

        if has_d3d8to9 {
            assert_eq!(
                fs::read(game.join("d3d8.dll")).unwrap(),
                wrapper,
                "the game's own wrapper must survive byte for byte"
            );
            assert!(game.join("d3d8R.dll").is_file(), "the translator goes beside it");
        }

        // Running it twice is what a person does when they are not sure it worked.
        let again = install(exe.to_str().unwrap(), &release, preset);
        assert!(!again.failed, "{}", again.to_log("x86 reinstall"));

        let removed = uninstall(exe.to_str().unwrap(), preset);
        assert!(!removed.failed, "{}", removed.to_log("x86 uninstall"));
        assert!(!game.join(ADDON32).exists(), "uninstall left the frontend behind");
        assert!(!game.join(HOST64).exists());
        if has_d3d8to9 {
            assert_eq!(
                fs::read(game.join("d3d8.dll")).unwrap(),
                wrapper,
                "uninstall must not touch the wrapper it never owned"
            );
            assert!(!game.join("d3d8R.dll").exists(), "the translator was ours to remove");
        }
    }

    /// The notes are wrapped across source lines with a trailing backslash, which is easy to lose
    /// in an edit -- and losing it bakes the indentation into the string, where it shows up as a
    /// run of spaces in the middle of a sentence on screen.
    #[test]
    fn no_preset_note_carries_the_indentation_of_its_own_source() {
        for p in Preset::ALL {
            let note = p.note();
            assert!(!note.contains("  "), "{:?} has a run of spaces in it: {note}", p);
            assert!(!note.contains('\n'), "{:?} has a hard line break; the pane wraps", p);
            assert!(note.len() > 40, "{:?} has no note worth showing", p);
        }
        for p in Preset::ALL {
            assert!(!p.label().is_empty() && !p.folder_label().trim().is_empty());
        }
    }

    /// One unpacked release folder has to serve both routes, or the download stops being one
    /// download. The bridge package keeps its payloads in `files\`; a folder someone unzipped the
    /// runtime into by itself keeps them loose. Both are field 1.
    #[test]
    fn field_one_finds_the_payloads_in_either_shape_of_folder() {
        let root = temp("payload-dir");

        // Loose, which is what this screen has always asked for.
        let loose = root.join("loose");
        fs::create_dir_all(&loose).unwrap();
        fs::write(loose.join(RUNTIME_NAME), b"stand-in").unwrap();
        assert_eq!(payload_dir(&loose), loose);

        // A release folder, where they sit under files/ next to the bridge pair.
        let release = root.join("release");
        fs::create_dir_all(release.join("files")).unwrap();
        fs::write(release.join("files").join(RUNTIME_NAME), b"stand-in").unwrap();
        assert_eq!(payload_dir(&release), release.join("files"));

        // Neither: the folder itself, so the caller reports what is missing by name.
        let empty = root.join("empty");
        fs::create_dir_all(&empty).unwrap();
        assert_eq!(payload_dir(&empty), empty);
    }

    #[test]
    fn an_executable_names_its_own_width() {
        let dir = temp("detect-exe");
        let x86 = dir.join("old-game.exe");
        let x64 = dir.join("new-game.exe");
        fs::write(&x86, pe_bytes(false)).unwrap();
        fs::write(&x64, pe_bytes(true)).unwrap();

        let d = detect(x86.to_str().unwrap());
        assert_eq!(d.route(), Some(Route::X86));
        assert!(d.line().unwrap().contains("old-game.exe"), "the evidence is named");

        assert_eq!(detect(x64.to_str().unwrap()).route(), Some(Route::X64));
        assert_eq!(detect("").route(), None);
    }

    #[test]
    fn a_folder_is_read_through_the_executables_in_it() {
        let dir = temp("detect-folder");
        fs::write(dir.join("game.exe"), pe_bytes(true)).unwrap();
        fs::write(dir.join("readme.txt"), b"not an executable").unwrap();
        assert_eq!(detect(dir.to_str().unwrap()).route(), Some(Route::X64));

        // A 32-bit launcher beside a 64-bit game is ordinary, and guessing between them would be
        // worse than saying so.
        fs::write(dir.join("launcher.exe"), pe_bytes(false)).unwrap();
        let mixed = detect(dir.to_str().unwrap());
        assert_eq!(mixed.route(), None);
        assert!(matches!(mixed, Detected::Mixed(_)));
        assert!(mixed.line().unwrap().contains("launcher.exe"));
    }

    #[test]
    fn a_folder_with_nothing_to_read_stays_unknown() {
        let dir = temp("detect-empty");
        assert_eq!(detect(dir.to_str().unwrap()), Detected::Unknown);
        fs::write(dir.join("notes.txt"), b"x").unwrap();
        assert_eq!(detect(dir.to_str().unwrap()), Detected::Unknown);
    }

    #[test]
    fn the_offered_presets_follow_the_detected_width() {
        let all = Preset::offered(&Detected::Unknown);
        assert_eq!(all.len(), Preset::ALL.len(), "nothing known yet offers everything");

        let x64 = Preset::offered(&Detected::Route(Route::X64, String::new()));
        assert!(x64.contains(&Preset::Pcsx2) && x64.contains(&Preset::Vulkan));
        assert!(
            !x64.iter().any(|p| p.route() == Route::X86),
            "a 64-bit target must not be offered the bridge presets"
        );

        let x86 = Preset::offered(&Detected::Route(Route::X86, String::new()));
        assert_eq!(x86, &[Preset::X86Dx11, Preset::X86Dx9, Preset::X86Dx8]);
        assert!(
            !x86.iter().any(|p| p.route() == Route::X64),
            "D3D12 and Vulkan have no 32-bit route at all"
        );
    }

    #[test]
    fn the_bridge_route_wants_the_executable_and_says_why() {
        let game = temp("x86-folder");
        fs::write(game.join("a.exe"), pe_bytes(false)).unwrap();
        fs::write(game.join("b.exe"), pe_bytes(false)).unwrap();
        let report = install(game.to_str().unwrap(), "", Preset::X86Dx9);
        assert!(report.failed);
        assert!(
            has_err(&report, "More than one 32-bit executable"),
            "{}",
            report.to_log("ambiguous")
        );

        // One candidate is unambiguous, so the folder is enough and the release folder is what is
        // missing next.
        fs::remove_file(game.join("b.exe")).unwrap();
        let report = install(game.to_str().unwrap(), "", Preset::X86Dx9);
        assert!(report.failed);
        assert!(has_err(&report, "payload.sha256") || has_err(&report, "unzipped the x86 release"),
            "{}", report.to_log("no release"));
    }

    #[test]
    fn a_64_bit_target_is_refused_by_the_bridge_route_before_anything_is_written() {
        let game = temp("x86-wrong-width");
        let exe = game.join("game64.exe");
        fs::write(&exe, pe_bytes(true)).unwrap();
        let release = temp("x86-wrong-width-release");
        fs::write(release.join("payload.sha256"), b"").unwrap();

        let report = install(exe.to_str().unwrap(), release.to_str().unwrap(), Preset::X86Dx11);
        assert!(report.failed);
        assert!(has_err(&report, "PE32/x86"), "{}", report.to_log("width"));
        assert!(!game.join(engine::MANIFEST_NAME).exists());
    }

    #[test]
    fn an_install_now_records_a_manifest_the_engine_can_read_back() {
        let game = temp("x64-manifest");
        let report = install(game.to_str().unwrap(), "", Preset::Dx12);
        assert!(!report.failed);

        let manifest = game.join(Route::X64.manifest_name());
        assert!(manifest.is_file(), "the x64 route must now journal what it did");
        assert_ne!(
            Route::X64.manifest_name(),
            engine::MANIFEST_NAME,
            "an x64 install must not drop the x86 bridge's filename into the folder"
        );

        let text = String::from_utf8(fs::read(&manifest).unwrap()).unwrap();
        let m = engine::decode(&text).expect("the manifest we just wrote must decode");
        assert_eq!(m.preset, "D3D12");
        assert_eq!(m.route, Route::X64);
        assert!(m.entries.iter().any(|e| e.name == ADDON_NAME && e.owned));
    }

    #[test]
    fn an_add_on_already_in_the_folder_is_backed_up_before_being_replaced() {
        let game = temp("x64-backup");
        // Somebody else's file under our name: it must be recoverable, not overwritten silently.
        fs::write(game.join(ADDON_NAME), b"a different add-on").unwrap();

        let report = install(game.to_str().unwrap(), "", Preset::Dx11);
        assert!(!report.failed, "{}", report.to_log("backup"));
        assert_eq!(fs::read(game.join(ADDON_NAME)).unwrap(), ADDON);

        let backups = game.join(engine::BACKUP_DIR);
        assert!(backups.is_dir(), "the displaced file must be kept");
        let found = walk(&backups);
        assert!(
            found.iter().any(|p| fs::read(p).unwrap() == b"a different add-on"),
            "the original bytes must be in the backup"
        );

        // And uninstall puts it back rather than deleting what was not ours to delete.
        let removed = uninstall(game.to_str().unwrap(), Preset::Dx11);
        assert!(!removed.failed, "{}", removed.to_log("restore"));
        assert_eq!(
            fs::read(game.join(ADDON_NAME)).unwrap(),
            b"a different add-on",
            "uninstall must restore the file the install displaced"
        );
    }

    fn walk(dir: &Path) -> Vec<PathBuf> {
        let mut out = Vec::new();
        if let Ok(entries) = fs::read_dir(dir) {
            for e in entries.flatten() {
                let p = e.path();
                if p.is_dir() {
                    out.extend(walk(&p));
                } else {
                    out.push(p);
                }
            }
        }
        out
    }

    #[test]
    fn the_x64_route_now_refuses_to_install_over_a_file_the_game_is_holding() {
        let game = temp("x64-locked");
        let held = game.join(ADDON_NAME);
        fs::write(&held, b"held open by the running game").unwrap();
        let mut perms = fs::metadata(&held).unwrap().permissions();
        perms.set_readonly(true);
        fs::set_permissions(&held, perms).unwrap();

        // Before step 3 this reached fs::write and came back with an OS error partway through.
        let report = install(game.to_str().unwrap(), "", Preset::Dx11);
        assert!(report.failed);
        assert!(
            has_err(&report, "open by another program"),
            "{}",
            report.to_log("locked")
        );
        assert!(!game.join(Route::X64.manifest_name()).exists());

        let mut perms = fs::metadata(&held).unwrap().permissions();
        #[allow(clippy::permissions_set_readonly_false)]
        perms.set_readonly(false);
        fs::set_permissions(&held, perms).unwrap();
        assert_eq!(fs::read(&held).unwrap(), b"held open by the running game");
    }

    #[test]
    fn an_install_from_before_the_manifest_existed_can_still_be_uninstalled() {
        let game = temp("x64-legacy");
        // Exactly what an older release left behind: our files, no manifest.
        fs::write(game.join(ADDON_NAME), ADDON).unwrap();
        fs::write(game.join(RUNTIME_NAME), b"old runtime").unwrap();
        fs::write(game.join(WEIGHTS_NAME), b"old weights").unwrap();
        fs::write(game.join("dlss5-neural.ini"), b"[dlss5]\nScale=0.5\n").unwrap();
        fs::create_dir_all(game.join("dlss5-runtime")).unwrap();
        assert!(!game.join(Route::X64.manifest_name()).exists());

        let report = uninstall(game.to_str().unwrap(), Preset::Dx11);
        assert!(!report.failed, "{}", report.to_log("legacy"));
        for name in [ADDON_NAME, RUNTIME_NAME, WEIGHTS_NAME] {
            assert!(!game.join(name).exists(), "{name} survived a legacy uninstall");
        }
        assert!(!game.join("dlss5-runtime").exists());
        assert!(game.join("dlss5-neural.ini").is_file(), "the ini is still the user's");
    }

    #[test]
    fn a_refused_payload_now_leaves_the_folder_untouched() {
        let game = temp("x64-atomic");
        let src = temp("x64-atomic-src");
        fs::write(src.join(RUNTIME_NAME), b"not the runtime").unwrap();
        fs::write(src.join(WEIGHTS_NAME), b"not the weights").unwrap();

        let report = install(game.to_str().unwrap(), src.to_str().unwrap(), Preset::Dx11);
        assert!(report.failed);
        // The add-on used to be written before the payloads were checked, so a refusal left it
        // behind on its own. Routing through the engine made the whole thing one transaction.
        assert!(!game.join(ADDON_NAME).exists(), "nothing may be written when a payload is refused");
        assert!(!game.join(Route::X64.manifest_name()).exists());
    }

    #[test]
    fn the_two_routes_do_not_mistake_each_other_for_the_same_install() {
        let game = temp("x64-crossroute");
        assert!(!install(game.to_str().unwrap(), "", Preset::Dx11).failed);

        // An x86 D3D11 install in the same folder must not look like a reinstall of the x64 one.
        let mut files: BTreeMap<String, Vec<u8>> = BTreeMap::new();
        files.insert("dlss5-neural.addon32".into(), b"x86 add-on".to_vec());
        let mut log = Vec::new();
        // Different manifest file, so it is a separate install rather than a silent merge.
        assert!(engine::apply(&game, "D3D11", Route::X86, &files, &mut log).is_ok());
        assert!(game.join(Route::X64.manifest_name()).is_file());
        assert!(game.join(engine::MANIFEST_NAME).is_file());
    }

    #[test]
    fn uninstall_takes_back_what_install_put_there_and_keeps_the_ini() {
        let game = temp("round-trip");
        install(game.to_str().unwrap(), "", Preset::Dx11);
        assert!(game.join(ADDON_NAME).is_file());

        // Tuning the user owns, plus a folder the add-on writes by itself.
        fs::write(game.join("dlss5-neural.ini"), b"[dlss5]\nScale=0.5\n").unwrap();
        fs::create_dir_all(game.join("dlss5-runtime")).unwrap();
        fs::write(game.join("dlss5-runtime").join("d3d12.dll"), b"x").unwrap();

        let report = uninstall(game.to_str().unwrap(), Preset::Dx11);
        assert!(!report.failed);
        assert!(!game.join(ADDON_NAME).exists());
        assert!(!game.join("dlss5-runtime").exists());
        assert!(game.join("dlss5-neural.ini").is_file(), "the ini is the user's, not ours");
    }

    #[test]
    fn uninstall_on_an_unrelated_folder_says_so_rather_than_failing() {
        let game = temp("empty");
        let report = uninstall(game.to_str().unwrap(), Preset::Dx11);
        assert!(!report.failed);
        assert!(has_any(&report, "Nothing of ours"));
    }

    #[test]
    fn a_quoted_path_is_accepted_because_windows_copies_them_that_way() {
        let game = temp("quoted");
        let quoted = format!("\"{}\"", game.display());
        let report = install(&quoted, "", Preset::Dx11);
        assert!(!report.failed, "a path with quotes around it should still work");
        assert!(game.join(ADDON_NAME).is_file());
    }

    #[test]
    fn a_path_that_is_not_a_folder_is_reported_not_ignored() {
        let report = install(r"Z:\definitely\not\here", "", Preset::Dx11);
        assert!(report.failed);
        assert!(has_err(&report, "is not a folder"));
    }

    #[test]
    fn preflight_asks_for_the_files_first_and_then_the_folder() {
        let empty = preflight("", "", Preset::Dx11);
        assert!(has_any(&empty, "Waiting for field 1"));
        assert!(!empty.failed, "an empty form is not an error");

        let game = temp("preflight-order");
        let half = preflight(game.to_str().unwrap(), "", Preset::Dx11);
        assert!(has_any(&half, "Waiting for field 1"));
    }

    #[test]
    fn preflight_names_a_wrong_sized_runtime_without_hashing_it() {
        let src = temp("preflight-wrong-size");
        fs::write(src.join(RUNTIME_NAME), b"far too small").unwrap();
        fs::write(src.join(WEIGHTS_NAME), vec![0u8; 32]).unwrap();

        let report = preflight("", src.to_str().unwrap(), Preset::Dx11);
        assert!(report.failed);
        assert!(has_err(&report, "different build"), "{}", report.to_log("size"));
    }

    #[test]
    fn preflight_spots_a_missing_file_in_the_runtime_folder() {
        let src = temp("preflight-missing");
        fs::write(src.join(RUNTIME_NAME), vec![0u8; RUNTIME_SIZE as usize]).unwrap();
        let report = preflight("", src.to_str().unwrap(), Preset::Dx11);
        assert!(has_err(&report, "dlssnr_on_amd_weights.bin is not in that folder"));
    }

    /// The failure nobody can diagnose from the game: ReShade quietly refusing to load the add-on
    /// because its ini still carries a DisabledAddons line from an old untick.
    #[test]
    fn preflight_finds_the_disabled_addons_line() {
        let game = temp("preflight-disabled");
        fs::write(game.join("d3d11.dll"), b"reshade").unwrap();
        fs::write(
            game.join("ReShade.ini"),
            b"[ADDON]\nDisabledAddons=dlss5 neural@dlss5-neural.addon64\n",
        )
        .unwrap();
        let report = preflight(game.to_str().unwrap(), "", Preset::Dx11);
        assert!(report.failed);
        assert!(has_err(&report, "DisabledAddons"), "{}", report.to_log("ini"));

        // An unrelated add-on being disabled is worth saying, but it is not a problem.
        fs::write(game.join("ReShade.ini"), b"[ADDON]\nDisabledAddons=SomeOther.addon64\n").unwrap();
        let clean = preflight(game.to_str().unwrap(), "", Preset::Dx11);
        assert!(!clean.failed, "{}", clean.to_log("ini"));
        assert!(has_any(&clean, "disables other add-ons"));
    }

    #[test]
    fn preflight_reports_a_file_another_program_is_holding_open() {
        let game = temp("preflight-locked");
        fs::write(game.join(ADDON_NAME), b"in place").unwrap();
        // An exclusive handle is what a running game looks like from out here.
        use std::os::windows::fs::OpenOptionsExt as _;
        let held = fs::OpenOptions::new().write(true).share_mode(0).open(game.join(ADDON_NAME));
        assert!(held.is_ok(), "could not take an exclusive handle to set the test up");

        let report = preflight(game.to_str().unwrap(), "", Preset::Dx11);
        assert!(report.failed);
        assert!(has_err(&report, "still running"), "{}", report.to_log("locked"));

        drop(held);
        let after = preflight(game.to_str().unwrap(), "", Preset::Dx11);
        assert!(!has_err(&after, "still running"), "closing it should clear the line");
    }

    #[test]
    fn preflight_is_quiet_when_there_is_genuinely_nothing_wrong() {
        let src = temp("preflight-clean-src");
        fs::write(src.join(RUNTIME_NAME), vec![0u8; RUNTIME_SIZE as usize]).unwrap();
        fs::write(src.join(WEIGHTS_NAME), vec![0u8; WEIGHTS_SIZE as usize]).unwrap();
        let game = temp("preflight-clean-game");
        fs::write(game.join("d3d11.dll"), b"reshade").unwrap();

        let report = preflight(game.to_str().unwrap(), src.to_str().unwrap(), Preset::Dx11);
        assert!(!report.failed, "{}", report.to_log("clean"));
        assert!(has_any(&report, "Nothing in the way"));
    }

    #[test]
    fn uninstall_with_no_folder_says_which_field_is_empty() {
        let report = uninstall("", Preset::Dx11);
        assert!(report.failed);
        assert!(has_err(&report, "No game folder given"));
    }

    #[test]
    fn every_preset_has_a_label_a_folder_word_and_a_note() {
        for p in Preset::ALL {
            assert!(!p.label().is_empty());
            assert!(p.folder_label().contains("folder"), "{}", p.label());
            assert!(p.note().len() > 40, "{} has no real note", p.label());
        }
        // Two entries carrying the same name is the kind of thing a copy-pasted arm produces, and
        // on screen it just looks like a preset that will not select.
        let mut labels: Vec<&str> = Preset::ALL.iter().map(|p| p.label()).collect();
        labels.sort_unstable();
        let count = labels.len();
        labels.dedup();
        assert_eq!(labels.len(), count, "two presets share a label");
    }

    #[test]
    fn the_vulkan_presets_expect_a_layer_and_the_d3d_ones_expect_a_proxy_dll() {
        let dir = temp("vulkan-clean");
        for p in [Preset::Vulkan, Preset::Rpcs3] {
            let report = install(dir.to_str().unwrap(), "", p);
            assert!(has_any(&report, "global layer"), "{} said the wrong thing", p.label());
            assert!(!has_any(&report, "No ReShade proxy DLL found"), "{}", p.label());
        }
        for p in [Preset::Dx11, Preset::Dx12] {
            let report = install(dir.to_str().unwrap(), "", p);
            assert!(has_any(&report, "No ReShade proxy DLL found"), "{}", p.label());
        }
        // With ReShade actually present the D3D route is happy and the Vulkan route objects.
        fs::write(dir.join("d3d11.dll"), b"not really reshade").unwrap();
        assert!(has_any(&install(dir.to_str().unwrap(), "", Preset::Dx11), "ReShade found"));
        assert!(has_any(&install(dir.to_str().unwrap(), "", Preset::Vulkan), "two ReShade"));
    }

    /// The one thing made-up bytes can never check: that the two SHA-256 constants this binary
    /// refuses everything else against are the hashes of the files people are actually given. If
    /// a release bumps the runtime and nobody bumps the constant, every other test still passes
    /// and every user gets "does not match the expected SHA-256".
    ///
    /// Needs a folder holding both files, named by `DLSS5_TEST_RUNTIME_DIR`. A clean clone has no
    /// such folder, so without it this reports that it was skipped instead of failing.
    #[test]
    fn a_real_install_round_trip_against_the_shipped_hashes() {
        let Ok(raw) = std::env::var("DLSS5_TEST_RUNTIME_DIR") else {
            eprintln!("skipped: set DLSS5_TEST_RUNTIME_DIR to a folder holding the two files");
            return;
        };
        let src = PathBuf::from(&raw);
        assert!(src.join(RUNTIME_NAME).is_file(), "{RUNTIME_NAME} is not in {raw}");
        assert!(src.join(WEIGHTS_NAME).is_file(), "{WEIGHTS_NAME} is not in {raw}");

        let game = temp("real-round-trip");
        fs::write(game.join("dxgi.dll"), b"stand-in for ReShade").unwrap();
        // Tuning that has to survive, and junk from the old layout that has to not.
        fs::write(game.join("dlss5-neural.ini"), b"StartOn=1\n").unwrap();
        fs::write(game.join("dlssnr_amd_pass4.dll"), b"dead").unwrap();

        let report = install(game.to_str().unwrap(), &raw, Preset::Dx11);
        assert!(!report.failed, "install failed:\n{}", report.to_log("real files"));
        for name in [ADDON_NAME, RUNTIME_NAME, WEIGHTS_NAME] {
            assert!(game.join(name).is_file(), "{name} was not installed");
        }
        assert_eq!(sha256(&game.join(RUNTIME_NAME)).unwrap(), RUNTIME_SHA);
        assert_eq!(sha256(&game.join(WEIGHTS_NAME)).unwrap(), WEIGHTS_SHA);
        assert!(!game.join("dlssnr_amd_pass4.dll").exists(), "the old per-pass file survived");

        // Running it twice is what a person does when they are not sure it worked.
        let again = install(game.to_str().unwrap(), &raw, Preset::Dx11);
        assert!(!again.failed);
        assert!(has_any(&again, "already correct, left alone"));

        let removed = uninstall(game.to_str().unwrap(), Preset::Dx11);
        assert!(!removed.failed, "uninstall failed:\n{}", removed.to_log("real files"));
        for name in [ADDON_NAME, RUNTIME_NAME, WEIGHTS_NAME] {
            assert!(!game.join(name).exists(), "{name} was left behind");
        }
        assert!(game.join("dlss5-neural.ini").is_file(), "the user's tuning was deleted");
        assert!(game.join("dxgi.dll").is_file(), "ReShade was touched, and it must not be");
    }
}
