use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::Mutex;

static SERIAL: Mutex<()> = Mutex::new(());

const TEST_DIR: &str = "/tmp/taskwarrior-cachingshim-test";
const LOG_FILE: &str = "/tmp/taskwarrior-cachingshim-test/shim.log";

const TASKRC_CONTENT: &str = "\
verbose=blank,header,footnote,label,new-id,news,affected,edit,special,sync,recur,context
report.next.filter=(status:pending or status:waiting) -BLOCKED
report.next.columns=id,start.age,urgency,due.relative,scheduled.relative,project,description.truncated,recur,priority,tags
report.next.labels=id,age,urg,due,sche,pr,descr,recur,p,tag
report.next.dateformat=M-D
context=99_inbox
context.99_inbox.read=-WAITING and -buy and project.not:blog.post and (+zombie or +notsmart or +inbox or (-niu -iu -inu -ninu))
";

fn shim_bin() -> &'static str {
    env!("CARGO_BIN_EXE_taskwarrior-cachingshim")
}

fn real_task() -> &'static str {
    "/usr/local/bin/task"
}

fn vectors() -> Vec<Vec<String>> {
    let raw = fs::read_to_string("src/test/resources/commands-to-test.jsonl").unwrap();
    raw.lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| serde_json::from_str(l).unwrap())
        .collect()
}

fn setup_isolated_env() -> (PathBuf, PathBuf) {
    let _ = fs::remove_dir_all(TEST_DIR);
    fs::create_dir_all(format!("{TEST_DIR}/data")).unwrap();
    let taskrc = PathBuf::from(format!("{TEST_DIR}/taskrc"));
    fs::write(&taskrc, TASKRC_CONTENT).unwrap();
    (PathBuf::from(format!("{TEST_DIR}/data")), taskrc)
}

