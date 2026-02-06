use serde::Deserialize;
use std::collections::HashMap;
use std::io::Write;

const MODELS_DEV_URL: &str = "https://models.dev/api.json";
const MODELS_PER_PROVIDER: usize = 5;

const PROVIDERS: &[(&str, &str)] = &[
    ("anthropic", "anthropic"),
    ("openai", "openai"),
    ("moonshot", "moonshotai"),
    ("openrouter", "openrouter"),
];

#[derive(Deserialize)]
struct Provider {
    models: Option<HashMap<String, Model>>,
}

#[derive(Deserialize)]
struct Model {
    id: String,
    name: Option<String>,
    family: Option<String>,
    limit: Option<Limit>,
    release_date: Option<String>,
}

#[derive(Deserialize)]
struct Limit {
    context: Option<u64>,
}

#[derive(serde::Serialize)]
struct CatalogEntry {
    id: String,
    name: String,
    context: u64,
}

fn is_chat_model(model: &Model) -> bool {
    let id = model.id.to_lowercase();
    let family = model.family.as_deref().unwrap_or("").to_lowercase();

    if id.contains("embed") || family.contains("embed") {
        return false;
    }
    if id.contains("tts") || id.contains("whisper") || id.contains("dall-e") {
        return false;
    }
    if id.contains("realtime") || id.contains("audio") {
        return false;
    }

    let context = model.limit.as_ref().and_then(|l| l.context).unwrap_or(0);
    context > 0
}

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=.git/HEAD");
    println!("cargo:rerun-if-changed=.git/refs/heads");

    embed_version_info();

    let catalog = match fetch_catalog() {
        Ok(c) => c,
        Err(e) => {
            eprintln!(
                "cargo:warning=Failed to fetch models.dev catalog: {e}. Using empty catalog."
            );
            HashMap::new()
        }
    };

    let out_dir = std::env::var("OUT_DIR").unwrap();
    let path = std::path::Path::new(&out_dir).join("model_catalog.json");
    let mut file = std::fs::File::create(&path).expect("Failed to create model_catalog.json");
    let json = serde_json::to_string_pretty(&catalog).unwrap();
    file.write_all(json.as_bytes()).unwrap();
}

fn embed_version_info() {
    let git_hash = std::process::Command::new("git")
        .args(["rev-parse", "--short", "HEAD"])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_else(|| "unknown".to_string());

    let git_dirty = std::process::Command::new("git")
        .args(["status", "--porcelain"])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| if o.stdout.is_empty() { "" } else { "-dirty" })
        .unwrap_or("");

    let build_date = std::process::Command::new("date")
        .args(["+%Y-%m-%d"])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_else(|| "unknown".to_string());

    println!(
        "cargo:rustc-env=MUESLI_VERSION_INFO={}{} {}",
        git_hash, git_dirty, build_date
    );

    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").unwrap_or_default();
    println!("cargo:rustc-env=MUESLI_SOURCE_DIR={}", manifest_dir);
}

fn fetch_catalog() -> Result<HashMap<String, Vec<CatalogEntry>>, Box<dyn std::error::Error>> {
    let body: String = ureq::get(MODELS_DEV_URL)
        .call()?
        .body_mut()
        .read_to_string()?;
    let data: HashMap<String, Provider> = serde_json::from_str(&body)?;

    let mut catalog = HashMap::new();

    for &(our_key, api_key) in PROVIDERS {
        let provider = match data.get(api_key) {
            Some(p) => p,
            None => continue,
        };

        let models = match &provider.models {
            Some(m) => m,
            None => continue,
        };

        let mut entries: Vec<(String, CatalogEntry)> = models
            .values()
            .filter(|m| is_chat_model(m))
            .map(|m| {
                let date = m.release_date.clone().unwrap_or_default();
                let entry = CatalogEntry {
                    id: m.id.clone(),
                    name: m.name.clone().unwrap_or_else(|| m.id.clone()),
                    context: m.limit.as_ref().and_then(|l| l.context).unwrap_or(0),
                };
                (date, entry)
            })
            .collect();

        entries.sort_by(|a, b| b.0.cmp(&a.0));
        entries.truncate(MODELS_PER_PROVIDER);

        catalog.insert(
            our_key.to_string(),
            entries.into_iter().map(|(_, e)| e).collect(),
        );
    }

    Ok(catalog)
}
