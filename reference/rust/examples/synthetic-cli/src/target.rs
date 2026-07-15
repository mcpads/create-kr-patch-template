mod format;
mod pipeline;

pub use format::{DEMO_SOURCE_ID, demo_source, source_spec};
pub use pipeline::{BuildResult, build};

#[cfg(test)]
#[path = "target_tests.rs"]
mod tests;
