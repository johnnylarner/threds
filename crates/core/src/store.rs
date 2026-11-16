use std::{
    collections::BTreeSet,
    fs::{self, File, OpenOptions},
    io::{self, Read, Write},
    os::unix::fs::OpenOptionsExt,
    path::{Path, PathBuf},
    time::SystemTime,
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
    pub modified: SystemTime,
}
impl Summary {
    pub fn from_source(
        name: NoteName,
        source: &str,
        recovered: bool,
        modified: SystemTime,
    ) -> Self {
        let title = crate::selected_title(source);
        let (before, after) = title
            .span
            .map(|span| (&source[..span.start], &source[span.end..]))
            .unwrap_or((source, ""));
        Self {
            title: title.text,
            excerpt: before
                .trim_start_matches('\u{feff}')
                .split_whitespace()
                .chain(after.split_whitespace())
                .collect::<Vec<_>>()
                .join(" ")
                .chars()
                .take(120)
                .collect(),
            source: source.into(),
            name,
            recovered,
            modified,
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
    pub modified: SystemTime,
}
#[derive(Default)]
pub struct ScanReport {
    pub notes: Vec<Summary>,
    pub problems: Vec<String>,
}
pub struct NoteStore {
    root: PathBuf,
}

fn regular_file(path: &Path) -> io::Result<File> {
    let file = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW)
        .open(path)?;
    if !file.metadata()?.is_file() {
        return Err(io::Error::other("Not a regular file"));
    }
    Ok(file)
}
fn regular_bytes(path: &Path) -> io::Result<Vec<u8>> {
    let mut file = regular_file(path)?;
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes)?;
    Ok(bytes)
}
fn source_with_modified(path: &Path) -> io::Result<(String, SystemTime)> {
    let mut file = regular_file(path)?;
    let modified = file.metadata()?.modified()?;
    let mut source = String::new();
    file.read_to_string(&mut source)?;
    Ok((source, modified))
}
fn source(path: &Path) -> io::Result<String> {
    source_with_modified(path).map(|(source, _)| source)
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
                    note.modified,
                )),
                Err(error) => report.problems.push(format!("{}: {error}", name.as_str())),
            }
        }
        report
            .notes
            .sort_by(|a, b| b.modified.cmp(&a.modified).then(a.name.cmp(&b.name)));
        report
    }
    pub fn load(&self, name: &NoteName) -> io::Result<LoadedNote> {
        match source_with_modified(&self.root.join(name.as_str())) {
            Ok((source, modified)) => Ok(LoadedNote {
                name: name.clone(),
                source,
                origin: LoadOrigin::Primary,
                recovery_reason: None,
                modified,
            }),
            Err(primary) => {
                let (source, modified) =
                    source_with_modified(&self.root.join(".recovery").join(name.as_str()))
                        .map_err(|backup| {
                            io::Error::other(format!(
                                "Could not read note ({primary}) or recovery ({backup})"
                            ))
                        })?;
                Ok(LoadedNote {
                    name: name.clone(),
                    source,
                    origin: LoadOrigin::Recovery,
                    recovery_reason: Some(primary.to_string()),
                    modified,
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
    fn excerpts_exclude_only_the_selected_title_and_keep_source_searchable() {
        for (source, title, excerpt) in [
            (
                "# Heading wins\n\nLatest synthetic writing.",
                "Heading wins",
                "Latest synthetic writing.",
            ),
            ("First line\nRemaining body", "First line", "Remaining body"),
            (
                "Before\n\n## **Heading** `wins`\n\nHeading wins again.",
                "Heading wins",
                "Before Heading wins again.",
            ),
            ("\u{feff}Café 🙂\r\n===\r\n\r\nBody\r\n", "Café 🙂", "Body"),
            ("\u{feff}\r\n  Café 🙂  \r\nbody\r\n", "Café 🙂", "body"),
            ("# Only title", "Only title", ""),
            ("Only title", "Only title", ""),
            (" \r\n\t", "Untitled", ""),
            ("# First\n\n# Second\nBody", "First", "# Second Body"),
        ] {
            let summary = Summary::from_source(
                NoteName("example.md".into()),
                source,
                false,
                SystemTime::UNIX_EPOCH,
            );
            assert_eq!(summary.title, title, "source {source:?}");
            assert_eq!(summary.excerpt, excerpt, "source {source:?}");
            assert_eq!(summary.source.as_bytes(), source.as_bytes());
            assert!(summary.matches(source));
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
    fn set_modified(path: &Path, seconds: u64) {
        File::options()
            .write(true)
            .open(path)
            .unwrap()
            .set_times(
                fs::FileTimes::new()
                    .set_modified(SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(seconds)),
            )
            .unwrap();
    }
    #[test]
    fn recent_edits_sort_first_and_equal_times_use_stable_filenames() {
        let fixture = Fixture::new();
        for (name, text, modified) in [
            ("z.md", "# Alphabetically first", 20),
            ("b.md", "# Zulu", 30),
            ("a.md", "Plain title\nbody", 30),
        ] {
            let path = fixture.0.join(name);
            fs::write(&path, text).unwrap();
            set_modified(&path, modified);
        }
        let store = fixture.store();
        let report = store.scan();
        assert!(report.problems.is_empty());
        assert_eq!(
            report
                .notes
                .iter()
                .map(|note| note.name.as_str())
                .collect::<Vec<_>>(),
            ["a.md", "b.md", "z.md"]
        );
        let oldest = report.notes[2].name.clone();
        store
            .save(&oldest, "Edited without a heading\r\n", LoadOrigin::Primary)
            .unwrap();
        let report = fixture.store().scan();
        assert!(report.problems.is_empty());
        assert_eq!(report.notes[0].name, oldest);
        assert_eq!(report.notes[0].title, "Edited without a heading");
        assert_eq!(
            fs::read(fixture.0.join("z.md")).unwrap(),
            b"Edited without a heading\r\n"
        );
        assert_eq!(report.notes.len(), 3);
        store.load(&report.notes[1].name).unwrap();
        assert_eq!(
            store
                .scan()
                .notes
                .iter()
                .map(|note| &note.name)
                .collect::<Vec<_>>(),
            report
                .notes
                .iter()
                .map(|note| &note.name)
                .collect::<Vec<_>>()
        );
    }
    #[test]
    fn recovery_order_uses_loaded_copy_time_and_scan_does_not_change_source() {
        let fixture = Fixture::new();
        let store = fixture.store();
        let original = "\u{feff}\r\n  Café 🙂  \r\nbody\r\n";
        fs::write(fixture.0.join("plain.md"), original).unwrap();
        fs::write(fixture.0.join("empty.md"), " \r\n\t").unwrap();
        fs::write(fixture.0.join("damaged.md"), [0xff]).unwrap();
        fs::write(fixture.0.join(".recovery/damaged.md"), "Recovered title").unwrap();
        fs::write(fixture.0.join(".recovery/only.md"), "# Recovery only").unwrap();
        for (name, seconds) in [
            ("plain.md", 20),
            ("empty.md", 10),
            ("damaged.md", 100),
            (".recovery/damaged.md", 30),
            (".recovery/only.md", 40),
        ] {
            set_modified(&fixture.0.join(name), seconds);
        }
        let report = store.scan();
        assert!(report.problems.is_empty());
        assert_eq!(
            report
                .notes
                .iter()
                .map(|note| (note.name.as_str(), note.title.as_str(), note.recovered))
                .collect::<Vec<_>>(),
            [
                ("only.md", "Recovery only", true),
                ("damaged.md", "Recovered title", true),
                ("plain.md", "Café 🙂", false),
                ("empty.md", "Untitled", false),
            ]
        );
        assert_eq!(
            fs::read(fixture.0.join("plain.md")).unwrap(),
            original.as_bytes()
        );
        assert_eq!(fs::read(fixture.0.join("damaged.md")).unwrap(), [0xff]);
        assert!(!fixture.0.join("only.md").exists());
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
