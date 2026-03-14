use regex::Regex;

pub struct TextProcessor;

impl TextProcessor {
    pub fn preprocess_text(text: &str) -> String {
        let text = text.replace("\r\n", "\n").replace('\r', "\n");
        let re = Regex::new(r"\n{3,}").unwrap();
        let text = re.replace_all(&text, "\n\n");
        let lines: Vec<&str> = text.lines().map(|l| l.trim()).collect();
        lines.join("\n").trim().to_string()
    }

    pub fn split_text(text: &str, chunk_size: usize, overlap: usize) -> Vec<String> {
        if text.len() <= chunk_size {
            return if text.trim().is_empty() { vec![] } else { vec![text.to_string()] };
        }

        let mut chunks = Vec::new();
        let chars: Vec<char> = text.chars().collect();
        let mut start = 0;

        let separators = [
            "\u{3002}", "\u{FF01}", "\u{FF1F}", ".\n", "!\n", "?\n", "\n\n", ". ", "! ", "? ",
        ];

        while start < chars.len() {
            let end = (start + chunk_size).min(chars.len());
            let chunk_str: String = chars[start..end].iter().collect();

            if end < chars.len() {
                let mut best_end = end;
                for sep in &separators {
                    if let Some(pos) = chunk_str.rfind(sep) {
                        if pos > chunk_size * 3 / 10 {
                            best_end = start + pos + sep.len();
                            break;
                        }
                    }
                }
                let final_chunk: String = chars[start..best_end].iter().collect();
                let trimmed = final_chunk.trim().to_string();
                if !trimmed.is_empty() {
                    chunks.push(trimmed);
                }
                start = if best_end > overlap { best_end - overlap } else { best_end };
            } else {
                let trimmed = chunk_str.trim().to_string();
                if !trimmed.is_empty() {
                    chunks.push(trimmed);
                }
                break;
            }
        }
        chunks
    }
}
