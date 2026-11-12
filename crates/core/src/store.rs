use std::{
    collections::BTreeSet,
    fs::{self, File, OpenOptions},
    io::{self, Read, Write},
    os::unix::fs::OpenOptionsExt,
    path::{Path, PathBuf},
};
use uuid::Uuid;

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct NoteName(String);
impl NoteName {
    pub fn as_str(&self) -> &str {
        &self.0
    }
    fn parse(name: &str) -> Option<Self> {
        (!name.is_empty() && name.ends_with(".md") && !name.contains(['/', '\\']) && name != ".md")
            .then(|| Self(name.into()))
    }
}
#[derive(Clone, Debug)]
pub struct Summary {
    pub name: NoteName,
    pub title: String,
    pub excerpt: String,
    pub source: String,
    pub recovered: bool,
}
impl Summary {
    pub fn from_source(name: NoteName, source: &str, recovered: bool) -> Self {
        Self {
            title: crate::title(source, name.as_str()),
            excerpt: source
                .trim_start_matches('\u{feff}')
                .split_whitespace()
                .collect::<Vec<_>>()
                .join(" ")
                .chars()
                .take(120)
                .collect(),
            source: source.into(),
            name,
            recovered,
        }
    }
    pub fn matches(&self, query: &str) -> bool {
        let query = query.trim().to_lowercase();
        self.title.to_lowercase().contains(&query) || self.source.to_lowercase().contains(&query)
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LoadOrigin {
    Primary,
    Recovery,
}
pub struct LoadedNote {
    pub name: NoteName,
    pub source: String,
    pub origin: LoadOrigin,
    pub recovery_reason: Option<String>,
}
#[derive(Default)]
pub struct ScanReport {
    pub notes: Vec<Summary>,
    pub problems: Vec<String>,
}
pub struct NoteStore {
    root: PathBuf,
}

fn regular_bytes(path: &Path) -> io::Result<Vec<u8>> {
    let mut file = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW)
        .open(path)?;
    if !file.metadata()?.is_file() {
        return Err(io::Error::other("Not a regular file"));
    }
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes)?;
    Ok(bytes)
}
fn source(path: &Path) -> io::Result<String> {
    String::from_utf8(regular_bytes(path)?)
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "Markdown is not UTF-8"))
}
fn directory(path: &Path) -> io::Result<()> {
    match fs::symlink_metadata(path) {
        Ok(meta) if !meta.file_type().is_dir() => {
            return Err(io::Error::other(
                "Notes directory is not a regular directory",
            ));
        }
        Ok(_) => {}
        Err(error) if error.kind() == io::ErrorKind::NotFound => fs::create_dir(path)?,
        Err(error) => return Err(error),
    }
    Ok(())
}

