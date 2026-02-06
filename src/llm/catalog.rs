use std::collections::HashMap;

use serde::Deserialize;

const CATALOG_JSON: &str = include_str!(concat!(env!("OUT_DIR"), "/model_catalog.json"));

#[derive(Debug, Clone, Deserialize)]
pub struct CatalogEntry {
    pub id: String,
    pub name: String,
    pub context: u64,
}

pub fn load_catalog() -> HashMap<String, Vec<CatalogEntry>> {
    serde_json::from_str(CATALOG_JSON).unwrap_or_default()
}

pub fn models_for_provider(provider: &str) -> Vec<CatalogEntry> {
    load_catalog().remove(provider).unwrap_or_default()
}

pub fn context_limit_for_model(provider: &str, model: &str) -> Option<u64> {
    models_for_provider(provider)
        .iter()
        .find(|e| e.id == model)
        .map(|e| e.context)
}
