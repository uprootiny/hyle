//! Model list management with caching
//!
//! Fetches models from OpenRouter, caches them, filters for free models.

#![allow(dead_code)] // Utility functions for model filtering

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::fs;

use crate::config;

/// Model information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Model {
    pub id: String,
    pub name: String,
    pub context_length: u32,
    pub pricing_prompt: f64,
    pub pricing_completion: f64,
}

impl Model {
    /// Check if model is free (both prompt and completion pricing are 0)
    pub fn is_free(&self) -> bool {
        self.pricing_prompt == 0.0 && self.pricing_completion == 0.0
    }

    /// Get display name (shorter version for UI)
    pub fn display_name(&self) -> String {
        // Extract just the model name without provider prefix for display
        if let Some(name) = self.id.split('/').nth(1) {
            name.to_string()
        } else {
            self.id.clone()
        }
    }
}

/// Cached models data
#[derive(Debug, Serialize, Deserialize)]
pub struct ModelsCache {
    pub models: Vec<Model>,
    pub fetched_at: chrono::DateTime<chrono::Utc>,
}

/// Cache file path
fn cache_path() -> Result<std::path::PathBuf> {
    Ok(config::cache_dir()?.join("models.json"))
}

/// Load models from cache
pub fn load_cache() -> Result<Option<ModelsCache>> {
    let path = cache_path()?;
    if !path.exists() {
        return Ok(None);
    }

    let content =
        fs::read_to_string(&path).with_context(|| format!("Failed to read {}", path.display()))?;

    let cache: ModelsCache = serde_json::from_str(&content)
        .with_context(|| format!("Failed to parse {}", path.display()))?;

    // Check if cache is stale (older than 24 hours)
    let age = chrono::Utc::now() - cache.fetched_at;
    if age > chrono::Duration::hours(24) {
        return Ok(None);
    }

    Ok(Some(cache))
}

/// Save models to cache
pub fn save_cache(models: &[Model]) -> Result<()> {
    config::ensure_dirs()?;
    let path = cache_path()?;

    let cache = ModelsCache {
        models: models.to_vec(),
        fetched_at: chrono::Utc::now(),
    };

    let content = serde_json::to_string_pretty(&cache)?;
    fs::write(&path, &content).with_context(|| format!("Failed to write {}", path.display()))?;

    Ok(())
}

/// Load models from cache or fetch from API
pub async fn load_or_fetch(api_key: &str) -> Result<Vec<Model>> {
    // Try cache first
    if let Some(cache) = load_cache()? {
        return Ok(cache.models);
    }

    // Fetch from API
    let models = crate::client::fetch_models(api_key).await?;
    save_cache(&models)?;
    Ok(models)
}

/// Get just the free models, sorted by context length
pub fn get_free_models(models: &[Model]) -> Vec<&Model> {
    let mut free: Vec<_> = models.iter().filter(|m| m.is_free()).collect();
    free.sort_by(|a, b| b.context_length.cmp(&a.context_length));
    free
}

/// Get pricing for a model (prompt, completion) in $/1M tokens
/// Returns (0.0, 0.0) for free models
pub fn get_model_pricing(model_id: &str) -> (f64, f64) {
    // Try to load from cache
    if let Ok(Some(cache)) = load_cache() {
        if let Some(model) = cache.models.iter().find(|m| m.id == model_id) {
            return (model.pricing_prompt, model.pricing_completion);
        }
    }
    // Default to free for unknown models
    (0.0, 0.0)
}

/// Calculate cost for a request
pub fn calculate_cost(model_id: &str, prompt_tokens: u32, completion_tokens: u32) -> f64 {
    let (prompt_price, completion_price) = get_model_pricing(model_id);
    // Prices are typically per 1M tokens
    (prompt_tokens as f64 * prompt_price / 1_000_000.0)
        + (completion_tokens as f64 * completion_price / 1_000_000.0)
}

/// Get context window for a model (from cache or default)
pub fn get_context_window(model_id: &str) -> u32 {
    // Try to load from cache
    if let Ok(Some(cache)) = load_cache() {
        if let Some(model) = cache.models.iter().find(|m| m.id == model_id) {
            return model.context_length;
        }
    }

    // Default context windows for common free models
    match model_id {
        m if m.contains("llama-3.2-3b") => 8192,
        m if m.contains("llama-3.1-8b") => 16384,
        m if m.contains("gemma") => 8192,
        m if m.contains("mistral") => 8192,
        m if m.contains("qwen") => 8192,
        m if m.contains("deepseek") => 131072,
        _ => 8192, // Conservative default
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_model_is_free() {
        let free = Model {
            id: "test/model:free".to_string(),
            name: "Test Model".to_string(),
            context_length: 8192,
            pricing_prompt: 0.0,
            pricing_completion: 0.0,
        };
        assert!(free.is_free());

        let paid = Model {
            id: "test/model".to_string(),
            name: "Test Model".to_string(),
            context_length: 8192,
            pricing_prompt: 0.001,
            pricing_completion: 0.002,
        };
        assert!(!paid.is_free());
    }

    #[test]
    fn test_display_name() {
        let model = Model {
            id: "meta-llama/llama-3.2-3b-instruct:free".to_string(),
            name: "Llama 3.2 3B".to_string(),
            context_length: 8192,
            pricing_prompt: 0.0,
            pricing_completion: 0.0,
        };
        assert_eq!(model.display_name(), "llama-3.2-3b-instruct:free");
    }
}
