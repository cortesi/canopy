//! Where option files are: the global file, and the chain of local files
//! from the start directory up to the home directory.

use std::path::{Path, PathBuf};

/// The name of an option file in its directory.
pub const OPTIONS_FILE: &str = "options.toml";

/// The option files of one run. The default has no files, for runs that
/// keep no options.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Layout {
    /// The global file, if the run has a config home.
    pub global: Option<PathBuf>,
    /// The local files that exist, from the outermost directory to the start
    /// directory.
    pub locals: Vec<PathBuf>,
    /// The file that a local save writes, if the run has one.
    pub local_target: Option<PathBuf>,
}

impl Layout {
    /// Find the option files of a run.
    ///
    /// `app_dir` is the name of the directory that holds a local file, such
    /// as `.fh`, and `config_home` is the directory that holds the global
    /// file. Local files are in `start` and in each of its ancestors below
    /// `home`. A start directory that is `home`, or that is outside it, has
    /// no local files. A local save writes the nearer of the nearest local
    /// file and the file in `project_root`, or else the file in `start`.
    #[must_use]
    pub fn discover(
        app_dir: &str,
        config_home: Option<&Path>,
        start: &Path,
        home: Option<&Path>,
        project_root: Option<&Path>,
    ) -> Self {
        let global = config_home.map(|home| home.join(OPTIONS_FILE));
        let Some(home) = home.map(canonical) else {
            return Self {
                global,
                ..Self::default()
            };
        };
        let below_home = |directory: &Path| directory != home && directory.starts_with(&home);
        let start = canonical(start);
        if !below_home(&start) {
            return Self {
                global,
                ..Self::default()
            };
        }
        let global_canonical = global.as_deref().map(canonical);
        let file_in = |directory: &Path| directory.join(app_dir).join(OPTIONS_FILE);
        let mut locals = Vec::new();
        for directory in start.ancestors() {
            if !below_home(directory) {
                break;
            }
            let file = file_in(directory);
            if global_canonical.as_deref() == Some(file.as_path()) {
                continue;
            }
            if file.is_file() {
                locals.push(file);
            }
        }
        locals.reverse();

        let project = project_root
            .map(canonical)
            .filter(|root| below_home(root) && start.starts_with(root))
            .map(|root| file_in(&root));
        let nearest = locals.last().cloned();
        let local_target = match (nearest, project) {
            (Some(nearest), Some(project)) => Some(if depth(&project) > depth(&nearest) {
                project
            } else {
                nearest
            }),
            (Some(file), None) | (None, Some(file)) => Some(file),
            (None, None) => Some(file_in(&start)),
        };
        // The global file is never a local one, which a config home inside
        // the walk could make it.
        let local_target = local_target.filter(|target| Some(target) != global_canonical.as_ref());
        Self {
            global,
            locals,
            local_target,
        }
    }
}

/// Return the canonical form of `path`, or `path` itself if it has none.
fn canonical(path: &Path) -> PathBuf {
    path.canonicalize().unwrap_or_else(|_| path.to_path_buf())
}

/// Return the number of components of `path`.
fn depth(path: &Path) -> usize {
    path.components().count()
}

#[cfg(test)]
mod tests {
    use std::{fs, os::unix::fs::symlink};

    use super::*;

    /// Make an option file in `directory`.
    fn options_in(directory: &Path) -> PathBuf {
        let file = directory.join(".fh").join(OPTIONS_FILE);
        fs::create_dir_all(file.parent().unwrap()).unwrap();
        fs::write(&file, "").unwrap();
        file.canonicalize().unwrap()
    }

    #[test]
    fn local_files_run_from_the_home_to_the_start() {
        let root = tempfile::tempdir().unwrap();
        let home = root.path().join("home");
        let work = home.join("work");
        let repo = work.join("repo");
        let start = repo.join("src");
        fs::create_dir_all(&start).unwrap();
        options_in(&home);
        let outer = options_in(&work);
        let inner = options_in(&repo);
        let config = home.join(".fh");
        let layout = Layout::discover(".fh", Some(&config), &start, Some(&home), None);
        assert_eq!(layout.locals, vec![outer, inner.clone()]);
        assert_eq!(layout.global, Some(config.join(OPTIONS_FILE)));
        assert_eq!(layout.local_target, Some(inner));
    }

    #[test]
    fn the_home_and_outside_have_no_local_files() {
        let root = tempfile::tempdir().unwrap();
        let home = root.path().join("home");
        let outside = root.path().join("srv");
        fs::create_dir_all(&home).unwrap();
        fs::create_dir_all(&outside).unwrap();
        options_in(&outside);
        for start in [&home, &outside] {
            let layout = Layout::discover(".fh", None, start, Some(&home), None);
            assert_eq!(layout.locals, Vec::<PathBuf>::new());
            assert_eq!(layout.local_target, None);
        }
    }

    #[test]
    fn a_save_targets_the_nearer_of_the_project_and_the_nearest_file() {
        let root = tempfile::tempdir().unwrap();
        let home = root.path().join("home").canonicalize_or_create();
        let work = home.join("work");
        let repo = work.join("repo");
        fs::create_dir_all(&repo).unwrap();
        let outer = options_in(&work);
        let layout = Layout::discover(".fh", None, &repo, Some(&home), Some(&repo));
        assert_eq!(layout.locals, vec![outer.clone()]);
        assert_eq!(
            layout.local_target,
            Some(repo.join(".fh").join(OPTIONS_FILE))
        );
        let layout = Layout::discover(".fh", None, &repo, Some(&home), Some(&home));
        assert_eq!(layout.local_target, Some(outer));
        let start = repo.join("deep");
        fs::create_dir_all(&start).unwrap();
        let layout = Layout::discover(".fh", None, &start, Some(&home), None);
        assert_eq!(
            layout.local_target,
            Some(work.join(".fh").join(OPTIONS_FILE))
        );
    }

    #[test]
    fn a_start_through_a_symlink_uses_the_canonical_path() {
        let root = tempfile::tempdir().unwrap();
        let home = root.path().join("home");
        let repo = home.join("repo");
        fs::create_dir_all(&repo).unwrap();
        let file = options_in(&repo);
        let link = root.path().join("link");
        symlink(&repo, &link).unwrap();
        let layout = Layout::discover(".fh", None, &link, Some(&home), None);
        assert_eq!(layout.locals, vec![file]);
    }

    #[test]
    fn a_config_home_inside_the_walk_is_not_local() {
        let root = tempfile::tempdir().unwrap();
        let home = root.path().join("home");
        let work = home.join("work");
        fs::create_dir_all(&work).unwrap();
        let file = options_in(&work);
        let layout = Layout::discover(
            ".fh",
            Some(file.parent().unwrap()),
            &work,
            Some(&home),
            None,
        );
        assert_eq!(layout.locals, Vec::<PathBuf>::new());
        assert_eq!(
            layout.local_target, None,
            "a local save never writes the global file"
        );
    }

    /// Create a directory and return its canonical path.
    trait CanonicalOrCreate {
        /// Create the directory and return its canonical path.
        fn canonicalize_or_create(self) -> PathBuf;
    }

    impl CanonicalOrCreate for PathBuf {
        fn canonicalize_or_create(self) -> PathBuf {
            fs::create_dir_all(&self).unwrap();
            self.canonicalize().unwrap()
        }
    }
}
