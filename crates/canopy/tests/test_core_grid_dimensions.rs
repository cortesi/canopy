//! Grid dimension tests.

#[cfg(test)]
mod tests {
    use canopy::core::tutils::grid::Grid;

    #[test]
    fn test_grid_dimensions() {
        // Test various grid configurations
        let test_cases = vec![
            (0, 2, (1, 1)),   // recursion=0: 1x1 grid
            (1, 2, (2, 2)),   // recursion=1, divisions=2: 2x2 grid
            (2, 2, (4, 4)),   // recursion=2, divisions=2: 4x4 grid
            (3, 2, (8, 8)),   // recursion=3, divisions=2: 8x8 grid
            (1, 3, (3, 3)),   // recursion=1, divisions=3: 3x3 grid
            (2, 3, (9, 9)),   // recursion=2, divisions=3: 9x9 grid
            (3, 3, (27, 27)), // recursion=3, divisions=3: 27x27 grid
            (1, 4, (4, 4)),   // recursion=1, divisions=4: 4x4 grid
            (2, 4, (16, 16)), // recursion=2, divisions=4: 16x16 grid
        ];

        for (recursion, divisions, expected) in test_cases {
            let grid = Grid::new(recursion, divisions);
            let dimensions = grid.dimensions();
            assert_eq!(
                dimensions, expected,
                "Grid({recursion}, {divisions}) should have dimensions {expected:?}, got {dimensions:?}"
            );

            // Also verify that dimensions match expected_size
            let expected_size = grid.expected_size();
            let expected_pixels = (expected.0 as u32 * 10, expected.1 as u32 * 10);
            assert_eq!(
                (expected_size.w, expected_size.h),
                expected_pixels,
                "Grid({recursion}, {divisions}) expected_size should match dimensions * 10"
            );
        }
    }
}
