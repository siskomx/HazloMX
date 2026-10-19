//! Upgrade only recognized companion bytes; retain the predecessor outside the plugin loader.
use crate::project::{atomic_json, pin_directory, pin_lineage, read_json, rename_handle};
use serde::{Deserialize, Serialize};
use std::{
    fs, io,
    path::{Path, PathBuf},
};
use uuid::Uuid;

const REFUSED:&str="El complemento o su preparación cambió. Se conservó su contenido; revisa la instalación antes de continuar.";
const PENDING:&str="La actualización local del complemento sigue pendiente. Se conservó la versión anterior; vuelve a preparar el complemento cuando la carpeta esté disponible.";
// Full three-file manifests from the prior owned companion revisions, never a name/version assertion.
const PREVIOUS: [[&str; 3]; 3] = [
    [
        "96b95465d63ebe8d258d97fca0b4bb4f519c3c53ada646629ebf4633c8b2dd0a",
        "41b43a52692594d04c7697081c046d0ded99d5138f3f1b214656e84de5fdbebb",
        "5c13cf6c7149a9ec45ef2c9d3bccf395e1313aa32eaf81a36828cfa3d1ba5f09",
    ],
    [
        "96b95465d63ebe8d258d97fca0b4bb4f519c3c53ada646629ebf4633c8b2dd0a",
        "41b43a52692594d04c7697081c046d0ded99d5138f3f1b214656e84de5fdbebb",
        "d46f81943beca49cca71988875036d58ed3013e1d7a6eb01108e60dcc671b612",
    ],
    [
        "96b95465d63ebe8d258d97fca0b4bb4f519c3c53ada646629ebf4633c8b2dd0a",
        "4e636e9355166b1c7350ec17358783c4e3af4ae332f1ad2231d7dacda433e80d",
        "c49b4790051d074308b223d9fa92918e6ab674ea2be8852baeee4b1b3ab2ced4",
    ],
];
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Upgrade {
    version: u32,
    id: Uuid,
    previous: [String; 3],
    desired: [String; 3],
}
impl Upgrade {
    fn stage(&self, profile: &Path) -> PathBuf {
        profile.join(format!(".hazlomexa-editor-update-{}", self.id))
    }
    fn backup(&self, profile: &Path) -> PathBuf {
        profile.join(format!(".hazlomexa-editor-backup-{}", self.id))
    }
}
fn manifest(directory: &Path, files: &[(&str, &[u8]); 3]) -> io::Result<[String; 3]> {
    if fs::read_dir(directory)?.count() != 3 {
        return Err(io::Error::other(REFUSED));
    }
    let mut hashes = Vec::new();
    for (name, _) in files {
        let bytes = crate::kicad_project::read_regular(&directory.join(name), 1024 * 1024)
            .map_err(io::Error::other)?;
        hashes.push(crate::kicad_project::digest(&bytes));
    }
    hashes.try_into().map_err(|_| io::Error::other(REFUSED))
}
fn inspect(directory: &Path, files: &[(&str, &[u8]); 3]) -> io::Result<Option<[String; 3]>> {
    if !directory.try_exists()? {
        return Ok(None);
    }
    let _pin = pin_directory(directory, false)?;
    manifest(directory, files).map(Some)
}
pub(crate) fn upgrade(profile: &Path, files: &[(&str, &[u8]); 3]) -> Result<bool, String> {
    prepare(profile, files, &PREVIOUS)
}
fn prepare(
    profile: &Path,
    files: &[(&str, &[u8]); 3],
    previous: &[[&str; 3]],
) -> Result<bool, String> {
    let _pins = pin_lineage(profile).map_err(|_| REFUSED)?;
    let plugins = profile.join("plugins");
    let _plugins = pin_directory(&plugins, false).map_err(|_| REFUSED)?;
    let desired = files.map(|(_, bytes)| crate::kicad_project::digest(bytes));
    let target = plugins.join("hazlomexa-arduino-editor");
    let journal = profile.join(".hazlomexa-editor-update.json");
    let plan: Upgrade = if journal.try_exists().map_err(|_| PENDING)? {
        read_json(&journal).map_err(|_| REFUSED)?
    } else {
        let installed = inspect(&target, files)
            .map_err(|_| REFUSED)?
            .ok_or(REFUSED)?;
        if installed == desired {
            return Ok(false);
        }
        if !previous
            .iter()
            .any(|known| *known == installed.each_ref().map(String::as_str))
        {
            return Err(REFUSED.into());
        }
        let plan = Upgrade {
            version: 1,
            id: Uuid::new_v4(),
            previous: installed,
            desired: desired.clone(),
        };
        atomic_json(&journal, &plan, false).map_err(|_| PENDING)?;
        plan
    };
    if plan.version != 1
        || plan.id.is_nil()
        || plan.desired != desired
        || !previous
            .iter()
            .any(|known| *known == plan.previous.each_ref().map(String::as_str))
    {
        return Err(REFUSED.into());
    }
    let stage = plan.stage(profile);
    let backup = plan.backup(profile);
    let installed = inspect(&target, files).map_err(|_| REFUSED)?;
    let retained = inspect(&backup, files).map_err(|_| REFUSED)?;
    if installed.as_ref() == Some(&desired) {
        if retained.as_ref() != Some(&plan.previous) || stage.try_exists().map_err(|_| REFUSED)? {
            return Err(REFUSED.into());
        }
        fs::remove_file(journal).map_err(|_| PENDING)?;
        return Ok(true);
    }
    if installed
        .as_ref()
        .is_some_and(|hashes| hashes != &plan.previous)
        || retained
            .as_ref()
            .is_some_and(|hashes| hashes != &plan.previous)
        || installed.is_some() == retained.is_some()
    {
        return Err(REFUSED.into());
    }
    match fs::create_dir(&stage) {
        Ok(()) => (),
        Err(e) if e.kind() == io::ErrorKind::AlreadyExists => (),
        Err(_) => return Err(PENDING.into()),
    }
    {
        let _stage = pin_directory(&stage, false).map_err(|_| REFUSED)?;
        for entry in fs::read_dir(&stage).map_err(|_| PENDING)? {
            let entry = entry.map_err(|_| PENDING)?;
            if !files.iter().any(|(name, _)| entry.file_name() == *name) {
                return Err(REFUSED.into());
            }
        }
        for (name, bytes) in files {
            let path = stage.join(name);
            if path.try_exists().map_err(|_| PENDING)? {
                if crate::kicad_project::read_regular(&path, bytes.len() as u64)
                    .map_err(|_| REFUSED)?
                    != *bytes
                {
                    return Err(REFUSED.into());
                }
            } else {
                crate::kicad_project::write_new(&path, bytes).map_err(|_| PENDING)?;
            }
        }
    }
    if installed.is_some() {
        let old = pin_directory(&target, true).map_err(|_| PENDING)?;
        if manifest(&target, files).map_err(|_| REFUSED)? != plan.previous {
            return Err(REFUSED.into());
        }
        rename_handle(&old, &backup, false).map_err(|_| PENDING)?;
        if manifest(&backup, files).map_err(|_| REFUSED)? != plan.previous {
            return Err(REFUSED.into());
        }
    }
    let ready = pin_directory(&stage, true).map_err(|_| PENDING)?;
    if manifest(&stage, files).map_err(|_| REFUSED)? != desired {
        return Err(REFUSED.into());
    }
    rename_handle(&ready, &target, false).map_err(|_| PENDING)?;
    drop(ready);
    if inspect(&target, files).map_err(|_| REFUSED)? != Some(desired)
        || inspect(&backup, files).map_err(|_| REFUSED)? != Some(plan.previous)
    {
        return Err(REFUSED.into());
    }
    fs::remove_file(journal).map_err(|_| PENDING)?;
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::windows::fs::OpenOptionsExt;
    const OLD: [(&str, &[u8]); 3] = [
        ("package.json", b"old manifest"),
        ("extension.cjs", b"old entry"),
        ("core.cjs", b"old core"),
    ];
    const NEW: [(&str, &[u8]); 3] = [
        ("package.json", b"new manifest"),
        ("extension.cjs", b"new entry"),
        ("core.cjs", b"new core"),
    ];
    fn known() -> [String; 3] {
        OLD.map(|(_, bytes)| crate::kicad_project::digest(bytes))
    }
    fn install(path: &Path, files: &[(&str, &[u8]); 3]) {
        fs::create_dir_all(path).unwrap();
        for (name, bytes) in files {
            fs::write(path.join(name), bytes).unwrap();
        }
    }
    fn setup() -> (PathBuf, PathBuf) {
        let root = std::env::temp_dir().join(format!("hazlo-companion-update-{}", Uuid::new_v4()));
        let target = root.join("plugins/hazlomexa-arduino-editor");
        install(&target, &OLD);
        assert!(root
            .canonicalize()
            .unwrap()
            .starts_with(std::env::temp_dir().canonicalize().unwrap()));
        fs::write(
            root.join("manual-sketch.ino"),
            b"preserve unsaved work fixture",
        )
        .unwrap();
        (root, target)
    }
    fn prepare_known(root: &Path) -> Result<bool, String> {
        let old = known();
        prepare(root, &NEW, &[old.each_ref().map(String::as_str)])
    }
    #[test]
    fn a_locked_child_file_keeps_the_prior_plugin_intact_until_upgrade_can_resume() {
        let (root, target) = setup();
        let file = fs::OpenOptions::new()
            .read(true)
            .share_mode(windows_sys::Win32::Storage::FileSystem::FILE_SHARE_READ)
            .open(target.join("core.cjs"))
            .unwrap();
        assert!(prepare_known(&root).is_err());
        assert_eq!(manifest(&target, &OLD).unwrap(), known());
        let saved: Upgrade = read_json(&root.join(".hazlomexa-editor-update.json")).unwrap();
        assert!(!saved.backup(&root).exists());
        drop(file);
        assert!(prepare_known(&root).unwrap());
        assert_eq!(manifest(&saved.backup(&root), &OLD).unwrap(), known());
        fs::remove_dir_all(root).unwrap();
    }
    fn plan(root: &Path) -> Upgrade {
        let plan = Upgrade {
            version: 1,
            id: Uuid::new_v4(),
            previous: known(),
            desired: NEW.map(|(_, b)| crate::kicad_project::digest(b)),
        };
        atomic_json(&root.join(".hazlomexa-editor-update.json"), &plan, false).unwrap();
        plan
    }
    #[test]
    fn known_upgrade_retains_previous_bytes_and_never_replaces_unknown_contents() {
        let (root, target) = setup();
        assert!(prepare_known(&root).unwrap());
        assert!(!prepare_known(&root).unwrap());
        let backups = fs::read_dir(&root)
            .unwrap()
            .flatten()
            .filter(|e| {
                e.file_name()
                    .to_string_lossy()
                    .starts_with(".hazlomexa-editor-backup-")
            })
            .collect::<Vec<_>>();
        assert_eq!(backups.len(), 1);
        assert_eq!(manifest(&backups[0].path(), &OLD).unwrap(), known());
        assert_eq!(
            manifest(&target, &NEW).unwrap(),
            NEW.map(|(_, b)| crate::kicad_project::digest(b))
        );
        assert_eq!(
            fs::read(root.join("manual-sketch.ino")).unwrap(),
            b"preserve unsaved work fixture"
        );
        fs::remove_dir_all(root).unwrap();
        for extra in [false, true] {
            let (root, target) = setup();
            let path = target.join(if extra { "user-extra.js" } else { "core.cjs" });
            fs::write(&path, b"manual plugin changes").unwrap();
            assert!(prepare_known(&root).is_err());
            assert_eq!(fs::read(&path).unwrap(), b"manual plugin changes");
            assert!(!root.join(".hazlomexa-editor-update.json").exists());
            fs::remove_dir_all(root).unwrap();
        }
    }
    #[test]
    fn locked_plugin_preserves_original_and_reuses_the_reserved_upgrade() {
        let (root, target) = setup();
        let lock = pin_directory(&target, false).unwrap();
        assert!(prepare_known(&root).is_err());
        let saved: Upgrade = read_json(&root.join(".hazlomexa-editor-update.json")).unwrap();
        assert_eq!(manifest(&target, &OLD).unwrap(), known());
        assert!(!saved.backup(&root).exists());
        drop(lock);
        assert!(prepare_known(&root).unwrap());
        assert_eq!(manifest(&saved.backup(&root), &OLD).unwrap(), known());
        assert!(!saved.stage(&root).exists());
        assert!(!root.join(".hazlomexa-editor-update.json").exists());
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn interrupted_upgrade_recovers_forward_without_replacing_retained_files() {
        for mode in ["partial_stage", "old_moved", "published", "locked_journal"] {
            let (root, target) = setup();
            let plan = plan(&root);
            let stage = plan.stage(&root);
            let backup = plan.backup(&root);
            if mode == "partial_stage" {
                fs::create_dir(&stage).unwrap();
                fs::write(stage.join(NEW[0].0), NEW[0].1).unwrap();
            } else {
                install(&stage, &NEW);
            }
            if matches!(mode, "old_moved" | "published") {
                fs::rename(&target, &backup).unwrap();
            }
            if mode == "published" {
                fs::rename(&stage, &target).unwrap();
            }
            if mode == "locked_journal" {
                let lock = fs::OpenOptions::new()
                    .read(true)
                    .share_mode(windows_sys::Win32::Storage::FileSystem::FILE_SHARE_READ)
                    .open(root.join(".hazlomexa-editor-update.json"))
                    .unwrap();
                assert!(prepare_known(&root).is_err());
                assert_eq!(manifest(&target, &NEW).unwrap(), plan.desired);
                assert_eq!(manifest(&backup, &OLD).unwrap(), plan.previous);
                drop(lock);
            }
            assert!(prepare_known(&root).unwrap());
            assert_eq!(manifest(&target, &NEW).unwrap(), plan.desired);
            assert_eq!(manifest(&backup, &OLD).unwrap(), plan.previous);
            assert!(!root.join(".hazlomexa-editor-update.json").exists());
            assert!(!prepare_known(&root).unwrap());
            fs::remove_dir_all(root).unwrap();
        }
    }
    #[test]
    fn changed_preparation_and_backups_are_preserved_instead_of_overwritten() {
        for mode in ["stage", "backup", "journal"] {
            let (root, target) = setup();
            let mut plan = plan(&root);
            let stage = plan.stage(&root);
            let backup = plan.backup(&root);
            let changed = if mode == "stage" {
                install(&stage, &NEW);
                let path = stage.join("core.cjs");
                fs::write(&path, b"user preparation changes").unwrap();
                path
            } else if mode == "backup" {
                fs::rename(&target, &backup).unwrap();
                let path = backup.join("core.cjs");
                fs::write(&path, b"user preparation changes").unwrap();
                path
            } else {
                plan.desired[0] = "a".repeat(64);
                let path = root.join(".hazlomexa-editor-update.json");
                atomic_json(&path, &plan, true).unwrap();
                path
            };
            let before = fs::read(&changed).unwrap();
            assert!(prepare_known(&root).is_err());
            assert_eq!(fs::read(&changed).unwrap(), before);
            if mode != "backup" {
                assert_eq!(manifest(&target, &OLD).unwrap(), known());
            }
            fs::remove_dir_all(root).unwrap();
        }
    }
    #[test]
    #[ignore = "Requires the local repository history for prior owned companion bytes"]
    fn historical_owned_companions_upgrade_without_losing_the_previous_version() {
        for (revision, expected) in ["36c9c6b", "f1694ab", "f1f76de"].into_iter().zip(PREVIOUS) {
            let root =
                std::env::temp_dir().join(format!("hazlo-historical-companion-{}", Uuid::new_v4()));
            let target = root.join("plugins/hazlomexa-arduino-editor");
            fs::create_dir_all(&target).unwrap();
            assert!(root
                .canonicalize()
                .unwrap()
                .starts_with(std::env::temp_dir().canonicalize().unwrap()));
            for (name, _) in crate::agent_editor::COMPANION_FILES {
                let output = std::process::Command::new("git")
                    .args([
                        "show",
                        &format!(
                            "{revision}:apps/desktop/src-tauri/resources/arduino-editor/{name}"
                        ),
                    ])
                    .output()
                    .unwrap();
                assert!(output.status.success());
                fs::write(target.join(name), output.stdout).unwrap();
            }
            assert_eq!(
                manifest(&target, &crate::agent_editor::COMPANION_FILES)
                    .unwrap()
                    .each_ref()
                    .map(String::as_str),
                expected
            );
            assert!(crate::agent_editor::install_companion(&root).unwrap());
            assert!(!crate::agent_editor::install_companion(&root).unwrap());
            for (name, bytes) in crate::agent_editor::COMPANION_FILES {
                assert_eq!(fs::read(target.join(name)).unwrap(), bytes);
            }
            let backup = fs::read_dir(&root)
                .unwrap()
                .flatten()
                .find(|e| {
                    e.file_name()
                        .to_string_lossy()
                        .starts_with(".hazlomexa-editor-backup-")
                })
                .unwrap();
            assert_eq!(
                manifest(&backup.path(), &crate::agent_editor::COMPANION_FILES)
                    .unwrap()
                    .each_ref()
                    .map(String::as_str),
                expected
            );
            fs::remove_dir_all(root).unwrap();
        }
    }
}
