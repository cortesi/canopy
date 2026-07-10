//! Repository-gate checks for tracked Todo Luau scripts.

#[cfg(test)]
mod tests {
    use std::{
        error::Error,
        fs, io,
        path::{Path, PathBuf},
    };

    use canopy::Canopy;

    fn collect_scripts(directory: &Path, scripts: &mut Vec<PathBuf>) -> io::Result<()> {
        for entry in fs::read_dir(directory)? {
            let entry = entry?;
            let path = entry.path();
            if entry.file_type()?.is_dir() {
                collect_scripts(&path, scripts)?;
            } else if path
                .extension()
                .is_some_and(|extension| extension == "luau")
            {
                scripts.push(path);
            }
        }
        Ok(())
    }

    #[test]
    fn tracked_luau_todo_scripts_typecheck() -> Result<(), Box<dyn Error>> {
        let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
        let suite = manifest.join("smoke");
        let mut scripts = Vec::new();
        collect_scripts(&suite, &mut scripts)?;
        scripts.sort();

        let mut canopy = Canopy::new();
        todo::setup_app(&mut canopy)?;
        for path in scripts {
            let relative = path
                .strip_prefix(
                    manifest
                        .parent()
                        .and_then(Path::parent)
                        .expect("workspace root"),
                )?
                .to_string_lossy();
            let source = fs::read_to_string(&path)?;
            let result = canopy.check_script(&relative, &source)?;
            let diagnostics = result
                .diagnostics()
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join("\n");
            assert!(result.is_ok(), "{diagnostics}");
        }
        Ok(())
    }
}
