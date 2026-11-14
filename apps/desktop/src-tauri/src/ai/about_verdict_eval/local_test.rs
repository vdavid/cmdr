//! The local class: the bundled llama-server and model, started with the same arguments
//! `ai::process::spawn_llama_server` gives it, on a scratch port, for the eval's length.

use std::process::{Child, Command, Stdio};
use std::time::Duration;

/// The release app's AI folder, where the bundled server and model are installed.
fn ai_dir() -> std::path::PathBuf {
    match std::env::var("CMDR_VERDICT_EVAL_AI_DIR") {
        Ok(dir) if !dir.trim().is_empty() => dir.into(),
        _ => dirs::home_dir()
            .expect("a home folder")
            .join("Library/Application Support/com.veszelovszki.cmdr/ai"),
    }
}

/// A llama-server the eval owns; dropping it stops the process.
pub(super) struct LocalServer {
    child: Child,
    pub(super) port: u16,
}

/// The context the app gives the server when `ai.localContextSize` is unset.
const CONTEXT: u32 = crate::agent::chat::budget::MIN_LOCAL_CONTEXT_TOKENS;

impl LocalServer {
    pub(super) async fn start() -> Self {
        let dir = ai_dir();
        let binary = dir.join("llama-server");
        let filename = super::super::get_default_model().filename;
        let model = dir.join(filename);
        assert!(
            binary.exists() && model.exists(),
            "no local model installed at {} (want llama-server and {filename})",
            dir.display(),
        );
        let port = 23_417;
        let child = Command::new(&binary)
            .env("DYLD_LIBRARY_PATH", &dir)
            .current_dir(&dir)
            .arg("-m")
            .arg(&model)
            .args(["--port", &port.to_string(), "--host", "127.0.0.1"])
            .args(["-c", &CONTEXT.to_string()])
            .args([
                "--temp", "0.6", "--top-p", "0.95", "-n", "4096", "--jinja", "-ngl", "99",
            ])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("start llama-server");
        let server = Self { child, port };
        let health = format!("http://127.0.0.1:{port}/health");
        let client = cmdr_http::client_builder().build().expect("build an HTTP client");
        for _ in 0..120 {
            if client
                .get(&health)
                .send()
                .await
                .is_ok_and(|response| response.status().is_success())
            {
                return server;
            }
            // allowed-test-sleep: polls a real llama-server's /health, an outside process with no event to wait on
            tokio::time::sleep(Duration::from_millis(500)).await;
        }
        panic!("llama-server didn't come up on {port} within a minute");
    }
}

impl Drop for LocalServer {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}
