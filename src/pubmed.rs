use anyhow::{anyhow, Result};
use shared::{ArticleSource, NewArticle};

#[derive(Debug, Clone)]
pub struct PubmedFailure {
    pub pmid: String,
    pub reason: String,
}

#[derive(Debug, Default)]
pub struct PubmedFetchResult {
    pub articles: Vec<NewArticle>,
    pub failures: Vec<PubmedFailure>,
}

pub fn parse_pubmed_token(token: &str) -> Option<String> {
    let token = token.trim_matches(|ch: char| {
        ch.is_whitespace() || matches!(ch, '<' | '>' | '(' | ')' | '[' | ']' | '"' | '\'')
    });
    let token = token.trim_end_matches(['.', '/', ',', ';']);
    if let Some(pmid) = parse_pmid_literal(token) {
        return Some(pmid);
    }

    let lower = token.to_ascii_lowercase();
    if let Some(rest) = lower.strip_prefix("pmid:") {
        return parse_pmid_literal(rest);
    }

    let url_text = if lower.starts_with("http://") || lower.starts_with("https://") {
        token.to_string()
    } else if lower.starts_with("pubmed.ncbi.nlm.nih.gov/")
        || lower.starts_with("www.ncbi.nlm.nih.gov/pubmed/")
        || lower.starts_with("ncbi.nlm.nih.gov/pubmed/")
    {
        format!("https://{token}")
    } else {
        return None;
    };

    let url = url::Url::parse(&url_text).ok()?;
    let host = url.host_str()?.to_ascii_lowercase();
    let segments = url.path_segments()?.collect::<Vec<_>>();
    if host == "pubmed.ncbi.nlm.nih.gov" {
        return segments
            .first()
            .and_then(|segment| parse_pmid_literal(segment));
    }
    if (host == "www.ncbi.nlm.nih.gov" || host == "ncbi.nlm.nih.gov")
        && segments.first().copied() == Some("pubmed")
    {
        return segments
            .get(1)
            .and_then(|segment| parse_pmid_literal(segment));
    }
    None
}

fn parse_pmid_literal(value: &str) -> Option<String> {
    let value = value.trim();
    if value.chars().all(|ch| ch.is_ascii_digit())
        && (4..=10).contains(&value.len())
        && value.chars().any(|ch| ch != '0')
    {
        Some(value.to_string())
    } else {
        None
    }
}

/// 通过 NCBI E-utilities 抓取一组 PubMed 文章的元数据，并报告未返回的 PMID。
pub fn fetch_pubmed_with_failures(pmids: &[String]) -> Result<PubmedFetchResult> {
    if pmids.is_empty() {
        return Ok(PubmedFetchResult::default());
    }
    let ids = pmids.join(",");
    let url = format!(
        "https://eutils.ncbi.nlm.nih.gov/entrez/eutils/efetch.fcgi?db=pubmed&id={ids}&retmode=xml"
    );
    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(45))
        .user_agent("RayviewMeta/0.1")
        .build()?;
    let xml = client.get(&url).send()?.error_for_status()?.text()?;
    let articles = parse_pubmed_xml(&xml)?;

    let returned_pmids = articles
        .iter()
        .filter_map(|article| article.pmid.as_deref())
        .collect::<std::collections::HashSet<_>>();
    let failures = pmids
        .iter()
        .filter(|pmid| !returned_pmids.contains(pmid.as_str()))
        .map(|pmid| PubmedFailure {
            pmid: pmid.clone(),
            reason: "PubMed 未返回该 PMID 的可导入文献".to_string(),
        })
        .collect();

    Ok(PubmedFetchResult { articles, failures })
}

