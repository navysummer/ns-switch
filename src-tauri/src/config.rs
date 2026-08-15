use crate::adapters;
use crate::db::AppState;
use crate::models::Provider;
use crate::AppResult;

/// Writes the given provider into the tool's real config file, and ensures
/// the MCP servers (for that tool) are also present.
pub fn write_active(state: &AppState, provider: &Provider) -> AppResult<()> {
    let targets: Vec<String> = if provider.is_universal {
        vec![
            "claude_code".to_string(),
            "codex".to_string(),
            "gemini".to_string(),
        ]
    } else {
        vec![provider.tool_id.clone()]
    };
    for tid in targets {
        let mut p = provider.clone();
        p.tool_id = tid.clone();
        adapters::switch_provider_to(&tid, &p)?;
        sync_mcp_for_tool(state, &tid);
    }
    Ok(())
}

/// Merge MCP servers that apply to `tool_id` into the tool's config file.
pub fn sync_mcp_for_tool(state: &AppState, tool_id: &str) {
    let servers = crate::commands::mcp::mcp_servers_for_tool(state, tool_id);
    if let Err(e) = adapters::write_mcp_to_tool(tool_id, &servers) {
        log::warn!("sync mcp for {tool_id} failed: {e}");
    }
}

/// Return (base_url, api_key, model) of the currently active provider (if any).
pub fn read_active(state: &AppState, tool_id: &str) -> (String, String, String) {
    if let Ok(Some(p)) = crate::commands::providers::get_active_provider_impl(state, tool_id) {
        return (p.base_url, p.api_key, p.model);
    }
    adapters::read_existing_config(tool_id)
}