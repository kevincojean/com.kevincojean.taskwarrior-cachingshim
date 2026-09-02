use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use crate::dispatcher::{CommandClass, Config};
use crate::taskwarrior_passthrough_commands as passthrough;

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
            emit(&result);
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
        emit(&result);
        result.exit_code
    }

    fn compute_key(&self, class: &CommandClass, argv: &[String], config: &Config) -> String {
        let mut material = String::new();
        material.push_str(class.config_key().unwrap_or(""));
        for a in argv {
            material.push('\u{0}');
            material.push_str(a);
        }
        for mtime in self.taskrc_mtimes(config) {
            material.push_str(&format!(",{}", mtime));
        }
        material.push_str(&format!(
            ",{}",
            file_mtime(Path::new(&config.task_path)).unwrap_or(0)
        ));
        format!("{:016x}", fnv1a(material.as_bytes()))
    }

    fn taskrc_mtimes(&self, _config: &Config) -> Vec<u64> {
        let taskrc = std::env::var("TASKRC")
            .map(PathBuf::from)
            .unwrap_or_else(|_| {
                let home = std::env::var("HOME").unwrap_or_default();
                PathBuf::from(home).join(".taskrc")
            });
        let mut files = Vec::new();
        collect_include_chain(&taskrc, &mut files);
        files.iter().map(|p| file_mtime(p).unwrap_or(0)).collect()
    }

    fn cache_path(&self, key: &str) -> PathBuf {
        self.dir.join(format!("{key}.cache"))
    }

    fn get(&self, key: &str) -> Option<CachedResult> {
        let bytes = fs::read(self.cache_path(key)).ok()?;
        parse_cache(&bytes)
    }

    fn put(&self, key: &str, result: &CachedResult) {
        if !self.dir.exists() {
            fs::create_dir_all(&self.dir).unwrap();
        }
        let path = self.cache_path(key);
        let tmp = self.dir.join(format!("{key}.tmp"));
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&result.exit_code.to_le_bytes());
        bytes.extend_from_slice(&(result.stdout.len() as u32).to_le_bytes());
        bytes.extend_from_slice(&result.stdout);
        bytes.extend_from_slice(&(result.stderr.len() as u32).to_le_bytes());
        bytes.extend_from_slice(&result.stderr);
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

fn file_mtime(path: &Path) -> Option<u64> {
    let meta = fs::metadata(path).ok()?;
    let modified = meta.modified().ok()?;
    Some(
        modified
            .duration_since(std::time::UNIX_EPOCH)
            .ok()?
            .as_nanos() as u64,
    )
}

fn parse_cache(bytes: &[u8]) -> Option<CachedResult> {
    let mut idx = 0;
    let exit_code = i32::from_le_bytes(bytes.get(idx..idx + 4)?.try_into().ok()?);
    idx += 4;
    let stdout_len = u32::from_le_bytes(bytes.get(idx..idx + 4)?.try_into().ok()?);
    idx += 4;
    let stdout = bytes.get(idx..idx + stdout_len as usize)?.to_vec();
    idx += stdout_len as usize;
    let stderr_len = u32::from_le_bytes(bytes.get(idx..idx + 4)?.try_into().ok()?);
    idx += 4;
    let stderr = bytes.get(idx..idx + stderr_len as usize)?.to_vec();
    Some(CachedResult {
        stdout,
        stderr,
        exit_code,
    })
}

fn emit(result: &CachedResult) {
    let mut out = std::io::stdout().lock();
    let _ = out.write_all(&result.stdout);
    let _ = out.flush();
    let mut err = std::io::stderr().lock();
    let _ = err.write_all(&result.stderr);
    let _ = err.flush();
}

fn fnv1a(bytes: &[u8]) -> u64 {
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
