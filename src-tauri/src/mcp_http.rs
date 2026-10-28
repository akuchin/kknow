//! The MCP server of the app: Streamable HTTP on `127.0.0.1`.
//!
//! An agent connects with one line, and nothing else needs to be installed:
//!
//! ```text
//! claude mcp add --transport http kknow http://127.0.0.1:7377/mcp \
//!   --header "Authorization: Bearer <token>"
//! ```
//!
//! The protocol is `kknow_app::mcp`. This module only carries it over HTTP
//! and gives it the vaults of the window: the same slots, the same indexes.
//! Thus the server costs no second index in memory, and an edit of the agent
//! reaches the open note at once (`vault-changed`).
//!
//! The server works while the app runs. The lamp in the status bar shows
//! that: green is "listening".
//!
//! Of the transport, the server uses the simple part. A POST carries one
//! message (or a batch) and gets one JSON reply. There is no SSE stream: the
//! tools answer at once and send no notifications, so GET gets 405, as the
//! specification allows. There are no sessions either.
//!
//! Protection. The port is open only on `127.0.0.1`. Every request needs
//! `Authorization: Bearer <token>`: other programs and other users of the
//! machine also reach `127.0.0.1`. The `Host` header must be ours, and an
//! `Origin`, if there is one, too: otherwise a web page could reach the port
//! through DNS rebinding. A browser also sends a CORS preflight for a JSON
//! POST, and the server does not answer it.
//!
//! Which vaults: `/mcp` serves all vaults of the list, `/mcp?vault=<id>` one
//! vault, `/mcp?label=work&label=ai` the vaults with one of the labels. The
//! scope is read on each request, so a new label works at once.

use std::io::Read;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;

use kknow_app::Workspace;
use kknow_app::mcp::{self, Host, McpConfig, PARSE_ERROR, VaultRef};
use kknow_app::vaults::{VaultEntry, clean_labels};
use kknow_core::NoteId;
use percent_encoding::percent_decode_str;
use serde::Serialize;
use serde_json::Value;
use tauri::{AppHandle, Emitter, Manager, Runtime, State};

use crate::live::{EVENT, VaultChanged};
use crate::state::{AppState, Result, VaultSlot, lock};

/// The largest request body. A JSON-RPC call of these tools is a few
/// kilobytes; a bigger body is a mistake or an attack.
const MAX_BODY: u64 = 4 * 1024 * 1024;

/// What the window shows about the server.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct McpStatus {
    pub enabled: bool,
    pub port: u16,
    /// The server listens now.
    pub running: bool,
    /// Why the server does not listen, for example a port in use.
    pub error: Option<String>,
    /// For the setup snippets: the agent sends it in `Authorization`.
    pub token: String,
}

/// The server and its settings. One per app.
pub struct McpServer {
    /// `None` in tests: nothing is written into the settings of the person.
    path: Option<PathBuf>,
    inner: Mutex<Inner>,
}

struct Inner {
    config: McpConfig,
    running: Option<Running>,
    error: Option<String>,
}

struct Running {
    server: Arc<tiny_http::Server>,
    /// The real port: with port 0 in tests, the system picks one.
    port: u16,
    thread: Option<JoinHandle<()>>,
}

impl McpServer {
    pub fn new(path: Option<PathBuf>, config: McpConfig) -> Self {
        McpServer {
            path,
            inner: Mutex::new(Inner {
                config,
                running: None,
                error: None,
            }),
        }
    }

    /// Reads `mcp.json` from the config directory, or makes it.
    pub fn load(config_dir: Option<PathBuf>) -> Self {
        let path = config_dir.map(McpConfig::path);
        let config = match &path {
            Some(path) => McpConfig::load_or_create(path).unwrap_or_else(|error| {
                eprintln!("не удалось прочитать настройки MCP: {error}");
                McpConfig::default()
            }),
            None => McpConfig::default(),
        };
        McpServer::new(path, config)
    }

    pub fn status(&self) -> McpStatus {
        let inner = lock(&self.inner);
        McpStatus {
            enabled: inner.config.enabled,
            port: inner
                .running
                .as_ref()
                .map_or(inner.config.port, |running| running.port),
            running: inner.running.is_some(),
            error: inner.error.clone(),
            token: inner.config.token.clone(),
        }
    }

