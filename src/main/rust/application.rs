include!(concat!(env!("OUT_DIR"), "/config.rs"));

#[path = "service/dispatcher.rs"]
mod dispatcher;
#[path = "service/taskwarrior_cached_commands.rs"]
mod taskwarrior_cached_commands;
#[path = "service/taskwarrior_passthrough_commands.rs"]
mod taskwarrior_passthrough_commands;

use dispatcher::{Config, Dispatcher};

fn main() {
    let config = Config::from_generated();
    let mut dispatcher = Dispatcher::new(config);
    let argv: Vec<String> = std::env::args().collect();
    std::process::exit(dispatcher.dispatch(&argv));
}
