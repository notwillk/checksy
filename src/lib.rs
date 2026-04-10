pub mod schema;
pub mod version;
pub mod doctor;
pub mod config;
pub mod cli;

pub use schema::{Config, Rule, Severity};
pub use version::VERSION;
pub use doctor::{diagnose, Options, Report, RuleResult};
pub use config::{load, resolve_path};
pub use cli::run;