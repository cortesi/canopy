//! Tracked Luau source typecheck test.

#[cfg(test)]
mod tests {
    use canopy::{Canopy, error::Result};

    #[test]
    fn tracked_luau_preamble_validates() -> Result<()> {
        let mut canopy = Canopy::new();
        canopy.finalize_api()
    }
}
