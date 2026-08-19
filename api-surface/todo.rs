// Ruskel skeleton - syntactically valid Rust with implementation omitted.
// settings: target=examples/todo, visibility=public, auto_impls=false, blanket_impls=false

pub mod todo {
    //! Todo application used as Canopy's end-to-end example and smoke-test target.

    pub mod store {
        //! Thread-local SQLite storage for the todo example.

        /// A persisted todo record.
        #[derive(Debug, Clone)]
        pub struct Todo {
            /// Database identifier.
            pub id: i64,
            /// User-provided todo text.
            pub item: String,
        }

        /// Handle to the current todo database.
        #[derive(Debug, Clone)]
        pub struct Store {}

        impl Store {
            /// Load every persisted todo.
            pub fn todos(&self) -> Result<Vec<Todo>> {}
        }

        /// Return the store opened for the current thread.
        pub fn get() -> anyhow::Result<Store> {}
    }

    /// Widget for a todo entry.
    pub struct TodoEntry {}

    impl Selectable for TodoEntry {
        fn set_selected(&mut self, selected: bool) {}
    }

    impl CommandNode for TodoEntry {
        fn commands() -> &'static [&'static canopy::commands::CommandSpec] {}
    }

    impl Widget for TodoEntry {
        fn layout(&self) -> Layout {}

        fn measure(&self, c: MeasureConstraints) -> Measurement {}

        fn render(&mut self, rndr: &mut Render<'_>, ctx: &dyn ViewContext) -> Result<()> {}

        fn accept_focus(&self, _ctx: &dyn ViewContext) -> bool {}

        fn name(&self) -> NodeName {}
    }

    /// Register and finalize the todo application API with default bindings.
    pub fn setup_app(cnpy: &mut Canopy) -> Result<()> {}

    /// Create a fully configured todo application backed by `db_path`.
    pub fn create_app(db_path: &str) -> anyhow::Result<Canopy> {}

    /// Create a todo canopy app with optional user config.
    pub fn create_app_with_config(
        db_path: &str,
        config: Option<&std::path::Path>,
    ) -> anyhow::Result<Canopy> {
    }
}
