use std::panic::{catch_unwind, AssertUnwindSafe};
use std::path::Path;

use anyhow::{anyhow, Result};
use shared::{ArticleSource, NewArticle};

/// 从 PDF 文本层中只识别 DOI，再通过 DOI 跳转到期刊页面提取元数据。
pub fn extract_from_pdf(path: &Path) -> Result<NewArticle> {
    let text = extract_text_from_pdf(path)?;
    let doi = extract_doi_from_text(&text)?;
    match article_from_pdf_text(&text, &doi) {
        Ok(fallback) => {
            crate::doi::fetch_article_from_doi_with_fallback(&doi, ArticleSource::Pdf, fallback)
        }
        Err(fallback_error) => crate::doi::fetch_article_from_doi(&doi, ArticleSource::Pdf)
            .map_err(|error| anyhow!("{error}; PDF 文本兜底失败: {fallback_error}")),
    }
}

fn extract_text_from_pdf(path: &Path) -> Result<String> {
    let bytes = std::fs::read(path)?;
    if !bytes.starts_with(b"%PDF") {
        return Err(anyhow!("文件不是有效 PDF，已拒绝导入"));
    }
    extract_text_from_pdf_bytes(&bytes)
}

fn extract_doi_from_text(text: &str) -> Result<String> {
    if text.trim().is_empty() {
        return Err(anyhow!("PDF 中未提取到任何文本，无法识别 DOI，已拒绝导入"));
    }
    crate::doi::extract_doi(text)
        .ok_or_else(|| anyhow!("PDF 中未找到 DOI 链接或 DOI 编号，已拒绝导入"))
}

fn extract_text_from_pdf_bytes(bytes: &[u8]) -> Result<String> {
    match catch_unwind(AssertUnwindSafe(|| {
        pdf_extract::extract_text_from_mem(bytes)
    })) {
        Ok(Ok(text)) => Ok(text),
        Ok(Err(error)) => Err(anyhow!("PDF 解析失败: {error}")),
        Err(_) => Err(anyhow!(
            "PDF 解析失败：该文件包含当前解析器无法处理的字体或结构，已拒绝导入"
        )),
    }
}

fn article_from_pdf_text(text: &str, doi: &str) -> Result<NewArticle> {
    let lines = normalized_lines(text);
    let title_block =
        find_title_block(&lines).ok_or_else(|| anyhow!("PDF 文本层未找到可识别的论文标题"))?;
    let abstract_text =
        extract_abstract(&lines).ok_or_else(|| anyhow!("PDF 文本层未找到可识别的摘要"))?;
    let authors = extract_authors(&lines, title_block.end);
    let year = extract_year(text);

    Ok(NewArticle {
        title: title_block.text,
        abstract_text,
        authors,
        journal: None,
        year,
        doi: Some(doi.to_string()),
        pmid: None,
        keywords: Vec::new(),
        source: ArticleSource::Pdf,
    })
}

#[derive(Debug)]
struct TextBlock {
    text: String,
    end: usize,
}

fn normalized_lines(text: &str) -> Vec<String> {
    text.lines().map(normalize_pdf_line).collect()
}

fn normalize_pdf_line(line: &str) -> String {
    line.replace('\u{00a0}', " ")
        .replace('\u{fb00}', "ff")
        .replace('\u{fb01}', "fi")
        .replace('\u{fb02}', "fl")
        .replace('\u{fb03}', "ffi")
        .replace('\u{fb04}', "ffl")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .trim()
        .to_string()
}

fn find_title_block(lines: &[String]) -> Option<TextBlock> {
    let limit = abstract_start(lines)
        .map(|start| start.index)
        .unwrap_or(lines.len().min(90));
    let mut index = 0;
    while index < limit {
        while index < limit && lines[index].is_empty() {
            index += 1;
        }
        let start = index;
        while index < limit && !lines[index].is_empty() {
            index += 1;
        }
        if start == index {
            continue;
        }
        let text = clean_join(&lines[start..index]);
        if is_plausible_pdf_title(&text) {
            return Some(TextBlock { text, end: index });
        }
    }
    None
}

