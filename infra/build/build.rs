use std::collections::BTreeMap;
use std::env;
use std::fs;
use std::path::PathBuf;

fn parse_properties(path: &PathBuf) -> BTreeMap<String, String> {
    let content = fs::read_to_string(path).expect("read properties file");
    let mut map = BTreeMap::new();
    for line in content.lines() {
        let line = line.trim_end();
        let trimmed = line.trim_start();
        if trimmed.is_empty() || trimmed.starts_with('#') || trimmed.starts_with('!') {
            continue;
        }
        if let Some(eq) = line.find('=') {
            let key = line[..eq].trim().to_string();
            let value = line[eq + 1..].trim().to_string();
            map.insert(key, value);
        }
    }
    map
}

fn main() {
    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    let base = manifest_dir.join("src/main/resources/application.properties");
    let test_override = manifest_dir.join("src/test/resources/application-test.properties");

    let mut merged = parse_properties(&base);

    // The test override is merged ONLY in debug builds; release ignores it.
    let profile = env::var("PROFILE").unwrap_or_else(|_| "release".to_string());
    if profile == "debug" {
        let overrides = parse_properties(&test_override);
        for (k, v) in overrides {
            merged.insert(k, v);
        }
    }

    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());
    let dest = out_dir.join("config.rs");

    let mut out = String::from("pub mod generated_config {\n");
    out.push_str("    pub const ENTRIES: &[(&str, &str)] = &[\n");
    for (k, v) in &merged {
        out.push_str(&format!("        ({:?}, {:?}),\n", k, v));
    }
    out.push_str("    ];\n");
    out.push_str("}\n");

    fs::write(&dest, out).expect("write config.rs");

    println!("cargo:rerun-if-changed={}", base.display());
    println!("cargo:rerun-if-changed={}", test_override.display());
}
