//! The todo's options: the file in the config home, and the list they change.

#[cfg(test)]
mod tests {
    use std::fs;

    use anyhow::Result as AnyResult;
    use canopy::{ContextExt, ViewContextExt, geom::Size, testing::harness::Harness};
    use canopy_mcp::ConfigHome;
    use canopy_widgets::List;
    use todo::{TodoEntry, create_app, store::Store};

    /// Build an app over a database that holds `items`, with the config home
    /// `home`.
    fn app(items: &[&str], home: Option<&ConfigHome>) -> AnyResult<Harness> {
        let store = Store::open(":memory:")?;
        for item in items {
            store.add_todo(item)?;
        }
        let canopy = create_app(store, home)?;
        let mut harness = Harness::from_canopy(canopy, Size::new(40, 20))?;
        harness.render()?;
        Ok(harness)
    }

    /// Return the todos of the list in the order it shows them, and the rows of
    /// each.
    fn shown(harness: &mut Harness) -> Vec<(String, u32)> {
        harness
            .canopy
            .with_root_context(|ctx| {
                ctx.with_unique_descendant::<List<TodoEntry, i64>, _>(|list, ctx| {
                    (0..list.len())
                        .filter_map(|index| list.item(index))
                        .map(|entry| {
                            let rows = ctx.view_of(entry.into()).map_or(0, |view| view.outer.h);
                            ctx.with_widget(entry, |entry: &TodoEntry| {
                                Ok((entry.text().to_owned(), rows))
                            })
                        })
                        .collect()
                })
            })
            .expect("the list is readable")
    }

    #[test]
    fn the_options_file_sets_the_first_frame() -> AnyResult<()> {
        let root = tempfile::tempdir()?;
        fs::write(
            root.path().join("options.toml"),
            "[list]\norder = \"newest_first\"\nwrap = false\n",
        )?;
        let home = ConfigHome::new(root.path());
        let long = "a todo long enough to need more than one row at this width";
        let mut harness = app(&["first", long], Some(&home))?;
        assert_eq!(
            shown(&mut harness),
            [(long.to_owned(), 1), ("first".to_owned(), 1)],
            "the newest todo comes first, on one row"
        );
        harness.script(
            r#"
            canopy.assert(options.source("list.order"):find("options.toml") ~= nil)
            options.set("list.wrap", true)
            local saved = options.save("global")
            canopy.assert(saved:find("options.toml") ~= nil, saved)
            "#,
        )?;
        assert!(shown(&mut harness)[0].1 > 1, "the long todo wraps again");
        let saved = fs::read_to_string(root.path().join("options.toml"))?;
        // Wrapping is the default, so the save drops the entry from the file.
        assert_eq!(saved, "[list]\norder = \"newest_first\"\n");
        Ok(())
    }

    #[test]
    fn a_run_without_a_config_home_keeps_no_file() -> AnyResult<()> {
        let mut harness = app(&["first", "second"], None)?;
        assert_eq!(
            shown(&mut harness),
            [("first".to_owned(), 1), ("second".to_owned(), 1)]
        );
        harness.script(
            r#"
            canopy.assert(options.source("list.order") == "default")
            local saved = pcall(function()
                options.save("global")
            end)
            canopy.assert(not saved, "there is no file to save")
            options.set("list.order", "newest_first")
            "#,
        )?;
        assert_eq!(
            shown(&mut harness),
            [("second".to_owned(), 1), ("first".to_owned(), 1)]
        );
        Ok(())
    }
}