impl NoteStore {
    pub fn open(root: PathBuf) -> io::Result<Self> {
        directory(&root)?;
        directory(&root.join(".recovery"))?;
        Ok(Self { root })
    }
    pub fn scan(&self) -> ScanReport {
        let mut report = ScanReport::default();
        let mut names = BTreeSet::new();
        for directory in [&self.root, &self.root.join(".recovery")] {
            match fs::read_dir(directory) {
                Ok(entries) => {
                    for entry in entries {
                        match entry {
                            Ok(entry) => {
                                let Some(name) =
                                    entry.file_name().to_str().and_then(NoteName::parse)
                                else {
                                    continue;
                                };
                                match entry.file_type() {
                                    Ok(kind) if kind.is_file() => {
                                        names.insert(name);
                                    }
                                    Ok(_) => report
                                        .problems
                                        .push(format!("{} is not a regular note", name.as_str())),
                                    Err(error) => {
                                        report.problems.push(format!("{}: {error}", name.as_str()))
                                    }
                                }
                            }
                            Err(error) => report.problems.push(error.to_string()),
                        }
                    }
                }
                Err(error) => report.problems.push(error.to_string()),
            }
        }
        for name in names {
            match self.load(&name) {
                Ok(note) => report.notes.push(Summary::from_source(
                    name,
                    &note.source,
                    note.origin == LoadOrigin::Recovery,
                )),
                Err(error) => report.problems.push(format!("{}: {error}", name.as_str())),
            }
        }
        report.notes.sort_by(|a, b| {
            a.title
                .to_lowercase()
                .cmp(&b.title.to_lowercase())
                .then(a.name.cmp(&b.name))
        });
        report
    }
    pub fn load(&self, name: &NoteName) -> io::Result<LoadedNote> {
        match source(&self.root.join(name.as_str())) {
            Ok(source) => Ok(LoadedNote {
                name: name.clone(),
                source,
                origin: LoadOrigin::Primary,
                recovery_reason: None,
            }),
            Err(primary) => {
                let source =
                    source(&self.root.join(".recovery").join(name.as_str())).map_err(|backup| {
                        io::Error::other(format!(
                            "Could not read note ({primary}) or recovery ({backup})"
                        ))
                    })?;
                Ok(LoadedNote {
                    name: name.clone(),
                    source,
                    origin: LoadOrigin::Recovery,
                    recovery_reason: Some(primary.to_string()),
                })
            }
        }
    }
    pub fn create(&self) -> io::Result<LoadedNote> {
        let name = NoteName(format!("{}.md", Uuid::new_v4()));
        self.save(&name, "", LoadOrigin::Primary)?;
        self.load(&name)
    }
    pub fn save(&self, name: &NoteName, text: &str, origin: LoadOrigin) -> io::Result<()> {
        self.save_with(name, text, origin, |_| Ok(()))
    }
    fn save_with(
        &self,
        name: &NoteName,
        text: &str,
        origin: LoadOrigin,
        mut checkpoint: impl FnMut(SaveStep) -> io::Result<()>,
    ) -> io::Result<()> {
        directory(&self.root)?;
        directory(&self.root.join(".recovery"))?;
        let primary = self.root.join(name.as_str());
        let staged = self.stage(&self.root, text.as_bytes(), &mut checkpoint)?;
        let result = (|| {
            if origin == LoadOrigin::Primary {
                match source(&primary) {
                    Ok(previous) => {
                        let backup = self.stage(
                            &self.root.join(".recovery"),
                            previous.as_bytes(),
                            &mut checkpoint,
                        )?;
                        let result =
                            fs::rename(&backup, self.root.join(".recovery").join(name.as_str()));
                        if result.is_err() {
                            let _ = fs::remove_file(&backup);
                        }
                        result?;
                        File::open(self.root.join(".recovery"))?.sync_all()?;
                    }
                    Err(error) if error.kind() == io::ErrorKind::NotFound => {}
                    Err(error) => return Err(error),
                }
            } else if fs::symlink_metadata(&primary).is_ok() {
                let bytes = regular_bytes(&primary)?;
                let damaged = self
                    .root
                    .join(".recovery")
                    .join(format!("damaged-{}", Uuid::new_v4()));
                let temporary =
                    self.stage(&self.root.join(".recovery"), &bytes, &mut checkpoint)?;
                fs::rename(temporary, damaged)?;
                File::open(self.root.join(".recovery"))?.sync_all()?;
            }
            checkpoint(SaveStep::Rename)?;
            fs::rename(&staged, &primary)?;
            checkpoint(SaveStep::DirectorySync)?;
            File::open(&self.root)?.sync_all()
        })();
        if result.is_err() {
            let _ = fs::remove_file(&staged);
        }
        result
    }
    fn stage(
        &self,
        directory: &Path,
        bytes: &[u8],
        checkpoint: &mut impl FnMut(SaveStep) -> io::Result<()>,
    ) -> io::Result<PathBuf> {
        let path = directory.join(format!(".pending-{}", Uuid::new_v4()));
        let result = (|| {
            let mut file = OpenOptions::new()
                .create_new(true)
                .write(true)
                .open(&path)?;
            checkpoint(SaveStep::Write)?;
            file.write_all(bytes)?;
            checkpoint(SaveStep::Sync)?;
            file.sync_all()
        })();
        if let Err(error) = result {
            let _ = fs::remove_file(&path);
            return Err(error);
        }
        Ok(path)
    }
}
#[derive(Clone, Copy, PartialEq, Eq)]
enum SaveStep {
    Write,
    Sync,
    Rename,
    DirectorySync,
}