fn is_plausible_pdf_title(text: &str) -> bool {
    let lower = text.to_ascii_lowercase();
    if [
        "new research papers",
        "original article",
        "research article",
        "review article",
        "article info",
        "contents lists available",
    ]
    .iter()
    .any(|label| lower.contains(label))
    {
        return false;
    }
    if lower.contains("vol.") || lower.contains("no.") || lower.contains("ieee transactions") {
        return false;
    }
    if text.chars().next().is_some_and(|ch| ch.is_ascii_digit()) {
        return false;
    }
    if looks_like_affiliation(&lower) || looks_like_pdf_authors(text) {
        return false;
    }
    let char_count = text.chars().count();
    (20..=500).contains(&char_count) && text.split_whitespace().count() >= 3
}

struct AbstractStart {
    index: usize,
    initial_text: String,
}

fn abstract_start(lines: &[String]) -> Option<AbstractStart> {
    lines.iter().enumerate().find_map(|(index, line)| {
        let compact = line
            .chars()
            .filter(|ch| ch.is_ascii_alphabetic())
            .collect::<String>()
            .to_ascii_lowercase();
        if compact == "abstract" {
            return Some(AbstractStart {
                index,
                initial_text: String::new(),
            });
        }
        let lower = line.to_ascii_lowercase();
        for marker in ["abstract—", "abstract-", "abstract:"] {
            if lower.starts_with(marker) {
                return Some(AbstractStart {
                    index,
                    initial_text: normalize_pdf_line(&line[marker.len()..]),
                });
            }
        }
        None
    })
}

fn extract_abstract(lines: &[String]) -> Option<String> {
    let start = abstract_start(lines)?;
    let mut index = start.index + 1;
    let mut collected = Vec::new();
    if !start.initial_text.is_empty() {
        collected.push(start.initial_text);
    }
    while index < lines.len() {
        let line = lines[index].trim();
        index += 1;
        if line.is_empty() {
            continue;
        }
        let lower = line.to_ascii_lowercase();
        if is_abstract_stop_line(&lower) {
            break;
        }
        let stripped = strip_pdf_trailer(line);
        if !stripped.is_empty() {
            collected.push(stripped);
        }
    }
    let abstract_text = clean_join(&collected);
    is_plausible_pdf_abstract(&abstract_text).then_some(abstract_text)
}

fn is_abstract_stop_line(lower: &str) -> bool {
    lower == "introduction"
        || lower.starts_with("i. introduction")
        || lower == "methods"
        || lower.starts_with("index terms")
        || lower.starts_with("from the ")
        || lower.starts_with("issn ")
        || lower.starts_with("journal homepage")
        || lower.starts_with("contents lists available")
        || lower.contains("all rights reserved")
        || lower.contains("published by elsevier")
}

fn strip_pdf_trailer(line: &str) -> String {
    let mut value = line.to_string();
    for marker in [" © ", " doi:", " https://doi.org/", " http://doi.org/"] {
        if let Some(index) = value.to_ascii_lowercase().find(marker.trim_start()) {
            value.truncate(index);
        }
    }
    normalize_pdf_line(&value)
}

fn is_plausible_pdf_abstract(text: &str) -> bool {
    text.chars().count() >= 80 && text.split_whitespace().count() >= 12
}

fn extract_authors(lines: &[String], title_end: usize) -> Vec<String> {
    let Some(block) = next_non_empty_block(lines, title_end) else {
        return Vec::new();
    };
    if looks_like_affiliation(&block.text.to_ascii_lowercase()) {
        return Vec::new();
    }
    let Ok(regex) = regex::Regex::new(r#"[A-Z][A-Za-z.\-']+(?:\s+[A-Z][A-Za-z.\-']+){1,4}"#) else {
        return Vec::new();
    };
    let mut authors = Vec::new();
    for matched in regex.find_iter(&block.text) {
        let name = normalize_pdf_line(matched.as_str());
        if !looks_like_pdf_author_noise(&name) && !authors.iter().any(|author| author == &name) {
            authors.push(name);
        }
    }
    authors
}

