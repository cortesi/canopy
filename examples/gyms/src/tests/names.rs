//! The style roots that gyms name their styles under.

use canopy::error::Result;

use crate::{Demo, demo_canopy};

#[test]
fn four_gym_style_roots_resolve_to_styles_of_their_own() -> Result<()> {
    let cases = [
        (Demo::Cursor, "cursor/label/focused"),
        (Demo::Font, "font/label"),
        (Demo::Motion, "motion/blink"),
        (Demo::Term, "term/frame/title"),
    ];
    for (demo, path) in cases {
        let canopy = demo.configure(demo_canopy()).build()?;
        let style = canopy.style();
        assert_ne!(
            style.resolve(path),
            style.resolve(""),
            "{path} has the gym's style"
        );
    }
    Ok(())
}
