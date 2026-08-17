/// A signed 2D point in integer cell coordinates.
#[derive(Debug, Clone, Copy, Hash, PartialEq, Eq, Default)]
pub struct PointI32 {
    /// X coordinate.
    pub x: i32,
    /// Y coordinate.
    pub y: i32,
}
