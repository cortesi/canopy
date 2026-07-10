//! Repository-gate checks for tracked widget Luau scripts.

#[cfg(test)]
mod tests {
    use std::{error::Error, fs, path::PathBuf};

    use canopy::{Canopy, Loader};
    use canopy_widgets::{Dropdown, List, Root, Text};

    fn finalized_surfaces() -> Result<(Canopy, Canopy, Canopy), Box<dyn Error>> {
        let mut dropdown = Canopy::new();
        dropdown.add_commands::<Dropdown<String>>()?;
        dropdown.finalize_api()?;

        let mut list = Canopy::new();
        list.add_commands::<List<Text>>()?;
        list.finalize_api()?;

        let mut root = Canopy::new();
        Root::load(&mut root)?;
        root.finalize_api()?;
        Ok((dropdown, list, root))
    }

    fn scripts() -> Result<Vec<PathBuf>, Box<dyn Error>> {
        let directory = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/luau");
        let mut scripts = fs::read_dir(directory)?
            .map(|entry| entry.map(|entry| entry.path()))
            .collect::<Result<Vec<_>, _>>()?;
        scripts.retain(|path| {
            path.extension()
                .is_some_and(|extension| extension == "luau")
        });
        scripts.sort();
        Ok(scripts)
    }

    fn assert_checks(
        canopy: &mut Canopy,
        source_name: &str,
        source: &str,
    ) -> Result<(), Box<dyn Error>> {
        let result = canopy.check_script(source_name, source)?;
        let diagnostics = result
            .diagnostics()
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join("\n");
        assert!(result.is_ok(), "{diagnostics}");
        Ok(())
    }

    #[test]
    fn tracked_luau_widget_scripts_typecheck() -> Result<(), Box<dyn Error>> {
        let (mut dropdown, mut list, mut root) = finalized_surfaces()?;
        for path in scripts()? {
            let file_name = path
                .file_name()
                .and_then(|name| name.to_str())
                .expect("tracked Luau scripts have UTF-8 file names");
            let source_name = format!("crates/canopy-widgets/tests/luau/{file_name}");
            let source = fs::read_to_string(&path)?;
            let canopy = if file_name.starts_with("dropdown_") {
                &mut dropdown
            } else if file_name.starts_with("list_") {
                &mut list
            } else if file_name.starts_with("root_") {
                &mut root
            } else {
                panic!("tracked widget script has no command-surface owner: {source_name}");
            };
            assert_checks(canopy, &source_name, &source)?;
        }
        Ok(())
    }
}
