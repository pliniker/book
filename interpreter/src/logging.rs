//! Logger initialization using env_logger.
//!
//! This module initializes the env_logger for the evalrus interpreter.
//! The log level is controlled via the RUST_LOG environment variable:
//! - `RUST_LOG=debug` - Debug level and above
//! - `RUST_LOG=trace` - Trace level and above
//! - `RUST_LOG=evalrus::compiler=debug` - Module-specific filtering
//!
//! Compile-time maximum log level (set in Cargo.toml):
//! - Debug builds: Trace level available
//! - Release builds: Error level only

/// Initialize the global logger.
///
/// This function must be called once at the start of the program, before any logging calls.
/// It configures env_logger to use the RUST_LOG environment variable for filtering.
pub fn init() {
    env_logger::Builder::from_default_env()
        .format_timestamp(None)
        .init();
}
