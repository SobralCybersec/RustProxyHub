#[path = "main.rs"]
mod inner;

pub use inner::config::build_embedded_config;
pub use inner::serve_embedded;
