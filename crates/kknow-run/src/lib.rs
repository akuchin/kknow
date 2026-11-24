pub mod error;
pub mod glob;
pub mod locate;
pub mod login;
pub mod outcome;
pub mod process;
pub mod runner;
pub mod spec;
pub mod target;

#[cfg(test)]
mod test_support;

pub use error::Error;
pub use locate::locate;
pub use outcome::{Outcome, RunEnd, Stream, Watch};
pub use process::{run, run_watched, run_with};
pub use runner::Runner;
pub use spec::{COMMANDS_FILE, CommandSet, CommandSpec, Cwd, Manifest};
pub use target::{Target, TargetKind, VaultRef, environment};

pub type Result<T, E = Error> = std::result::Result<T, E>;
