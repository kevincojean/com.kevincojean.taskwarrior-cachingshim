use std::process::Command;

use crate::dispatcher::Config;

pub struct CapturedOutput {
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
    pub exit_code: i32,
}

pub fn run(argv: &[String], config: &Config) -> i32 {
    let status = Command::new(&config.task_path)
        .args(&argv[1..])
        .status()
        .unwrap_or_else(|e| {
            eprintln!(
                "taskwarrior-cachingshim: failed to exec {}: {}",
                config.task_path, e
            );
            std::process::exit(1);
        });
    status.code().unwrap_or(1)
}

pub fn capture(argv: &[String], config: &Config) -> CapturedOutput {
    let output = Command::new(&config.task_path)
        .args(&argv[1..])
        .output()
        .unwrap_or_else(|e| {
            eprintln!(
                "taskwarrior-cachingshim: failed to exec {}: {}",
                config.task_path, e
            );
            std::process::exit(1);
        });
    CapturedOutput {
        stdout: output.stdout,
        stderr: output.stderr,
        exit_code: output.status.code().unwrap_or(1),
    }
}