    /// Starts the server if the settings say so. A port in use is not an
    /// error of the app: the status tells it, and the person picks another.
    pub fn start<R: Runtime>(&self, app: AppHandle<R>) {
        let mut inner = lock(&self.inner);
        stop(&mut inner);
        if !inner.config.enabled {
            return;
        }

        match tiny_http::Server::http(("127.0.0.1", inner.config.port)) {
            Ok(server) => {
                let server = Arc::new(server);
                let port = server
                    .server_addr()
                    .to_ip()
                    .map_or(inner.config.port, |address| address.port());
                let token = inner.config.token.clone();
                let listener = server.clone();
                let thread = std::thread::spawn(move || {
                    for request in listener.incoming_requests() {
                        answer(&app, port, &token, request);
                    }
                });
                inner.running = Some(Running {
                    server,
                    port,
                    thread: Some(thread),
                });
            }
            Err(error) => inner.error = Some(error.to_string()),
        }
    }

    /// New settings from the window: they go to disk, and the server starts
    /// again with them. The token stays.
    pub fn configure<R: Runtime>(&self, app: AppHandle<R>, enabled: bool, port: u16) -> Result<()> {
        {
            let mut inner = lock(&self.inner);
            inner.config.enabled = enabled;
            inner.config.port = if port == 0 { mcp::DEFAULT_PORT } else { port };
            if let Some(path) = &self.path {
                inner.config.save(path)?;
            }
        }
        self.start(app);
        Ok(())
    }
}

fn stop(inner: &mut Inner) {
    inner.error = None;
    if let Some(mut running) = inner.running.take() {
        // `unblock` ends `incoming_requests`, so the thread exits.
        running.server.unblock();
        if let Some(thread) = running.thread.take() {
            let _ = thread.join();
        }
    }
}

/// One HTTP request, without the socket: the tests build it by hand.
pub struct Request {
    pub method: String,
    pub url: String,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}

impl Request {
    fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(key, _)| key.eq_ignore_ascii_case(name))
            .map(|(_, value)| value.as_str())
    }
}

/// An HTTP reply: the status and an optional JSON body.
#[derive(Debug, PartialEq)]
pub struct Reply {
    pub status: u16,
    pub body: Option<Value>,
}

impl Reply {
    fn empty(status: u16) -> Self {
        Reply { status, body: None }
    }

    fn json(status: u16, body: Value) -> Self {
        Reply {
            status,
            body: Some(body),
        }
    }

    /// A refusal with the reason in words: a person reads it in the agent log.
    fn refuse(status: u16, text: &str) -> Self {
        Reply::json(status, mcp::failure(Value::Null, mcp::INVALID_REQUEST, text))
    }
}

fn answer<R: Runtime>(app: &AppHandle<R>, port: u16, token: &str, mut request: tiny_http::Request) {
    let mut body = Vec::new();
    let read = request
        .as_reader()
        .take(MAX_BODY + 1)
        .read_to_end(&mut body);
    let reply = if read.is_err() || body.len() as u64 > MAX_BODY {
        Reply::refuse(413, "тело запроса слишком большое")
    } else {
        let parsed = Request {
            method: request.method().as_str().to_string(),
            url: request.url().to_string(),
            headers: request
                .headers()
                .iter()
                .map(|header| (header.field.to_string(), header.value.to_string()))
                .collect(),
            body,
        };
        route(app, port, token, &parsed)
    };

    let response = match reply.body {
        Some(body) => tiny_http::Response::from_string(body.to_string())
            .with_status_code(reply.status)
            .with_header(header("Content-Type", "application/json")),
        None => tiny_http::Response::from_string(String::new()).with_status_code(reply.status),
    };
    let response = match reply.status {
        401 => response.with_header(header("WWW-Authenticate", "Bearer")),
        405 => response.with_header(header("Allow", "POST")),
        _ => response,
    };
    let _ = request.respond(response);
}

fn header(name: &str, value: &str) -> tiny_http::Header {
    tiny_http::Header::from_bytes(name.as_bytes(), value.as_bytes()).expect("заголовок из ASCII")
}

