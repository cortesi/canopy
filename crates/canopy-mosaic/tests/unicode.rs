//! The geometry of each mosaic character agrees with its Unicode name.

#[cfg(test)]
mod tests {
    use canopy_mosaic::{Tier, geometry::Shape, symbols::Family};

    /// The fixture of names, extracted from `UnicodeData.txt`.
    const FIXTURE: &str = include_str!("../testdata/unicode.txt");

    /// A point of the cell, in unit coordinates.
    type Point = (f32, f32);

    /// A region of the cell that a name states.
    enum Region {
        /// An axis-aligned rectangle: left, top, right, bottom.
        Rect(f32, f32, f32, f32),
        /// The side of the line from the first point to the second that
        /// holds the third.
        Side(Point, Point, Point),
        /// One of the four triangles that meet at the center: left, upper,
        /// right, or lower.
        Triangle(&'static str),
    }

    impl Region {
        /// Whether the region holds the point `(x, y)`.
        fn contains(&self, x: f32, y: f32) -> bool {
            match *self {
                Self::Rect(x0, y0, x1, y1) => x >= x0 && x < x1 && y >= y0 && y < y1,
                Self::Side(start, end, corner) => {
                    let side = |p: Point| {
                        (end.0 - start.0) * (p.1 - start.1) - (end.1 - start.1) * (p.0 - start.0)
                    };
                    side((x, y)) * side(corner) > 0.0
                }
                Self::Triangle(name) => {
                    let (dx, dy) = (x - 0.5, y - 0.5);
                    match name {
                        "LEFT" => -dx > dy.abs(),
                        "UPPER" => -dy > dx.abs(),
                        "RIGHT" => dx > dy.abs(),
                        _ => dy > dx.abs(),
                    }
                }
            }
        }
    }

    /// A fraction of the cell as Unicode names it.
    fn fraction(words: &str) -> Option<f32> {
        Some(match words {
            "ONE EIGHTH" => 1.0 / 8.0,
            "ONE QUARTER" => 0.25,
            "THREE EIGHTHS" => 3.0 / 8.0,
            "HALF" => 0.5,
            "FIVE EIGHTHS" => 5.0 / 8.0,
            "THREE QUARTERS" => 0.75,
            "SEVEN EIGHTHS" => 7.0 / 8.0,
            _ => return None,
        })
    }

    /// A point on the edge of the cell as the smooth mosaics name it.
    fn edge(words: &str) -> Point {
        match words {
            "UPPER LEFT" => (0.0, 0.0),
            "UPPER CENTRE" => (0.5, 0.0),
            "UPPER RIGHT" => (1.0, 0.0),
            "UPPER MIDDLE LEFT" => (0.0, 1.0 / 3.0),
            "UPPER MIDDLE RIGHT" => (1.0, 1.0 / 3.0),
            "LOWER MIDDLE LEFT" => (0.0, 2.0 / 3.0),
            "LOWER MIDDLE RIGHT" => (1.0, 2.0 / 3.0),
            "LOWER LEFT" => (0.0, 1.0),
            "LOWER CENTRE" => (0.5, 1.0),
            "LOWER RIGHT" => (1.0, 1.0),
            other => panic!("unknown edge point {other:?}"),
        }
    }

    /// The band of the cell next to one edge.
    fn band(side: &str, f: f32) -> Region {
        match side {
            "UPPER" => Region::Rect(0.0, 0.0, 1.0, f),
            "LOWER" => Region::Rect(0.0, 1.0 - f, 1.0, 1.0),
            "LEFT" => Region::Rect(0.0, 0.0, f, 1.0),
            _ => Region::Rect(1.0 - f, 0.0, 1.0, 1.0),
        }
    }

    /// The cells of a lattice from the digits of a name.
    fn lattice(digits: &str, rows: f32) -> Vec<Region> {
        digits
            .chars()
            .map(|d| {
                let n = d.to_digit(10).expect("digit") as f32 - 1.0;
                let (x, y) = (n % 2.0 / 2.0, (n / 2.0).floor() / rows);
                Region::Rect(x, y, x + 0.5, y + 1.0 / rows)
            })
            .collect()
    }

