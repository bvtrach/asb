use super::super::{StaticCollector, UsageContext};
use crate::measurements::TokenUsage;
use std::{fs::File, io::Read, path::Path};

pub(super) static ADAPTER: StaticCollector =
    StaticCollector::new("token_usage", None, None, false).with_usage(collect, from_path);
fn collect(context: &UsageContext<'_>) -> TokenUsage {
    from_sources(
        context.provider,
        context.agent,
        context.stdout,
        context.stderr,
        context.cassette,
    )
}

fn from_sources(
    provider: &str,
    agent: &str,
    stdout: &str,
    stderr: &str,
    cassette: Option<&Path>,
) -> TokenUsage {
    let mut usage = usage_from_environment(provider, agent);
    let output_usage = usage_from_text(stdout).merge(usage_from_text(stderr));
    usage = usage.merge_with_source(output_usage, "agent-output");
    if let Some(path) = cassette {
        if let Ok(text) = read_bounded_text(path, 16 * 1024 * 1024) {
            usage = usage.merge_with_source(usage_from_text(&text), "cassette");
        }
    }
    usage.with_derived_total()
}

/// Read a bounded machine-readable agent artifact (for example mini-SWE's
/// trajectory) and normalize any usage events it contains.
fn from_path(path: &Path) -> TokenUsage {
    read_bounded_text(path, 16 * 1024 * 1024)
        .map(|text| usage_from_text(&text).with_derived_total())
        .unwrap_or_default()
}

impl TokenUsage {
    fn merge(self, other: Self) -> Self {
        Self {
            input_tokens: self.input_tokens.or(other.input_tokens),
            output_tokens: self.output_tokens.or(other.output_tokens),
            cached_tokens: self.cached_tokens.or(other.cached_tokens),
            // Agent artifacts occasionally report a partial or stale total
            // (mini-SWE has emitted a small per-step total).  Keep the
            // largest observed total until the canonical input+output total
            // is derived below instead of letting a later artifact overwrite
            // the complete run.
            total_tokens: match (self.total_tokens, other.total_tokens) {
                (Some(left), Some(right)) => Some(left.max(right)),
                (Some(value), None) | (None, Some(value)) => Some(value),
                (None, None) => None,
            },
            source: self.source.or(other.source),
        }
    }

    fn add(self, other: Self) -> Self {
        Self {
            input_tokens: add_values(self.input_tokens, other.input_tokens),
            output_tokens: add_values(self.output_tokens, other.output_tokens),
            cached_tokens: add_values(self.cached_tokens, other.cached_tokens),
            total_tokens: add_values(self.total_tokens, other.total_tokens),
            source: self.source.or(other.source),
        }
    }

    pub(crate) fn merge_with_source(self, other: Self, source: &str) -> Self {
        let found = other.any();
        let mut merged = self.merge(other);
        if found {
            merged.source = Some(match merged.source.take() {
                Some(existing) if existing != source => format!("{existing}+{source}"),
                _ => source.into(),
            });
        }
        merged
    }

    pub(crate) fn with_derived_total(mut self) -> Self {
        // Cached input is a subset of input tokens, so total is input+output
        // rather than input+output+cached.  Prefer this canonical value over
        // provider-specific totals whenever both components are available.
        if let Some((input, output)) = self.input_tokens.zip(self.output_tokens) {
            self.total_tokens = Some(input.saturating_add(output));
        }
        self
    }

    fn any(&self) -> bool {
        self.input_tokens.is_some()
            || self.output_tokens.is_some()
            || self.cached_tokens.is_some()
            || self.total_tokens.is_some()
    }
}