#[cfg(test)]
mod tests {
    use super::*;
    struct Fixture(PathBuf);
    impl Fixture {
        fn new() -> Self {
            let path = std::env::temp_dir().join(format!("threds-{}", Uuid::new_v4()));
            fs::create_dir(&path).unwrap();
            Self(path)
        }
        fn store(&self) -> NoteStore {
            NoteStore::open(self.0.clone()).unwrap()
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
    #[test]
    fn exact_bytes_survive_restart_and_existing_names_are_retained() {
        let fixture = Fixture::new();
        let bytes = "\u{feff}# 名称\r\n\r\nA🙂\r\n";
        fs::write(fixture.0.join("Existing name.md"), bytes).unwrap();
        let store = fixture.store();
        let report = store.scan();
        assert!(report.problems.is_empty());
        let name = report.notes[0].name.clone();
        assert_eq!(name.as_str(), "Existing name.md");
        assert_eq!(report.notes[0].title, "名称");
        assert!(report.notes[0].matches("🙂"));
        store.save(&name, bytes, LoadOrigin::Primary).unwrap();
        drop(store);
        assert_eq!(
            fixture.store().load(&name).unwrap().source.as_bytes(),
            bytes.as_bytes()
        );
        let new = fixture.store().create().unwrap();
        assert!(Uuid::parse_str(new.name.as_str().trim_end_matches(".md")).is_ok());
    }
    #[test]
    fn failed_write_sync_and_rename_leave_last_good_primary() {
        for fault in [SaveStep::Write, SaveStep::Sync, SaveStep::Rename] {
            let fixture = Fixture::new();
            let store = fixture.store();
            let note = store.create().unwrap();
            store
                .save(&note.name, "last good", LoadOrigin::Primary)
                .unwrap();
            let result = store.save_with(&note.name, "unsaved🙂", LoadOrigin::Primary, |step| {
                if step == fault {
                    Err(io::Error::other("injected failure"))
                } else {
                    Ok(())
                }
            });
            assert!(result.is_err());
            assert_eq!(
                fixture.store().load(&note.name).unwrap().source,
                "last good"
            );
            assert_eq!(store.scan().notes.len(), 1);
        }
    }
    #[test]
    fn directory_sync_failure_is_reported_after_atomic_replacement() {
        let fixture = Fixture::new();
        let store = fixture.store();
        let note = store.create().unwrap();
        let result = store.save_with(
            &note.name,
            "visible but not certified durable",
            LoadOrigin::Primary,
            |step| {
                if step == SaveStep::DirectorySync {
                    Err(io::Error::other("sync failed"))
                } else {
                    Ok(())
                }
            },
        );
        assert!(result.is_err());
        assert_eq!(
            fixture.store().load(&note.name).unwrap().source,
            "visible but not certified durable"
        );
    }
    #[test]
    fn recovery_only_notes_are_discovered_and_bad_primary_is_preserved_on_repair() {
        let fixture = Fixture::new();
        let store = fixture.store();
        let note = store.create().unwrap();
        store
            .save(&note.name, "first🙂", LoadOrigin::Primary)
            .unwrap();
        store
            .save(&note.name, "second", LoadOrigin::Primary)
            .unwrap();
        fs::remove_file(fixture.0.join(note.name.as_str())).unwrap();
        let report = store.scan();
        assert_eq!(report.notes.len(), 1);
        assert!(report.notes[0].recovered);
        assert_eq!(store.load(&note.name).unwrap().source, "first🙂");
        fs::write(fixture.0.join(note.name.as_str()), [0xff, 0xfe]).unwrap();
        let recovered = store.load(&note.name).unwrap();
        assert_eq!(recovered.origin, LoadOrigin::Recovery);
        store
            .save(&note.name, &recovered.source, recovered.origin)
            .unwrap();
        assert_eq!(store.load(&note.name).unwrap().source, "first🙂");
        assert_eq!(
            source(&fixture.0.join(".recovery").join(note.name.as_str())).unwrap(),
            "first🙂"
        );
        let damaged = fs::read_dir(fixture.0.join(".recovery"))
            .unwrap()
            .filter_map(Result::ok)
            .find(|entry| entry.file_name().to_string_lossy().starts_with("damaged-"))
            .unwrap();
        assert_eq!(fs::read(damaged.path()).unwrap(), [0xff, 0xfe]);
    }
    #[test]
    fn invalid_utf8_and_symlinks_are_visible_errors_and_never_empty_notes() {
        let fixture = Fixture::new();
        fs::write(fixture.0.join("bad.md"), [0xff]).unwrap();
        fs::write(fixture.0.join("private.txt"), "secret").unwrap();
        std::os::unix::fs::symlink(fixture.0.join("private.txt"), fixture.0.join("link.md"))
            .unwrap();
        let report = fixture.store().scan();
        assert!(report.notes.is_empty());
        assert_eq!(report.problems.len(), 2);
        assert_eq!(fs::read(fixture.0.join("bad.md")).unwrap(), [0xff]);
        assert!(NoteName::parse("../bad.md").is_none());
    }
}