/// The reply to one request. The checks go from the cheapest to the most
/// expensive: a wrong path or a foreign page never reaches the vaults.
pub fn route<R: Runtime>(app: &AppHandle<R>, port: u16, token: &str, request: &Request) -> Reply {
    let (path, query) = request.url.split_once('?').unwrap_or((&request.url, ""));
    if path != "/mcp" {
        return Reply::refuse(404, "адрес сервера: /mcp");
    }
    if !ours(request.header("Host"), port, false) || !ours(request.header("Origin"), port, true) {
        return Reply::refuse(403, "запрос не с этого компьютера");
    }
    if request.method != "POST" {
        return Reply::empty(405);
    }
    let given = request
        .header("Authorization")
        .and_then(|value| value.strip_prefix("Bearer "))
        .unwrap_or("");
    if !mcp::same_token(given.trim(), token) {
        return Reply::refuse(401, "нужен заголовок Authorization: Bearer <токен из kknow>");
    }

    let message: Value = match serde_json::from_slice(&request.body) {
        Ok(message) => message,
        Err(error) => {
            return Reply::json(400, mcp::failure(Value::Null, PARSE_ERROR, &error.to_string()));
        }
    };
    let mut host = match WindowHost::new(app, query) {
        Ok(host) => host,
        Err(text) => return Reply::refuse(404, &text),
    };

    // A batch came in the 2025-03-26 version of the protocol; one message is
    // the usual case.
    let replies: Vec<Value> = match &message {
        Value::Array(messages) => messages
            .iter()
            .filter_map(|message| mcp::handle(&mut host, message))
            .collect(),
        single => mcp::handle(&mut host, single).into_iter().collect(),
    };
    match (message.is_array(), replies.len()) {
        // Only notifications and replies of the client: nothing to answer.
        (_, 0) => Reply::empty(202),
        (false, _) => Reply::json(200, replies.into_iter().next().expect("один ответ")),
        (true, _) => Reply::json(200, Value::Array(replies)),
    }
}

/// `Host` or `Origin` names this server. A missing `Origin` is fine: programs
/// do not send it, browsers do.
fn ours(value: Option<&str>, port: u16, origin: bool) -> bool {
    let Some(value) = value else {
        return origin;
    };
    let value = if origin {
        match value.strip_prefix("http://") {
            Some(rest) => rest,
            None => return false,
        }
    } else {
        value
    };
    value == format!("127.0.0.1:{port}") || value == format!("localhost:{port}")
}

/// The vaults of the window for one request.
struct WindowHost<'a, R: Runtime> {
    app: &'a AppHandle<R>,
    refs: Vec<VaultRef>,
    slots: Vec<Arc<VaultSlot>>,
}

impl<'a, R: Runtime> WindowHost<'a, R> {
    /// The vaults of the query: `vault=<id or name>`, or `label=…` (repeated),
    /// or all. An unavailable folder is skipped, as in the search.
    fn new(app: &'a AppHandle<R>, query: &str) -> std::result::Result<Self, String> {
        let state = app.state::<AppState>();
        let entries = scope(state.entries(), query)?;

        let mut host = WindowHost {
            app,
            refs: Vec::new(),
            slots: Vec::new(),
        };
        for entry in entries.into_iter().filter(VaultEntry::available) {
            // The first request to a closed vault opens it, as a search does.
            let slot = state
                .slot(Some(&entry.id))
                .map_err(|error| format!("{}: {error}", entry.name))?;
            host.refs.push(VaultRef {
                id: entry.id.clone(),
                name: entry.name.clone(),
                labels: entry.labels.clone(),
            });
            host.slots.push(slot);
        }
        Ok(host)
    }
}

/// The entries that the query asks for.
fn scope(entries: Vec<VaultEntry>, query: &str) -> std::result::Result<Vec<VaultEntry>, String> {
    let mut vault = None;
    let mut labels = Vec::new();
    for pair in query.split('&').filter(|pair| !pair.is_empty()) {
        let (key, value) = pair.split_once('=').unwrap_or((pair, ""));
        let value = percent_decode_str(&value.replace('+', " "))
            .decode_utf8_lossy()
            .into_owned();
        match key {
            "vault" => vault = Some(value),
            "label" => labels.push(value),
            _ => {}
        }
    }

    if let Some(wanted) = vault {
        let lower = wanted.to_lowercase();
        return entries
            .into_iter()
            .find(|entry| entry.id == wanted || entry.name.to_lowercase() == lower)
            .map(|entry| vec![entry])
            .ok_or_else(|| format!("хранилище не найдено: {wanted}"));
    }
    if labels.is_empty() {
        return Ok(entries);
    }
    let wanted = clean_labels(labels.iter().map(String::as_str));
    Ok(entries
        .into_iter()
        .filter(|entry| entry.labels.iter().any(|label| wanted.contains(label)))
        .collect())
}

