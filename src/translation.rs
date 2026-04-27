use anyhow::{anyhow, Result};
use once_cell::sync::Lazy;
use reqwest::blocking::{Client, Response};
use reqwest::header::HeaderMap;
use reqwest::StatusCode;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::env;
use std::sync::Mutex;
use std::time::{Duration, Instant};

const MYMEMORY_ENDPOINT: &str = "https://api.mymemory.translated.net/get";
const GOOGLE_TRANSLATE_ENDPOINT: &str = "https://translate.googleapis.com/translate_a/single";
const DEFAULT_LIBRETRANSLATE_ENDPOINT: &str = "https://libretranslate.com/translate";
const DEFAULT_LLM_BASE_URL: &str = "https://api.openai.com/v1";
const MAX_CHUNK_CHARS: usize = 450;
const MIN_REQUEST_INTERVAL: Duration = Duration::from_millis(1800);
const MAX_RATE_LIMIT_RETRIES: usize = 4;
const DEFAULT_RATE_LIMIT_BACKOFF: [Duration; MAX_RATE_LIMIT_RETRIES] = [
    Duration::from_secs(15),
    Duration::from_secs(30),
    Duration::from_secs(60),
    Duration::from_secs(120),
];

static NEXT_REQUEST_AT: Lazy<Mutex<Instant>> = Lazy::new(|| Mutex::new(Instant::now()));

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

#[derive(Debug, Deserialize)]
struct LibreTranslateResponse {
    #[serde(rename = "translatedText")]
    translated_text: String,
}

#[derive(Debug, Serialize)]
struct LibreTranslateRequest<'a> {
    q: &'a str,
    source: &'static str,
    target: &'static str,
    format: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    api_key: Option<&'a str>,
}

#[derive(Debug, Serialize)]
struct ChatCompletionRequest {
    model: String,
    messages: Vec<ChatMessage>,
    temperature: f32,
}

#[derive(Debug, Serialize)]
struct ChatMessage {
    role: &'static str,
    content: String,
}

#[derive(Debug, Deserialize)]
struct ChatCompletionResponse {
    choices: Vec<ChatChoice>,
}

#[derive(Debug, Deserialize)]
struct ChatChoice {
    message: ChatResponseMessage,
}

#[derive(Debug, Deserialize)]
struct ChatResponseMessage {
    content: String,
}

#[derive(Debug, Deserialize)]
struct LlmTranslationResponse {
    translated_text: String,
    #[serde(default)]
    translated_keywords: Vec<String>,
}

#[derive(Debug, Clone, Copy)]
enum TranslationProvider {
    Google,
    MyMemory,
    LibreTranslate,
    OpenAiCompatible,
}

pub fn translate_abstract(text: &str, keywords: &[String]) -> Result<TranslatedAbstract> {
    let client = Client::builder()
        .timeout(std::time::Duration::from_secs(45))
        .build()?;
    match translation_provider()? {
        TranslationProvider::Google => {
            translate_with_chunked_provider(&client, text, keywords, translate_google_chunk)
        }
        TranslationProvider::MyMemory => {
            translate_with_chunked_provider(&client, text, keywords, translate_mymemory_chunk)
        }
        TranslationProvider::LibreTranslate => {
            translate_with_chunked_provider(&client, text, keywords, translate_libretranslate_chunk)
        }
        TranslationProvider::OpenAiCompatible => {
            translate_with_openai_compatible(&client, text, keywords)
        }
    }
}

fn translation_provider() -> Result<TranslationProvider> {
    let provider = env::var("RAYVIEW_TRANSLATION_PROVIDER")
        .unwrap_or_else(|_| "google".to_string())
        .trim()
        .to_ascii_lowercase();
    match provider.as_str() {
        "" | "google" | "google-translate" | "google_translate" => Ok(TranslationProvider::Google),
        "mymemory" | "my-memory" => Ok(TranslationProvider::MyMemory),
        "libre" | "libretranslate" | "libre-translate" => Ok(TranslationProvider::LibreTranslate),
        "llm" | "openai" | "openai-compatible" | "openai_compatible" => {
            Ok(TranslationProvider::OpenAiCompatible)
        }
        other => Err(anyhow!(
            "未知翻译后端 {other}，可用值: google, mymemory, libretranslate, openai"
        )),
    }
}

