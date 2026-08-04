mod control_room;
mod runtime;

pub use crate::runtime::{browser_bridge, ids, proxy_core};

pub fn run() {
    control_room::run()
}
