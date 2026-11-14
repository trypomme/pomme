//! Finding SF Pro, iOS's text font, on the machine pomme runs on. pomme
//! can't ship it (Apple licenses it only to be installed by whoever accepts
//! its terms), so a host looks for a copy the user has: macOS's own system
//! font, or SF Pro installed from developer.apple.com/fonts on macOS, Linux
//! or Windows (its variable `SF-Pro.ttf`). Without one, text is in Inter, the
//! sysroot's stand-in. Not in the browser, which can't read the machine's fonts.

use std::path::{Path, PathBuf};

/// SF Pro's variable font as Apple's download installs it.
const INSTALLED: &[&str] = &["SF-Pro.ttf", "SF Pro.ttf"];

/// Where each platform keeps fonts, the user's own first.
fn font_dirs() -> Vec<PathBuf> {
    let home = std::env::var_os("HOME").map(PathBuf::from);
    let mut dirs = Vec::new();
    if cfg!(target_os = "macos") {
        dirs.extend(home.iter().map(|h| h.join("Library/Fonts")));
        dirs.push("/Library/Fonts".into());
    } else if cfg!(windows) {
        dirs.extend(std::env::var_os("LOCALAPPDATA").map(|d| PathBuf::from(d).join("Microsoft/Windows/Fonts")));
        dirs.push(std::env::var_os("WINDIR").map_or("C:/Windows".into(), PathBuf::from).join("Fonts"));
    } else {
        dirs.extend(std::env::var_os("XDG_DATA_HOME").map(|d| PathBuf::from(d).join("fonts")));
        dirs.extend(home.iter().flat_map(|h| [h.join(".local/share/fonts"), h.join(".fonts")]));
        dirs.extend(["/usr/local/share/fonts".into(), "/usr/share/fonts".into()]);
    }
    dirs
}

/// `name` in `dir` or the folders under it (a few levels: Linux files fonts
/// by family).
fn find_in(dir: &Path, name: &str, depth: usize) -> Option<PathBuf> {
    let direct = dir.join(name);
    if direct.is_file() {
        return Some(direct);
    }
    if depth == 0 {
        return None;
    }
    let mut subdirs: Vec<PathBuf> = std::fs::read_dir(dir).ok()?.flatten().map(|e| e.path()).filter(|p| p.is_dir()).collect();
    subdirs.sort();
    subdirs.iter().find_map(|d| find_in(d, name, depth - 1))
}

/// SF Pro on this machine: an installed SF Pro (Apple's download), else
/// macOS's system font (SF Pro by another name).
pub fn find_sf() -> Option<PathBuf> {
    for dir in font_dirs() {
        for name in INSTALLED {
            if let Some(p) = find_in(&dir, name, 3) {
                return Some(p);
            }
        }
    }
    let system = Path::new("/System/Library/Fonts/SFNS.ttf");
    (cfg!(target_os = "macos") && system.is_file()).then(|| system.to_path_buf())
}