fn usage_from_environment(provider: &str, agent: &str) -> TokenUsage {
    let mut prefixes = vec!["ASB".to_owned()];
    for value in [provider, agent] {
        let normalized = value
            .chars()
            .map(|character| {
                if character.is_ascii_alphanumeric() {
                    character.to_ascii_uppercase()
                } else {
                    '_'
                }
            })
            .collect::<String>();
        if !normalized.is_empty() && !prefixes.iter().any(|prefix| prefix == &normalized) {
            prefixes.push(normalized);
        }
    }
    let lookup = |suffixes: &[&str]| {
        prefixes.iter().find_map(|prefix| {
            suffixes
                .iter()
                .find_map(|suffix| env_u64(&format!("{prefix}_{suffix}")))
        })
    };
    let input = lookup(&["INPUT_TOKENS", "PROMPT_TOKENS", "INPUT_TOKEN_COUNT"]);
    let output = lookup(&["OUTPUT_TOKENS", "COMPLETION_TOKENS", "OUTPUT_TOKEN_COUNT"]);
    let cached = lookup(&[
        "CACHED_TOKENS",
        "CACHE_READ_INPUT_TOKENS",
        "PROMPT_CACHED_TOKENS",
    ]);
    let total = lookup(&["TOTAL_TOKENS", "TOKEN_COUNT"]);
    let usage = TokenUsage {
        input_tokens: input,
        output_tokens: output,
        cached_tokens: cached,
        total_tokens: total,
        source: None,
    };
    if usage.any() {
        TokenUsage {
            source: Some("environment".into()),
            ..usage
        }
    } else {
        usage
    }
}

impl Default for TokenUsage {
    fn default() -> Self {
        Self {
            input_tokens: None,
            output_tokens: None,
            cached_tokens: None,
            total_tokens: None,
            source: None,
        }
    }
}

fn usage_from_text(text: &str) -> TokenUsage {
    let mut usage = TokenUsage::default();
    if text.contains('\n')
        && let Ok(value) = serde_json::from_str::<serde_json::Value>(text.trim())
    {
        usage = usage.add(usage_from_json(&value));
    }
    let mut lines = text.lines().peekable();
    while let Some(line) = lines.next() {
        if let Ok(value) = serde_json::from_str::<serde_json::Value>(line.trim()) {
            usage = usage.add(usage_from_json(&value));
            continue;
        }
        for value in embedded_json_values(line) {
            usage = usage.add(usage_from_json(&value));
        }
        if line.to_ascii_lowercase().contains("tokens used")
            && let Some(value) = lines
                .peek()
                .and_then(|next| next.trim().replace(',', "").parse::<u64>().ok())
        {
            usage.total_tokens = Some(value);
            let _ = lines.next();
            continue;
        }
        // JSON fields are already accounted for above.  Avoid treating their
        // `"name":number` members as a second key/value event.
        if !line.contains(['{', '}']) {
            usage = usage.add(usage_from_key_values(line));
        }
    }
    usage
}

/// Extract complete JSON objects from agent prefixes such as `event: {...}`.
/// Agents frequently mix human-readable progress text with JSON usage events;
/// requiring a whole line to be JSON made token accounting work only for the
/// agent that emits a pure JSON stream.
pub(super) fn embedded_json_values(text: &str) -> Vec<serde_json::Value> {
    let mut values = Vec::new();
    let mut start = None;
    let mut depth = 0_u32;
    let mut in_string = false;
    let mut escaped = false;
    for (index, character) in text.char_indices() {
        if in_string {
            if escaped {
                escaped = false;
            } else if character == '\\' {
                escaped = true;
            } else if character == '"' {
                in_string = false;
            }
            continue;
        }
        match character {
            '"' => in_string = true,
            '{' if depth == 0 => {
                start = Some(index);
                depth = 1;
            }
            '{' if depth > 0 => depth += 1,
            '}' if depth > 0 => {
                depth -= 1;
                if depth == 0 {
                    if let Some(start) = start.take()
                        && let Ok(value) = serde_json::from_str(&text[start..=index])
                    {
                        values.push(value);
                    }
                }
            }
            _ => {}
        }
    }
    values
}

