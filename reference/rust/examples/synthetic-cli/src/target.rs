mod format;
mod pipeline;

pub use format::demo_source;
pub use pipeline::{BuildResult, build};

#[cfg(test)]
#[path = "target_tests.rs"]
mod tests;