fn fresh_data_dir(name: &str) -> PathBuf {
    let dir = PathBuf::from(format!("{TEST_DIR}/data-{name}"));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn run_shim(args: &[String], data: &Path, taskrc: &Path) -> Output {
    Command::new(shim_bin())
        .args(args)
        .env("TASKDATA", data)
        .env("TASKRC", taskrc)
        .output()
        .unwrap()
}

fn run_real(args: &[String], data: &Path, taskrc: &Path) -> Output {
    Command::new(real_task())
        .args(args)
        .env("TASKDATA", data)
        .env("TASKRC", taskrc)
        .output()
        .unwrap()
}

fn is_cacheable(argv: &[String]) -> bool {
    let mut iter = argv.iter().skip(1).filter(|a| !a.starts_with("rc."));
    match iter.next().map(String::as_str) {
        Some("_get") | Some("show") => true,
        Some("context") => iter.next().is_none(),
        Some("--version") => iter.next().is_none(),
        _ => false,
    }
}

fn log_hit_count() -> usize {
    let content = fs::read_to_string(LOG_FILE).unwrap_or_default();
    content.lines().filter(|l| l.starts_with("HIT")).count()
}

fn log_miss_count() -> usize {
    let content = fs::read_to_string(LOG_FILE).unwrap_or_default();
    content.lines().filter(|l| l.starts_with("MISS")).count()
}

#[test]
fn given_cacheable_read_when_run_twice_then_second_is_hit() {
    let _guard = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let (data, taskrc) = setup_isolated_env();
    let cacheable: Vec<_> = vectors().into_iter().filter(|v| is_cacheable(v)).collect();
    assert!(!cacheable.is_empty());

    let mut hits = 0;
    let mut misses = 0;
    for argv in &cacheable {
        let args: Vec<String> = argv[1..].to_vec();
        let real = run_real(&args, &data, &taskrc);
        let first = run_shim(&args, &data, &taskrc);
        assert_eq!(
            first.stdout, real.stdout,
            "stdout byte-identity (first run): {:?}",
            argv
        );
        assert_eq!(
            first.stderr, real.stderr,
            "stderr byte-identity (first run): {:?}",
            argv
        );
        assert_eq!(
            first.status.code(),
            real.status.code(),
            "exit code (first run): {:?}",
            argv
        );
        let second = run_shim(&args, &data, &taskrc);
        assert_eq!(
            second.stdout, first.stdout,
            "2nd run stdout identical: {:?}",
            argv
        );
        assert_eq!(
            second.stderr, first.stderr,
            "2nd run stderr identical: {:?}",
            argv
        );
        assert_eq!(
            second.status.code(),
            first.status.code(),
            "2nd run exit identical: {:?}",
            argv
        );
        hits += 1;
        misses += 1;
    }
    assert_eq!(
        log_hit_count(),
        hits,
        "each cacheable vector must log one HIT on 2nd run"
    );
    assert_eq!(
        log_miss_count(),
        misses,
        "each cacheable vector must log one MISS on 1st run"
    );
}

#[test]
fn given_passthrough_command_when_run_then_always_execs_real_task() {
    let _guard = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let (_, taskrc) = setup_isolated_env();
    let passthrough: Vec<_> = vectors().into_iter().filter(|v| !is_cacheable(v)).collect();
    assert!(!passthrough.is_empty());

    for argv in &passthrough {
        let args: Vec<String> = argv[1..].to_vec();
        let real_data = fresh_data_dir("real");
        let shim_data = fresh_data_dir("shim");
        let real = run_real(&args, &real_data, &taskrc);
        let shim = run_shim(&args, &shim_data, &taskrc);
        assert_eq!(
            shim.stdout, real.stdout,
            "stdout byte-identity (passthrough): {:?}",
            argv
        );
        assert_eq!(
            shim.stderr, real.stderr,
            "stderr byte-identity (passthrough): {:?}",
            argv
        );
        assert_eq!(
            shim.status.code(),
            real.status.code(),
            "exit code (passthrough): {:?}",
            argv
        );
    }
    assert_eq!(log_hit_count(), 0, "passthrough must never be cached");
}

#[test]
fn given_warm_cache_when_taskrc_touched_then_next_run_misses() {
    let _guard = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let (data, taskrc) = setup_isolated_env();
    let argv = vectors().into_iter().find(|v| is_cacheable(v)).unwrap();
    let args: Vec<String> = argv[1..].to_vec();

    run_shim(&args, &data, &taskrc);
    run_shim(&args, &data, &taskrc);
    assert_eq!(log_hit_count(), 1, "second run must be a HIT");

    let mut touched = TASKRC_CONTENT.to_string();
    touched.push_str("\n# touched\n");
    fs::write(&taskrc, touched).unwrap();
    let after_touch = run_shim(&args, &data, &taskrc);
    assert_eq!(after_touch.status.code(), Some(0));
    assert_eq!(
        log_miss_count(),
        2,
        "touching taskrc must invalidate the cache key -> MISS"
    );
}

#[test]
fn given_context_switch_when_run_twice_then_never_cached() {
    let _guard = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let (data, taskrc) = setup_isolated_env();
    let mut content = TASKRC_CONTENT.to_string();
    content.push_str("context.home.read=project:home\n");
    content.push_str("context.work.read=project:work\n");
    content.push_str("context=home\n");
    fs::write(&taskrc, content).unwrap();

    let args = vec!["context".to_string(), "work".to_string()];
    run_shim(&args, &data, &taskrc);
    run_shim(&args, &data, &taskrc);
    assert_eq!(
        log_hit_count(),
        0,
        "context <name> is a write and must never be cached"
    );
    assert_eq!(
        log_miss_count(),
        0,
        "context <name> must never reach the cache module at all"
    );
    let final_content = fs::read_to_string(&taskrc).unwrap();
    assert!(
        final_content.contains("context=work"),
        "taskrc must be updated to context=work"
    );
}

#[test]
fn given_warm_cache_when_add_passes_then_cache_survives() {
    let _guard = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let (data, taskrc) = setup_isolated_env();
    let get_args = vec!["_get".to_string(), "rc.context".to_string()];

    // warm the cache for the config read
    run_shim(&get_args, &data, &taskrc);
    run_shim(&get_args, &data, &taskrc);
    assert_eq!(log_hit_count(), 1, "2nd config read must be a HIT");

    // a mutation op passes through but must not wipe the cache
    let add_data = fresh_data_dir("add");
    let add_args = vec!["add".to_string(), "STRACE-PROBE delete me".to_string()];
    let add = run_shim(&add_args, &add_data, &taskrc);
    assert_eq!(add.status.code(), Some(0));

    // the next config read is still a HIT and the cache dir survives
    run_shim(&get_args, &data, &taskrc);
    assert_eq!(log_hit_count(), 2, "add must not clear the config cache");
    assert_eq!(log_miss_count(), 1, "only the first config read may MISS");
}
