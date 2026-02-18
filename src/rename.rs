use std::collections::HashMap;
use std::env;
use std::sync::RwLock;

use tracing::info;

/// Bidirectional model name renamer.
///
/// Forward (rename): pattern-based, applied to each model ID when the model list
/// is fetched from Copilot. Handles two cases for claude models:
///   - Version-first: `claude-3.5-sonnet` → `claude-sonnet-3-5` (reorder + dot→dash)
///   - Variant-first: `claude-sonnet-4.5` → `claude-sonnet-4-5` (dot→dash only)
///
/// Reverse (resolve): uses a learned map built from the actual model list at startup.
/// Custom mappings from `MODEL_RENAME_MAP` take priority in both directions.
pub struct ModelRenamer {
    auto_enabled: bool,
    custom_forward: HashMap<String, String>,
    custom_reverse: HashMap<String, String>,
    learned_reverse: RwLock<HashMap<String, String>>,
}

/// Replace dots between digits with dashes: `4.6` → `4-6`, `3.5.1` → `3-5-1`.
/// Leaves other dots alone (e.g. hypothetical `v2.0` stays).
fn replace_version_dots(s: &str) -> String {
    let chars: Vec<char> = s.chars().collect();
    let mut result = String::with_capacity(s.len());
    for (i, &c) in chars.iter().enumerate() {
        if c == '.'
            && i > 0
            && chars[i - 1].is_ascii_digit()
            && i + 1 < chars.len()
            && chars[i + 1].is_ascii_digit()
        {
            result.push('-');
        } else {
            result.push(c);
        }
    }
    result
}

/// Pattern-based forward rename for claude models.
///
/// Version-first format (`claude-{version}-{variant}`):
///   `claude-3.5-sonnet` → `claude-sonnet-3-5`
///
/// Variant-first format (`claude-{variant}-{version}...`):
///   `claude-sonnet-4.5` → `claude-sonnet-4-5`
///   `claude-opus-4.6-fast` → `claude-opus-4-6-fast`
///
/// Returns None if no transformation is needed.
fn auto_rename(name: &str) -> Option<String> {
    let rest = name.strip_prefix("claude-")?;
    let segments: Vec<&str> = rest.split('-').collect();
    if segments.is_empty() {
        return None;
    }

    let first_is_version = segments[0]
        .chars()
        .next()
        .map(|c| c.is_ascii_digit())
        .unwrap_or(false);

    if first_is_version {
        // Version-first: claude-{version segments}-{variant segments}
        // Version segments start with a digit, variant segments don't.
        let mut version_end = 0;
        while version_end < segments.len()
            && segments[version_end]
                .chars()
                .next()
                .map(|c| c.is_ascii_digit())
                .unwrap_or(false)
        {
            version_end += 1;
        }
        if version_end == segments.len() {
            // Everything looks like a version, no variant to reorder.
            return None;
        }
        let version_raw = segments[..version_end].join("-");
        let variant = segments[version_end..].join("-");
        let version_dashed = replace_version_dots(&version_raw);
        Some(format!("claude-{variant}-{version_dashed}"))
    } else {
        // Variant-first: just normalize dots in the whole thing.
        let normalized = replace_version_dots(rest);
        if normalized == rest {
            None
        } else {
            Some(format!("claude-{normalized}"))
        }
    }
}

impl ModelRenamer {
    /// Build from environment variables:
    ///
    /// - `MODEL_RENAME_AUTO` — set to `"false"` to disable pattern-based auto renaming.
    ///   Default: enabled.
    /// - `MODEL_RENAME_MAP` — JSON object `{"upstream-name": "display-name", ...}`
    ///   applied on top of auto rules (custom entries take priority).
    pub fn from_env() -> Self {
        let auto_enabled = env::var("MODEL_RENAME_AUTO")
            .map(|v| v != "false")
            .unwrap_or(true);

        let custom: HashMap<String, String> = env::var("MODEL_RENAME_MAP")
            .ok()
            .and_then(|raw| match serde_json::from_str(&raw) {
                Ok(m) => Some(m),
                Err(e) => {
                    tracing::warn!(error = %e, "MODEL_RENAME_MAP is not valid JSON, ignoring");
                    None
                }
            })
            .unwrap_or_default();

        let custom_reverse: HashMap<String, String> =
            custom.iter().map(|(k, v)| (v.clone(), k.clone())).collect();

        if auto_enabled || !custom.is_empty() {
            info!(
                auto = auto_enabled,
                custom = custom.len(),
                "model renaming active"
            );
        }

        Self {
            auto_enabled,
            custom_forward: custom,
            custom_reverse,
            learned_reverse: RwLock::new(HashMap::new()),
        }
    }

