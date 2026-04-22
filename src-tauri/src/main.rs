// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.iter().any(|a| a == "--mcp") {
        // Run headless Model Context Protocol Server
        let rt = tokio::runtime::Runtime::new().expect("Failed to build tokio runtime");
        rt.block_on(async {
            smart_search_lib::mcp::run_mcp_server().await;
        });
    } else {
        smart_search_lib::run()
    }
}