fn translate_with_chunked_provider(
    client: &Client,
    text: &str,
    keywords: &[String],
    translate_chunk: fn(&Client, &str) -> Result<String>,
) -> Result<TranslatedAbstract> {
    let translated_text = translate_text(client, text, translate_chunk)?;
    let translated_keywords =
        translate_keywords(client, keywords, translate_chunk).unwrap_or_default();
    Ok(TranslatedAbstract {
        text: translated_text,
        translated_keywords,
    })
}

fn translate_keywords(
    client: &Client,
    keywords: &[String],
    translate_chunk: fn(&Client, &str) -> Result<String>,
) -> Result<Vec<String>> {
    let clean_keywords = keywords
        .iter()
        .map(|keyword| keyword.trim())
        .filter(|keyword| !keyword.is_empty())
        .collect::<Vec<_>>();
    if clean_keywords.is_empty() {
        return Ok(Vec::new());
    }
    let joined = clean_keywords.join("\n");
    let translated = translate_text(client, &joined, translate_chunk)?;
    Ok(translated
        .lines()
        .flat_map(|line| line.split(['；', ';', '，', ',']))
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(ToOwned::to_owned)
        .collect())
}

fn translate_text(
    client: &Client,
    text: &str,
    translate_chunk: fn(&Client, &str) -> Result<String>,
) -> Result<String> {
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

fn translate_mymemory_chunk(client: &Client, text: &str) -> Result<String> {
    let response = send_with_rate_limit_retries(|| {
        client
            .get(MYMEMORY_ENDPOINT)
            .query(&[("q", text), ("langpair", "en|zh-CN")])
            .send()
    })?;
    let response = check_response(response, "MyMemory")?;
    let body: MyMemoryResponse = response.json()?;
    let translated = body.response_data.translated_text.trim().to_string();
    if translated.is_empty() {
        return Err(anyhow!("翻译服务未返回有效内容"));
    }
    Ok(translated)
}

fn translate_google_chunk(client: &Client, text: &str) -> Result<String> {
    let response = send_with_rate_limit_retries(|| {
        client
            .get(GOOGLE_TRANSLATE_ENDPOINT)
            .query(&[
                ("client", "gtx"),
                ("sl", "en"),
                ("tl", "zh-CN"),
                ("dt", "t"),
                ("q", text),
            ])
            .send()
    })?;
    let response = check_response(response, "Google Translate")?;
    let body: serde_json::Value = response.json()?;
    let translated = parse_google_translation(&body)?;
    if translated.trim().is_empty() {
        return Err(anyhow!("Google Translate 未返回有效内容"));
    }
    Ok(translated)
}

fn parse_google_translation(value: &serde_json::Value) -> Result<String> {
    let segments = value
        .get(0)
        .and_then(|value| value.as_array())
        .ok_or_else(|| anyhow!("Google Translate 返回格式无法解析"))?;
    let translated = segments
        .iter()
        .filter_map(|segment| segment.get(0))
        .filter_map(|text| text.as_str())
        .map(str::trim)
        .filter(|text| !text.is_empty())
        .collect::<Vec<_>>()
        .join("");
    Ok(translated)
}

fn translate_libretranslate_chunk(client: &Client, text: &str) -> Result<String> {
    let endpoint = env::var("RAYVIEW_LIBRETRANSLATE_URL")
        .unwrap_or_else(|_| DEFAULT_LIBRETRANSLATE_ENDPOINT.to_string());
    let api_key = env::var("RAYVIEW_LIBRETRANSLATE_API_KEY")
        .ok()
        .filter(|value| !value.trim().is_empty());
    let payload = LibreTranslateRequest {
        q: text,
        source: "en",
        target: "zh",
        format: "text",
        api_key: api_key.as_deref(),
    };
    let response = send_with_rate_limit_retries(|| client.post(&endpoint).json(&payload).send())?;
    let response = check_response(response, "LibreTranslate")?;
    let body: LibreTranslateResponse = response.json()?;
    let translated = body.translated_text.trim().to_string();
    if translated.is_empty() {
        return Err(anyhow!("LibreTranslate 未返回有效内容"));
    }
    Ok(translated)
}

fn translate_with_openai_compatible(
    client: &Client,
    text: &str,
    keywords: &[String],
) -> Result<TranslatedAbstract> {
    let api_key = required_env("RAYVIEW_LLM_API_KEY")?;
    let model = required_env("RAYVIEW_LLM_MODEL")?;
    let base_url =
        env::var("RAYVIEW_LLM_BASE_URL").unwrap_or_else(|_| DEFAULT_LLM_BASE_URL.to_string());
    let endpoint = format!("{}/chat/completions", base_url.trim_end_matches('/'));
    let referer = env::var("RAYVIEW_LLM_HTTP_REFERER")
        .ok()
        .filter(|value| !value.trim().is_empty());
    let title = env::var("RAYVIEW_LLM_APP_TITLE")
        .ok()
        .filter(|value| !value.trim().is_empty());

    let prompt_payload = json!({
        "abstract": text,
        "keywords": keywords,
    });
    let request = ChatCompletionRequest {
        model,
        temperature: 0.1,
        messages: vec![
            ChatMessage {
                role: "system",
                content: "You are a biomedical translation engine. Translate English academic abstracts into accurate Simplified Chinese. Preserve study design terms, drug names, disease names, abbreviations, numbers, and citation-like tokens. Return only valid JSON.".to_string(),
            },
            ChatMessage {
                role: "user",
                content: format!(
                    "Translate the JSON fields below. Return exactly one JSON object with this schema: {{\"translated_text\": string, \"translated_keywords\": string[]}}. If keywords are empty, return an empty translated_keywords array.\n\n{}",
                    prompt_payload
                ),
            },
        ],
    };

    let response = send_with_rate_limit_retries(|| {
        let mut builder = client.post(&endpoint).bearer_auth(&api_key).json(&request);
        if let Some(referer) = &referer {
            builder = builder.header("HTTP-Referer", referer);
        }
        if let Some(title) = &title {
            builder = builder.header("X-Title", title);
        }
        builder.send()
    })?;
    let response = check_response(response, "OpenAI-compatible translation")?;
    let body: ChatCompletionResponse = response.json()?;
    let content = body
        .choices
        .first()
        .map(|choice| choice.message.content.trim())
        .filter(|content| !content.is_empty())
        .ok_or_else(|| anyhow!("大模型翻译接口未返回有效内容"))?;
    parse_llm_translation(content)
}

fn parse_llm_translation(content: &str) -> Result<TranslatedAbstract> {
    let json_text =
        extract_json_object(content).ok_or_else(|| anyhow!("大模型翻译接口未返回 JSON 对象"))?;
    let parsed: LlmTranslationResponse = serde_json::from_str(json_text)?;
    let text = parsed.translated_text.trim().to_string();
    if text.is_empty() {
        return Err(anyhow!("大模型翻译接口返回了空翻译"));
    }
    Ok(TranslatedAbstract {
        text,
        translated_keywords: parsed
            .translated_keywords
            .into_iter()
            .map(|keyword| keyword.trim().to_string())
            .filter(|keyword| !keyword.is_empty())
            .collect(),
    })
}

fn extract_json_object(content: &str) -> Option<&str> {
    let trimmed = content.trim();
    let without_fence = trimmed
        .strip_prefix("```json")
        .or_else(|| trimmed.strip_prefix("```"))
        .and_then(|value| value.strip_suffix("```"))
        .map(str::trim)
        .unwrap_or(trimmed);
    let start = without_fence.find('{')?;
    let end = without_fence.rfind('}')?;
    (start <= end).then_some(&without_fence[start..=end])
}

fn required_env(name: &str) -> Result<String> {
    env::var(name)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .ok_or_else(|| anyhow!("使用大模型翻译需要设置环境变量 {name}"))
}

fn send_with_rate_limit_retries<F>(mut send: F) -> Result<Response>
where
    F: FnMut() -> reqwest::Result<Response>,
{
    let retry_delays = DEFAULT_RATE_LIMIT_BACKOFF
        .iter()
        .copied()
        .map(Some)
        .chain(std::iter::once(None));
    for fallback_delay in retry_delays {
        wait_for_request_slot();
        let response = send()?;
        let status = response.status();
        if status == StatusCode::TOO_MANY_REQUESTS {
            let Some(fallback_delay) = fallback_delay else {
                return Err(anyhow!(
                    "翻译服务返回 HTTP 429 Too Many Requests；已自动降速并退避重试，仍被限流，请稍后重试"
                ));
            };
            let delay = retry_after_delay(response.headers()).unwrap_or(fallback_delay);
            extend_request_cooldown(delay);
            continue;
        }
        if !status.is_success() {
            return Ok(response);
        }
        return Ok(response);
    }
    Err(anyhow!("翻译服务限流重试失败"))
}

fn check_response(response: Response, service: &str) -> Result<Response> {
    let status = response.status();
    if status.is_success() {
        return Ok(response);
    }
    let body = response.text().unwrap_or_default();
    let detail = body.trim();
    if detail.is_empty() {
        Err(anyhow!("{service} 返回 HTTP {status}"))
    } else {
        Err(anyhow!("{service} 返回 HTTP {status}: {detail}"))
    }
}

fn wait_for_request_slot() {
    let mut next_request_at = NEXT_REQUEST_AT
        .lock()
        .expect("translation throttle lock poisoned");
    let now = Instant::now();
    if *next_request_at > now {
        std::thread::sleep(*next_request_at - now);
    }
    *next_request_at = Instant::now() + MIN_REQUEST_INTERVAL;
}

fn extend_request_cooldown(delay: Duration) {
    let mut next_request_at = NEXT_REQUEST_AT
        .lock()
        .expect("translation throttle lock poisoned");
    let cooldown_until = Instant::now() + delay;
    if *next_request_at < cooldown_until {
        *next_request_at = cooldown_until;
    }
}

fn retry_after_delay(headers: &HeaderMap) -> Option<Duration> {
    headers
        .get(reqwest::header::RETRY_AFTER)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.trim().parse::<u64>().ok())
        .map(Duration::from_secs)
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

    #[test]
    fn parses_plain_llm_translation_json() {
        let translated = parse_llm_translation(
            r#"{"translated_text":"中文摘要","translated_keywords":["随机对照试验",""]}"#,
        )
        .unwrap();

        assert_eq!(translated.text, "中文摘要");
        assert_eq!(translated.translated_keywords, vec!["随机对照试验"]);
    }

    #[test]
    fn extracts_fenced_llm_translation_json() {
        let translated = parse_llm_translation(
            "```json\n{\"translated_text\":\"中文摘要\",\"translated_keywords\":[]}\n```",
        )
        .unwrap();

        assert_eq!(translated.text, "中文摘要");
        assert!(translated.translated_keywords.is_empty());
    }

    #[test]
    fn parses_google_translation_segments() {
        let value = serde_json::json!([
            [
                ["这是一项", "This is", null, null],
                ["随机试验。", "a randomized trial.", null, null]
            ],
            null,
            "en"
        ]);

        assert_eq!(
            parse_google_translation(&value).unwrap(),
            "这是一项随机试验。"
        );
    }
}
