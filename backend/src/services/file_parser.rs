use std::path::Path;
use anyhow::Result;
use tokio::fs;

pub struct FileParser;

impl FileParser {
    pub async fn extract_text(file_path: &Path) -> Result<String> {
        let ext = file_path.extension()
            .map(|e| e.to_string_lossy().to_lowercase())
            .unwrap_or_default();

        match ext.as_str() {
            "pdf" => Self::extract_from_pdf(file_path),
            "md" | "markdown" | "txt" => Self::extract_from_text(file_path).await,
            _ => anyhow::bail!("Unsupported file format: {}", ext),
        }
    }

    fn extract_from_pdf(file_path: &Path) -> Result<String> {
        let bytes = std::fs::read(file_path)?;
        let text = pdf_extract::extract_text_from_mem(&bytes)
            .map_err(|e| anyhow::anyhow!("PDF extraction failed: {}", e))?;
        Ok(text)
    }

    async fn extract_from_text(file_path: &Path) -> Result<String> {
        let bytes = fs::read(file_path).await?;

        // Try UTF-8 first
        if let Ok(text) = String::from_utf8(bytes.clone()) {
            return Ok(text);
        }

        // Fallback: try with lossy UTF-8
        Ok(String::from_utf8_lossy(&bytes).to_string())
    }
}
