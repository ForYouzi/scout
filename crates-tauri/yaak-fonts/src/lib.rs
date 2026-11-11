use tauri::{
    generate_handler,
    plugin::{Builder, TauriPlugin},
    Runtime,
};

mod commands;
mod error;

use crate::commands::{faces, list};
pub use error::{Error, Result};

pub fn init<R: Runtime>() -> TauriPlugin<R> {
    Builder::new("yaak-fonts").invoke_handler(generate_handler![list, faces]).build()
}
