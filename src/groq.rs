//! Groq AI API client.
//! Fetches dynamic chat models and reasoning effort options, and executes completions.

use serde::{Deserialize, Serialize};
use std::time::Instant;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GroqConfig {
    pub api_key: String,
    pub model: String,
    pub reasoning_effort: String,
    pub temperature: f32,
    pub top_p: f32,
    pub max_completion_tokens: u32,
    pub global_system_prompt: String,
}

impl Default for GroqConfig {
    fn default() -> Self {
        Self {
            api_key: String::new(),
            // Benchmark winner: matches gpt-oss-120b's polish/simplify/
            // expand/structure quality while being ~30-70% faster on the
            // short-form actions that dominate real use.
            model: DEFAULT_MODEL.to_string(),
            reasoning_effort: "high".to_string(),
            temperature: 1.0,
            top_p: 1.0,
            max_completion_tokens: 32768,
            global_system_prompt: "You are Refined, a precision writing and editing assistant. Your task is to transform the provided text according to the requested action with utmost clarity, elegance, and stylistic excellence. Preserve the original structural formatting (paragraphs, line breaks, indentation, bullet points, numbering, and capitalization style). Output standard ASCII punctuation only; never substitute Unicode lookalikes such as non-breaking hyphens or zero-width spaces. Return ONLY the refined text directly with no greeting, explanation, surrounding quotes, or commentary.".to_string(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GroqModel {
    pub id: String,
    #[serde(default)]
    pub owned_by: Option<String>,
    #[serde(default)]
    pub context_window: Option<u64>,
    #[serde(default)]
    pub active: Option<bool>,
}

#[derive(Debug, Clone)]
pub struct CompletionResponse {
    pub text: String,
    pub duration_ms: u128,
    pub model: String,
}

#[derive(Deserialize)]
struct ModelsListResponse {
    data: Vec<GroqModel>,
}

#[derive(Serialize)]
struct ChatMessage {
    role: String,
    content: String,
}

#[derive(Serialize)]
struct ChatCompletionRequest {
    model: String,
    messages: Vec<ChatMessage>,
    temperature: f32,
    max_completion_tokens: u32,
    top_p: f32,
    #[serde(skip_serializing_if = "Option::is_none")]
    reasoning_effort: Option<String>,
    stream: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    stop: Option<Vec<String>>,
}

#[derive(Deserialize)]
struct ChatChoice {
    message: ChatChoiceMessage,
    #[serde(default)]
    finish_reason: Option<String>,
}

#[derive(Deserialize)]
struct ChatChoiceMessage {
    content: Option<String>,
}

#[derive(Deserialize)]
struct ChatCompletionResponse {
    choices: Vec<ChatChoice>,
}

/// Upper bound for any single Groq HTTP call. ureq has no default timeout,
/// so without this a hung connection would freeze the HUD in Processing
/// forever (and the 400-fallback ladder would multiply the wait).
const HTTP_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(45);

/// The benchmarked default model: matches gpt-oss-120b quality on every
/// action while leading on speed for the short-form ones.
pub const DEFAULT_MODEL: &str = "openai/gpt-oss-20b";

/// Groq's current chat-capable lineup, shown before the user fetches the
/// live list. Non-chat models (Whisper, Orpheus, prompt guards, safeguard)
/// are deliberately absent.
pub const FALLBACK_MODELS: &[&str] = &[
    "openai/gpt-oss-20b",
    "openai/gpt-oss-120b",
    "qwen/qwen3.8-27b",
    "allam-2-7b",
];

/// Shared HTTP agent with the overall call timeout configured, so a hung
/// TLS/HTTP connection surfaces as an error instead of a frozen HUD.
pub fn http_agent() -> &'static ureq::Agent {
    static AGENT: std::sync::OnceLock<ureq::Agent> = std::sync::OnceLock::new();
    AGENT.get_or_init(|| {
        // schannel (Windows' own TLS) instead of ring+rustls: same trust
        // behavior via the Windows certificate store, roughly a megabyte
        // less binary.
        ureq::Agent::config_builder()
            .timeout_per_call(Some(HTTP_TIMEOUT))
            .tls_config(
                ureq::tls::TlsConfig::builder()
                    .provider(ureq::tls::TlsProvider::NativeTls)
                    .root_certs(ureq::tls::RootCerts::PlatformVerifier)
                    .build(),
            )
            .build()
            .new_agent()
    })
}

/// Cleans raw completion output for pasting into the user's document.
///
/// - Strips only the newlines the model wrapped its answer in, then restores
///   the newline run the ORIGINAL selection ended with; dropping it would
///   merge two paragraphs on paste (the diff would show a phantom Delete).
/// - Maps U+2011 (non-breaking hyphen) and removes the zero-width character
///   class (ZWSP, ZWNJ, ZWJ, BOM, word joiner), which otherwise lands
///   verbatim in the document and silently breaks search, spellcheck, and
///   hyphenation.
pub fn clean_generated_text(raw: &str, original: &str) -> String {
    let mut out: String = raw
        .trim_matches(|c| c == '\r' || c == '\n')
        .chars()
        .map(|c| if c == '\u{2011}' { '-' } else { c })
        .filter(|c| {
            !matches!(c, '\u{200B}' | '\u{200C}' | '\u{200D}' | '\u{2060}' | '\u{FEFF}')
        })
        .collect();

    let orig_tail: String = original
        .chars()
        .rev()
        .take_while(|c| *c == '\n' || *c == '\r')
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();
    if !orig_tail.is_empty() && !out.ends_with('\n') && !out.ends_with('\r') {
        out.push_str(&orig_tail);
    }
    out
}

/// Fetches models list from Groq API and filters to chat models only.
pub fn fetch_chat_models(api_key: &str) -> Result<Vec<GroqModel>, String> {
    let key = api_key.trim();
    if key.is_empty() {
        return Err("Groq API key is empty. Please enter your API key in Refined Settings.".to_string());
    }

    let url = "https://api.groq.com/openai/v1/models";
    let resp = http_agent()
        .get(url)
        .header("Authorization", &format!("Bearer {key}"))
        .call()
        .map_err(|e| format!("Failed to reach Groq API: {e}"))?;

    let parsed: ModelsListResponse = resp
        .into_body()
        .read_json()
        .map_err(|e| format!("Failed to parse Groq models JSON: {e}"))?;

    let mut chat_models: Vec<GroqModel> = parsed
        .data
        .into_iter()
        .filter(|m| {
            if m.active == Some(false) {
                return false;
            }
            let lower = m.id.to_lowercase();
            // Filter out non-chat models
            if lower.contains("whisper")
                || lower.contains("tts")
                || lower.contains("audio")
                || lower.contains("embed")
                || lower.contains("bge")
                || lower.contains("guard")
                || lower.contains("safeguard")
                || lower.contains("moderation")
                || lower.contains("orpheus")
            {
                return false;
            }
            true
        })
        .collect();

    // Sort models so preferred ones are on top, matching the default pick.
    chat_models.sort_by(|a, b| {
        let pref = |id: &str| {
            if id == "openai/gpt-oss-20b" {
                0
            } else if id == "openai/gpt-oss-120b" {
                1
            } else if id.contains("llama-3.3") {
                2
            } else {
                3
            }
        };
        pref(&a.id).cmp(&pref(&b.id)).then_with(|| a.id.cmp(&b.id))
    });

    Ok(chat_models)
}

/// Returns the standard reasoning effort options.
pub fn available_reasoning_efforts() -> Vec<&'static str> {
    vec!["high", "medium", "low", "none"]
}

/// Executes a chat completion with Groq API.
pub fn complete_action(
    config: &GroqConfig,
    action_instruction: &str,
    user_text: &str,
    language_target: &str,
) -> Result<CompletionResponse, String> {
    let key = config.api_key.trim();
    if key.is_empty() {
        return Err("Groq API key is missing. Open Refined Settings to add your API key.".to_string());
    }

    let mut system_prompt = config.global_system_prompt.clone();
    if !action_instruction.trim().is_empty() {
        system_prompt.push_str("\n\nSPECIFIC ACTION INSTRUCTIONS:\n");
        system_prompt.push_str(action_instruction);
    }

    if language_target.to_lowercase() != "auto" && !language_target.trim().is_empty() {
        system_prompt.push_str(&format!(
            "\n\nTARGET LANGUAGE: Output must strictly be in {}\n",
            language_target
        ));
    } else {
        system_prompt.push_str("\n\nTARGET LANGUAGE: Maintain the original language of the input text exactly.\n");
    }

    let effort = if config.reasoning_effort.to_lowercase() == "none" {
        None
    } else {
        Some(config.reasoning_effort.clone())
    };

    let mut req_body = ChatCompletionRequest {
        model: config.model.clone(),
        messages: vec![
            ChatMessage {
                role: "system".to_string(),
                content: system_prompt,
            },
            ChatMessage {
                role: "user".to_string(),
                content: user_text.to_string(),
            },
        ],
        temperature: config.temperature,
        max_completion_tokens: config.max_completion_tokens.clamp(256, 32768),
        top_p: config.top_p,
        reasoning_effort: effort,
        stream: false,
        stop: None,
    };

    let url = "https://api.groq.com/openai/v1/chat/completions";
    let start = Instant::now();

    // Fallback ladder: some models reject `reasoning_effort` and/or have
    // context windows smaller than the requested token budget (HTTP 400).
    // Retry with the effort dropped, then with progressively smaller budgets,
    // so every chat model in the live list actually works.
    let mut attempts: Vec<(Option<String>, u32)> = vec![
        (req_body.reasoning_effort.clone(), req_body.max_completion_tokens),
        (None, req_body.max_completion_tokens),
        (req_body.reasoning_effort.clone(), 16384),
        (None, 8192),
        (None, 4096),
        (None, 2048),
        (None, 1024),
    ];
    attempts.dedup();

    let mut last_err = String::new();
    let res = loop {
        let Some((effort, tokens)) = attempts.first().cloned() else {
            break Err(format!("Groq API error: {last_err}"));
        };
        req_body.reasoning_effort = effort;
        req_body.max_completion_tokens = tokens;

        match http_agent()
            .post(url)
            .header("Authorization", &format!("Bearer {key}"))
            .header("Content-Type", "application/json")
            .send_json(&req_body)
        {
            Ok(r) => break Ok(r),
            Err(e) => {
                last_err = e.to_string();
                match &e {
                    ureq::Error::StatusCode(400) => {
                        // Parameter-rejection: retry with the effort dropped
                        // first, then smaller budgets.
                        if attempts.len() > 1 {
                            attempts.remove(0);
                        } else {
                            break Err(format!("Groq API error: {last_err}"));
                        }
                    }
                    ureq::Error::StatusCode(413) => {
                        // Requested tokens count against the org's
                        // tokens-per-minute budget: a same-size retry can
                        // never help, so hop straight to the next smaller
                        // completion budget.
                        loop {
                            match attempts.first() {
                                Some((_, t)) if *t == tokens => {
                                    attempts.remove(0);
                                }
                                _ => break,
                            }
                        }
                        if attempts.is_empty() {
                            break Err(format!(
                                "Groq rejected the request as too large - most often a tokens-per-minute budget: wait a minute, or shorten the selection ({last_err})"
                            ));
                        }
                    }
                    _ => {
                        // Auth/quota/network failures: retrying is pointless.
                        break Err(format!("Groq API error: {last_err}"));
                    }
                }
            }
        }
    };

    let duration_ms = start.elapsed().as_millis();
    let res = match res {
        Ok(r) => r,
        Err(e) => return Err(e),
    };
    let body: ChatCompletionResponse = res
        .into_body()
        .read_json()
        .map_err(|e| format!("Failed to parse Groq response: {e}"))?;

    // A response with no usable content is an error, never a paste: an
    // empty paste would silently erase the user's selection and still
    // report success. Same for output cut off mid-sentence by the token
    // budget - pasting a truncated rewrite reads as complete.
    let mut content: Option<String> = None;
    let mut finish_reason: Option<String> = None;
    for choice in body.choices {
        if content.is_none() {
            content = choice.message.content;
            finish_reason = choice.finish_reason;
        }
    }
    if finish_reason.as_deref() == Some("length") {
        return Err("The result was cut short by the completion token budget - try again, or shorten the selected text.".to_string());
    }
    let content = match content {
        Some(c) if !c.trim().is_empty() => c,
        _ => {
            return Err(
                "The model returned an empty response. Try again or switch models in Settings."
                    .to_string(),
            )
        }
    };

    Ok(CompletionResponse {
        text: clean_generated_text(&content, user_text),
        duration_ms,
        model: config.model.clone(),
    })
}

#[cfg(test)]
mod tests {
    use super::clean_generated_text;

    #[test]
    fn clean_strips_wrapper_newlines_and_maps_nb_hyphen() {
        let out = clean_generated_text("\r\nFixed text.\r\n", "Original text.");
        assert_eq!(out, "Fixed text.");
        assert_eq!(clean_generated_text("convincingly\u{2011}sounding", "x"), "convincingly-sounding");
    }

    #[test]
    fn clean_strips_zero_width_chars() {
        for zw in ['\u{200B}', '\u{200C}', '\u{200D}', '\u{2060}', '\u{FEFF}'] {
            let out = clean_generated_text(&format!("a{zw}b"), "x");
            assert_eq!(out, "ab");
        }
    }

    #[test]
    fn clean_restores_original_trailing_newlines_only() {
        // A selection ending at a paragraph boundary keeps its newline;
        // the model's own wrapper newlines are gone either way.
        assert_eq!(clean_generated_text("\nFixed.\n", "Orig.\n"), "Fixed.\n");
        assert_eq!(clean_generated_text("\nFixed.\n", "Orig.\r\n"), "Fixed.\r\n");
        // A selection without a trailing newline does not gain one.
        assert_eq!(clean_generated_text("\nFixed.\n", "Orig."), "Fixed.");
    }
}
