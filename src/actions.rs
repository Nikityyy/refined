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

    pub fn is_pinned(&self) -> bool {
        self.is_builtin && self.is_rewrite_as
    }

    pub fn reset_to_default(&mut self) {
        self.prompt_instruction = self.default_instruction.clone();
    }
}

pub fn default_actions() -> Vec<ActionItem> {
    use crate::theme::icons;
    [
        ("polish", "Polish", "Fix grammar and improve clarity.", icons::EDIT,
         "Fix grammar, punctuation, and wording. Keep the author's voice and all the original facts. Preserve the paragraph and list layout."),
        ("condense", "Condense", "Keep the essentials and remove repetition.", icons::CHEVRON_DOWN,
         "Shorten the notes by cutting repetition and filler. Keep definitions, distinctions, numerical values, conditions, and exceptions that could matter on an exam."),
        ("expand", "Expand", "Explain the idea with a short example.", icons::CHEVRON_UP,
         "Explain the reasoning behind the notes. When helpful, add one short, clearly labeled illustrative example consistent with the notes. Do not invent supporting facts, citations, or dates."),
        ("structure", "Structure", "Turn dense notes into a study outline.", icons::BULLETED_LIST,
         "Organize the passage using Markdown headings and either bullets or numbering for each list. Reuse the original wording as much as possible. Keep the full meaning of each sentence, every qualifier, and the exact terms used in the notes. Correct obvious misspellings, such as swapped letters, to the closest valid spelling in context. Keep the corrected word in the output; do not replace it with a synonym, a new label, or a paraphrase. If the intended spelling is uncertain, keep the word as written. Do not invent facts or dates."),
        ("simplify", "Simplify", "Explain the same idea in plainer language.", icons::CLEAR,
         "Rewrite in shorter sentences with familiar words. Briefly define necessary technical terms in context. Keep direction, cause and effect, and exceptions intact."),
        ("flashcards", "Flashcards", "Turn notes into questions for active recall.", icons::BULLETED_LIST,
         "Replace the notes with only flashcard pairs. Use a Q: line and an A: line for each pair, with a blank line between pairs. Use one meaningful fact, formula, or relationship per card. Every answer must be supported directly by the notes. Do not repeat the source notes or add headings, commentary, or Markdown."),
        ("rewrite_as", "Rewrite as...", "Give your own instructions for the rewrite.", icons::REPAIR,
         "Rewrite the text according to this instruction: {user_instruction}"),
    ]
    .into_iter()
    .enumerate()
    .map(|(index, (id, name, description, icon, instruction))| ActionItem {
        id: id.into(),
        name: name.into(),
        description: description.into(),
        icon: icon.into(),
        shortcut: (index + 1).to_string(),
        prompt_instruction: instruction.into(),
        default_instruction: instruction.into(),
        is_builtin: true,
        is_rewrite_as: id == "rewrite_as",
    })
    .collect()
}

/// Update shipped defaults while retaining all custom prompts and actions.
pub fn refresh_builtins(actions: &mut Vec<ActionItem>) {
    for default in default_actions() {
        if let Some(action) = actions
            .iter_mut()
            .find(|action| action.is_builtin && action.id == default.id)
        {
            if !action.is_modified() {
                action
                    .prompt_instruction
                    .clone_from(&default.prompt_instruction);
            }
            action.default_instruction = default.default_instruction;
            action.description = default.description;
        } else if default.id == "flashcards"
            && !actions.iter().any(|action| action.id == default.id)
        {
            actions.push(default);
        }
    }
    order_actions(actions);
}

pub fn order_actions(actions: &mut [ActionItem]) {
    if let Some(index) = actions.iter().position(ActionItem::is_pinned) {
        actions[index..].rotate_left(1);
    }
    for (index, action) in actions.iter_mut().enumerate() {
        action.shortcut = if index < 9 {
            (index + 1).to_string()
        } else {
            String::new()
        };
    }
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
            LanguageChoice {
                label: "Auto (Same as text)".to_string(),
                code: "auto".to_string(),
            },
            LanguageChoice {
                label: "English".to_string(),
                code: "English".to_string(),
            },
            LanguageChoice {
                label: "German (Deutsch)".to_string(),
                code: "German".to_string(),
            },
            LanguageChoice {
                label: "French (Français)".to_string(),
                code: "French".to_string(),
            },
            LanguageChoice {
                label: "Spanish (Español)".to_string(),
                code: "Spanish".to_string(),
            },
            LanguageChoice {
                label: "Italian (Italiano)".to_string(),
                code: "Italian".to_string(),
            },
            LanguageChoice {
                label: "Portuguese (Português)".to_string(),
                code: "Portuguese".to_string(),
            },
            LanguageChoice {
                label: "Japanese (日本語)".to_string(),
                code: "Japanese".to_string(),
            },
            LanguageChoice {
                label: "Chinese (简体中文)".to_string(),
                code: "Simplified Chinese".to_string(),
            },
            LanguageChoice {
                label: "Custom...".to_string(),
                code: "custom".to_string(),
            },
        ]
    })
}

#[cfg(test)]
mod tests {
    #[test]
    fn rewrite_moves_to_end_without_reordering_other_actions() {
        let mut actions = super::default_actions();
        actions.swap(5, 6);
        let mut custom = actions[0].clone();
        custom.id = "custom".into();
        custom.is_builtin = false;
        actions.push(custom);
        super::refresh_builtins(&mut actions);
        assert_eq!(actions[5].id, "flashcards");
        assert_eq!(actions[6].id, "custom");
        assert_eq!(actions[7].id, "rewrite_as");
        assert_eq!(actions[7].shortcut, "8");
    }

    #[test]
    fn refresh_keeps_custom_prompts_and_adds_flashcards_once() {
        let mut actions = super::default_actions();
        actions.remove(5);
        actions[0].prompt_instruction = "Use my template".into();
        actions[1].prompt_instruction = "Old default".into();
        actions[1].default_instruction = "Old default".into();
        super::refresh_builtins(&mut actions);
        super::refresh_builtins(&mut actions);
        assert_eq!(actions.len(), 7);
        assert_eq!(actions[0].prompt_instruction, "Use my template");
        assert_eq!(
            actions[1].prompt_instruction,
            actions[1].default_instruction
        );
        assert_eq!(actions[5].id, "flashcards");
        assert_eq!(actions[5].shortcut, "6");
        assert_eq!(actions[6].id, "rewrite_as");
        assert_eq!(actions[6].shortcut, "7");
    }
}
