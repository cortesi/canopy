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
}