fn parse_pubmed_xml(xml: &str) -> Result<Vec<NewArticle>> {
    use quick_xml::events::Event;
    use quick_xml::Reader;

    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(true);

    let mut out: Vec<NewArticle> = Vec::new();
    let mut current: Option<NewArticle> = None;
    let mut path: Vec<String> = Vec::new();
    let mut text_buf = String::new();
    let mut last_author_last = String::new();
    let mut last_author_init = String::new();
    let mut buf = Vec::new();

    loop {
        match reader.read_event_into(&mut buf) {
            Err(e) => return Err(anyhow!("XML 解析错误: {e}")),
            Ok(Event::Eof) => break,
            Ok(Event::Start(e)) => {
                let name = String::from_utf8_lossy(e.name().as_ref()).to_string();
                path.push(name.clone());
                text_buf.clear();
                if name == "PubmedArticle" {
                    current = Some(NewArticle {
                        title: String::new(),
                        abstract_text: String::new(),
                        authors: Vec::new(),
                        journal: None,
                        year: None,
                        doi: None,
                        pmid: None,
                        keywords: Vec::new(),
                        source: ArticleSource::Pubmed,
                    });
                }
                if name == "Author" {
                    last_author_last.clear();
                    last_author_init.clear();
                }
            }
            Ok(Event::End(e)) => {
                let name = String::from_utf8_lossy(e.name().as_ref()).to_string();
                if let Some(art) = current.as_mut() {
                    let txt = text_buf.trim().to_string();
                    match name.as_str() {
                        "ArticleTitle" => art.title = txt.clone(),
                        "AbstractText" => {
                            if !art.abstract_text.is_empty() {
                                art.abstract_text.push('\n');
                            }
                            art.abstract_text.push_str(&txt);
                        }
                        "Title" if path_contains(&path, "Journal") => {
                            art.journal = Some(txt.clone());
                        }
                        "Year" if path_contains(&path, "PubDate") => {
                            if let Ok(y) = txt.parse::<i32>() {
                                art.year = Some(y);
                            }
                        }
                        "LastName" if path_contains(&path, "Author") => {
                            last_author_last = txt.clone();
                        }
                        "Initials" if path_contains(&path, "Author") => {
                            last_author_init = txt.clone();
                        }
                        "Author" => {
                            let n = if last_author_init.is_empty() {
                                last_author_last.clone()
                            } else {
                                format!("{} {}", last_author_last, last_author_init)
                            };
                            if !n.trim().is_empty() {
                                art.authors.push(n);
                            }
                        }
                        "PMID" if path_contains(&path, "MedlineCitation") => {
                            if art.pmid.is_none() {
                                art.pmid = Some(txt.clone());
                            }
                        }
                        "Keyword" if path_contains(&path, "KeywordList") => {
                            if !txt.is_empty()
                                && !art
                                    .keywords
                                    .iter()
                                    .any(|keyword| keyword.eq_ignore_ascii_case(&txt))
                            {
                                art.keywords.push(txt.clone());
                            }
                        }
                        "ArticleId" => {
                            // DOI: ArticleIdList/ArticleId IdType="doi"
                            // 我们无法在 End 时拿到属性，故在 Start 中处理。
                        }
                        _ => {}
                    }
                }
                if name == "PubmedArticle" {
                    if let Some(a) = current.take() {
                        if !a.title.trim().is_empty() {
                            out.push(a);
                        }
                    }
                }
                path.pop();
                text_buf.clear();
            }
            Ok(Event::Empty(_)) => {}
            Ok(Event::Text(t)) => {
                text_buf.push_str(&t.unescape().unwrap_or_default());
            }
            Ok(Event::CData(t)) => {
                text_buf.push_str(&String::from_utf8_lossy(t.as_ref()));
            }
            _ => {}
        }
        buf.clear();
    }

    // DOI 用简易正则在原文中再扫一遍（按 PMID 关联较复杂，实用即可）。
    let doi_re =
        regex::Regex::new(r#"<ArticleId\s+IdType\s*=\s*["']doi["']\s*>([^<]+)</ArticleId>"#)
            .unwrap();
    let elocation_doi_re =
        regex::Regex::new(r#"<ELocationID\s+EIdType\s*=\s*["']doi["'][^>]*>([^<]+)</ELocationID>"#)
            .unwrap();
    let pmid_re =
        regex::Regex::new(r#"<ArticleId\s+IdType\s*=\s*["']pubmed["']\s*>([^<]+)</ArticleId>"#)
            .unwrap();
    // 按 <PubmedArticle> 块切分（注意末尾的 `>` 以避免匹配 `<PubmedArticleSet>`）。
    let split_re = regex::Regex::new(r#"<PubmedArticle[\s>]"#).unwrap();
    for (idx, block) in split_re.split(xml).enumerate() {
        if idx == 0 || idx > out.len() {
            continue;
        }
        if let Some(c) = doi_re.captures(block) {
            out[idx - 1].doi = Some(c[1].trim().to_string());
        } else if let Some(c) = elocation_doi_re.captures(block) {
            out[idx - 1].doi = Some(c[1].trim().to_string());
        }
        if out[idx - 1].pmid.is_none() {
            if let Some(c) = pmid_re.captures(block) {
                out[idx - 1].pmid = Some(c[1].trim().to_string());
            }
        }
    }

    Ok(out)
}

fn path_contains(path: &[String], needle: &str) -> bool {
    path.iter().any(|s| s == needle)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_valid_pubmed_tokens() {
        assert_eq!(parse_pubmed_token("12345678").as_deref(), Some("12345678"));
        assert_eq!(
            parse_pubmed_token("PMID:23456789").as_deref(),
            Some("23456789")
        );
        assert_eq!(
            parse_pubmed_token("https://pubmed.ncbi.nlm.nih.gov/34567890/").as_deref(),
            Some("34567890")
        );
        assert!(parse_pubmed_token("https://example.com/12345678").is_none());
    }

    #[test]
    fn rejects_invalid_pubmed_tokens() {
        assert!(parse_pubmed_token("0000").is_none());
        assert!(parse_pubmed_token("not-a-pmid").is_none());
    }
}
