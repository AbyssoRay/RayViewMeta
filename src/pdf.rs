use std::path::Path;

use anyhow::{anyhow, Result};
use once_cell::sync::Lazy;
use shared::{ArticleSource, NewArticle};

static CITATION_RE: Lazy<regex::Regex> = Lazy::new(|| {
    regex::Regex::new(r#"\(\s*\d+(?:\s*[,–-]\s*\d+)*\s*\)"#).expect("valid citation regex")
});
static HYPHEN_SPACE_RE: Lazy<regex::Regex> = Lazy::new(|| {
    regex::Regex::new(r#"([A-Za-z])-\s+([A-Za-z])"#).expect("valid hyphen spacing regex")
});

/// 从 PDF 文件中尽量提取标题与摘要。
pub fn extract_from_pdf(path: &Path) -> Result<NewArticle> {
    let bytes = std::fs::read(path)?;
    if !bytes.starts_with(b"%PDF") {
        return Err(anyhow!("文件不是有效 PDF，已拒绝导入"));
    }
    let text =
        pdf_extract::extract_text_from_mem(&bytes).map_err(|e| anyhow!("PDF 解析失败: {e}"))?;
    if text.trim().is_empty() {
        return Err(anyhow!("PDF 中未提取到任何文本"));
    }
    let (title, abstract_text) = parse_title_and_abstract(&text)?;
    Ok(NewArticle {
        title,
        abstract_text,
        authors: Vec::new(),
        journal: None,
        year: None,
        doi: find_doi(&text),
        pmid: None,
        source: ArticleSource::Pdf,
    })
}

fn parse_title_and_abstract(raw: &str) -> Result<(String, String)> {
    let lines = clean_lines(raw);
    validate_text_quality(&lines)?;

    let (title, abstract_text) = if let Some(abstract_index) = find_abstract_line(&lines) {
        let title = guess_title(&lines[..abstract_index])
            .ok_or_else(|| anyhow!("未能可靠识别标题，已拒绝导入"))?;
        let abstract_text = extract_abstract(&lines, abstract_index)?;
        (title, abstract_text)
    } else {
        extract_front_matter_without_abstract(&lines)?
    };

    Ok((title, abstract_text))
}

fn extract_front_matter_without_abstract(lines: &[String]) -> Result<(String, String)> {
    let front_end = front_matter_limit(lines);
    let front_lines = &lines[..front_end];
    if let Some(summary_index) = find_front_matter_summary_start(front_lines) {
        let title = guess_title(&front_lines[..summary_index])
            .ok_or_else(|| anyhow!("未能可靠识别标题，已拒绝导入"))?;
        let abstract_text = extract_fallback_abstract_from_start(front_lines, summary_index)?;
        return Ok((title, abstract_text));
    }

    let (title, author_index) = guess_title_and_author(front_lines)
        .ok_or_else(|| anyhow!("未找到 Abstract 标题，也未能可靠识别作者信息结束位置"))?;
    let abstract_text = extract_abstract_after_author_block(front_lines, author_index)?;
    Ok((title, abstract_text))
}

fn find_front_matter_summary_start(lines: &[String]) -> Option<usize> {
    lines
        .iter()
        .enumerate()
        .skip(3)
        .find(|(index, line)| {
            looks_like_summary_start(line)
                && has_author_signal(&lines[..*index])
                && guess_title(&lines[..*index]).is_some()
        })
        .map(|(index, _)| index)
}

fn looks_like_summary_start(line: &str) -> bool {
    let char_count = line.chars().count();
    let word_count = line.split_whitespace().count();
    char_count >= 50
        && word_count >= 7
        && starts_with_uppercase_word(line)
        && !is_title_junk_line(line)
        && !is_author_metadata_line(line)
        && !looks_like_author_line(line)
        && !looks_like_person_name_line(line)
}

fn has_author_signal(lines: &[String]) -> bool {
    lines
        .iter()
        .any(|line| looks_like_author_line(line) || looks_like_person_name_line(line))
}

fn front_matter_limit(lines: &[String]) -> usize {
    let default_limit = usize::min(lines.len(), 160);
    for (index, line) in lines.iter().enumerate().skip(25) {
        if is_numbered_section_heading(line) {
            return index;
        }
    }
    default_limit
}

fn clean_lines(s: &str) -> Vec<String> {
    s.replace('\r', "\n")
        .replace('\u{ad}', "")
        .lines()
        .map(normalize_line)
        .filter(|line| !line.is_empty())
        .collect()
}

fn normalize_line(line: &str) -> String {
    line.replace('ﬁ', "fi")
        .replace('ﬂ', "fl")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn validate_text_quality(lines: &[String]) -> Result<()> {
    let text = lines.join(" ");
    let alpha_count = text.chars().filter(|ch| ch.is_alphabetic()).count();
    if lines.len() < 6 || alpha_count < 120 {
        return Err(anyhow!("PDF 文本层过少或不可复制，无法可靠识别标题和摘要"));
    }
    Ok(())
}

fn find_abstract_line(lines: &[String]) -> Option<usize> {
    lines
        .iter()
        .position(|line| abstract_line_body(line).is_some())
}

fn abstract_line_body(line: &str) -> Option<String> {
    let trimmed = line.trim();
    let lower = trimmed.to_ascii_lowercase();
    for marker in ["abstract", "summary"] {
        if lower == marker {
            return Some(String::new());
        }
        if let Some(rest_lower) = lower.strip_prefix(marker) {
            let Some(first) = rest_lower.chars().next() else {
                return Some(String::new());
            };
            if first.is_whitespace() || matches!(first, ':' | '.' | '-' | '—' | '–') {
                let rest = &trimmed[marker.len()..];
                let body = rest
                    .trim_start_matches(|ch: char| {
                        ch.is_whitespace() || matches!(ch, ':' | '.' | '-' | '—' | '–')
                    })
                    .trim()
                    .to_string();
                return Some(body);
            }
        }
    }
    None
}

fn guess_title(pre_abstract_lines: &[String]) -> Option<String> {
    if let Some((title, _)) = guess_title_and_author(pre_abstract_lines) {
        return Some(title);
    }

    let window_start = pre_abstract_lines.len().saturating_sub(80);
    let lines = &pre_abstract_lines[window_start..];

    best_title_candidate(lines)
}

fn guess_title_and_author(pre_body_lines: &[String]) -> Option<(String, usize)> {
    let window_start = pre_body_lines.len().saturating_sub(80);
    let lines = &pre_body_lines[window_start..];

    for (index, line) in lines.iter().enumerate() {
        if looks_like_author_line(line) {
            if let Some(title) = collect_title_before_author(lines, index) {
                return Some((title, window_start + index));
            }
        }
    }

    None
}

fn collect_title_before_author(lines: &[String], author_index: usize) -> Option<String> {
    let mut selected = Vec::new();
    for line in lines[..author_index].iter().rev() {
        if is_title_junk_line(line)
            || looks_like_author_line(line)
            || looks_like_non_title_person_line(line)
        {
            if selected.is_empty() {
                continue;
            }
            break;
        }
        selected.push(line.as_str());
        if selected.len() >= 4 {
            break;
        }
    }
    selected.reverse();
    let title = clean_title(&selected.join(" "));
    is_plausible_title(&title).then_some(title)
}

fn best_title_candidate(lines: &[String]) -> Option<String> {
    let candidates: Vec<&String> = lines
        .iter()
        .take(40)
        .filter(|line| {
            !is_title_junk_line(line)
                && !looks_like_author_line(line)
                && !looks_like_non_title_person_line(line)
        })
        .collect();

    let mut best: Option<(i32, String)> = None;
    for start in 0..candidates.len() {
        for end in start..usize::min(start + 4, candidates.len()) {
            let parts = candidates[start..=end]
                .iter()
                .map(|line| line.as_str())
                .collect::<Vec<_>>();
            let candidate = clean_title(&parts.join(" "));
            if !is_plausible_title(&candidate) {
                continue;
            }
            let char_count = candidate.chars().count() as i32;
            let score = char_count.min(180) - (start as i32 * 4) - ((end - start) as i32 * 8);
            if best.as_ref().map(|(best_score, _)| score > *best_score) != Some(false) {
                best = Some((score, candidate));
            }
        }
    }
    best.map(|(_, title)| title)
}

fn clean_title(title: &str) -> String {
    let title = title
        .trim_matches(|ch: char| ch.is_whitespace() || matches!(ch, '|' | '·'))
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    normalize_joined_text(&title)
}

fn normalize_joined_text(text: &str) -> String {
    HYPHEN_SPACE_RE.replace_all(text, "$1-$2").to_string()
}

fn is_plausible_title(title: &str) -> bool {
    let char_count = title.chars().count();
    let word_count = title.split_whitespace().count();
    if !(12..=300).contains(&char_count) || word_count < 3 {
        return false;
    }
    let lower = title.to_ascii_lowercase();
    !lower.starts_with("abstract")
        && !lower.starts_with("keywords")
        && !lower.contains("doi:")
        && !lower.contains("copyright")
}

fn looks_like_author_line(line: &str) -> bool {
    let lower = line.to_ascii_lowercase();
    if line.contains(':')
        || line.chars().count() > 360
        || lower.contains("department")
        || lower.contains("university")
        || lower.contains("hospital")
        || lower.contains("institute")
        || lower.contains("correspond")
    {
        return false;
    }
    let separator_count = line.matches(',').count()
        + line.matches(';').count()
        + usize::from(lower.contains(" and "));
    if separator_count == 0 {
        return false;
    }
    let word_count = line.split_whitespace().count();
    if !(2..=60).contains(&word_count) {
        return false;
    }
    let capitalized_words = line
        .split(|ch: char| !ch.is_alphabetic() && ch != '\'')
        .filter(|token| token.chars().next().is_some_and(|ch| ch.is_uppercase()))
        .count();
    capitalized_words >= 2
}

fn is_title_junk_line(line: &str) -> bool {
    let lower = line.to_ascii_lowercase();
    let word_count = line.split_whitespace().count();
    word_count < 2
        || abstract_line_body(line).is_some()
        || looks_like_journal_page_header(line)
        || lower == "new research papers"
        || lower == "research articles"
        || lower == "article info"
        || lower.replace(' ', "") == "articleinfo"
        || lower.starts_with("science ")
        || lower.starts_with("keywords")
        || lower.starts_with("key words")
        || lower.starts_with("research article")
        || lower.starts_with("original article")
        || lower.starts_with("review article")
        || lower.starts_with("journal of ")
        || lower.contains("doi:")
        || lower.contains("http://")
        || lower.contains("https://")
        || lower.contains("www.")
        || lower.contains("issn")
        || lower.contains("pmid")
        || lower.contains("received")
        || lower.contains("accepted")
        || lower.contains("published")
        || lower.contains("copyright")
        || lower.contains("creative commons")
        || lower.contains("open access")
        || looks_like_short_category_label(line)
}

fn looks_like_journal_page_header(line: &str) -> bool {
    let lower = line.to_ascii_lowercase();
    let starts_with_page = line
        .trim()
        .chars()
        .next()
        .is_some_and(|ch| ch.is_ascii_digit());
    starts_with_page
        && (lower.contains(" vol. ")
            || lower.contains(" no. ")
            || lower.contains("transactions on")
            || lower.contains("j am coll cardiol")
            || lower.contains("clinical electrophysiology"))
}

fn extract_abstract(lines: &[String], abstract_index: usize) -> Result<String> {
    let mut body_lines = Vec::new();
    if let Some(first_line_body) = abstract_line_body(&lines[abstract_index]) {
        if !first_line_body.is_empty() {
            body_lines.push(first_line_body);
        }
    }
    for line in &lines[abstract_index + 1..] {
        if is_abstract_end_marker(line) {
            break;
        }
        if !is_running_header(line) {
            body_lines.push(line.clone());
        }
    }

    let abstract_text = join_wrapped_lines(&body_lines);
    validate_abstract_text(abstract_text)
}

fn extract_abstract_after_author_block(lines: &[String], author_index: usize) -> Result<String> {
    let start_index = find_content_after_author_block(lines, author_index)
        .ok_or_else(|| anyhow!("未找到作者信息之后可作为摘要的正文内容"))?;
    extract_fallback_abstract_from_start(lines, start_index)
}

fn extract_fallback_abstract_from_start(lines: &[String], start_index: usize) -> Result<String> {
    let mut body_lines = Vec::new();
    for (offset, line) in lines[start_index..].iter().enumerate() {
        let lookahead_start = start_index + offset + 1;
        let lookahead_end = usize::min(lines.len(), lookahead_start + 3);
        let lookahead = &lines[lookahead_start..lookahead_end];
        if !body_lines.is_empty() && is_fallback_body_start(&body_lines, line, lookahead) {
            break;
        }
        if body_lines.is_empty() {
            if is_running_header(line) || is_author_metadata_line(line) || is_title_junk_line(line)
            {
                continue;
            }
            body_lines.push(line.clone());
            continue;
        }
        if is_fallback_abstract_end_marker(line) {
            break;
        }
        if !is_running_header(line) && !is_author_metadata_line(line) {
            body_lines.push(line.clone());
        }
    }

    validate_abstract_text(join_wrapped_lines(&body_lines))
}

fn find_content_after_author_block(lines: &[String], author_index: usize) -> Option<usize> {
    for (index, line) in lines
        .iter()
        .enumerate()
        .skip(author_index.saturating_add(1))
    {
        if is_running_header(line)
            || is_title_junk_line(line)
            || (looks_like_author_line(line) && !line.contains(':'))
            || looks_like_author_continuation_line(line)
            || is_author_metadata_line(line)
        {
            continue;
        }
        if line.split_whitespace().count() < 3 && !is_section_heading(line) {
            continue;
        }
        return Some(index);
    }
    None
}

fn validate_abstract_text(abstract_text: String) -> Result<String> {
    let word_count = abstract_text.split_whitespace().count();
    if abstract_text.chars().count() < 80 || word_count < 12 {
        return Err(anyhow!("摘要内容过短或无法可靠识别，已拒绝导入"));
    }
    Ok(abstract_text)
}

fn is_fallback_body_start(body_lines: &[String], current: &str, lookahead: &[String]) -> bool {
    let word_count = body_lines
        .iter()
        .flat_map(|line| line.split_whitespace())
        .count();
    if word_count < 50 {
        return false;
    }
    let previous_ends_sentence = body_lines
        .last()
        .and_then(|line| line.trim().chars().last())
        .is_some_and(|ch| matches!(ch, '.' | '?' | '!'));
    if !previous_ends_sentence || !starts_with_uppercase_word(current) {
        return false;
    }
    contains_citation_marker(current)
        || lookahead
            .iter()
            .any(|line| contains_citation_marker(line.as_str()))
}

fn starts_with_uppercase_word(line: &str) -> bool {
    line.trim()
        .chars()
        .find(|ch| ch.is_alphabetic())
        .is_some_and(|ch| ch.is_uppercase())
}

fn contains_citation_marker(line: &str) -> bool {
    CITATION_RE.is_match(line)
}

fn is_numbered_section_heading(line: &str) -> bool {
    let trimmed = line.trim();
    let Some(first) = trimmed.chars().next() else {
        return false;
    };
    first.is_ascii_digit() && is_section_heading(trimmed)
}

fn is_abstract_end_marker(line: &str) -> bool {
    let lower = line.trim().to_ascii_lowercase();
    lower == "introduction"
        || lower == "1 introduction"
        || lower == "1. introduction"
        || lower.starts_with("i. introduction")
        || lower.starts_with("introduction ")
        || lower.starts_with("1. introduction ")
        || lower.starts_with("index terms")
        || lower.starts_with("keywords")
        || lower.starts_with("key words")
        || lower.starts_with("references")
        || lower.starts_with("acknowledg")
        || lower.starts_with("issn ")
        || lower.starts_with("from the ")
        || lower.starts_with("manuscript received")
        || lower.starts_with("digital object identifier")
        || lower.starts_with("0278-0062")
        || lower.contains("american college of cardiology foundation")
}

fn is_fallback_abstract_end_marker(line: &str) -> bool {
    is_section_heading(line)
        || line.trim().to_ascii_lowercase().starts_with("keywords")
        || line.trim().to_ascii_lowercase().starts_with("key words")
        || line.trim().to_ascii_lowercase().starts_with("references")
}

fn is_section_heading(line: &str) -> bool {
    let heading = normalize_heading(line);
    let word_count = heading.split_whitespace().count();
    if heading.chars().count() > 80 || word_count > 6 {
        return false;
    }
    matches!(
        heading.as_str(),
        "background"
            | "objective"
            | "objectives"
            | "aim"
            | "aims"
            | "purpose"
            | "introduction"
            | "methods"
            | "method"
            | "materials and methods"
            | "patients and methods"
            | "results"
            | "discussion"
            | "conclusion"
            | "conclusions"
            | "keywords"
            | "key words"
            | "references"
            | "acknowledgments"
            | "acknowledgements"
    )
}

fn normalize_heading(line: &str) -> String {
    let trimmed = line.trim().trim_end_matches([':', '.']).trim();
    let without_number = trimmed
        .trim_start_matches(|ch: char| ch.is_ascii_digit() || matches!(ch, '.' | ')' | ' '))
        .trim();
    without_number.to_ascii_lowercase()
}

fn is_author_metadata_line(line: &str) -> bool {
    let lower = line.trim().to_ascii_lowercase();
    lower.contains("department")
        || lower.contains("university")
        || lower.contains("hospital")
        || lower.contains("institute")
        || lower.contains("school of")
        || lower.contains("faculty")
        || lower.contains("college")
        || lower.contains("center")
        || lower.contains("centre")
        || lower.contains("laboratory")
        || lower.contains("correspond")
        || lower.contains("affiliation")
        || lower.contains("e-mail")
        || lower.contains("email")
        || lower.contains('@')
}

fn looks_like_author_continuation_line(line: &str) -> bool {
    let trimmed = line.trim();
    if trimmed.len() <= 4
        && trimmed
            .chars()
            .all(|ch| ch.is_ascii_digit() || matches!(ch, ',' | '*' | ' '))
    {
        return true;
    }
    looks_like_person_name_line(trimmed)
}

fn looks_like_person_name_line(line: &str) -> bool {
    let line = line.trim();
    if line.contains(':') || line.chars().count() > 90 {
        return false;
    }
    let lower = line.to_ascii_lowercase();
    if lower.contains(" and ")
        || lower.contains(" of ")
        || lower.contains(" for ")
        || lower.contains(" with ")
        || lower.contains(" using ")
        || lower.contains("guided")
        || lower.contains("printing")
    {
        return false;
    }
    let name_tokens = line
        .split(|ch: char| ch.is_whitespace() || matches!(ch, ','))
        .map(|token| token.trim_matches(|ch: char| ch.is_ascii_digit() || matches!(ch, '*' | ',')))
        .filter(|token| !token.is_empty())
        .collect::<Vec<_>>();
    if !(2..=6).contains(&name_tokens.len()) {
        return false;
    }
    let capitalized = name_tokens
        .iter()
        .filter(|token| {
            token.len() == 1
                || token.ends_with('.')
                || token
                    .chars()
                    .next()
                    .is_some_and(|ch| ch.is_ascii_uppercase())
        })
        .count();
    capitalized == name_tokens.len()
}

fn looks_like_non_title_person_line(line: &str) -> bool {
    looks_like_person_name_line(line) && !is_plausible_title(line)
}

fn looks_like_short_category_label(line: &str) -> bool {
    let word_count = line.split_whitespace().count();
    if word_count > 4 || line.chars().count() > 45 {
        return false;
    }
    let mut alphabetic = 0usize;
    let mut uppercase = 0usize;
    for ch in line.chars().filter(|ch| ch.is_alphabetic()) {
        alphabetic += 1;
        if ch.is_uppercase() {
            uppercase += 1;
        }
    }
    alphabetic >= 3 && uppercase * 2 >= alphabetic
}

fn is_running_header(line: &str) -> bool {
    let lower = line.trim().to_ascii_lowercase();
    lower.starts_with("downloaded from")
        || lower.starts_with("copyright")
        || lower.contains("all rights reserved")
}

fn join_wrapped_lines(lines: &[String]) -> String {
    let mut out = String::new();
    for line in lines {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        if out.ends_with('-') {
            out.pop();
            out.push_str(line);
        } else {
            if !out.is_empty() {
                out.push(' ');
            }
            out.push_str(line);
        }
    }
    normalize_joined_text(&out)
}

fn find_doi(text: &str) -> Option<String> {
    let re = regex::Regex::new(r#"(?i)\b10\.\d{4,9}/[-._;()/:A-Z0-9]+"#).ok()?;
    re.find(text)
        .map(|m| m.as_str().trim_end_matches('.').to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_title_before_authors_and_abstract() {
        let raw = r#"
            Journal of Clinical Examples
            A pragmatic trial of example therapy for chronic conditions
            Jane Smith, John Doe, Alice Chen
            Department of Medicine, Example University
            Abstract
            Background: Example therapy may improve outcomes in chronic conditions.
            Methods: We reviewed randomized trials and extracted clinical endpoints.
            Results: Treatment improved the primary endpoint without serious harms.
            Conclusions: Example therapy may be useful for selected patients.
            Keywords: trial; therapy
            Introduction
            Long body text.
        "#;

        let (title, abstract_text) = parse_title_and_abstract(raw).unwrap();

        assert_eq!(
            title,
            "A pragmatic trial of example therapy for chronic conditions"
        );
        assert!(abstract_text.starts_with("Background:"));
        assert!(abstract_text.contains("Methods:"));
        assert!(!abstract_text.contains("Keywords"));
    }

    #[test]
    fn extracts_fallback_abstract_without_abstract_heading() {
        let raw = r#"
            Journal of Practical Evidence
            Long term outcomes after example therapy in adults with chronic disease
            Jane Smith, John Doe, Alice Chen
            Department of Medicine, Example University
            Background: Example therapy is frequently used in adults with chronic disease, but the durability of its benefit remains uncertain across longer follow up periods.
            We reviewed consecutive clinical records and summarized outcomes, adverse events, and discontinuation patterns across multiple treatment cycles.
            Methods
            The remaining article body starts here and should not be treated as part of the extracted abstract.
            Results
            Long body text.
        "#;

        let (title, abstract_text) = parse_title_and_abstract(raw).unwrap();

        assert_eq!(
            title,
            "Long term outcomes after example therapy in adults with chronic disease"
        );
        assert!(abstract_text.starts_with("Background:"));
        assert!(abstract_text.contains("durability of its benefit"));
        assert!(!abstract_text.contains("The remaining article body"));
    }

    #[test]
    fn extracts_science_style_front_matter_without_abstract_heading() {
        let raw = r#"
            ReseaRch aRticles
            Science 8 MAy 2025 616
            3D PRiNtiNG
            Imaging- guided deep tissue in vivo
            sound printing
            Elham Davoodi
            1, Jiahong Li1, Xiaotian Ma
            1,
            Alireza Hasani Najafabadi
            2, Jounghyun Yoo
            1, Gengxi Lu
            3,
            Ehsan Shirzaei Sani
            1, Sunho Lee
            1, Hossein Montazerian
            2,4,
            Gwangmook Kim1, Jason Williams
            5, Jee Won Yang
            6, Yushun Zeng
            3,
            Lei S. Li
            1,7, Zhiyang Jin
            1,6, Behnam Sadri
            1, Shervin S. Nia
            5,8,
            Lihong V. Wang
            1, Tzung K. Hsiai
            4, Paul S. Weiss
            4,5,8,9, Qifa Zhou
            3,
            Ali Khademhosseini
            2, Di Wu
            6, Mikhail G. Shapiro
            1,6, Wei Gao
            1*
            Three- dimensional printing offers promise for patient- specific
            implants and therapies but is often limited by the need for
            invasive surgical procedures. To address this, we developed an
            imaging- guided deep tissue in vivo sound printing (DISP)
            platform. By incorporating cross-linking agent-loaded low-
            temperature-sensitive liposomes into bioinks, DISP enables
            precise, rapid, on-demand cross-linking of diverse functional
            biomaterials using focused ultrasound. Gas vesicle-based
            ultrasound imaging provides real-time monitoring and allows for
            customized pattern creation in live animals. We validated DISP
            by successfully printing near diseased areas in the mouse
            bladder and deep within rabbit leg muscles in vivo,
            demonstrating its potential for localized drug delivery and
            tissue replacement. DISP’s ability to print conductive, drug-
            loaded, cell-laden, and bioadhesive biomaterials demonstrates
            its versatility for diverse biomedical applications.
            Three-dimensional (3D) bioprinting has emerged as a transformative
            tool in medicine, enabling the creation of patient-specific implants
            (1, 2), intricate medical devices (3, 4), and tissue replacements (5-7).
        "#;

        let (title, abstract_text) = parse_title_and_abstract(raw).unwrap();

        assert_eq!(title, "Imaging-guided deep tissue in vivo sound printing");
        assert!(abstract_text.starts_with("Three-dimensional printing offers promise"));
        assert!(abstract_text.contains("imaging-guided deep tissue in vivo sound printing"));
        assert!(!abstract_text.contains("bioprinting has emerged"));
    }

    #[test]
    fn extracts_local_pdf_fixtures_when_present() {
        let cases = [
            (
                "test/nieminen2022.pdf",
                "Multi-locus transcranial magnetic stimulation system for electronically targeted brain stimulation",
                "Transcranial magnetic stimulation (TMS) allows non-invasive stimulation",
            ),
            (
                "test/science.adt0293(1).pdf",
                "Imaging-guided deep tissue in vivo sound printing",
                "Three-dimensional printing offers promise for patient-specific implants",
            ),
            (
                "test/1-s2.0-S2405500X18305309-main.pdf",
                "Targeted Ganglionated Plexi Denervation Using Magnetic Nanoparticles Carrying Calcium Chloride Payload",
                "This study sought to develop a novel targeted delivery therapy",
            ),
            (
                "test/vogel2014.pdf",
                "Traveling Wave Magnetic Particle Imaging",
                "Most 3-D magnetic particle imaging (MPI) scanners currently use permanent magnets",
            ),
        ];

        for (path, expected_title, expected_abstract) in cases {
            let path = std::path::Path::new(path);
            if !path.exists() {
                continue;
            }
            let article = extract_from_pdf(path).unwrap();
            assert_eq!(article.title, expected_title);
            assert!(
                article.abstract_text.contains(expected_abstract),
                "{} abstract was: {}",
                path.display(),
                article.abstract_text
            );
        }
    }

    #[test]
    fn rejects_documents_without_heading_or_author_boundary() {
        let raw = r#"
            Some random scanned text
            This document has no useful heading and no reliable article metadata.
            It should not be imported as a literature record.
            The rest of the text is just arbitrary content repeated several times.
            The rest of the text is just arbitrary content repeated several times.
            The rest of the text is just arbitrary content repeated several times.
        "#;

        assert!(parse_title_and_abstract(raw).is_err());
    }
}
