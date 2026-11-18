//! Word-level text diffing with whitespace preservation.
//! Computes additions, deletions, and unchanged segments using LCS.
//! Supports CJK (Chinese, Japanese, Korean) fine-grained tokenization.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
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

/// The LCS table is allocated as (n+1) x (m+1) u32 cells. A plain DP over a
/// large selection would need ~4 bytes per cell (a 100 KB selection would
/// try to reserve over a gigabyte, killing the app under `panic = "abort"`).
/// Solved in three steps: common prefix/suffix tokens are matched directly,
/// the LCS runs only over the changed middle part, and if that middle is
/// still too large it falls back to a wholesale delete+insert chunk pair,
/// which is a valid (just coarser) diff. MAX_LCS_CELLS keeps the DP at
/// a few hundred MB worst case — fine relative to typical documents —
/// while bounding memory deterministically.
const MAX_LCS_CELLS: usize = 256 * 1024 * 1024 / 4; // 256 MB worth of u32 cells

/// Computes word-level diff between `original` and `refined`.
pub fn compute_diff(original: &str, refined: &str) -> (Vec<DiffChunk>, DiffStats) {
    let orig_tokens = tokenize(original);
    let ref_tokens = tokenize(refined);

    // 1. Match the common prefix and suffix directly; the DP only has to
    //    explain the changed middle.
    let mut start = 0;
    while start < orig_tokens.len()
        && start < ref_tokens.len()
        && orig_tokens[start] == ref_tokens[start]
    {
        start += 1;
    }
    let mut end_o = orig_tokens.len();
    let mut end_r = ref_tokens.len();
    while end_o > start && end_r > start && orig_tokens[end_o - 1] == ref_tokens[end_r - 1] {
        end_o -= 1;
        end_r -= 1;
    }

    let mut chunks: Vec<DiffChunk> = Vec::new();
    if start > 0 {
        chunks.push(DiffChunk {
            kind: DiffKind::Equal,
            text: orig_tokens[..start].concat(),
        });
    }

    let mid_orig = &orig_tokens[start..end_o];
    let mid_ref = &ref_tokens[start..end_r];
    if !mid_orig.is_empty() || !mid_ref.is_empty() {
        if mid_orig.len().saturating_mul(mid_ref.len()) > MAX_LCS_CELLS {
            // 2a. Changed middle too large for the DP table: a wholesale
            //     replace keeps the diff correct and the memory bounded.
            if !mid_orig.is_empty() {
                chunks.push(DiffChunk {
                    kind: DiffKind::Delete,
                    text: mid_orig.concat(),
                });
            }
            if !mid_ref.is_empty() {
                chunks.push(DiffChunk {
                    kind: DiffKind::Insert,
                    text: mid_ref.concat(),
                });
            }
        } else {
            // 2b. Standard LCS over the changed middle only. A flat row-major
            //     table keeps the allocation at exactly the guarded cell
            //     count (a Vec<Vec<_>> would add unbounded per-row headers).
            let n = mid_orig.len();
            let m = mid_ref.len();
            let stride = m + 1;
            let mut dp = vec![0u32; (n + 1) * stride];
            for i in 0..n {
                for j in 0..m {
                    let here = (i + 1) * stride + (j + 1);
                    if mid_orig[i] == mid_ref[j] {
                        dp[here] = dp[i * stride + j] + 1;
                    } else {
                        dp[here] = dp[(i + 1) * stride + j].max(dp[i * stride + j + 1]);
                    }
                }
            }

            let mut middle = Vec::new();
            let mut i = n;
            let mut j = m;
            while i > 0 || j > 0 {
                if i > 0 && j > 0 && mid_orig[i - 1] == mid_ref[j - 1] {
                    middle.push(DiffChunk {
                        kind: DiffKind::Equal,
                        text: mid_orig[i - 1].to_string(),
                    });
                    i -= 1;
                    j -= 1;
                } else if j > 0 && (i == 0 || dp[i * stride + j - 1] >= dp[(i - 1) * stride + j]) {
                    middle.push(DiffChunk {
                        kind: DiffKind::Insert,
                        text: mid_ref[j - 1].to_string(),
                    });
                    j -= 1;
                } else if i > 0 {
                    middle.push(DiffChunk {
                        kind: DiffKind::Delete,
                        text: mid_orig[i - 1].to_string(),
                    });
                    i -= 1;
                }
            }
            middle.reverse();
            chunks.extend(middle);
        }
    }

    if end_o < orig_tokens.len() {
        chunks.push(DiffChunk {
            kind: DiffKind::Equal,
            text: orig_tokens[end_o..].concat(),
        });
    }

    // 3. Merge adjacent chunks of the same kind
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

    #[test]
    fn test_diff_reconstruction() {
        // Both directions must reconstruct exactly: Insert+Equal -> refined,
        // Delete+Equal -> original.
        let (chunks, _) = compute_diff("The quick brown fox", "The fast brown fox jumps");
        let refined: String = chunks
            .iter()
            .filter(|c| c.kind != DiffKind::Delete)
            .map(|c| c.text.as_str())
            .collect();
        let original: String = chunks
            .iter()
            .filter(|c| c.kind != DiffKind::Insert)
            .map(|c| c.text.as_str())
            .collect();
        assert_eq!(refined, "The fast brown fox jumps");
        assert_eq!(original, "The quick brown fox");
    }

    #[test]
    fn test_diff_large_input_bounded() {
        // A wholesomely different large pair takes the bounded fallback path;
        // reconstruction must stay exact without reserving a giant DP table.
        let original: String = "alpha ".repeat(200_000);
        let refined: String = "omega ".repeat(200_000);
        let (chunks, _) = compute_diff(&original, &refined);
        let rebuilt: String = chunks
            .iter()
            .filter(|c| c.kind != DiffKind::Delete)
            .map(|c| c.text.as_str())
            .collect();
        assert_eq!(rebuilt, refined);
    }
}
