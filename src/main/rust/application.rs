use taskwarrior_cachingshim::dispatcher::{Config, Dispatcher};

fn main() {
    let config = Config::from_generated();
    let mut dispatcher = Dispatcher::new(config);
    let argv: Vec<String> = std::env::args().collect();
    std::process::exit(dispatcher.dispatch(&argv));
}
