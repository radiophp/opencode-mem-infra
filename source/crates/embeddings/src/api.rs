use std::time::Duration;

use opencode_mem_core::EMBEDDING_DIMENSION;
use reqwest::blocking::Client;
use reqwest::header::{AUTHORIZATION, CONTENT_TYPE, HeaderMap, HeaderValue};
use serde_json::Value;

use crate::error::EmbeddingError;
use crate::EmbeddingProvider;

const DEFAULT_API_URL: &str = "https://api.cohere.com/v1/embed";
const MAX_RETRIES: u32 = 3;
const RETRY_BASE_MS: u64 = 2000;

pub struct ApiEmbeddingProvider {
    client: Client,
    api_url: String,
    model: String,
}

impl ApiEmbeddingProvider {
    pub fn new(api_url: &str, api_key: &str) -> Self {
        Self::with_model(api_url, api_key, "embed-multilingual-v3.0")
    }

    pub fn with_model(api_url: &str, api_key: &str, model: &str) -> Self {
        let mut headers = HeaderMap::new();
        let bearer = format!("Bearer {}", api_key);
        headers.insert(
            AUTHORIZATION,
            HeaderValue::from_str(&bearer).expect("valid header value"),
        );
        headers.insert(
            CONTENT_TYPE,
            HeaderValue::from_static("application/json"),
        );

        let client = Client::builder()
            .default_headers(headers)
            .timeout(Duration::from_secs(120))
            .build()
            .expect("valid reqwest client");

        Self {
            client,
            api_url: api_url.to_owned(),
            model: model.to_owned(),
        }
    }

    fn post_embeddings(&self, texts: &[&str]) -> Result<Value, EmbeddingError> {
        let body = serde_json::json!({
            "model": self.model,
            "texts": texts,
            "input_type": "search_document",
        });

        let mut last_error = None;
        for attempt in 0..MAX_RETRIES {
            let response = self
                .client
                .post(&self.api_url)
                .json(&body)
                .send()
                .map_err(|e| EmbeddingError::Generation(format!("HTTP request failed: {e}")))?;

            let status = response.status();
            if status.is_success() {
                return response
                    .json::<Value>()
                    .map_err(|e| EmbeddingError::Generation(format!("JSON parse failed: {e}")));
            }

            if status.as_u16() == 429 || status.as_u16() >= 500 {
                last_error = Some(EmbeddingError::Generation(format!(
                    "API error ({}), attempt {}/{MAX_RETRIES}",
                    status.as_u16(),
                    attempt + 1
                )));
                std::thread::sleep(Duration::from_millis(
                    RETRY_BASE_MS * u64::from(attempt + 1),
                ));
                continue;
            }

            let text = response
                .text()
                .unwrap_or_else(|_| "unknown".to_owned());
            return Err(EmbeddingError::Generation(format!(
                "API error ({}): {text}",
                status.as_u16()
            )));
        }

        Err(last_error.unwrap_or_else(|| {
            EmbeddingError::Generation("Max retries exceeded".to_owned())
        }))
    }

    fn parse_embeddings(response: Value) -> Result<Vec<Vec<f32>>, EmbeddingError> {
        let embeddings = response
            .get("embeddings")
            .and_then(|e| e.as_array())
            .ok_or_else(|| EmbeddingError::Generation("missing embeddings array".to_owned()))?;

        embeddings
            .iter()
            .map(|entry| {
                let arr = entry.as_array().ok_or_else(|| {
                    EmbeddingError::Generation("expected array in embeddings".to_owned())
                })?;

                if arr.len() != EMBEDDING_DIMENSION {
                    return Err(EmbeddingError::Generation(format!(
                        "expected {} dimensions, got {}",
                        EMBEDDING_DIMENSION,
                        arr.len()
                    )));
                }

                arr.iter()
                    .map(|v| {
                        v.as_f64()
                            .map(|f| f as f32)
                            .ok_or_else(|| {
                                EmbeddingError::Generation("non-float in embedding".to_owned())
                            })
                    })
                    .collect()
            })
            .collect()
    }
}

impl EmbeddingProvider for ApiEmbeddingProvider {
    fn embed(&self, text: &str) -> Result<Vec<f32>, EmbeddingError> {
        let response = self.post_embeddings(&[text])?;
        let mut results = Self::parse_embeddings(response)?;
        results.pop().ok_or_else(|| EmbeddingError::EmptyResult)
    }

    fn embed_batch(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>, EmbeddingError> {
        let response = self.post_embeddings(texts)?;
        Self::parse_embeddings(response)
    }

    fn dimension(&self) -> usize {
        EMBEDDING_DIMENSION
    }
}

impl std::fmt::Debug for ApiEmbeddingProvider {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ApiEmbeddingProvider")
            .field("api_url", &self.api_url)
            .field("model", &self.model)
            .finish()
    }
}
