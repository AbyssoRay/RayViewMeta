use std::path::Path;

use anyhow::{anyhow, Result};
use shared::{ArticleSource, NewArticle};

/// 从 PDF 文本层中只识别 DOI，再通过 DOI 跳转到期刊页面提取元数据。
pub fn extract_from_pdf(path: &Path) -> Result<NewArticle> {
    let bytes = std::fs::read(path)?;
    if !bytes.starts_with(b"%PDF") {
        return Err(anyhow!("文件不是有效 PDF，已拒绝导入"));
    }
    let text = pdf_extract::extract_text_from_mem(&bytes)
        .map_err(|error| anyhow!("PDF 解析失败: {error}"))?;
    if text.trim().is_empty() {
        return Err(anyhow!("PDF 中未提取到任何文本，无法识别 DOI，已拒绝导入"));
    }
    let doi = crate::doi::extract_doi(&text)
        .ok_or_else(|| anyhow!("PDF 中未找到 DOI 链接或 DOI 编号，已拒绝导入"))?;
    crate::doi::fetch_article_from_doi(&doi, ArticleSource::Pdf)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_non_pdf_files() {
        let path = std::env::temp_dir().join("rayview_not_a_pdf.txt");
        std::fs::write(&path, b"not a pdf").unwrap();

        let error = extract_from_pdf(&path).unwrap_err().to_string();

        assert!(error.contains("文件不是有效 PDF"));
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn extracts_doi_from_text_formats() {
        assert_eq!(
            crate::doi::extract_doi("Digital Object Identifier 10.1109/TMI.2014.2351234.")
                .as_deref(),
            Some("10.1109/tmi.2014.2351234")
        );
        assert_eq!(
            crate::doi::extract_doi("Available at https://doi.org/10.1016/j.cell.2020.01.001")
                .as_deref(),
            Some("10.1016/j.cell.2020.01.001")
        );
    }
}
