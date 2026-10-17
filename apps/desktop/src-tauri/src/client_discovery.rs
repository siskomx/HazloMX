//! Discover installed clients without invoking shell shims or reading credentials.
use std::path::{Path, PathBuf};

fn candidates(
    path: &std::ffi::OsStr,
    appdata: Option<&Path>,
    home: Option<&Path>,
    name: &str,
) -> Vec<PathBuf> {
    let mut paths = Vec::new();
    for folder in std::env::split_paths(path).filter(|p| p.is_absolute()) {
        paths.push(folder.join(format!("{name}.exe")));
        if name == "codex" {
            paths.push(folder.join("node_modules/@openai/codex/node_modules/@openai/codex-win32-x64/vendor/x86_64-pc-windows-msvc/bin/codex.exe"));
        }
    }
    if name == "codex" {
        if let Some(folder) = appdata {
            paths.push(folder.join("npm/node_modules/@openai/codex/node_modules/@openai/codex-win32-x64/vendor/x86_64-pc-windows-msvc/bin/codex.exe"));
        }
    } else if let Some(folder) = home {
        paths.push(folder.join(".local/bin/claude.exe"));
    }
    let mut unique = Vec::<PathBuf>::new();
    for candidate in paths {
        if !unique
            .iter()
            .any(|p| p.as_os_str().eq_ignore_ascii_case(candidate.as_os_str()))
        {
            unique.push(candidate);
        }
    }
    unique
}

pub fn discover() -> (
    Option<crate::codex::Runtime>,
    Option<crate::claude::Runtime>,
) {
    let path = std::env::var_os("PATH").unwrap_or_default();
    let appdata = std::env::var_os("APPDATA").map(PathBuf::from);
    let home = std::env::var_os("USERPROFILE").map(PathBuf::from);
    let codex = candidates(&path, appdata.as_deref(), home.as_deref(), "codex")
        .into_iter()
        .filter(|p| p.is_file())
        .take(8)
        .find_map(|p| crate::codex::identify(p).ok());
    let claude = candidates(&path, appdata.as_deref(), home.as_deref(), "claude")
        .into_iter()
        .filter(|p| p.is_file())
        .take(8)
        .find_map(|p| crate::claude::identify(p).ok());
    (codex, claude)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn finds_native_npm_binary_and_deduplicates_without_shell_or_current_directory() {
        let paths = candidates(
            std::ffi::OsStr::new(".;C:\\npm;C:\\NPM;relative"),
            Some(Path::new("C:/profile/AppData/Roaming")),
            None,
            "codex",
        );
        assert!(paths
            .iter()
            .all(|p| p.is_absolute() && p.extension().is_some_and(|e| e == "exe")));
        assert_eq!(paths.len(), 3);
        assert!(paths[1].ends_with("vendor/x86_64-pc-windows-msvc/bin/codex.exe"));
        let claude = candidates(
            std::ffi::OsStr::new(""),
            None,
            Some(Path::new("C:/profile")),
            "claude",
        );
        assert_eq!(claude, [PathBuf::from("C:/profile/.local/bin/claude.exe")]);
    }
}