fn next_non_empty_block(lines: &[String], mut index: usize) -> Option<TextBlock> {
    while index < lines.len() && lines[index].is_empty() {
        index += 1;
    }
    let start = index;
    while index < lines.len() && !lines[index].is_empty() {
        index += 1;
    }
    (start < index).then(|| TextBlock {
        text: clean_join(&lines[start..index]),
        end: index,
    })
}

fn looks_like_pdf_authors(text: &str) -> bool {
    let lower = text.to_ascii_lowercase();
    lower.contains(", phd") || lower.contains(", md") || lower.contains("corresponding author")
}

fn looks_like_affiliation(lower: &str) -> bool {
    lower.contains("department")
        || lower.contains("university")
        || lower.contains("institute")
        || lower.contains("hospital")
        || lower.contains("school of")
}

fn looks_like_pdf_author_noise(name: &str) -> bool {
    matches!(
        name,
        "Original Article" | "Article History" | "Available Online" | "United States"
    ) || name.contains("Department")
        || name.contains("University")
}

fn extract_year(text: &str) -> Option<i32> {
    let regex = regex::Regex::new(r#"\b(19|20)\d{2}\b"#).ok()?;
    regex
        .find(text)
        .and_then(|matched| matched.as_str().parse::<i32>().ok())
}

fn clean_join(lines: &[String]) -> String {
    lines
        .iter()
        .map(String::as_str)
        .filter(|line| !line.trim().is_empty())
        .collect::<Vec<_>>()
        .join(" ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
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

    #[test]
    fn parser_panic_pdf_returns_error_instead_of_unwinding() {
        let path = std::path::Path::new("test/chen2020.pdf");
        if !path.exists() {
            return;
        }
        let bytes = std::fs::read(path).unwrap();

        let error = extract_text_from_pdf_bytes(&bytes).unwrap_err().to_string();

        assert!(error.contains("PDF 解析失败"));
    }

    #[test]
    fn test_pdfs_do_not_unwind_during_doi_recognition() {
        let dir = std::path::Path::new("test");
        if !dir.exists() {
            return;
        }

        let mut tested = 0;
        let mut recognized = 0;
        for entry in std::fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            if path.extension().and_then(|ext| ext.to_str()) != Some("pdf") {
                continue;
            }
            tested += 1;
            let result = catch_unwind(AssertUnwindSafe(|| {
                let text = extract_text_from_pdf(&path)?;
                extract_doi_from_text(&text)
            }));
            assert!(result.is_ok(), "{} caused an unwind", path.display());
            if result.unwrap().is_ok() {
                recognized += 1;
            }
        }

        assert!(tested > 0);
        assert!(recognized > 0);
    }

    #[test]
    fn extracts_pdf_text_metadata_from_common_test_layouts() {
        let cases = [
            (
                "test/1-s2.0-S2405500X18305309-main.pdf",
                "Targeted Ganglionated Plexi Denervation",
                "magnetic nanoparticles carrying a CaCl2 payload",
            ),
            (
                "test/vogel2014.pdf",
                "Traveling Wave Magnetic Particle Imaging",
                "Most 3-D magnetic particle imaging",
            ),
        ];

        for (path, title_part, abstract_part) in cases {
            let path = std::path::Path::new(path);
            if !path.exists() {
                continue;
            }
            let text = extract_text_from_pdf(path).unwrap();
            let doi = extract_doi_from_text(&text).unwrap();

            let article = article_from_pdf_text(&text, &doi).unwrap();

            assert!(article.title.contains(title_part));
            assert!(article.abstract_text.contains(abstract_part));
        }
    }
}
