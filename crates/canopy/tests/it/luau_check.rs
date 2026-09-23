//! Tracked Luau source typecheck test and repository-wide inventory.

#[cfg(test)]
mod tests {
    use std::{
        error::Error,
        fs, io,
        path::{Path, PathBuf},
    };

    use canopy::{CanopyBuilder, error::Result as CanopyResult};

    #[test]
    fn tracked_luau_preamble_validates() -> CanopyResult<()> {
        CanopyBuilder::new().build().map(|_| ())
    }

    /// Return the workspace root, two levels above this crate's manifest.
    fn workspace_root() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(Path::parent)
            .expect("the canopy crate lives two levels under the workspace root")
            .to_path_buf()
    }

    /// Recursively collect `.luau` files, skipping VCS, cargo, and build
    /// directories.
    fn collect_luau_files(dir: &Path, files: &mut Vec<PathBuf>) -> io::Result<()> {
        for entry in fs::read_dir(dir)? {
            let entry = entry?;
            let path = entry.path();
            let file_type = entry.file_type()?;
            if file_type.is_dir() {
                if matches!(
                    entry.file_name().to_str(),
                    Some(".git" | ".cargo" | "target" | "tmp")
                ) {
                    continue;
                }
                collect_luau_files(&path, files)?;
                continue;
            }
            if file_type.is_file()
                && path
                    .extension()
                    .is_some_and(|extension| extension == "luau")
            {
                files.push(path);
            }
        }
        Ok(())
    }

    /// Every tracked `.luau` file must live under a directory with an
    /// explicit checker owner: a dedicated typecheck test, or a smoke suite
    /// that runs it.
    #[test]
    fn every_luau_file_has_a_checker_owner() -> Result<(), Box<dyn Error>> {
        let workspace_root = workspace_root();
        let mut files = Vec::new();
        collect_luau_files(&workspace_root, &mut files)?;
        let mut files = files
            .into_iter()
            .map(|path| {
                path.strip_prefix(&workspace_root)
                    .expect("collected paths live under the workspace root")
                    .to_str()
                    .expect("workspace-relative Luau paths are UTF-8")
                    .replace('\\', "/")
            })
            .collect::<Vec<_>>();
        files.sort();
        for file in files {
            let owned = file == "crates/canopy/luau/preamble.d.luau"
                || file.starts_with("crates/canopy-widgets/tests/luau/")
                || file.starts_with("examples/todo/smoke/")
                || file.starts_with("examples/hello/smoke/")
                || file.starts_with("examples/gyms/smoke/")
                // Each app's tests build it without a home, which compiles
                // its defaults under the startup-script contract.
                || file == "examples/hello/src/default_config.luau"
                || file == "examples/todo/src/default_config.luau";
            assert!(owned, "tracked Luau file has no checker owner: {file}");
        }
        Ok(())
    }
}
