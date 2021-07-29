pub trait Actions: core::fmt::Debug + Send + Copy + Clone + PartialEq {}

impl Actions for () {}