impl<R: Runtime> Host for WindowHost<'_, R> {
    fn vaults(&self) -> &[VaultRef] {
        &self.refs
    }

    fn read<T>(&self, index: usize, action: impl FnOnce(&Workspace) -> T) -> std::result::Result<T, String> {
        self.slots[index]
            .read(|workspace| Ok(action(workspace)))
            .map_err(|error| error.to_string())
    }

    fn write<T>(
        &mut self,
        index: usize,
        action: impl FnOnce(&mut Workspace) -> T,
    ) -> std::result::Result<T, String> {
        self.slots[index]
            .write(|workspace| Ok(action(workspace)))
            .map_err(|error| error.to_string())
    }

    /// The write of the agent goes through the same workspace as the window,
    /// so the watcher takes it for our own and stays quiet. The window must
    /// hear about it anyway: the note can be open there.
    fn changed(&mut self, index: usize, note: &NoteId) {
        let _ = self.app.emit(
            EVENT,
            VaultChanged {
                vault: self.refs[index].id.clone(),
                updated: vec![note.as_str().to_string()],
                removed: Vec::new(),
            },
        );
    }
}

/// The server state for the window.
#[tauri::command]
pub fn mcp_status(server: State<'_, McpServer>) -> McpStatus {
    server.status()
}

