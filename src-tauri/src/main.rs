// Prevents an extra console window on Windows in release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    // A bot's agent starts AgentDeck itself as the `agentdeck` MCP server: answer on stdio and
    // relay to the running app, before anything of the app (Tauri, windows) starts.
    if std::env::args_os()
        .nth(1)
        .is_some_and(|a| a == agentdeck_lib::BRIDGE_FLAG)
    {
        std::process::exit(agentdeck_lib::mcp_bridge());
    }
    agentdeck_lib::run()
}
