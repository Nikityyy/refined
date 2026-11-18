//! Word-level text diffing with whitespace preservation.
//! Computes additions, deletions, and unchanged segments using LCS.
//! Supports CJK (Chinese, Japanese, Korean) fine-grained tokenization.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum DiffKind {
    Equal,
    Delete,
    Insert,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiffChunk {
    pub kind: DiffKind,
    pub text: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct DiffStats {
    pub original_words: usize,
    pub refined_words: usize,
    pub original_chars: usize,
    pub refined_chars: usize,
}

impl DiffStats {
    pub fn word_delta(&self) -> isize {
        self.refined_words as isize - self.original_words as isize
    }

    #[allow(dead_code)]
    pub fn percent_change(&self) -> f32 {
        if self.original_words == 0 {
            return 0.0;
        }
        ((self.refined_words as f32 - self.original_words as f32) / self.original_words as f32) * 100.0
    }
}

/// Returns true if character is a CJK ideograph, Kana, or Hangul symbol
/// that should be tokenized individually rather than lumped into a space-delimited word.
fn is_cjk(ch: char) -> bool {
    matches!(ch,
        '\u{2E80}'..='\u{2FD5}' | // CJK Radicals / Kangxi Radicals
        '\u{3040}'..='\u{309F}' | // Hiragana
        '\u{30A0}'..='\u{30FF}' | // Katakana
        '\u{31F0}'..='\u{31FF}' | // Katakana Phonetic Extensions
        '\u{3400}'..='\u{4DBF}' | // CJK Unified Ideographs Ext A
        '\u{4E00}'..='\u{9FFF}' | // CJK Unified Ideographs
        '\u{F900}'..='\u{FAFF}' | // CJK Compatibility Ideographs
        '\u{AC00}'..='\u{D7AF}'   // Hangul Syllables
    )
}

/// Tokenizes text into words, CJK characters, and intervening whitespace/punctuation.
fn tokenize(s: &str) -> Vec<&str> {
    let mut tokens = Vec::new();
    let mut chars = s.char_indices().peekable();

    while let Some((start, ch)) = chars.next() {
        if is_cjk(ch) {
            // Emit single CJK character token
            let next_idx = chars.peek().map(|&(idx, _)| idx).unwrap_or(s.len());
            tokens.push(&s[start..next_idx]);
        } else if ch.is_whitespace() {
            // Group whitespace together
            while let Some(&(next_idx, next_ch)) = chars.peek() {
                if next_ch.is_whitespace() {
                    chars.next();
                } else {
                    tokens.push(&s[start..next_idx]);
                    break;
                }
            }
            if chars.peek().is_none() {
                tokens.push(&s[start..]);
            }
        } else if ch.is_alphanumeric() {
            // Group alphanumeric word (stopping if next is CJK or space)
            while let Some(&(next_idx, next_ch)) = chars.peek() {
                if next_ch.is_alphanumeric() && !is_cjk(next_ch) {
                    chars.next();
                } else {
                    tokens.push(&s[start..next_idx]);
                    break;
                }
            }
            if chars.peek().is_none() {
                tokens.push(&s[start..]);
            }
        } else {
            // Punctuation or symbol: single character token
            let next_idx = chars.peek().map(|&(idx, _)| idx).unwrap_or(s.len());
            tokens.push(&s[start..next_idx]);
        }
    }
    tokens
}

/// Computes word-level diff between `original` and `refined`.
pub fn compute_diff(original: &str, refined: &str) -> (Vec<DiffChunk>, DiffStats) {
    let orig_tokens = tokenize(original);
    let ref_tokens = tokenize(refined);

    let n = orig_tokens.len();
    let m = ref_tokens.len();

    // Standard LCS table
    let mut dp = vec![vec![0u32; m + 1]; n + 1];
    for i in 0..n {
        for j in 0..m {
            if orig_tokens[i] == ref_tokens[j] {
                dp[i + 1][j + 1] = dp[i][j] + 1;
            } else {
                dp[i + 1][j + 1] = dp[i + 1][j].max(dp[i][j + 1]);
            }
        }
    }

    // Backtrack to find diff
    let mut chunks = Vec::new();
    let mut i = n;
    let mut j = m;

    while i > 0 || j > 0 {
        if i > 0 && j > 0 && orig_tokens[i - 1] == ref_tokens[j - 1] {
            chunks.push(DiffChunk {
                kind: DiffKind::Equal,
                text: orig_tokens[i - 1].to_string(),
            });
            i -= 1;
            j -= 1;
        } else if j > 0 && (i == 0 || dp[i][j - 1] >= dp[i - 1][j]) {
            chunks.push(DiffChunk {
                kind: DiffKind::Insert,
                text: ref_tokens[j - 1].to_string(),
            });
            j -= 1;
        } else if i > 0 {
            chunks.push(DiffChunk {
                kind: DiffKind::Delete,
                text: orig_tokens[i - 1].to_string(),
            });
            i -= 1;
        }
    }

    chunks.reverse();

    // Merge adjacent chunks of the same kind
    let mut merged: Vec<DiffChunk> = Vec::new();
    for chunk in chunks {
        if let Some(last) = merged.last_mut() {
            if last.kind == chunk.kind {
                last.text.push_str(&chunk.text);
                continue;
            }
        }
        merged.push(chunk);
    }

    let stats = DiffStats {
        original_words: original.split_whitespace().count(),
        refined_words: refined.split_whitespace().count(),
        original_chars: original.chars().count(),
        refined_chars: refined.chars().count(),
    };

    (merged, stats)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_diff_basic() {
        let (chunks, stats) = compute_diff("The quick brown fox", "The fast brown fox jumps");
        assert_eq!(stats.original_words, 4);
        assert_eq!(stats.refined_words, 5);
        assert!(chunks.iter().any(|c| c.kind == DiffKind::Delete && c.text.contains("quick")));
        assert!(chunks.iter().any(|c| c.kind == DiffKind::Insert && c.text.contains("fast")));
    }

    #[test]
    fn test_diff_cjk() {
        let (chunks, _) = compute_diff("今日的天气很好", "今日的天气非常非常好");
        assert!(chunks.iter().any(|c| c.kind == DiffKind::Insert && c.text.contains("非常非常")));
    }
}