    /// Map an upstream (Copilot) model ID to its display name.
    /// Custom mappings take priority over auto rules.
    /// Returns the original name unchanged if nothing matches.
    pub fn rename(&self, upstream_name: &str) -> String {
        if let Some(custom) = self.custom_forward.get(upstream_name) {
            return custom.clone();
        }
        if self.auto_enabled
            && let Some(renamed) = auto_rename(upstream_name)
        {
            return renamed;
        }
        upstream_name.to_string()
    }

    /// Record a concrete upstream↔display mapping learned from the model list.
    /// Called once per model when the model list is fetched.
    pub fn register(&self, upstream_name: &str, display_name: &str) {
        if upstream_name != display_name {
            self.learned_reverse
                .write()
                .unwrap()
                .insert(display_name.to_string(), upstream_name.to_string());
        }
    }

    /// Map a display name back to the upstream (Copilot) model ID.
    /// Priority: custom → learned (from model list) → pass through.
    pub fn resolve(&self, display_name: &str) -> String {
        if let Some(custom) = self.custom_reverse.get(display_name) {
            return custom.clone();
        }
        if let Some(learned) = self.learned_reverse.read().unwrap().get(display_name) {
            return learned.clone();
        }
        display_name.to_string()
    }

    pub fn has_rules(&self) -> bool {
        self.auto_enabled || !self.custom_forward.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn renamer(auto: bool, custom: &[(&str, &str)]) -> ModelRenamer {
        let custom_forward: HashMap<String, String> = custom
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();
        let custom_reverse = custom_forward
            .iter()
            .map(|(k, v)| (v.clone(), k.clone()))
            .collect();
        ModelRenamer {
            auto_enabled: auto,
            custom_forward,
            custom_reverse,
            learned_reverse: RwLock::new(HashMap::new()),
        }
    }

    /// Simulate what main.rs does: rename each model + register.
    fn apply_model_list(r: &ModelRenamer, models: &[&str]) -> Vec<(String, String)> {
        let mut results = Vec::new();
        for &m in models {
            let display = r.rename(m);
            r.register(m, &display);
            results.push((m.to_string(), display));
        }
        results
    }

    // --- auto_rename unit tests ---

    #[test]
    fn auto_rename_version_first() {
        assert_eq!(
            auto_rename("claude-3.5-sonnet"),
            Some("claude-sonnet-3-5".into())
        );
        assert_eq!(
            auto_rename("claude-3.5-haiku"),
            Some("claude-haiku-3-5".into())
        );
        assert_eq!(auto_rename("claude-3-opus"), Some("claude-opus-3".into()));
    }

    #[test]
    fn auto_rename_variant_first_with_dots() {
        assert_eq!(
            auto_rename("claude-sonnet-4.5"),
            Some("claude-sonnet-4-5".into())
        );
        assert_eq!(
            auto_rename("claude-opus-4.6"),
            Some("claude-opus-4-6".into())
        );
        assert_eq!(
            auto_rename("claude-opus-4.6-fast"),
            Some("claude-opus-4-6-fast".into())
        );
        assert_eq!(
            auto_rename("claude-sonnet-4.6"),
            Some("claude-sonnet-4-6".into())
        );
        assert_eq!(
            auto_rename("claude-haiku-4.5"),
            Some("claude-haiku-4-5".into())
        );
    }

    #[test]
    fn auto_rename_no_change_needed() {
        assert_eq!(auto_rename("claude-sonnet-4"), None);
        assert_eq!(auto_rename("claude-opus-4"), None);
        assert_eq!(auto_rename("gpt-4o"), None);
        assert_eq!(auto_rename("o1-mini"), None);
    }

    // --- full model list from Copilot ---

    #[test]
    fn real_copilot_model_list() {
        let r = renamer(true, &[]);
        let models = &[
            "claude-opus-4.6-fast",
            "claude-opus-4.6",
            "claude-sonnet-4.6",
            "gpt-5.2-codex",
            "gpt-5.3-codex",
            "gpt-5-mini",
            "gpt-5",
            "gpt-4o-mini-2024-07-18",
            "gpt-4o-2024-11-20",
            "gpt-4o-2024-08-06",
            "grok-code-fast-1",
            "gpt-5.1",
            "gpt-5.1-codex",
            "gpt-5.1-codex-mini",
            "gpt-5.1-codex-max",
            "gpt-5-codex",
            "text-embedding-3-small",
            "text-embedding-3-small-inference",
            "claude-sonnet-4",
            "claude-sonnet-4.5",
            "claude-opus-4.5",
            "claude-haiku-4.5",
            "gemini-3-pro-preview",
            "gemini-3-flash-preview",
            "gemini-2.5-pro",
            "gpt-4.1-2025-04-14",
            "oswe-vscode-prime",
            "oswe-vscode-secondary",
            "gpt-5.2",
            "gpt-41-copilot",
            "gpt-3.5-turbo-0613",
            "gpt-4",
            "gpt-4-0613",
            "gpt-4-0125-preview",
            "gpt-4o-2024-05-13",
            "gpt-4-o-preview",
            "gpt-4.1",
            "gpt-3.5-turbo",
            "gpt-4o-mini",
            "gpt-4o",
            "gpt-4-o-preview",
            "text-embedding-ada-002",
        ];

        let results = apply_model_list(&r, models);

        let map: HashMap<&str, &str> = results
            .iter()
            .map(|(k, v)| (k.as_str(), v.as_str()))
            .collect();

        // Claude models get dot→dash normalization
        assert_eq!(map["claude-opus-4.6-fast"], "claude-opus-4-6-fast");
        assert_eq!(map["claude-opus-4.6"], "claude-opus-4-6");
        assert_eq!(map["claude-sonnet-4.6"], "claude-sonnet-4-6");
        assert_eq!(map["claude-sonnet-4.5"], "claude-sonnet-4-5");
        assert_eq!(map["claude-opus-4.5"], "claude-opus-4-5");
        assert_eq!(map["claude-haiku-4.5"], "claude-haiku-4-5");

        // No dot in version → unchanged
        assert_eq!(map["claude-sonnet-4"], "claude-sonnet-4");

        // Non-claude models unchanged
        assert_eq!(map["gpt-4o"], "gpt-4o");
        assert_eq!(map["gpt-5"], "gpt-5");
        assert_eq!(map["gpt-4o-mini"], "gpt-4o-mini");
        assert_eq!(map["gemini-2.5-pro"], "gemini-2.5-pro");
        assert_eq!(map["text-embedding-3-small"], "text-embedding-3-small");
    }

    // --- resolve via learned map ---

    #[test]
    fn resolve_learned_from_model_list() {
        let r = renamer(true, &[]);
        let models = &[
            "claude-sonnet-4.5",
            "claude-opus-4.6-fast",
            "claude-sonnet-4",
        ];
        apply_model_list(&r, models);

        assert_eq!(r.resolve("claude-sonnet-4-5"), "claude-sonnet-4.5");
        assert_eq!(r.resolve("claude-opus-4-6-fast"), "claude-opus-4.6-fast");
        // No rename happened, so resolve is identity
        assert_eq!(r.resolve("claude-sonnet-4"), "claude-sonnet-4");
    }

    #[test]
    fn resolve_version_first_learned() {
        let r = renamer(true, &[]);
        apply_model_list(&r, &["claude-3.5-sonnet"]);

        assert_eq!(r.resolve("claude-sonnet-3-5"), "claude-3.5-sonnet");
    }

    // --- custom overrides ---

    #[test]
    fn custom_overrides_auto() {
        let r = renamer(true, &[("claude-sonnet-4.5", "my-sonnet")]);
        let results = apply_model_list(&r, &["claude-sonnet-4.5"]);

        assert_eq!(results[0].1, "my-sonnet");
        assert_eq!(r.resolve("my-sonnet"), "claude-sonnet-4.5");
    }

    #[test]
    fn custom_with_date_suffix() {
        let r = renamer(true, &[("claude-sonnet-4", "claude-sonnet-4-20250514")]);
        let results = apply_model_list(&r, &["claude-sonnet-4"]);

        assert_eq!(results[0].1, "claude-sonnet-4-20250514");
        assert_eq!(r.resolve("claude-sonnet-4-20250514"), "claude-sonnet-4");
    }

    #[test]
    fn custom_only_no_auto() {
        let r = renamer(false, &[("foo", "bar")]);
        assert_eq!(r.rename("foo"), "bar");
        assert_eq!(r.resolve("bar"), "foo");
        // Auto disabled: dots not normalized
        assert_eq!(r.rename("claude-sonnet-4.5"), "claude-sonnet-4.5");
    }

    #[test]
    fn auto_disabled_passes_through() {
        let r = renamer(false, &[]);
        assert_eq!(r.rename("claude-3.5-sonnet"), "claude-3.5-sonnet");
        assert_eq!(r.rename("claude-sonnet-4.5"), "claude-sonnet-4.5");
    }

    #[test]
    fn unknown_model_passes_through() {
        let r = renamer(true, &[]);
        assert_eq!(r.rename("some-unknown-model"), "some-unknown-model");
        assert_eq!(r.resolve("some-unknown-model"), "some-unknown-model");
    }

    // --- replace_version_dots ---

    #[test]
    fn dots_between_digits_replaced() {
        assert_eq!(replace_version_dots("4.6"), "4-6");
        assert_eq!(replace_version_dots("3.5.1"), "3-5-1");
        assert_eq!(replace_version_dots("sonnet-4.5"), "sonnet-4-5");
        assert_eq!(replace_version_dots("opus-4.6-fast"), "opus-4-6-fast");
    }

    #[test]
    fn dots_not_between_digits_kept() {
        assert_eq!(replace_version_dots("v2.beta"), "v2.beta");
        assert_eq!(replace_version_dots("sonnet"), "sonnet");
        assert_eq!(replace_version_dots(".5"), ".5");
        assert_eq!(replace_version_dots("4."), "4.");
    }
}
