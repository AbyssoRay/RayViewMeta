use anyhow::{anyhow, Result};
use reqwest::blocking::Client;
use serde::Deserialize;

const TRANSLATE_ENDPOINT: &str = "https://api.mymemory.translated.net/get";
const MAX_CHUNK_CHARS: usize = 450;

#[derive(Debug, Clone)]
pub struct TranslatedAbstract {
    pub text: String,
    pub translated_keywords: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct MyMemoryResponse {
    #[serde(rename = "responseData")]
    response_data: MyMemoryData,
}

#[derive(Debug, Deserialize)]
struct MyMemoryData {
    #[serde(rename = "translatedText")]
    translated_text: String,
}

pub fn translate_abstract(text: &str, keywords: &[String]) -> Result<TranslatedAbstract> {
    let client = Client::builder()
        .timeout(std::time::Duration::from_secs(45))
        .build()?;
    let translated_text = translate_text(&client, text)?;
    let translated_keywords = translate_keywords(&client, keywords).unwrap_or_default();
    Ok(TranslatedAbstract {
        text: translated_text,
        translated_keywords,
    })
}

fn translate_keywords(client: &Client, keywords: &[String]) -> Result<Vec<String>> {
    let clean_keywords = keywords
        .iter()
        .map(|keyword| keyword.trim())
        .filter(|keyword| !keyword.is_empty())
        .collect::<Vec<_>>();
    if clean_keywords.is_empty() {
        return Ok(Vec::new());
    }
    let joined = clean_keywords.join("\n");
    let translated = translate_text(client, &joined)?;
    Ok(translated
        .lines()
        .flat_map(|line| line.split(['；', ';', '，', ',']))
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(ToOwned::to_owned)
        .collect())
}

fn translate_text(client: &Client, text: &str) -> Result<String> {
    let chunks = chunk_text(text, MAX_CHUNK_CHARS);
    if chunks.is_empty() {
        return Ok(String::new());
    }
    let mut translated = Vec::with_capacity(chunks.len());
    for chunk in chunks {
        translated.push(translate_chunk(client, &chunk)?);
    }
    Ok(translated
        .join(" ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" "))
}

fn translate_chunk(client: &Client, text: &str) -> Result<String> {
    let response = client
        .get(TRANSLATE_ENDPOINT)
        .query(&[("q", text), ("langpair", "en|zh-CN")])
        .send()?;
    let status = response.status();
    if !status.is_success() {
        return Err(anyhow!("翻译服务返回 HTTP {status}"));
    }
    let body: MyMemoryResponse = response.json()?;
    let translated = body.response_data.translated_text.trim().to_string();
    if translated.is_empty() {
        return Err(anyhow!("翻译服务未返回有效内容"));
    }
    Ok(translated)
}

fn chunk_text(text: &str, max_chars: usize) -> Vec<String> {
    let mut chunks = Vec::new();
    let mut current = String::new();
    for sentence in split_sentences(text) {
        let sentence_len = sentence.chars().count();
        if current.chars().count() + sentence_len + 1 > max_chars && !current.is_empty() {
            chunks.push(current.trim().to_string());
            current.clear();
        }
        if sentence_len > max_chars {
            chunks.extend(split_long_sentence(sentence, max_chars));
        } else {
            if !current.is_empty() {
                current.push(' ');
            }
            current.push_str(sentence);
        }
    }
    if !current.trim().is_empty() {
        chunks.push(current.trim().to_string());
    }
    chunks
}

fn split_sentences(text: &str) -> Vec<&str> {
    let mut sentences = Vec::new();
    let mut start = 0usize;
    for (index, ch) in text.char_indices() {
        if matches!(ch, '.' | '?' | '!' | ';') {
            let end = index + ch.len_utf8();
            let sentence = text[start..end].trim();
            if !sentence.is_empty() {
                sentences.push(sentence);
            }
            start = end;
        }
    }
    let tail = text[start..].trim();
    if !tail.is_empty() {
        sentences.push(tail);
    }
    sentences
}

fn split_long_sentence(sentence: &str, max_chars: usize) -> Vec<String> {
    let mut chunks = Vec::new();
    let mut current = String::new();
    for word in sentence.split_whitespace() {
        if current.chars().count() + word.chars().count() + 1 > max_chars && !current.is_empty() {
            chunks.push(current.trim().to_string());
            current.clear();
        }
        if !current.is_empty() {
            current.push(' ');
        }
        current.push_str(word);
    }
    if !current.trim().is_empty() {
        chunks.push(current.trim().to_string());
    }
    chunks
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chunks_text_without_exceeding_limit() {
        let text = "One sentence. Another sentence is a little longer. Final sentence.";
        let chunks = chunk_text(text, 32);
        assert!(chunks.len() > 1);
        assert!(chunks.iter().all(|chunk| chunk.chars().count() <= 32));
    }
}
