use crate::{Error, Point};

/// A signed 2D point in integer cell coordinates.
#[derive(Debug, Clone, Copy, Hash, PartialEq, Eq, Default)]
pub struct PointI32 {
    /// X coordinate.
    pub x: i32,
    /// Y coordinate.
    pub y: i32,
}

impl TryFrom<Point> for PointI32 {
    type Error = Error;

    fn try_from(point: Point) -> Result<Self, Self::Error> {
        let error = Error::CoordinateOutOfRange {
            x: i64::from(point.x),
            y: i64::from(point.y),
        };
        Ok(Self {
            x: i32::try_from(point.x).map_err(|_| error.clone())?,
            y: i32::try_from(point.y).map_err(|_| error)?,
        })
    }
}

impl PointI32 {
    /// Origin point.
    pub const ZERO: Self = Self { x: 0, y: 0 };

    /// Construct a point from its coordinates.
    pub const fn new(x: i32, y: i32) -> Self {
        Self { x, y }
    }

    /// Convert widened coordinates without losing out-of-range information.
    pub fn try_from_i64(x: i64, y: i64) -> Result<Self, Error> {
        let error = Error::CoordinateOutOfRange { x, y };
        Ok(Self {
            x: i32::try_from(x).map_err(|_| error.clone())?,
            y: i32::try_from(y).map_err(|_| error)?,
        })
    }

    /// Clamp widened coordinates to the signed point range.
    pub fn clamped_from_i64(x: i64, y: i64) -> Self {
        Self {
            x: x.clamp(i64::from(i32::MIN), i64::from(i32::MAX)) as i32,
            y: y.clamp(i64::from(i32::MIN), i64::from(i32::MAX)) as i32,
        }
    }
}

impl TryFrom<PointI32> for Point {
    type Error = Error;

    fn try_from(point: PointI32) -> Result<Self, Self::Error> {
        let error = Error::CoordinateOutOfRange {
            x: i64::from(point.x),
            y: i64::from(point.y),
        };
        Ok(Self {
            x: u32::try_from(point.x).map_err(|_| error.clone())?,
            y: u32::try_from(point.y).map_err(|_| error)?,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn checked_points_preserve_values_and_reject_unrepresentable_coordinates() {
        let point = Point {
            x: i32::MAX as u32,
            y: 7,
        };
        assert_eq!(
            Point::try_from(PointI32::try_from(point).unwrap()).unwrap(),
            point
        );
        assert_eq!(
            PointI32::try_from(Point { x: u32::MAX, y: 7 }),
            Err(Error::CoordinateOutOfRange {
                x: i64::from(u32::MAX),
                y: 7
            }),
        );
        assert_eq!(
            Point::try_from(PointI32 { x: 7, y: -1 }),
            Err(Error::CoordinateOutOfRange { x: 7, y: -1 }),
        );
    }

    #[test]
    fn widened_point_conversions_are_explicit_about_overflow() {
        assert_eq!(
            PointI32::try_from_i64(i64::from(i32::MIN), i64::from(i32::MAX)).unwrap(),
            PointI32::new(i32::MIN, i32::MAX)
        );
        assert!(PointI32::try_from_i64(i64::from(i32::MAX) + 1, 0).is_err());
        assert_eq!(
            PointI32::clamped_from_i64(i64::MIN, i64::MAX),
            PointI32::new(i32::MIN, i32::MAX)
        );
    }
}
