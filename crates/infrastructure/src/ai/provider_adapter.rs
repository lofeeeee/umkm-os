//! Adapter OpenAI-compatible (P5-4, `PRD.md §8.1`). Dipakai untuk OpenAI, OpenRouter, Groq, Together,
//! vLLM/Ollama (lewat provider platform) dan endpoint custom. Semua permintaan lewat `Egress` (SSRF guard).
//!
//! Capability registry ditegakkan DI SINI: tool tidak dikirim ke provider tanpa `tool_calling`
//! (AC-AIC-002), embedding ditolak tanpa `embeddings`, usage `None` bila tidak dilaporkan.
//! Streaming tercatat sebagai capability tetapi belum dipakai (balasan dikirim utuh oleh worker).

use serde_json::{Value, json};
use umkm_application::ai::llm::{
    ChatMessage, ChatRequest, ChatResponse, LlmError, LlmPort, Role, ToolCall, Usage,
};
use umkm_domain::ai::Capabilities;

use crate::http_client::{Egress, SsrfError};

/// Batas isi dari provider (untrusted): teks jawaban & jumlah tool call per giliran.
pub const MAX_CONTENT_CHARS: usize = 8_000;
pub const MAX_TOOL_CALLS: usize = 5;
pub const MAX_TOOL_ARGS_BYTES: usize = 4_000;
pub const MAX_EMBED_INPUTS: usize = 64;

/// Kunci API dalam memori; tidak pernah tampil di Debug.
pub struct ApiKey(String);

impl ApiKey {
    pub fn new(k: String) -> Self {
        Self(k)
    }
}

impl std::fmt::Debug for ApiKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("ApiKey(••••)")
    }
}

impl Drop for ApiKey {
    fn drop(&mut self) {
        // Best-effort: timpa isi sebelum dibebaskan (tanpa unsafe).
        let mut b = std::mem::take(&mut self.0).into_bytes();
        b.iter_mut().for_each(|x| *x = 0);
    }
}

#[derive(Debug)]
pub struct OpenAiCompatible {
    egress: Egress,
    base_url: String,
    key: Option<ApiKey>,
    caps: Capabilities,
}

impl OpenAiCompatible {
    pub fn new(egress: Egress, base_url: &str, key: Option<ApiKey>, caps: Capabilities) -> Self {
        Self {
            egress,
            base_url: base_url.trim_end_matches('/').to_owned(),
            key,
            caps,
        }
    }

    async fn post(&self, path: &str, body: Value) -> Result<Value, LlmError> {
        let bytes = serde_json::to_vec(&body).map_err(|_| LlmError::BadRequest)?;
        let r = self
            .egress
            .post_json(
                &format!("{}{path}", self.base_url),
                self.key.as_ref().map(|k| k.0.as_str()),
                bytes,
            )
            .await
            .map_err(egress_err)?;
        match r.status {
            200..=299 => serde_json::from_slice(&r.body).map_err(|_| LlmError::InvalidResponse),
            401 | 403 => Err(LlmError::Unauthorized),
            429 => Err(LlmError::RateLimited),
            400..=499 => Err(LlmError::BadRequest),
            _ => Err(LlmError::Upstream),
        }
    }
}

fn egress_err(e: SsrfError) -> LlmError {
    match e {
        SsrfError::Timeout => LlmError::Timeout,
        SsrfError::TooLarge => LlmError::InvalidResponse,
        other => LlmError::Network(other.code()),
    }
}

fn message_json(m: &ChatMessage) -> Value {
    let mut v = json!({ "role": m.role.as_str(), "content": m.content });
    if let Some(id) = &m.tool_call_id {
        v["tool_call_id"] = json!(id);
    }
    if !m.tool_calls.is_empty() {
        v["tool_calls"] = m
            .tool_calls
            .iter()
            .map(|c| json!({ "id": c.id, "type": "function", "function": { "name": c.name, "arguments": c.arguments } }))
            .collect();
    }
    v
}

