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
            model: "openai/gpt-oss-120b".to_string(),
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
    #[allow(dead_code)]
pub prompt_tokens: Option<u64>,
    #[allow(dead_code)]
pub completion_tokens: Option<u64>,
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
struct ChatUsage {
    prompt_tokens: Option<u64>,
    completion_tokens: Option<u64>,
}

#[derive(Deserialize)]
struct ChatCompletionResponse {
    choices: Vec<ChatChoice>,
    #[serde(default)]
    usage: Option<ChatUsage>,
}

/// Upper bound for any single Groq HTTP call. ureq has no default timeout,
/// so without this a hung connection would freeze the HUD in Processing
/// forever (and the 400-fallback ladder would multiply the wait).
const HTTP_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(45);

/// Shared HTTP agent with the overall call timeout configured, so a hung
/// TLS/HTTP connection surfaces as an error instead of a frozen HUD.
fn http_agent() -> &'static ureq::Agent {
    static AGENT: std::sync::OnceLock<ureq::Agent> = std::sync::OnceLock::new();
    AGENT.get_or_init(|| {
        ureq::Agent::config_builder()
            .timeout_per_call(Some(HTTP_TIMEOUT))
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

    // Sort models so that preferred ones (like gpt-oss-120b and llama-3.3) are on top
    chat_models.sort_by(|a, b| {
        let a_pref = if a.id == "openai/gpt-oss-120b" { 0 } else if a.id.contains("llama-3.3") { 1 } else { 2 };
        let b_pref = if b.id == "openai/gpt-oss-120b" { 0 } else if b.id.contains("llama-3.3") { 1 } else { 2 };
        a_pref.cmp(&b_pref).then_with(|| a.id.cmp(&b.id))
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
                // Only fall back on parameter-rejection style errors; on
                // auth/quota/network failures retrying is pointless.
                let msg = last_err.to_lowercase();
                if !msg.contains("400") && !msg.contains("bad request") {
                    break Err(format!("Groq API error: {last_err}"));
                }
                if attempts.len() > 1 {
                    attempts.remove(0);
                } else {
                    break Err(format!("Groq API error: {last_err}"));
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
        return Err(
            "The model ran out of completion tokens and the result was cut short. Raise \"Max completion tokens\" in Settings or pick a shorter action."
                .to_string(),
        );
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

    let (prompt_tokens, completion_tokens) = if let Some(u) = body.usage {
        (u.prompt_tokens, u.completion_tokens)
    } else {
        (None, None)
    };

    Ok(CompletionResponse {
        text: clean_generated_text(&content, user_text),
        duration_ms,
        model: config.model.clone(),
        prompt_tokens,
        completion_tokens,
    })
}
