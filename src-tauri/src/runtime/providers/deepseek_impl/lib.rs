#[path = "main.rs"]
mod inner;

pub use inner::{serve_embedded, DeepseekServiceConfig};