/// Body permintaan chat sesuai capability. Dipisah agar matriks capability dapat diuji tanpa jaringan.
pub fn chat_body(req: &ChatRequest, caps: Capabilities) -> Result<Value, LlmError> {
    if !caps.chat {
        return Err(LlmError::Unsupported);
    }
    // Tanpa tool_calling: riwayat tool juga tidak boleh dikirim dalam bentuk khusus tool.
    let messages: Vec<Value> = req
        .messages
        .iter()
        .filter(|m| caps.tool_calling || m.role != Role::Tool)
        .map(|m| {
            if caps.tool_calling {
                message_json(m)
            } else {
                json!({ "role": m.role.as_str(), "content": m.content })
            }
        })
        .collect();
    let mut body = json!({
        "model": req.model,
        "messages": messages,
        "temperature": req.temperature,
        "max_tokens": req.max_output_tokens,
        "stream": false,
    });
    if caps.tool_calling && !req.tools.is_empty() {
        body["tools"] = req
            .tools
            .iter()
            .map(|t| json!({ "type": "function", "function": { "name": t.name, "description": t.description, "parameters": t.parameters } }))
            .collect();
        body["tool_choice"] = json!("auto");
    }
    Ok(body)
}

/// Parsing respons secara defensif: tipe salah → InvalidResponse, isi dipotong ke batas.
pub fn parse_chat(v: &Value, caps: Capabilities) -> Result<ChatResponse, LlmError> {
    let msg = v
        .pointer("/choices/0/message")
        .ok_or(LlmError::InvalidResponse)?;
    let content = match msg.get("content") {
        None | Some(Value::Null) => None,
        Some(Value::String(s)) => Some(s.chars().take(MAX_CONTENT_CHARS).collect::<String>()),
        Some(_) => return Err(LlmError::InvalidResponse),
    };
    let mut tool_calls = Vec::new();
    if caps.tool_calling
        && let Some(arr) = msg.get("tool_calls").and_then(Value::as_array)
    {
        for c in arr.iter().take(MAX_TOOL_CALLS) {
            let name = c
                .pointer("/function/name")
                .and_then(Value::as_str)
                .ok_or(LlmError::InvalidResponse)?;
            let args = c
                .pointer("/function/arguments")
                .and_then(Value::as_str)
                .unwrap_or("{}");
            let id = c.get("id").and_then(Value::as_str).unwrap_or("call");
            if name.len() > 64 || args.len() > MAX_TOOL_ARGS_BYTES || id.len() > 64 {
                return Err(LlmError::InvalidResponse);
            }
            tool_calls.push(ToolCall {
                id: id.to_owned(),
                name: name.to_owned(),
                arguments: args.to_owned(),
            });
        }
    }
    let usage = v.get("usage").and_then(|u| {
        let p = u.get("prompt_tokens")?.as_u64()?;
        let c = u.get("completion_tokens")?.as_u64()?;
        Some(Usage {
            prompt_tokens: p.min(u32::MAX as u64) as u32,
            completion_tokens: c.min(u32::MAX as u64) as u32,
        })
    });
    let model = v
        .get("model")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .chars()
        .take(120)
        .collect();
    Ok(ChatResponse {
        content,
        tool_calls,
        usage,
        model,
    })
}

impl LlmPort for OpenAiCompatible {
    fn capabilities(&self) -> Capabilities {
        self.caps
    }

    async fn chat(&self, req: ChatRequest) -> Result<ChatResponse, LlmError> {
        let body = chat_body(&req, self.caps)?;
        let v = self.post("/chat/completions", body).await?;
        parse_chat(&v, self.caps)
    }