fn usage_from_json(value: &serde_json::Value) -> TokenUsage {
    let mut usage = TokenUsage::default();
    if let Some(object) = value.as_object() {
        let short_usage = object.contains_key("total")
            || (object.contains_key("input") && object.contains_key("output"))
            || object.contains_key("cached")
            || object.contains_key("cacheRead");
        usage.input_tokens = find_number(
            object,
            &[
                "input_tokens",
                "prompt_tokens",
                "inputTokens",
                "promptTokens",
                "input_token_count",
                "prompt_token_count",
                "promptTokenCount",
                "inputTokenCount",
            ],
        )
        .or_else(|| {
            short_usage
                .then(|| find_number(object, &["input", "prompt"]))
                .flatten()
        });
        usage.output_tokens = find_number(
            object,
            &[
                "output_tokens",
                "completion_tokens",
                "outputTokens",
                "completionTokens",
                "output_token_count",
                "completion_token_count",
                "candidatesTokenCount",
                "outputTokenCount",
            ],
        )
        .or_else(|| {
            short_usage
                .then(|| find_number(object, &["output", "completion"]))
                .flatten()
        });
        usage.cached_tokens = find_number(
            object,
            &[
                "cached_tokens",
                "cache_read_input_tokens",
                "cache_creation_input_tokens",
                "cachedTokens",
                "cache_read_tokens",
                "cacheRead",
                "cache_read",
                "cacheReadInputTokens",
                "cacheCreationInputTokens",
                "cachedContentTokenCount",
            ],
        )
        .or_else(|| {
            short_usage
                .then(|| find_number(object, &["cached"]))
                .flatten()
        });
        usage.total_tokens = find_number(
            object,
            &[
                "total_tokens",
                "totalTokens",
                "token_count",
                "tokens_used",
                "totalTokenCount",
            ],
        )
        .or_else(|| {
            short_usage
                .then(|| find_number(object, &["total"]))
                .flatten()
        });
        for (name, child) in object {
            if name == "tokens" {
                usage = usage.merge(usage_from_opencode_tokens(child));
            } else if ![
                "input_tokens",
                "prompt_tokens",
                "inputTokens",
                "promptTokens",
                "input_token_count",
                "prompt_token_count",
                "promptTokenCount",
                "inputTokenCount",
                "output_tokens",
                "completion_tokens",
                "outputTokens",
                "completionTokens",
                "output_token_count",
                "completion_token_count",
                "candidatesTokenCount",
                "outputTokenCount",
                "cached_tokens",
                "cache_read_input_tokens",
                "cache_creation_input_tokens",
                "cachedTokens",
                "cache_read_tokens",
                "cacheRead",
                "cache_read",
                "cacheReadInputTokens",
                "cacheCreationInputTokens",
                "cachedContentTokenCount",
                "total_tokens",
                "totalTokens",
                "token_count",
                "tokens_used",
                "totalTokenCount",
                "input",
                "prompt",
                "output",
                "completion",
                "cached",
                "cache_creation_input_tokens",
                "cacheCreationInputTokens",
                "total",
            ]
            .contains(&name.as_str())
            {
                usage = usage.merge(usage_from_json(child));
            }
        }
    } else if let Some(array) = value.as_array() {
        for child in array {
            usage = usage.merge(usage_from_json(child));
        }
    } else if let Some(text) = value.as_str() {
        if let Ok(nested) = serde_json::from_str::<serde_json::Value>(text) {
            usage = usage.merge(usage_from_json(&nested));
        }
    }
    usage
}

fn usage_from_opencode_tokens(value: &serde_json::Value) -> TokenUsage {
    let Some(object) = value.as_object() else {
        return TokenUsage::default();
    };
    let cached = object
        .get("cache")
        .and_then(|cache| {
            cache
                .get("read")
                .and_then(|value| value.as_u64().or_else(|| value.as_str()?.parse().ok()))
        })
        .or_else(|| find_number(object, &["cached", "cacheRead", "cache_read"]));
    TokenUsage {
        input_tokens: find_number(object, &["input"]),
        output_tokens: find_number(object, &["output"]),
        cached_tokens: cached,
        total_tokens: find_number(object, &["total"]),
        source: None,
    }
}

