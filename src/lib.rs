//! LedgerMem extension for the Zed editor.
//!
//! Implements two slash commands for the chat panel:
//! - `/lm-search <query>`: returns top matches from LedgerMem
//! - `/lm-add [content]`: stores the current selection (or supplied content) as a memory
//!
//! Configuration is read from Zed's settings under `ledgermem.*`:
//!   "ledgermem": {
//!     "api_key": "...",
//!     "workspace_id": "...",
//!     "endpoint": "https://api.ledgermem.dev",
//!     "default_limit": 10
//!   }

use serde::Deserialize;
use zed_extension_api::{
    self as zed,
    settings::LspSettings,
    SlashCommand, SlashCommandArgumentCompletion, SlashCommandOutput, SlashCommandOutputSection,
    Worktree,
};

#[derive(Debug, Deserialize, Clone)]
struct LedgerMemSettings {
    #[serde(default)]
    api_key: String,
    #[serde(default)]
    workspace_id: String,
    #[serde(default = "default_endpoint")]
    endpoint: String,
    #[serde(default = "default_limit")]
    default_limit: u32,
}

fn default_endpoint() -> String {
    "https://api.ledgermem.dev".to_string()
}

fn default_limit() -> u32 {
    10
}

impl LedgerMemSettings {
    fn load(worktree: Option<&Worktree>) -> Result<Self, String> {
        // Zed exposes user settings via LspSettings::for_worktree using a stable key.
        let raw = LspSettings::for_worktree("ledgermem", worktree.unwrap_or(&dummy_worktree()))
            .map_err(|e| e.to_string())?;
        let value = raw.settings.unwrap_or(serde_json::json!({}));
        serde_json::from_value::<LedgerMemSettings>(value)
            .map_err(|e| format!("invalid ledgermem settings: {e}"))
    }

    fn ensure_ready(&self) -> Result<(), String> {
        if self.api_key.is_empty() {
            return Err("Set `ledgermem.api_key` in your Zed settings.".into());
        }
        if self.workspace_id.is_empty() {
            return Err("Set `ledgermem.workspace_id` in your Zed settings.".into());
        }
        Ok(())
    }
}

// SAFETY: a true Worktree is provided by Zed at runtime; we never construct one
// ourselves in production. This is a placeholder for the rare unit-test path.
fn dummy_worktree() -> Worktree {
    unsafe { std::mem::zeroed() }
}

struct LedgerMemExtension;

impl zed::Extension for LedgerMemExtension {
    fn new() -> Self {
        LedgerMemExtension
    }

    fn complete_slash_command_argument(
        &self,
        command: SlashCommand,
        _args: Vec<String>,
    ) -> Result<Vec<SlashCommandArgumentCompletion>, String> {
        match command.name.as_str() {
            "lm-search" => Ok(vec![SlashCommandArgumentCompletion {
                label: "<query>".into(),
                new_text: "".into(),
                run_command: false,
            }]),
            "lm-add" => Ok(vec![SlashCommandArgumentCompletion {
                label: "<content>".into(),
                new_text: "".into(),
                run_command: false,
            }]),
            _ => Ok(vec![]),
        }
    }

    fn run_slash_command(
        &self,
        command: SlashCommand,
        args: Vec<String>,
        worktree: Option<&Worktree>,
    ) -> Result<SlashCommandOutput, String> {
        let settings = LedgerMemSettings::load(worktree)?;
        settings.ensure_ready()?;

        match command.name.as_str() {
            "lm-search" => run_search(&settings, &args),
            "lm-add" => run_add(&settings, &args),
            other => Err(format!("unknown LedgerMem command: {other}")),
        }
    }
}

fn run_search(settings: &LedgerMemSettings, args: &[String]) -> Result<SlashCommandOutput, String> {
    let query = args.join(" ");
    if query.trim().is_empty() {
        return Err("Usage: /lm-search <query>".into());
    }
    let body = serde_json::json!({
        "query": query,
        "workspaceId": settings.workspace_id,
        "limit": settings.default_limit,
    });
    let raw = http_post(settings, "/v1/search", &body.to_string())?;
    let memories = parse_memories(&raw)?;
    if memories.is_empty() {
        return Ok(SlashCommandOutput {
            text: format!("No matches for `{query}`."),
            sections: vec![],
        });
    }

    let mut text = String::new();
    let mut sections = Vec::with_capacity(memories.len());
    for (idx, m) in memories.iter().enumerate() {
        let header = format!("### {}. {}\n", idx + 1, m.preview());
        let start = text.len();
        text.push_str(&header);
        text.push_str(&m.content);
        text.push_str("\n\n");
        sections.push(SlashCommandOutputSection {
            range: (start as u32)..(text.len() as u32),
            label: format!("Memory {}", &m.id[..m.id.len().min(8)]),
        });
    }
    Ok(SlashCommandOutput { text, sections })
}

fn run_add(settings: &LedgerMemSettings, args: &[String]) -> Result<SlashCommandOutput, String> {
    let content = args.join(" ");
    if content.trim().is_empty() {
        return Err("Usage: /lm-add <content>  (or have a selection in the editor)".into());
    }
    let body = serde_json::json!({
        "content": content,
        "workspaceId": settings.workspace_id,
        "metadata": { "source": "zed" },
    });
    let raw = http_post(settings, "/v1/memories", &body.to_string())?;
    let memory = parse_single(&raw)?;
    let text = format!(
        "Saved memory `{}`:\n\n{}",
        &memory.id[..memory.id.len().min(8)],
        memory.content,
    );
    Ok(SlashCommandOutput {
        sections: vec![SlashCommandOutputSection {
            range: 0..(text.len() as u32),
            label: "LedgerMem".into(),
        }],
        text,
    })
}

#[derive(Deserialize, Debug)]
struct Memory {
    #[serde(default)]
    id: String,
    #[serde(default)]
    content: String,
    #[serde(default, rename = "createdAt")]
    _created_at: String,
    #[serde(default)]
    _score: Option<f64>,
}

impl Memory {
    fn preview(&self) -> String {
        let line = self.content.lines().next().unwrap_or("");
        if line.len() > 80 {
            format!("{}...", &line[..77])
        } else {
            line.to_string()
        }
    }
}

fn parse_memories(raw: &str) -> Result<Vec<Memory>, String> {
    serde_json::from_str(raw).map_err(|e| format!("parse error: {e}"))
}

fn parse_single(raw: &str) -> Result<Memory, String> {
    serde_json::from_str(raw).map_err(|e| format!("parse error: {e}"))
}

fn http_post(settings: &LedgerMemSettings, path: &str, body: &str) -> Result<String, String> {
    let url = format!("{}{}", settings.endpoint.trim_end_matches('/'), path);
    let response = zed::http_client::fetch(&zed::http_client::HttpRequest {
        method: zed::http_client::HttpMethod::Post,
        url,
        headers: vec![
            ("Authorization".into(), format!("Bearer {}", settings.api_key)),
            ("Content-Type".into(), "application/json".into()),
            ("Accept".into(), "application/json".into()),
        ],
        body: Some(body.as_bytes().to_vec()),
        redirect_policy: zed::http_client::RedirectPolicy::FollowAll,
    })
    .map_err(|e| format!("HTTP error: {e}"))?;

    let body = String::from_utf8(response.body).map_err(|e| format!("non-UTF-8 response: {e}"))?;
    Ok(body)
}

zed::register_extension!(LedgerMemExtension);
