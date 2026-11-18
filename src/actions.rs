//! Actions and prompt templates for Refined.
//! Includes built-in actions, user customization, reset options, and language support.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ActionItem {
    pub id: String,
    pub name: String,
    pub description: String,
    pub icon: String,
    pub shortcut: String,
    pub prompt_instruction: String,
    pub default_instruction: String,
    pub is_builtin: bool,
    pub is_rewrite_as: bool,
}

impl ActionItem {
    pub fn is_modified(&self) -> bool {
        self.is_builtin && self.prompt_instruction != self.default_instruction
    }

    pub fn reset_to_default(&mut self) {
        self.prompt_instruction = self.default_instruction.clone();
    }
}

pub fn default_actions() -> Vec<ActionItem> {
    vec![
        ActionItem {
            id: "polish".to_string(),
            name: "Polish".to_string(),
            description: "Enhance flow, fix grammar, and elevate vocabulary without changing your voice.".to_string(),
            icon: crate::theme::icons::EDIT.to_string(),
            shortcut: "1".to_string(),
            prompt_instruction: "Thoroughly polish the text. Eliminate grammatical errors, improve word choice, and refine rhythm and flow while staying true to the author's original voice and intent. Keep formatting intact.".to_string(),
            default_instruction: "Thoroughly polish the text. Eliminate grammatical errors, improve word choice, and refine rhythm and flow while staying true to the author's original voice and intent. Keep formatting intact.".to_string(),
            is_builtin: true,
            is_rewrite_as: false,
        },
        ActionItem {
            id: "condense".to_string(),
            name: "Condense".to_string(),
            description: "Trim filler words and make the text concise, crisp, and direct.".to_string(),
            icon: crate::theme::icons::CHEVRON_DOWN.to_string(),
            shortcut: "2".to_string(),
            prompt_instruction: "Condense the text. Remove all filler, redundancy, and fluff. Make every word count while retaining every essential factual detail and nuance. Keep formatting intact.".to_string(),
            default_instruction: "Condense the text. Remove all filler, redundancy, and fluff. Make every word count while retaining every essential factual detail and nuance. Keep formatting intact.".to_string(),
            is_builtin: true,
            is_rewrite_as: false,
        },
        ActionItem {
            id: "expand".to_string(),
            name: "Expand".to_string(),
            description: "Flesh out ideas with nuance, vivid detail, and comprehensive depth.".to_string(),
            icon: crate::theme::icons::CHEVRON_UP.to_string(),
            shortcut: "3".to_string(),
            prompt_instruction: "Elaborate and expand upon the core ideas in the text. Add depth, helpful context, clear transitions, and articulate explanations without excessive repetition. Maintain clean paragraph structure.".to_string(),
            default_instruction: "Elaborate and expand upon the core ideas in the text. Add depth, helpful context, clear transitions, and articulate explanations without excessive repetition. Maintain clean paragraph structure.".to_string(),
            is_builtin: true,
            is_rewrite_as: false,
        },
        ActionItem {
            id: "structure".to_string(),
            name: "Structure".to_string(),
            description: "Organize dense prose into clean headings, bullet points, and logical hierarchy.".to_string(),
            icon: crate::theme::icons::BULLETED_LIST.to_string(),
            shortcut: "4".to_string(),
            prompt_instruction: "Reorganize and structure the input into a clear, readable hierarchy. Use markdown headers (###), bullet points, or numbered lists where logical. Make it instantly scannable and beautifully ordered.".to_string(),
            default_instruction: "Reorganize and structure the input into a clear, readable hierarchy. Use markdown headers (###), bullet points, or numbered lists where logical. Make it instantly scannable and beautifully ordered.".to_string(),
            is_builtin: true,
            is_rewrite_as: false,
        },
        ActionItem {
            id: "simplify".to_string(),
            name: "Simplify".to_string(),
            description: "Translate complex thoughts into clear, accessible plain language.".to_string(),
            icon: crate::theme::icons::CLEAR.to_string(),
            shortcut: "5".to_string(),
            prompt_instruction: "Simplify the text into plain, direct language. Replace jargon and complex sentence structures with straightforward clarity that anyone can understand effortlessly. Preserve key meaning.".to_string(),
            default_instruction: "Simplify the text into plain, direct language. Replace jargon and complex sentence structures with straightforward clarity that anyone can understand effortlessly. Preserve key meaning.".to_string(),
            is_builtin: true,
            is_rewrite_as: false,
        },
        ActionItem {
            id: "rewrite_as".to_string(),
            name: "Rewrite as...".to_string(),
            description: "Transform into a custom tone, format, or perspective of your choice.".to_string(),
            icon: crate::theme::icons::REPAIR.to_string(),
            shortcut: "6".to_string(),
            prompt_instruction: "Rewrite the following text matching the requested style/instructions: {user_instruction}".to_string(),
            default_instruction: "Rewrite the following text matching the requested style/instructions: {user_instruction}".to_string(),
            is_builtin: true,
            is_rewrite_as: true,
        },
    ]
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LanguageChoice {
    pub label: String,
    pub code: String,
}

pub fn standard_languages() -> &'static [LanguageChoice] {
    static LANGUAGES: std::sync::OnceLock<Vec<LanguageChoice>> = std::sync::OnceLock::new();
    LANGUAGES.get_or_init(|| {
        vec![
            LanguageChoice { label: "Auto (Same as text)".to_string(), code: "auto".to_string() },
            LanguageChoice { label: "English".to_string(), code: "English".to_string() },
            LanguageChoice { label: "German (Deutsch)".to_string(), code: "German".to_string() },
            LanguageChoice { label: "French (Français)".to_string(), code: "French".to_string() },
            LanguageChoice { label: "Spanish (Español)".to_string(), code: "Spanish".to_string() },
            LanguageChoice { label: "Italian (Italiano)".to_string(), code: "Italian".to_string() },
            LanguageChoice { label: "Portuguese (Português)".to_string(), code: "Portuguese".to_string() },
            LanguageChoice { label: "Japanese (日本語)".to_string(), code: "Japanese".to_string() },
            LanguageChoice { label: "Chinese (简体中文)".to_string(), code: "Simplified Chinese".to_string() },
            LanguageChoice { label: "Custom...".to_string(), code: "custom".to_string() },
        ]
    })
}