    async fn embed(&self, model: &str, inputs: Vec<String>) -> Result<Vec<Vec<f32>>, LlmError> {
        if !self.caps.embeddings {
            return Err(LlmError::Unsupported);
        }
        if inputs.is_empty() || inputs.len() > MAX_EMBED_INPUTS {
            return Err(LlmError::BadRequest);
        }
        let n = inputs.len();
        let v = self
            .post("/embeddings", json!({ "model": model, "input": inputs }))
            .await?;
        let data = v
            .get("data")
            .and_then(Value::as_array)
            .ok_or(LlmError::InvalidResponse)?;
        if data.len() != n {
            return Err(LlmError::InvalidResponse);
        }
        let mut out = vec![Vec::new(); n];
        for (i, d) in data.iter().enumerate() {
            let idx = d
                .get("index")
                .and_then(Value::as_u64)
                .map_or(i, |x| x as usize);
            let emb = d
                .get("embedding")
                .and_then(Value::as_array)
                .ok_or(LlmError::InvalidResponse)?;
            let vec: Option<Vec<f32>> = emb.iter().map(|x| x.as_f64().map(|f| f as f32)).collect();
            let slot = out.get_mut(idx).ok_or(LlmError::InvalidResponse)?;
            *slot = vec
                .filter(|v| !v.is_empty() && v.len() <= 4096)
                .ok_or(LlmError::InvalidResponse)?;
        }
        if out.iter().any(Vec::is_empty) || out.windows(2).any(|w| w[0].len() != w[1].len()) {
            return Err(LlmError::InvalidResponse);
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use umkm_application::ai::llm::ToolSpec;

    fn req() -> ChatRequest {
        let mut asst = ChatMessage::new(Role::Assistant, "");
        asst.tool_calls.push(ToolCall {
            id: "c1".into(),
            name: "check_stock".into(),
            arguments: "{}".into(),
        });
        let mut tool = ChatMessage::new(Role::Tool, "{\"stock\":3}");
        tool.tool_call_id = Some("c1".into());
        ChatRequest {
            model: "m".into(),
            messages: vec![
                ChatMessage::new(Role::System, "s"),
                ChatMessage::new(Role::User, "u"),
                asst,
                tool,
            ],
            temperature: 0.3,
            max_output_tokens: 256,
            tools: vec![ToolSpec {
                name: "check_stock",
                description: "d",
                parameters: json!({"type": "object"}),
            }],
        }
    }

    /// Matriks capability (AC-AIC-002): tool hanya dikirim ke provider dengan tool_calling.
    #[test]
    fn capability_matrix() {
        let full = Capabilities::defaults_for("openai");
        let chat_only = Capabilities::defaults_for("custom");
        let b = chat_body(&req(), full).unwrap();
        assert_eq!(b["tools"][0]["function"]["name"], "check_stock");
        assert_eq!(b["messages"].as_array().unwrap().len(), 4);
        assert_eq!(b["messages"][3]["tool_call_id"], "c1");
        let b = chat_body(&req(), chat_only).unwrap();
        assert!(
            b.get("tools").is_none() && b.get("tool_choice").is_none(),
            "{b}"
        );
        let msgs = b["messages"].as_array().unwrap();
        assert_eq!(msgs.len(), 3, "pesan role tool tidak dikirim");
        assert!(
            msgs.iter()
                .all(|m| m.get("tool_calls").is_none() && m.get("tool_call_id").is_none())
        );
        assert_eq!(
            chat_body(&req(), Capabilities::default()).unwrap_err(),
            LlmError::Unsupported
        );
        assert_eq!(b["stream"], false);
    }

    #[test]
    fn response_parsing_is_defensive() {
        let full = Capabilities::defaults_for("openai");
        let ok = json!({"model": "m1", "choices": [{"message": {"content": "Halo", "tool_calls": [
            {"id": "a", "type": "function", "function": {"name": "search_products", "arguments": "{\"q\":\"kopi\"}"}}]}}],
            "usage": {"prompt_tokens": 12, "completion_tokens": 3}});
        let r = parse_chat(&ok, full).unwrap();
        assert_eq!(r.content.as_deref(), Some("Halo"));
        assert_eq!(r.tool_calls[0].name, "search_products");
        assert_eq!(
            r.usage,
            Some(Usage {
                prompt_tokens: 12,
                completion_tokens: 3
            })
        );
        // Tanpa capability: tool call dari provider diabaikan.
        assert!(
            parse_chat(&ok, Capabilities::defaults_for("custom"))
                .unwrap()
                .tool_calls
                .is_empty()
        );
        // Tanpa usage → None (metering memakai estimasi).
        let no_usage = json!({"choices": [{"message": {"content": "x"}}]});
        assert_eq!(parse_chat(&no_usage, full).unwrap().usage, None);
        // Isi panjang dipotong; tipe salah ditolak.
        let long = json!({"choices": [{"message": {"content": "a".repeat(20_000)}}]});
        assert_eq!(
            parse_chat(&long, full).unwrap().content.unwrap().len(),
            MAX_CONTENT_CHARS
        );
        for bad in [
            json!({}),
            json!({"choices": []}),
            json!({"choices": [{"message": {"content": 5}}]}),
            json!({"choices": [{"message": {"tool_calls": [{"function": {"name": "x".repeat(65)}}]}}]}),
        ] {
            assert_eq!(
                parse_chat(&bad, full).unwrap_err(),
                LlmError::InvalidResponse,
                "{bad}"
            );
        }
        assert_eq!(
            format!("{:?}", ApiKey::new("sk-rahasia".into())),
            "ApiKey(••••)"
        );
    }
}
