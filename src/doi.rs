use anyhow::{anyhow, Context, Result};
use once_cell::sync::Lazy;
use reqwest::blocking::Client;
use scraper::{Html, Selector};
use serde_json::Value;
use shared::{ArticleSource, NewArticle};

static DOI_URL_RE: Lazy<regex::Regex> = Lazy::new(|| {
    regex::Regex::new(r#"(?i)(?:https?://)?(?:dx\.)?doi\.org/([^\s<>'\"]+)"#)
        .expect("valid DOI URL regex")
});
static DOI_PREFIX_RE: Lazy<regex::Regex> = Lazy::new(|| {
    regex::Regex::new(r#"(?i)\bdoi\s*[:：]\s*(10\.\d{4,9}/[^\s<>'\"]+)"#)
        .expect("valid DOI prefix regex")
});
static BARE_DOI_RE: Lazy<regex::Regex> = Lazy::new(|| {
    regex::Regex::new(r#"(?i)\b10\.\d{4,9}/[^\s<>'\"]+"#).expect("valid bare DOI regex")
});
static YEAR_RE: Lazy<regex::Regex> =
    Lazy::new(|| regex::Regex::new(r#"\b(19|20)\d{2}\b"#).expect("valid year regex"));

#[derive(Debug, Default, Clone)]
struct ArticleMetadata {
    title: Option<String>,
    abstract_text: Option<String>,
    authors: Vec<String>,
    journal: Option<String>,
    year: Option<i32>,
    doi: Option<String>,
    keywords: Vec<String>,
}

pub fn extract_doi(text: &str) -> Option<String> {
    for regex in [&*DOI_URL_RE, &*DOI_PREFIX_RE, &*BARE_DOI_RE] {
        if let Some(captures) = regex.captures(text) {
            let raw = captures.get(1).or_else(|| captures.get(0))?.as_str();
            if let Some(doi) = clean_doi(raw) {
                return Some(doi);
            }
        }
    }
    None
}

pub fn is_supported_article_input(input: &str) -> bool {
    extract_doi(input).is_some() || parse_http_url(input).is_some()
}

pub fn canonical_input_key(input: &str) -> String {
    extract_doi(input).unwrap_or_else(|| input.trim().trim_end_matches(['.', ',', ';']).to_string())
}

pub fn fetch_article_from_input(input: &str, source: ArticleSource) -> Result<NewArticle> {
    if let Some(doi) = extract_doi(input) {
        return fetch_article_from_doi(&doi, source);
    }
    let url = parse_http_url(input).ok_or_else(|| anyhow!("无法识别 DOI 或论文网页链接"))?;
    fetch_article_from_url(&url, source)
}

pub fn fetch_article_from_doi(doi: &str, source: ArticleSource) -> Result<NewArticle> {
    let doi = clean_doi(doi).ok_or_else(|| anyhow!("无法识别 DOI"))?;
    let metadata = fetch_metadata_from_clean_doi(&doi)?;
    metadata_to_article(metadata, source, &format!("DOI {doi}"))
}

pub fn fetch_article_from_doi_with_fallback(
    doi: &str,
    source: ArticleSource,
    fallback: NewArticle,
) -> Result<NewArticle> {
    let doi = clean_doi(doi).ok_or_else(|| anyhow!("无法识别 DOI"))?;
    let fallback_metadata = article_to_metadata(fallback, &doi);
    match fetch_metadata_from_clean_doi(&doi) {
        Ok(mut metadata) => {
            merge_missing(&mut metadata, fallback_metadata.clone());
            metadata_to_article(metadata, source, &format!("DOI {doi}")).or_else(|_| {
                metadata_to_article(fallback_metadata, source, &format!("PDF DOI {doi}"))
            })
        }
        Err(error) => metadata_to_article(fallback_metadata, source, &format!("PDF DOI {doi}"))
            .with_context(|| format!("无法通过 DOI 访问期刊网页: {doi}: {error}")),
    }
}

fn fetch_metadata_from_clean_doi(doi: &str) -> Result<ArticleMetadata> {
    let client = build_client()?;
    let url = format!("https://doi.org/{doi}");
    let html = match fetch_html(&client, &url) {
        Ok(html) => html,
        Err(page_error) => {
            return fetch_crossref(&client, doi).with_context(|| {
                format!(
                    "无法通过 DOI 访问期刊网页: {doi}: {page_error}; Crossref 也未返回可用元数据"
                )
            });
        }
    };
    let mut metadata = parse_article_page(&html.body);
    if let Some(page_doi) = metadata.doi.as_deref().and_then(clean_doi) {
        if page_doi != doi {
            return Err(anyhow!("DOI 跳转后的页面 DOI 不匹配，已拒绝导入"));
        }
        metadata.doi = Some(page_doi);
    } else {
        metadata.doi = Some(doi.to_string());
    }
    enrich_from_crossref(&client, &mut metadata);
    Ok(metadata)
}

pub fn fetch_article_from_url(article_url: &str, source: ArticleSource) -> Result<NewArticle> {
    let url = parse_http_url(article_url).ok_or_else(|| anyhow!("无法识别网页链接"))?;
    let client = build_client()?;
    let html = fetch_html(&client, &url).with_context(|| format!("无法访问网页: {url}"))?;
    let mut metadata = parse_article_page(&html.body);
    if metadata.doi.is_none() {
        return Err(anyhow!(
            "该链接页面未在论文元数据中提供 DOI，可能不是论文页面，已拒绝导入"
        ));
    }
    enrich_from_crossref(&client, &mut metadata);
    metadata_to_article(metadata, source, &html.final_url)
}

fn build_client() -> Result<Client> {
    Ok(Client::builder()
        .timeout(std::time::Duration::from_secs(45))
        .user_agent("RayviewMeta/0.1 (+https://github.com/AbyssoRay/RayViewMeta)")
        .redirect(reqwest::redirect::Policy::limited(10))
        .build()?)
}

struct FetchedHtml {
    final_url: String,
    body: String,
}

fn fetch_html(client: &Client, url: &str) -> Result<FetchedHtml> {
    let response = client
        .get(url)
        .header(reqwest::header::ACCEPT, "text/html,application/xhtml+xml")
        .send()?;
    let status = response.status();
    if !status.is_success() {
        return Err(anyhow!("网页返回 HTTP {status}"));
    }
    let final_url = response.url().to_string();
    let body = response.text()?;
    if body.trim().is_empty() {
        return Err(anyhow!("网页内容为空"));
    }
    Ok(FetchedHtml { final_url, body })
}

fn parse_article_page(html: &str) -> ArticleMetadata {
    let document = Html::parse_document(html);
    let mut metadata = ArticleMetadata::default();
    collect_meta_tags(&document, &mut metadata);
    collect_json_ld(&document, &mut metadata);
    collect_dom_fallbacks(&document, &mut metadata);
    metadata
}

fn collect_meta_tags(document: &Html, metadata: &mut ArticleMetadata) {
    let selector = Selector::parse("meta").expect("valid selector");
    for element in document.select(&selector) {
        let value = element.value();
        let Some(content) = value.attr("content").map(clean_text) else {
            continue;
        };
        if content.is_empty() {
            continue;
        }
        let key = value
            .attr("name")
            .or_else(|| value.attr("property"))
            .or_else(|| value.attr("itemprop"))
            .unwrap_or_default()
            .trim()
            .to_ascii_lowercase();
        match key.as_str() {
            "citation_title" | "dc.title" | "dcterms.title" | "prism.title" => {
                set_if_empty(&mut metadata.title, content)
            }
            "og:title" | "twitter:title" => set_if_empty(&mut metadata.title, content),
            "citation_abstract"
            | "dc.description"
            | "dcterms.abstract"
            | "description"
            | "og:description"
            | "twitter:description" => set_better_abstract(&mut metadata.abstract_text, content),
            "citation_author" | "dc.creator" | "dcterms.creator" | "article:author" => {
                push_unique(&mut metadata.authors, content)
            }
            "citation_journal_title"
            | "citation_conference_title"
            | "prism.publicationname"
            | "dc.source"
            | "dcterms.source" => set_if_empty(&mut metadata.journal, content),
            "citation_publication_date"
            | "citation_online_date"
            | "citation_date"
            | "prism.publicationdate"
            | "dc.date"
            | "dcterms.issued" => {
                if metadata.year.is_none() {
                    metadata.year = parse_year(&content);
                }
            }
            "citation_doi" | "dc.identifier" | "dc.identifier.doi" | "prism.doi" | "doi" => {
                if metadata.doi.is_none() {
                    metadata.doi = extract_doi(&content).or_else(|| clean_doi(&content));
                }
            }
            "citation_keywords" | "citation_keyword" | "keywords" | "dc.subject"
            | "dcterms.subject" | "article:tag" => {
                extend_keywords(&mut metadata.keywords, &content)
            }
            _ => {
                if metadata.doi.is_none() && key.contains("doi") {
                    metadata.doi = extract_doi(&content).or_else(|| clean_doi(&content));
                }
            }
        }
    }
}

fn collect_json_ld(document: &Html, metadata: &mut ArticleMetadata) {
    let selector =
        Selector::parse(r#"script[type="application/ld+json"]"#).expect("valid selector");
    for element in document.select(&selector) {
        let text = element.text().collect::<Vec<_>>().join(" ");
        let Ok(value) = serde_json::from_str::<Value>(&text) else {
            continue;
        };
        collect_json_ld_value(&value, metadata);
    }
}

fn collect_json_ld_value(value: &Value, metadata: &mut ArticleMetadata) {
    match value {
        Value::Array(items) => {
            for item in items {
                collect_json_ld_value(item, metadata);
            }
        }
        Value::Object(object) => {
            if let Some(graph) = object.get("@graph") {
                collect_json_ld_value(graph, metadata);
            }
            if !looks_like_article_json(object) {
                return;
            }
            set_if_empty(
                &mut metadata.title,
                first_string(object, &["headline", "name"]).unwrap_or_default(),
            );
            set_better_abstract(
                &mut metadata.abstract_text,
                first_string(object, &["abstract", "description"]).unwrap_or_default(),
            );
            if metadata.doi.is_none() {
                metadata.doi = first_string(object, &["doi", "identifier"])
                    .and_then(|value| extract_doi(&value).or_else(|| clean_doi(&value)));
            }
            if metadata.year.is_none() {
                metadata.year = first_string(object, &["datePublished", "dateCreated"])
                    .and_then(|value| parse_year(&value));
            }
            if metadata.journal.is_none() {
                metadata.journal = journal_from_json(object);
            }
            collect_authors_from_json(object.get("author"), &mut metadata.authors);
            collect_keywords_from_json(object.get("keywords"), &mut metadata.keywords);
        }
        _ => {}
    }
}

fn collect_dom_fallbacks(document: &Html, metadata: &mut ArticleMetadata) {
    if metadata.title.is_none() {
        if let Ok(selector) = Selector::parse("h1") {
            if let Some(title) = document
                .select(&selector)
                .map(|element| clean_text(&element.text().collect::<Vec<_>>().join(" ")))
                .find(|text| is_plausible_title(text))
            {
                metadata.title = Some(title);
            }
        }
    }
    if metadata.abstract_text.is_none() {
        let Ok(selector) = Selector::parse("section, div, article") else {
            return;
        };
        for element in document.select(&selector) {
            let class_or_id = format!(
                "{} {}",
                element.value().attr("class").unwrap_or_default(),
                element.value().attr("id").unwrap_or_default()
            )
            .to_ascii_lowercase();
            if !class_or_id.contains("abstract") && !class_or_id.contains("summary") {
                continue;
            }
            let text = clean_text(&element.text().collect::<Vec<_>>().join(" "));
            if is_plausible_abstract(&text) {
                metadata.abstract_text = Some(strip_abstract_prefix(&text));
                break;
            }
        }
    }
}

fn enrich_from_crossref(client: &Client, metadata: &mut ArticleMetadata) {
    let Some(doi) = metadata.doi.as_deref().and_then(clean_doi) else {
        return;
    };
    let Ok(crossref) = fetch_crossref(client, &doi) else {
        return;
    };
    merge_missing(metadata, crossref);
}

fn fetch_crossref(client: &Client, doi: &str) -> Result<ArticleMetadata> {
    let encoded = url::form_urlencoded::byte_serialize(doi.as_bytes()).collect::<String>();
    let url = format!("https://api.crossref.org/works/{encoded}");
    let response = client
        .get(url)
        .header(reqwest::header::ACCEPT, "application/json")
        .send()?
        .error_for_status()?;
    let value: Value = response.json()?;
    let message = value
        .get("message")
        .ok_or_else(|| anyhow!("Crossref response missing message"))?;
    let mut metadata = ArticleMetadata {
        doi: Some(doi.to_string()),
        ..Default::default()
    };
    if let Some(title) = message
        .get("title")
        .and_then(Value::as_array)
        .and_then(|items| items.first())
        .and_then(Value::as_str)
    {
        metadata.title = Some(clean_text(title));
    }
    if let Some(abstract_text) = message.get("abstract").and_then(Value::as_str) {
        metadata.abstract_text = Some(strip_html_tags(abstract_text));
    }
    if let Some(journal) = message
        .get("container-title")
        .and_then(Value::as_array)
        .and_then(|items| items.first())
        .and_then(Value::as_str)
    {
        metadata.journal = Some(clean_text(journal));
    }
    metadata.year = crossref_year(message);
    if let Some(authors) = message.get("author").and_then(Value::as_array) {
        for author in authors {
            let given = author
                .get("given")
                .and_then(Value::as_str)
                .unwrap_or_default();
            let family = author
                .get("family")
                .and_then(Value::as_str)
                .unwrap_or_default();
            let name = clean_text(&format!("{given} {family}"));
            push_unique(&mut metadata.authors, name);
        }
    }
    if let Some(subjects) = message.get("subject").and_then(Value::as_array) {
        for subject in subjects.iter().filter_map(Value::as_str) {
            push_unique(&mut metadata.keywords, clean_text(subject));
        }
    }
    Ok(metadata)
}

fn metadata_to_article(
    metadata: ArticleMetadata,
    source: ArticleSource,
    item: &str,
) -> Result<NewArticle> {
    let doi = metadata
        .doi
        .as_deref()
        .and_then(clean_doi)
        .ok_or_else(|| anyhow!("{item} 未提供 DOI，可能不是论文页面，已拒绝导入"))?;
    let title = metadata
        .title
        .map(|title| clean_text(&title))
        .filter(|title| is_plausible_title(title))
        .ok_or_else(|| anyhow!("{item} 未提供可识别的论文标题，已拒绝导入"))?;
    let abstract_text = metadata
        .abstract_text
        .map(|abstract_text| strip_abstract_prefix(&abstract_text))
        .filter(|abstract_text| is_plausible_abstract(abstract_text))
        .ok_or_else(|| {
            anyhow!("{item} 未提供可识别的摘要，可能不是论文页面或摘要需要脚本加载，已拒绝导入")
        })?;
    Ok(NewArticle {
        title,
        abstract_text,
        authors: metadata.authors,
        journal: metadata.journal,
        year: metadata.year,
        doi: Some(doi),
        pmid: None,
        keywords: metadata.keywords,
        source,
    })
}

fn article_to_metadata(article: NewArticle, doi: &str) -> ArticleMetadata {
    ArticleMetadata {
        title: Some(article.title),
        abstract_text: Some(article.abstract_text),
        authors: article.authors,
        journal: article.journal,
        year: article.year,
        doi: article
            .doi
            .and_then(|value| clean_doi(&value))
            .or_else(|| Some(doi.to_string())),
        keywords: article.keywords,
    }
}

fn parse_http_url(input: &str) -> Option<String> {
    let trimmed = input
        .trim()
        .trim_matches(|ch: char| matches!(ch, '<' | '>' | '(' | ')' | '[' | ']' | '"' | '\''))
        .trim_end_matches(['.', ',', ';']);
    let lower = trimmed.to_ascii_lowercase();
    let url = if lower.starts_with("http://") || lower.starts_with("https://") {
        trimmed.to_string()
    } else if lower.starts_with("doi.org/") || lower.starts_with("dx.doi.org/") {
        format!("https://{trimmed}")
    } else {
        return None;
    };
    let parsed = url::Url::parse(&url).ok()?;
    matches!(parsed.scheme(), "http" | "https").then_some(url)
}

fn clean_doi(raw: &str) -> Option<String> {
    let mut value = raw.trim().to_string();
    value = value
        .trim_start_matches(|ch: char| ch.is_whitespace())
        .to_string();
    if value.to_ascii_lowercase().starts_with("doi:") {
        value = value[4..].trim().to_string();
    }
    if let Ok(url) = url::Url::parse(&value) {
        if url.host_str().is_some_and(|host| {
            matches!(host.to_ascii_lowercase().as_str(), "doi.org" | "dx.doi.org")
        }) {
            value = url.path().trim_start_matches('/').to_string();
        }
    } else {
        for prefix in [
            "https://doi.org/",
            "http://doi.org/",
            "https://dx.doi.org/",
            "http://dx.doi.org/",
        ] {
            if value.to_ascii_lowercase().starts_with(prefix) {
                value = value[prefix.len()..].to_string();
                break;
            }
        }
    }
    let value = value
        .split(['?', '#'])
        .next()
        .unwrap_or(&value)
        .trim()
        .trim_end_matches(['.', ',', ';', ':', '，', '。'])
        .to_ascii_lowercase();
    BARE_DOI_RE
        .find(&value)
        .map(|matched| matched.as_str().to_string())
}

fn set_if_empty(target: &mut Option<String>, value: String) {
    if target.is_none() && !value.trim().is_empty() {
        *target = Some(value);
    }
}

fn set_better_abstract(target: &mut Option<String>, value: String) {
    let value = strip_abstract_prefix(&value);
    if !is_plausible_abstract(&value) {
        return;
    }
    if target
        .as_ref()
        .map(|current| value.chars().count() > current.chars().count())
        .unwrap_or(true)
    {
        *target = Some(value);
    }
}

fn push_unique(values: &mut Vec<String>, value: String) {
    let value = clean_text(&value);
    if value.is_empty()
        || values
            .iter()
            .any(|existing| existing.eq_ignore_ascii_case(&value))
    {
        return;
    }
    values.push(value);
}

fn extend_keywords(values: &mut Vec<String>, text: &str) {
    for keyword in text.split([';', ',', '|', '；', '，']) {
        let keyword = clean_text(keyword);
        if (2..=80).contains(&keyword.chars().count()) {
            push_unique(values, keyword);
        }
    }
}

fn clean_text(text: &str) -> String {
    text.replace('\u{a0}', " ")
        .replace("\r", " ")
        .replace("\n", " ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .trim()
        .to_string()
}

fn strip_abstract_prefix(text: &str) -> String {
    let text = strip_html_tags(text);
    let lower = text.to_ascii_lowercase();
    for prefix in ["abstract", "summary"] {
        if lower == prefix {
            return String::new();
        }
        if let Some(rest) = lower.strip_prefix(prefix) {
            if rest
                .chars()
                .next()
                .is_some_and(|ch| ch.is_whitespace() || matches!(ch, ':' | '.' | '-' | '—' | '–'))
            {
                return text[prefix.len()..]
                    .trim_start_matches(|ch: char| {
                        ch.is_whitespace() || matches!(ch, ':' | '.' | '-' | '—' | '–')
                    })
                    .trim()
                    .to_string();
            }
        }
    }
    text
}

fn strip_html_tags(text: &str) -> String {
    let without_tags = regex::Regex::new(r#"<[^>]+>"#)
        .expect("valid html tag regex")
        .replace_all(text, " ")
        .to_string();
    clean_text(&without_tags)
}

fn parse_year(text: &str) -> Option<i32> {
    YEAR_RE
        .find(text)
        .and_then(|matched| matched.as_str().parse::<i32>().ok())
}

fn is_plausible_title(text: &str) -> bool {
    let text = clean_text(text);
    let char_count = text.chars().count();
    let word_count = text.split_whitespace().count();
    (8..=500).contains(&char_count)
        && word_count >= 2
        && !text.to_ascii_lowercase().contains("access denied")
}

fn is_plausible_abstract(text: &str) -> bool {
    let text = clean_text(text);
    text.chars().count() >= 40 && text.split_whitespace().count() >= 6
}

fn looks_like_article_json(object: &serde_json::Map<String, Value>) -> bool {
    let Some(value) = object.get("@type") else {
        return object.contains_key("headline") && object.contains_key("doi");
    };
    let types = match value {
        Value::String(text) => vec![text.to_ascii_lowercase()],
        Value::Array(items) => items
            .iter()
            .filter_map(Value::as_str)
            .map(str::to_ascii_lowercase)
            .collect(),
        _ => Vec::new(),
    };
    types.iter().any(|item| {
        item.contains("scholarlyarticle")
            || item.contains("medicalscholarlyarticle")
            || item == "article"
            || item.contains("researcharticle")
            || item.contains("report")
    })
}

fn first_string(object: &serde_json::Map<String, Value>, keys: &[&str]) -> Option<String> {
    for key in keys {
        if let Some(value) = object.get(*key).and_then(value_to_string) {
            let value = clean_text(&value);
            if !value.is_empty() {
                return Some(value);
            }
        }
    }
    None
}

fn value_to_string(value: &Value) -> Option<String> {
    match value {
        Value::String(text) => Some(text.clone()),
        Value::Object(object) => object
            .get("name")
            .and_then(Value::as_str)
            .map(ToOwned::to_owned),
        Value::Array(items) => items.iter().find_map(value_to_string),
        _ => None,
    }
}

fn collect_authors_from_json(value: Option<&Value>, authors: &mut Vec<String>) {
    match value {
        Some(Value::Array(items)) => {
            for item in items {
                collect_authors_from_json(Some(item), authors);
            }
        }
        Some(Value::Object(object)) => {
            if let Some(name) = first_string(object, &["name"]) {
                push_unique(authors, name);
            }
        }
        Some(Value::String(name)) => push_unique(authors, name.clone()),
        _ => {}
    }
}

fn collect_keywords_from_json(value: Option<&Value>, keywords: &mut Vec<String>) {
    match value {
        Some(Value::Array(items)) => {
            for item in items {
                collect_keywords_from_json(Some(item), keywords);
            }
        }
        Some(Value::String(text)) => extend_keywords(keywords, text),
        _ => {}
    }
}

fn journal_from_json(object: &serde_json::Map<String, Value>) -> Option<String> {
    for key in ["isPartOf", "publisher"] {
        if let Some(value) = object.get(key).and_then(value_to_string) {
            let value = clean_text(&value);
            if !value.is_empty() {
                return Some(value);
            }
        }
    }
    None
}

fn crossref_year(message: &Value) -> Option<i32> {
    for key in ["published-print", "published-online", "published", "issued"] {
        let Some(parts) = message
            .get(key)
            .and_then(|value| value.get("date-parts"))
            .and_then(Value::as_array)
            .and_then(|items| items.first())
            .and_then(Value::as_array)
        else {
            continue;
        };
        if let Some(year) = parts.first().and_then(Value::as_i64) {
            return Some(year as i32);
        }
    }
    None
}

fn merge_missing(target: &mut ArticleMetadata, fallback: ArticleMetadata) {
    if target.title.is_none() {
        target.title = fallback.title;
    }
    if target.abstract_text.is_none() {
        target.abstract_text = fallback.abstract_text;
    }
    if target.authors.is_empty() {
        target.authors = fallback.authors;
    }
    if target.journal.is_none() {
        target.journal = fallback.journal;
    }
    if target.year.is_none() {
        target.year = fallback.year;
    }
    if target.doi.is_none() {
        target.doi = fallback.doi;
    }
    for keyword in fallback.keywords {
        push_unique(&mut target.keywords, keyword);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_common_doi_formats() {
        assert_eq!(
            extract_doi("https://doi.org/10.1016/j.cell.2020.01.001").as_deref(),
            Some("10.1016/j.cell.2020.01.001")
        );
        assert_eq!(
            extract_doi("doi: 10.1109/TMI.2014.2351234.").as_deref(),
            Some("10.1109/tmi.2014.2351234")
        );
    }

    #[test]
    fn parses_highwire_meta_tags() {
        let html = r#"
            <html><head>
            <meta name="citation_title" content="Accurate article title">
            <meta name="citation_abstract" content="Abstract: This study reports a robust approach for metadata extraction from journal pages.">
            <meta name="citation_author" content="Jane Smith">
            <meta name="citation_journal_title" content="Journal of Examples">
            <meta name="citation_publication_date" content="2025/04/01">
            <meta name="citation_doi" content="10.1234/example.2025.001">
            <meta name="citation_keywords" content="screening; metadata; doi">
            </head></html>
        "#;
        let article = metadata_to_article(parse_article_page(html), ArticleSource::Web, "test")
            .expect("metadata should be article");

        assert_eq!(article.title, "Accurate article title");
        assert_eq!(article.doi.as_deref(), Some("10.1234/example.2025.001"));
        assert_eq!(article.journal.as_deref(), Some("Journal of Examples"));
        assert_eq!(article.year, Some(2025));
        assert!(article.keywords.contains(&"metadata".to_string()));
    }

    #[test]
    fn rejects_non_article_page_metadata() {
        let html = r#"<html><head><title>Example Domain</title></head><body><h1>Example Domain</h1></body></html>"#;

        assert!(metadata_to_article(parse_article_page(html), ArticleSource::Web, "bad").is_err());
    }
}
