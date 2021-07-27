pub trait Actions: Send + Copy + Clone + PartialEq {}

impl Actions for () {}
