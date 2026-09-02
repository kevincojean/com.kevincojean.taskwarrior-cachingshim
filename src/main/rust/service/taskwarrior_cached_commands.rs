use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::dispatcher::{CommandClass, Config};
use crate::taskwarrior_passthrough_commands as passthrough;

#[derive(Serialize, Deserialize)]
pub struct CachedResult {
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
    pub exit_code: i32,
}

pub struct Cache {
    dir: PathBuf,
    log: Option<PathBuf>,
}

impl Cache {
    pub fn new(config: &Config) -> Cache {
        Cache {
            dir: config.cache_dir.join("entries"),
            log: config.log_output.clone(),
        }
    }

    pub fn run(&mut self, class: &CommandClass, argv: &[String], config: &Config) -> i32 {
        let key = self.compute_key(class, argv, config);
        if let Some(result) = self.get(&key) {
            self.log("HIT", &key);
            replay_output(&result);
            return result.exit_code;
        }
        self.log("MISS", &key);
        let captured = passthrough::capture(argv, config);
        let result = CachedResult {
            stdout: captured.stdout,
            stderr: captured.stderr,
            exit_code: captured.exit_code,
        };
        self.put(&key, &result);
        replay_output(&result);
        result.exit_code
    }

    fn compute_key(&self, class: &CommandClass, argv: &[String], config: &Config) -> String {
        let mut material = String::new();
        material.push_str(class.config_key().unwrap_or(""));
        for a in argv {
            material.push('\u{0}');
            material.push_str(a);
        }
        for mtime in self.taskrc_last_modified_times(config) {
            material.push_str(&format!(",{}", mtime));
        }
        material.push_str(&format!(
            ",{}",
            file_last_modified_ns(Path::new(&config.task_path)).unwrap_or(0)
        ));
        format!("{:016x}", fnv1a_algorithm(material.as_bytes()))
    }

    fn taskrc_last_modified_times(&self, _config: &Config) -> Vec<u64> {
        let taskrc = std::env::var("TASKRC")
            .map(PathBuf::from)
            .unwrap_or_else(|_| {
                let home = std::env::var("HOME").unwrap_or_default();
                PathBuf::from(home).join(".taskrc")
            });
        let mut files = Vec::new();
        collect_include_chain(&taskrc, &mut files);
        files
            .iter()
            .map(|p| file_last_modified_ns(p).unwrap_or(0))
            .collect()
    }

    fn cache_path(&self, key: &str) -> PathBuf {
        self.dir.join(format!("{key}.cache"))
    }

    fn get(&self, key: &str) -> Option<CachedResult> {
        let bytes = fs::read(self.cache_path(key)).ok()?;
        bincode::deserialize(&bytes).ok()
    }

    fn put(&self, key: &str, result: &CachedResult) {
        if !self.dir.exists() {
            fs::create_dir_all(&self.dir).unwrap();
        }
        let path = self.cache_path(key);
        let tmp = self.dir.join(format!("{key}.tmp"));
        let bytes = bincode::serialize(result).unwrap();
        fs::write(&tmp, bytes).unwrap();
        fs::rename(&tmp, &path).unwrap();
    }

    fn log(&self, tag: &str, key: &str) {
        if let Some(path) = &self.log {
            if let Some(parent) = path.parent() {
                let _ = fs::create_dir_all(parent);
            }
            let mut f = fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(path)
                .unwrap();
            let _ = writeln!(f, "{} {}", tag, key);
        }
    }
}

/// A taskrc can pull in other files via `include <path>` lines (themes,
/// UDAs, secrets). Config edits in those files must invalidate the cache
/// too, so the cache key collects the last-modified time of every file in
/// the include chain, not just the top-level taskrc.
fn collect_include_chain(path: &Path, out: &mut Vec<PathBuf>) {
    if let Ok(content) = fs::read_to_string(path) {
        out.push(path.to_path_buf());
        for line in content.lines() {
            if let Some(rest) = line.trim_start().strip_prefix("include ") {
                let inc = PathBuf::from(rest.trim());
                collect_include_chain(&inc, out);
            }
        }
    }
}

fn file_last_modified_ns(path: &Path) -> Option<u64> {
    let meta = fs::metadata(path).ok()?;
    let modified = meta.modified().ok()?;
    Some(
        modified
            .duration_since(std::time::UNIX_EPOCH)
            .ok()?
            .as_nanos() as u64,
    )
}

fn replay_output(result: &CachedResult) {
    let mut out = std::io::stdout().lock();
    let _ = out.write_all(&result.stdout);
    let _ = out.flush();
    let mut err = std::io::stderr().lock();
    let _ = err.write_all(&result.stderr);
    let _ = err.flush();
}

/// FNV-1a: chosen because it is deterministic across processes (cache
/// entries written by one run must be found by the next), needs no
/// dependency, and non-cryptographic is fine - it hashes argv and
/// mtimes for a cache filename, nothing security-sensitive.
fn fnv1a_algorithm(bytes: &[u8]) -> u64 {
    let mut hash: u64 = 0xcbf29ce484222325;
    for b in bytes {
        hash ^= *b as u64;
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cache_key_is_deterministic_and_argv_sensitive() {
        let config = Config::from_generated();
        let cache = Cache::new(&config);
        let argv_a = vec!["task".to_string(), "show".to_string()];
        let argv_b = vec![
            "task".to_string(),
            "show".to_string(),
            "report.next.columns".to_string(),
        ];
        let k1 = cache.compute_key(&CommandClass::ShowConfig, &argv_a, &config);
        let k2 = cache.compute_key(&CommandClass::ShowConfig, &argv_a, &config);
        assert_eq!(k1, k2);
        let k3 = cache.compute_key(&CommandClass::ShowConfig, &argv_b, &config);
        assert_ne!(k1, k3);
    }
}