fn add_values(left: Option<u64>, right: Option<u64>) -> Option<u64> {
    match (left, right) {
        (Some(left), Some(right)) => Some(left.saturating_add(right)),
        (Some(value), None) | (None, Some(value)) => Some(value),
        (None, None) => None,
    }
}

fn find_number(object: &serde_json::Map<String, serde_json::Value>, names: &[&str]) -> Option<u64> {
    names.iter().find_map(|name| {
        object.get(*name).and_then(|value| {
            value
                .as_u64()
                .or_else(|| value.as_str().and_then(|text| text.parse().ok()))
        })
    })
}

fn usage_from_key_values(line: &str) -> TokenUsage {
    let mut usage = TokenUsage::default();
    let normalized = line.replace(['=', ':', ',', ';', '{', '}', '[', ']'], " ");
    let tokens = normalized.split_whitespace().collect::<Vec<_>>();
    for (index, token) in tokens.iter().enumerate() {
        let key = token
            .trim_matches(['"', '\'', '.', ','])
            .to_ascii_lowercase()
            .replace(['-', '_'], "");
        if matches!(key.as_str(), "sent" | "received") {
            if let Some(number) = index
                .checked_sub(1)
                .and_then(|previous| tokens.get(previous))
                .and_then(|value| parse_token_count(value.trim_matches(['"', '\''])))
            {
                if key == "sent" {
                    usage.input_tokens = Some(number);
                } else {
                    usage.output_tokens = Some(number);
                }
            }
            continue;
        }
        let (key, number) = if let Some(number) = tokens
            .get(index + 1)
            .and_then(|value| parse_token_count(value.trim_matches(['"', '\''])))
        {
            (key, number)
        } else if let Some(number) = tokens
            .get(index + 2)
            .and_then(|value| parse_token_count(value.trim_matches(['"', '\''])))
            && matches!(
                key.as_str(),
                "input" | "output" | "cache" | "cached" | "total"
            )
        {
            let compound = tokens[index + 1]
                .to_ascii_lowercase()
                .replace(['-', '_'], "");
            let key = format!("{key}{compound}");
            (key, number)
        } else {
            continue;
        };
        match key.as_str() {
            "input" | "inputtokens" | "inputtoken" | "prompt" | "prompttokens"
            | "prompttokencount" | "inputtokencount" => usage.input_tokens = Some(number),
            "output"
            | "outputtokens"
            | "outputtoken"
            | "completion"
            | "completiontokens"
            | "candidatestokencount"
            | "outputtokencount" => usage.output_tokens = Some(number),
            "cached"
            | "cachedtokens"
            | "cacheread"
            | "cachereadinputtokens"
            | "cachecreationinputtokens"
            | "cachedcontenttokencount" => usage.cached_tokens = Some(number),
            "total" | "totaltokens" | "totaltokencount" | "tokencount" | "tokensused" => {
                usage.total_tokens = Some(number)
            }
            _ => {}
        }
    }
    usage
}

fn parse_token_count(value: &str) -> Option<u64> {
    let value = value.trim_matches(['"', '\'', '.', ',']);
    let (number, multiplier) = match value
        .chars()
        .last()
        .map(|suffix| suffix.to_ascii_lowercase())
    {
        Some('k') => (&value[..value.len() - 1], 1_000.0),
        Some('m') => (&value[..value.len() - 1], 1_000_000.0),
        _ => (value, 1.0),
    };
    number.parse::<f64>().ok().and_then(|value| {
        let value = value * multiplier;
        (value.is_finite() && value >= 0.0 && value <= u64::MAX as f64)
            .then_some(value.round() as u64)
    })
}

fn read_bounded_text(path: &Path, max_bytes: u64) -> Result<String, std::io::Error> {
    let file = File::open(path)?;
    let mut bytes = Vec::new();
    file.take(max_bytes).read_to_end(&mut bytes)?;
    Ok(String::from_utf8_lossy(&bytes).into_owned())
}

fn env_u64(name: &str) -> Option<u64> {
    std::env::var(name).ok()?.parse().ok()
}