    /// The regions that a name states.
    fn regions(name: &str) -> Vec<Region> {
        if name == "SPACE" {
            return Vec::new();
        }
        if name == "FULL BLOCK" {
            return vec![Region::Rect(0.0, 0.0, 1.0, 1.0)];
        }
        if let Some(digits) = name.strip_prefix("BLOCK SEXTANT-") {
            return lattice(digits, 3.0);
        }
        if let Some(digits) = name.strip_prefix("BLOCK OCTANT-") {
            return lattice(digits, 4.0);
        }
        if let Some(quadrants) = name.strip_prefix("QUADRANT ") {
            return quadrants
                .split(" AND ")
                .map(|part| {
                    let x = if part.ends_with("LEFT") { 0.0 } else { 0.5 };
                    let y = if part.starts_with("UPPER") { 0.0 } else { 0.5 };
                    Region::Rect(x, y, x + 0.5, y + 0.5)
                })
                .collect();
        }
        if let Some(n) = name.strip_prefix("VERTICAL ONE EIGHTH BLOCK-") {
            let n: f32 = n.parse().expect("column");
            return vec![Region::Rect((n - 1.0) / 8.0, 0.0, n / 8.0, 1.0)];
        }
        if let Some(digits) = name.strip_prefix("HORIZONTAL ONE EIGHTH BLOCK-") {
            return digits
                .chars()
                .map(|d| {
                    let n = d.to_digit(10).expect("digit") as f32;
                    Region::Rect(0.0, (n - 1.0) / 8.0, 1.0, n / 8.0)
                })
                .collect();
        }
        if let Some((corner, line)) = name.split_once(" BLOCK DIAGONAL ") {
            let (a, b) = line.split_once(" TO ").expect("two ends");
            return vec![Region::Side(edge(a), edge(b), edge(corner))];
        }
        if let Some(sides) = name
            .strip_suffix(" TRIANGULAR THREE QUARTERS BLOCK")
            .or_else(|| name.strip_suffix(" TRIANGULAR ONE QUARTER BLOCK"))
        {
            return sides
                .split(" AND ")
                .map(|side| match side {
                    "LEFT" => Region::Triangle("LEFT"),
                    "UPPER" => Region::Triangle("UPPER"),
                    "RIGHT" => Region::Triangle("RIGHT"),
                    _ => Region::Triangle("LOWER"),
                })
                .collect();
        }
        if let Some(sides) = name.strip_suffix(" ONE EIGHTH BLOCK")
            && sides.contains(" AND ")
        {
            return sides
                .split(" AND ")
                .map(|side| band(side, 1.0 / 8.0))
                .collect();
        }
        if let Some(rest) = name.strip_suffix(" ONE QUARTER BLOCK") {
            if let Some(side) = rest.strip_prefix("MIDDLE ") {
                let x = if side == "LEFT" { 0.0 } else { 0.5 };
                return vec![Region::Rect(x, 0.25, x + 0.5, 0.75)];
            }
            if let Some((half, end)) = rest.split_once(" HALF ") {
                let x = if half == "LEFT" { 0.0 } else { 0.5 };
                let y = if end == "UPPER" { 0.0 } else { 0.75 };
                return vec![Region::Rect(x, y, x + 0.5, y + 0.25)];
            }
        }
        let words: Vec<&str> = name.split(' ').collect();
        if words.last() == Some(&"BLOCK")
            && let Some(f) = fraction(&words[1..words.len() - 1].join(" "))
        {
            return vec![band(words[0], f)];
        }
        panic!("cannot read the geometry of {name:?}");
    }

    /// The coverage of a pixel by the regions, by supersampling. The samples
    /// sit off the diagonals of the cell, so no sample lies on an edge.
    fn coverage(regions: &[Region], x0: f32, y0: f32, size: f32) -> f32 {
        const SAMPLES: usize = 16;
        let mut hits = 0;
        for j in 0..SAMPLES {
            for i in 0..SAMPLES {
                let x = x0 + (i as f32 + 0.37) / SAMPLES as f32 * size;
                let y = y0 + (j as f32 + 0.61) / SAMPLES as f32 * size;
                if regions.iter().any(|region| region.contains(x, y)) {
                    hits += 1;
                }
            }
        }
        hits as f32 / (SAMPLES * SAMPLES) as f32
    }

    /// Every character of the fixture, with its name.
    fn characters() -> Vec<(char, &'static str)> {
        FIXTURE
            .lines()
            .filter(|line| !line.starts_with('#'))
            .map(|line| {
                let (code, name) = line.split_once(';').expect("CODE;NAME");
                let code = u32::from_str_radix(code, 16).expect("hex code point");
                (char::from_u32(code).expect("char"), name)
            })
            .collect()
    }

    #[test]
    fn every_shape_covers_what_its_name_states() {
        let characters = characters();
        assert_eq!(characters.len(), 406);
        for (ch, name) in characters {
            let shape = Shape::of(ch).unwrap_or_else(|| panic!("no shape for {name}"));
            let regions = regions(name);
            let step = 1.0 / 8.0;
            for y in 0..8 {
                for x in 0..8 {
                    let (x0, y0) = (x as f32 * step, y as f32 * step);
                    let expected = coverage(&regions, x0, y0, step);
                    let actual = shape.coverage_of(x0, y0, x0 + step, y0 + step);
                    assert!(
                        (expected - actual).abs() < 0.05,
                        "{name}: pixel ({x}, {y}) covers {actual}, the name says {expected}"
                    );
                    // A pixel the name covers clearly is set in the mask,
                    // and one it clearly misses is clear.
                    if (expected - 0.5).abs() > 0.1 {
                        assert_eq!(shape.mask().get(x, y), expected > 0.5, "{name} ({x}, {y})");
                    }
                }
            }
        }
    }

    #[test]
    fn the_fixture_holds_exactly_the_full_tier() {
        let mut fixture: Vec<char> = characters().into_iter().map(|(ch, _)| ch).collect();
        let mut full: Vec<char> = Tier::Full
            .families()
            .iter()
            .flat_map(|family: &Family| family.chars())
            .collect();
        fixture.sort_unstable();
        full.sort_unstable();
        assert_eq!(fixture, full);
    }
}
