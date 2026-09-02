include!(concat!(env!("OUT_DIR"), "/config.rs"));

#[path = "service/dispatcher.rs"]
pub mod dispatcher;
#[path = "service/taskwarrior_cached_commands.rs"]
pub mod taskwarrior_cached_commands;
#[path = "service/taskwarrior_passthrough_commands.rs"]
pub mod taskwarrior_passthrough_commands;