/// Turns the server on or off and sets the port. A port in use comes back
/// in the status, not as an error: the setting is saved anyway.
#[tauri::command(async)]
pub fn set_mcp(
    app: AppHandle,
    server: State<'_, McpServer>,
    enabled: bool,
    port: u16,
) -> Result<McpStatus> {
    server.configure(app.clone(), enabled, port)?;
    Ok(server.status())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::io::Write;
    use std::net::TcpStream;

    use serde_json::json;
    use tauri::Listener;
    use tauri::test::{MockRuntime, mock_builder, mock_context, noop_assets};

    const TOKEN: &str = "0123456789abcdef0123456789abcdef";

    /// An app with two vaults: «Работа» (label `work`) and «Дом».
    fn app() -> (Vec<tempfile::TempDir>, tauri::App<MockRuntime>) {
        let work = tempfile::tempdir().unwrap();
        fs::write(
            work.path().join("План.md"),
            "- [ ] отчёт<!-- @agent: проверить цифры -->\n",
        )
        .unwrap();
        let home = tempfile::tempdir().unwrap();
        fs::write(home.path().join("Дом.md"), "- [ ] купить\n").unwrap();

        let state = crate::state::testing(&[work.path(), home.path()]);
        let ids: Vec<String> = state.entries().into_iter().map(|entry| entry.id).collect();
        state.set_labels(&ids[0], &["work".to_string()]).unwrap();
        let app = mock_builder()
            .manage(state)
            .build(mock_context(noop_assets()))
            .unwrap();
        (vec![work, home], app)
    }

    fn post(url: &str, body: Value) -> Request {
        Request {
            method: "POST".to_string(),
            url: url.to_string(),
            headers: vec![
                ("Host".to_string(), "127.0.0.1:7377".to_string()),
                ("Authorization".to_string(), format!("Bearer {TOKEN}")),
                ("Content-Type".to_string(), "application/json".to_string()),
            ],
            body: body.to_string().into_bytes(),
        }
    }

    fn call(tool: &str, arguments: Value) -> Value {
        json!({
            "jsonrpc": "2.0", "id": 7, "method": "tools/call",
            "params": { "name": tool, "arguments": arguments },
        })
    }

    fn text(reply: &Reply) -> &str {
        reply.body.as_ref().unwrap()["result"]["content"][0]["text"]
            .as_str()
            .unwrap()
    }

    #[test]
    fn a_tool_call_gets_a_json_reply_over_all_vaults() {
        let (_dirs, app) = app();

        let reply = route(app.handle(), 7377, TOKEN, &post("/mcp", call("tasks", json!({}))));

        assert_eq!(reply.status, 200);
        let rows = text(&reply);
        assert!(rows.contains("План:1  отчёт"), "{rows}");
        assert!(rows.contains("Дом:1  купить"), "{rows}");
    }

    #[test]
    fn the_query_picks_the_vaults() {
        let (_dirs, app) = app();

        let reply = route(
            app.handle(),
            7377,
            TOKEN,
            &post("/mcp?label=WORK", call("tasks", json!({}))),
        );
        assert_eq!(text(&reply), "План:1  отчёт", "одно хранилище: без префикса");

        let id = app.state::<AppState>().entries()[1].id.clone();
        let reply = route(
            app.handle(),
            7377,
            TOKEN,
            &post(&format!("/mcp?vault={id}"), call("tasks", json!({}))),
        );
        assert_eq!(text(&reply), "Дом:1  купить");

        let reply = route(
            app.handle(),
            7377,
            TOKEN,
            &post("/mcp?vault=%D0%9D%D0%B5%D1%82", call("tasks", json!({}))),
        );
        assert_eq!(reply.status, 404);
    }

    #[test]
    fn a_write_of_the_agent_reaches_the_window() {
        let (dirs, app) = app();
        let heard = Arc::new(Mutex::new(Vec::new()));
        let sink = heard.clone();
        app.listen(EVENT, move |event| sink.lock().unwrap().push(event.payload().to_string()));

        let reply = route(
            app.handle(),
            7377,
            TOKEN,
            &post(
                "/mcp?label=work",
                call(
                    "resolve_comment",
                    json!({ "note": "План", "line": 1, "text": "проверить цифры" }),
                ),
            ),
        );

        assert_eq!(text(&reply), "комментарий План:1 снят");
        assert_eq!(
            fs::read_to_string(dirs[0].path().join("План.md")).unwrap(),
            "- [ ] отчёт\n"
        );
        let heard = heard.lock().unwrap();
        assert_eq!(heard.len(), 1, "{heard:?}");
        assert!(heard[0].contains(r#""updated":["План"]"#), "{heard:?}");
    }

    #[test]
    fn requests_without_the_token_or_from_a_page_are_refused() {
        let (_dirs, app) = app();
        let mut request = post("/mcp", call("tasks", json!({})));

        request.headers[1].1 = "Bearer чужой".to_string();
        assert_eq!(route(app.handle(), 7377, TOKEN, &request).status, 401);

        let mut request = post("/mcp", call("tasks", json!({})));
        request.headers[0].1 = "evil.example:7377".to_string();
        assert_eq!(route(app.handle(), 7377, TOKEN, &request).status, 403, "DNS rebinding");

        let mut request = post("/mcp", call("tasks", json!({})));
        request
            .headers
            .push(("Origin".to_string(), "https://evil.example".to_string()));
        assert_eq!(route(app.handle(), 7377, TOKEN, &request).status, 403);

        let mut request = post("/mcp", call("tasks", json!({})));
        request
            .headers
            .push(("Origin".to_string(), "http://localhost:7377".to_string()));
        assert_eq!(route(app.handle(), 7377, TOKEN, &request).status, 200);
    }

    #[test]
    fn other_methods_paths_and_notifications() {
        let (_dirs, app) = app();

        let mut request = post("/mcp", json!({}));
        request.method = "GET".to_string();
        assert_eq!(route(app.handle(), 7377, TOKEN, &request).status, 405, "SSE нет");

        let request = post("/other", json!({}));
        assert_eq!(route(app.handle(), 7377, TOKEN, &request).status, 404);

        let note = json!({ "jsonrpc": "2.0", "method": "notifications/initialized" });
        assert_eq!(route(app.handle(), 7377, TOKEN, &post("/mcp", note)).status, 202);

        let mut request = post("/mcp", json!({}));
        request.body = b"not json".to_vec();
        assert_eq!(route(app.handle(), 7377, TOKEN, &request).status, 400);
    }

    #[test]
    fn the_server_listens_and_stops() {
        let (_dirs, app) = app();
        let server = McpServer::new(
            None,
            McpConfig {
                enabled: true,
                port: 0,
                token: TOKEN.to_string(),
            },
        );

        server.start(app.handle().clone());
        let status = server.status();
        assert!(status.running, "{status:?}");
        let port = status.port;

        let body = json!({ "jsonrpc": "2.0", "id": 1, "method": "ping" }).to_string();
        let mut stream = TcpStream::connect(("127.0.0.1", port)).unwrap();
        write!(
            stream,
            "POST /mcp HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nAuthorization: Bearer {TOKEN}\r\n\
             Content-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        )
        .unwrap();
        let mut answer = String::new();
        stream.read_to_string(&mut answer).unwrap();
        assert!(answer.starts_with("HTTP/1.1 200"), "{answer}");
        assert!(answer.ends_with(r#"{"id":1,"jsonrpc":"2.0","result":{}}"#), "{answer}");

        // A second server on the same port fails, and the status says why.
        let busy = McpServer::new(
            None,
            McpConfig {
                enabled: true,
                port,
                token: TOKEN.to_string(),
            },
        );
        busy.start(app.handle().clone());
        let status = busy.status();
        assert!(!status.running);
        assert!(status.error.is_some());

        server.configure(app.handle().clone(), false, port).unwrap();
        assert!(!server.status().running);
        assert!(TcpStream::connect(("127.0.0.1", port)).is_err(), "порт закрыт");
    }
}
