use std::collections::HashMap;
use std::path::PathBuf;

use crate::taskwarrior_cached_commands::Cache;
use crate::taskwarrior_passthrough_commands as passthrough;

pub enum CommandClass {
    GetConfig,
    ShowConfig,
    ListContexts,
    Version,
    Passthrough,
}

impl CommandClass {
    pub fn config_key(&self) -> Option<&'static str> {
        match self {
            CommandClass::GetConfig => Some("cached_commands._get"),
            CommandClass::ShowConfig => Some("cached_commands.show"),
            CommandClass::ListContexts => Some("cached_commands.context"),
            CommandClass::Version => Some("cached_commands.version"),
            CommandClass::Passthrough => None,
        }
    }
}

pub fn classify(argv: &[String]) -> CommandClass {
    let mut iter = argv.iter().skip(1).filter(|a| !a.starts_with("rc."));
    match iter.next().map(String::as_str) {
        Some("_get") => CommandClass::GetConfig,
        Some("show") => CommandClass::ShowConfig,
        Some("context") if iter.next().is_none() => CommandClass::ListContexts,
        Some("--version") if iter.next().is_none() => CommandClass::Version,
        _ => CommandClass::Passthrough,
    }
}

pub struct Config {
    pub task_path: String,
    pub cache_dir: PathBuf,
    pub cached_commands: HashMap<&'static str, bool>,
    pub log_output: Option<PathBuf>,
}

impl Config {
    pub fn from_generated() -> Config {
        let mut task_path = "/usr/local/bin/task".to_string();
        let mut cache_dir = PathBuf::from("~/.cache/taskwarrior-cachingshim");
        let mut cached_commands = HashMap::new();
        let mut log_output = None;
        for &(k, v) in crate::generated_config::ENTRIES {
            match k {
                "task.path" => task_path = v.to_string(),
                "cache.dir" => cache_dir = expand_tilde(v),
                "cached_commands._get" => {
                    cached_commands.insert("cached_commands._get", v == "true");
                }
                "cached_commands.show" => {
                    cached_commands.insert("cached_commands.show", v == "true");
                }
                "cached_commands.context" => {
                    cached_commands.insert("cached_commands.context", v == "true");
                }
                "cached_commands.version" => {
                    cached_commands.insert("cached_commands.version", v == "true");
                }
                "logging.output_file_path" => log_output = Some(PathBuf::from(v)),
                _ => {}
            }
        }
        Config {
            task_path,
            cache_dir,
            cached_commands,
            log_output,
        }
    }

    pub fn is_cached(&self, class: &CommandClass) -> bool {
        match class.config_key() {
            Some(key) => self.cached_commands.get(key).copied().unwrap_or(false),
            None => false,
        }
    }
}

fn expand_tilde(value: &str) -> PathBuf {
    if let Some(rest) = value.strip_prefix("~/") {
        if let Ok(home) = std::env::var("HOME") {
            return PathBuf::from(home).join(rest);
        }
    }
    PathBuf::from(value)
}

pub struct Dispatcher {
    config: Config,
    cache: Cache,
}

impl Dispatcher {
    pub fn new(config: Config) -> Dispatcher {
        let cache = Cache::new(&config);
        Dispatcher { config, cache }
    }

    pub fn dispatch(&mut self, argv: &[String]) -> i32 {
        let class = classify(argv);
        match class {
            CommandClass::GetConfig
            | CommandClass::ShowConfig
            | CommandClass::ListContexts
            | CommandClass::Version
                if self.config.is_cached(&class) =>
            {
                self.cache.run(&class, argv, &self.config)
            }
            CommandClass::Passthrough => passthrough::run(argv, &self.config),
            _ => passthrough::run(argv, &self.config),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classify_recognizes_cacheable_shapes() {
        let get = classify(&[
            "task".to_string(),
            "_get".to_string(),
            "rc.context".to_string(),
        ]);
        assert!(matches!(get, CommandClass::GetConfig));
        let show = classify(&["task".to_string(), "show".to_string()]);
        assert!(matches!(show, CommandClass::ShowConfig));
        let ctx = classify(&["task".to_string(), "context".to_string()]);
        assert!(matches!(ctx, CommandClass::ListContexts));
        let ver = classify(&["task".to_string(), "--version".to_string()]);
        assert!(matches!(ver, CommandClass::Version));
        let ctx_work = classify(&[
            "task".to_string(),
            "context".to_string(),
            "work".to_string(),
        ]);
        assert!(matches!(ctx_work, CommandClass::Passthrough));
        let ver_arg = classify(&[
            "task".to_string(),
            "--version".to_string(),
            "extra".to_string(),
        ]);
        assert!(matches!(ver_arg, CommandClass::Passthrough));
        let summary = classify(&["task".to_string(), "summary".to_string()]);
        assert!(matches!(summary, CommandClass::Passthrough));
    }

    #[test]
    fn config_key_matches_cached_commands_property_names() {
        assert_eq!(
            CommandClass::GetConfig.config_key(),
            Some("cached_commands._get")
        );
        assert_eq!(
            CommandClass::ShowConfig.config_key(),
            Some("cached_commands.show")
        );
        assert_eq!(
            CommandClass::ListContexts.config_key(),
            Some("cached_commands.context")
        );
        assert_eq!(
            CommandClass::Version.config_key(),
            Some("cached_commands.version")
        );
        assert_eq!(CommandClass::Passthrough.config_key(), None);
    }
}
