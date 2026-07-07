pub mod browser_bridge;
pub mod proxy_core;

#[path = "providers/browser_runtime.rs"]
pub mod browser_runtime;
#[path = "providers/deepseek_impl/lib.rs"]
pub mod deepseek;
#[path = "hub_impl/lib.rs"]
pub mod hub;
#[path = "providers/kimi_impl/lib.rs"]
pub mod kimi;
#[path = "providers/qwen_impl/lib.rs"]
pub mod qwen;

pub use browser_runtime::{
    serve_browser_provider, BrowserProviderKind, BrowserProviderServerConfig,
};
pub use deepseek::{serve_embedded as serve_deepseek, DeepseekServiceConfig};
pub use hub::{serve_embedded as serve_hub, HubServiceConfig, ProviderConfig};
pub use kimi::{serve_embedded as serve_kimi, KimiServiceConfig};
pub use qwen::{build_embedded_config, serve_embedded as serve_qwen};
