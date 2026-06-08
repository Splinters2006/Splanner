use std::env;
use std::fs;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

const DEFAULT_ADDRESS: &str = "127.0.0.1:8100";
const BIND_ADDRESS_FILE: &str = "data/bind_address.txt";
const ADMIN_PASSWORD_FILE: &str = "data/admin_password.txt";
const ACCOUNTS_FILE: &str = "data/accounts.txt";
const DEFAULT_ACCOUNTS: &[&str] = &["Maya", "Dad", "Mum", "Sam"];
const HASH_SALT: &str = "splanner-local-admin-v1";

fn main() -> std::io::Result<()> {
    let args: Vec<String> = env::args().collect();
    if args.get(1).map(String::as_str) == Some("--set-admin-password") {
        let password = if let Some(password) = args.get(2) {
            password.to_string()
        } else {
            let mut password = String::new();
            std::io::stdin().read_to_string(&mut password)?;
            password
        };
        let password = password.trim_end_matches(['\r', '\n']);
        if password.is_empty() {
            eprintln!("admin password cannot be empty");
            std::process::exit(2);
        }
        set_admin_password(password)?;
        ensure_accounts_file()?;
        println!("Admin password saved.");
        return Ok(());
    }

    ensure_accounts_file()?;
    let address = read_bind_address();
    let listener = TcpListener::bind(&address)?;
    println!("Splanner is running at http://{address}");

    for stream in listener.incoming() {
        match stream {
            Ok(mut stream) => {
                if let Err(error) = handle_connection(&mut stream) {
                    eprintln!("request failed: {error}");
                }
            }
            Err(error) => eprintln!("connection failed: {error}"),
        }
    }

    Ok(())
}

fn handle_connection(stream: &mut TcpStream) -> std::io::Result<()> {
    let mut buffer = [0; 16 * 1024];
    let read = stream.read(&mut buffer)?;
    let request = String::from_utf8_lossy(&buffer[..read]);
    let parsed = parse_request(&request);

    let (status, content_type, body) = match (parsed.method, parsed.path.as_str()) {
        ("GET", "/") | ("GET", "/index.html") => {
            ("200 OK", "text/html; charset=utf-8", INDEX_HTML.to_string())
        }
        ("GET", "/styles.css") => ("200 OK", "text/css; charset=utf-8", STYLES_CSS.to_string()),
        ("GET", "/app.js") => (
            "200 OK",
            "application/javascript; charset=utf-8",
            APP_JS.to_string(),
        ),
        ("GET", "/api/accounts") => (
            "200 OK",
            "application/json; charset=utf-8",
            accounts_json(&read_accounts()),
        ),
        ("GET", "/api/now") => (
            "200 OK",
            "application/json; charset=utf-8",
            host_time_json(),
        ),
        ("POST", "/api/admin/login") => handle_admin_login(&parsed.body),
        ("POST", "/api/accounts") => handle_add_account(&parsed.body),
        ("POST", "/api/accounts/delete") => handle_delete_account(&parsed.body),
        _ => (
            "404 Not Found",
            "text/plain; charset=utf-8",
            "Not found".to_string(),
        ),
    };

    write_response(stream, status, content_type, &body)
}

struct Request {
    method: &'static str,
    path: String,
    body: String,
}

fn parse_request(request: &str) -> Request {
    let mut lines = request.lines();
    let first_line = lines.next().unwrap_or_default();
    let mut first_parts = first_line.split_whitespace();
    let method = match first_parts.next().unwrap_or_default() {
        "GET" => "GET",
        "POST" => "POST",
        _ => "OTHER",
    };
    let path = first_parts
        .next()
        .unwrap_or("/")
        .split('?')
        .next()
        .unwrap_or("/")
        .to_string();
    let body = request
        .split_once("\r\n\r\n")
        .map(|(_, body)| body.to_string())
        .unwrap_or_default();

    Request { method, path, body }
}

fn handle_admin_login(body: &str) -> (&'static str, &'static str, String) {
    let password = json_field(body, "password").unwrap_or_default();
    if verify_admin_password(&password) {
        json_response("200 OK", r#"{"ok":true}"#)
    } else {
        json_response(
            "401 Unauthorized",
            r#"{"ok":false,"error":"Wrong password"}"#,
        )
    }
}

fn handle_add_account(body: &str) -> (&'static str, &'static str, String) {
    let password = json_field(body, "password").unwrap_or_default();
    if !verify_admin_password(&password) {
        return json_response(
            "401 Unauthorized",
            r#"{"ok":false,"error":"Wrong password"}"#,
        );
    }

    let name = sanitize_account_name(&json_field(body, "name").unwrap_or_default());
    if name.is_empty() {
        return json_response("400 Bad Request", r#"{"ok":false,"error":"Name required"}"#);
    }

    let mut accounts = read_accounts();
    if !accounts.iter().any(|account| same_name(account, &name)) {
        accounts.push(name);
        if let Err(error) = write_accounts(&accounts) {
            return json_response(
                "500 Internal Server Error",
                &format!(
                    r#"{{"ok":false,"error":"{}"}}"#,
                    escape_json(&error.to_string())
                ),
            );
        }
    }

    json_response(
        "200 OK",
        &format!(r#"{{"ok":true,"accounts":{}}}"#, accounts_json(&accounts)),
    )
}

fn handle_delete_account(body: &str) -> (&'static str, &'static str, String) {
    let password = json_field(body, "password").unwrap_or_default();
    if !verify_admin_password(&password) {
        return json_response(
            "401 Unauthorized",
            r#"{"ok":false,"error":"Wrong password"}"#,
        );
    }

    let name = sanitize_account_name(&json_field(body, "name").unwrap_or_default());
    let mut accounts = read_accounts();
    accounts.retain(|account| !same_name(account, &name));
    if accounts.is_empty() {
        return json_response(
            "400 Bad Request",
            r#"{"ok":false,"error":"Keep at least one account"}"#,
        );
    }

    if let Err(error) = write_accounts(&accounts) {
        return json_response(
            "500 Internal Server Error",
            &format!(
                r#"{{"ok":false,"error":"{}"}}"#,
                escape_json(&error.to_string())
            ),
        );
    }

    json_response(
        "200 OK",
        &format!(r#"{{"ok":true,"accounts":{}}}"#, accounts_json(&accounts)),
    )
}

fn json_response(status: &'static str, body: &str) -> (&'static str, &'static str, String) {
    (status, "application/json; charset=utf-8", body.to_string())
}

fn host_time_json() -> String {
    let now_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or_default();
    format!(r#"{{"nowMs":{now_ms}}}"#)
}

fn write_response(
    stream: &mut TcpStream,
    status: &str,
    content_type: &str,
    body: &str,
) -> std::io::Result<()> {
    let response = format!(
        "HTTP/1.1 {status}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nCache-Control: no-store\r\n\r\n{body}",
        body.len()
    );
    stream.write_all(response.as_bytes())?;
    stream.flush()
}

fn set_admin_password(password: &str) -> std::io::Result<()> {
    ensure_data_dir()?;
    fs::write(ADMIN_PASSWORD_FILE, password_hash(password))
}

fn verify_admin_password(password: &str) -> bool {
    fs::read_to_string(ADMIN_PASSWORD_FILE)
        .map(|saved| saved.trim() == password_hash(password))
        .unwrap_or(false)
}

fn password_hash(password: &str) -> String {
    let mut hash: u64 = 0xcbf29ce484222325;
    for byte in HASH_SALT.bytes().chain(password.bytes()) {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    format!("{hash:016x}")
}

fn ensure_data_dir() -> std::io::Result<()> {
    fs::create_dir_all("data")
}

fn ensure_accounts_file() -> std::io::Result<()> {
    ensure_data_dir()?;
    if !Path::new(ACCOUNTS_FILE).exists() {
        write_accounts(
            &DEFAULT_ACCOUNTS
                .iter()
                .map(|name| name.to_string())
                .collect::<Vec<_>>(),
        )?;
    }
    Ok(())
}

fn read_bind_address() -> String {
    fs::read_to_string(BIND_ADDRESS_FILE)
        .map(|address| address.trim().to_string())
        .ok()
        .filter(|address| !address.is_empty())
        .unwrap_or_else(|| DEFAULT_ADDRESS.to_string())
}

fn read_accounts() -> Vec<String> {
    let accounts = fs::read_to_string(ACCOUNTS_FILE)
        .unwrap_or_default()
        .lines()
        .map(sanitize_account_name)
        .filter(|name| !name.is_empty())
        .fold(Vec::<String>::new(), |mut list, name| {
            if !list.iter().any(|existing| same_name(existing, &name)) {
                list.push(name);
            }
            list
        });

    if accounts.is_empty() {
        DEFAULT_ACCOUNTS
            .iter()
            .map(|name| name.to_string())
            .collect()
    } else {
        accounts
    }
}

fn write_accounts(accounts: &[String]) -> std::io::Result<()> {
    ensure_data_dir()?;
    fs::write(ACCOUNTS_FILE, format!("{}\n", accounts.join("\n")))
}

fn sanitize_account_name(name: &str) -> String {
    name.trim()
        .chars()
        .filter(|character| {
            character.is_ascii_alphanumeric()
                || character.is_ascii_whitespace()
                || matches!(character, '-' | '_' | '.')
        })
        .take(32)
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn same_name(left: &str, right: &str) -> bool {
    left.eq_ignore_ascii_case(right)
}

fn accounts_json(accounts: &[String]) -> String {
    format!(
        "[{}]",
        accounts
            .iter()
            .map(|account| format!(r#""{}""#, escape_json(account)))
            .collect::<Vec<_>>()
            .join(",")
    )
}

fn json_field(body: &str, field: &str) -> Option<String> {
    let needle = format!(r#""{field}""#);
    let after_field = body.split_once(&needle)?.1;
    let after_colon = after_field.split_once(':')?.1.trim_start();
    let value = after_colon.strip_prefix('"')?;
    let mut result = String::new();
    let mut escaped = false;
    for character in value.chars() {
        if escaped {
            result.push(match character {
                '"' => '"',
                '\\' => '\\',
                'n' => '\n',
                'r' => '\r',
                't' => '\t',
                other => other,
            });
            escaped = false;
        } else if character == '\\' {
            escaped = true;
        } else if character == '"' {
            return Some(result);
        } else {
            result.push(character);
        }
    }
    None
}

fn escape_json(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
        .replace('\r', "\\r")
}

const INDEX_HTML: &str = r#"<!doctype html>
<html lang="en">
<head>
  <meta charset="utf-8">
  <meta name="viewport" content="width=device-width, initial-scale=1, viewport-fit=cover">
  <title>Splanner</title>
  <link rel="stylesheet" href="/styles.css">
</head>
<body>
  <main class="app-shell">
    <header class="topbar">
      <section class="brand-block" aria-label="Current week">
        <p class="eyebrow">Family planner</p>
        <h1 id="week-title">This week</h1>
        <p id="week-range" class="week-range"></p>
      </section>
      <section class="clock-panel" aria-label="Host date and time">
        <time id="host-time" class="host-time">--:--</time>
        <time id="host-date" class="host-date">Loading date</time>
      </section>
      <nav class="week-actions" aria-label="Week navigation">
        <button id="prev-week" class="icon-button" type="button" aria-label="Previous week">&lsaquo;</button>
        <button id="today" class="text-button" type="button">Today</button>
        <button id="next-week" class="icon-button" type="button" aria-label="Next week">&rsaquo;</button>
        <button id="admin-open" class="icon-button secondary admin-cog" type="button" aria-label="Admin settings">&#9881;</button>
      </nav>
    </header>

    <section class="planner-stage" aria-label="Weekly planner">
      <div class="member-rail" id="member-rail" aria-label="Family members"></div>
      <section id="week-grid" class="week-grid" aria-live="polite"></section>
    </section>

    <aside class="quick-add" aria-label="Add plan item">
      <form id="task-form" autocomplete="off">
        <label>
          <span>What</span>
          <input id="task-title" name="title" required maxlength="80" placeholder="Pick up groceries">
        </label>
        <label>
          <span>Day</span>
          <select id="task-day" name="day"></select>
        </label>
        <label>
          <span>For</span>
          <select id="task-assignee" name="assignee"></select>
        </label>
        <label>
          <span>Asked by</span>
          <select id="task-requester" name="requester"></select>
        </label>
        <button type="submit">Add</button>
      </form>
    </aside>
  </main>

  <dialog id="admin-dialog" class="admin-dialog">
    <form id="admin-login" class="admin-form" autocomplete="off">
      <header class="dialog-header">
        <h2>Admin</h2>
        <button id="admin-close" class="plain-button" value="cancel" formmethod="dialog" type="button" aria-label="Close admin">Close</button>
      </header>
      <label>
        <span>Password</span>
        <input id="admin-password" name="password" type="password" required>
      </label>
      <button type="submit">Log in</button>
      <p id="admin-message" class="admin-message" role="status"></p>
    </form>

    <section id="admin-panel" class="admin-panel" hidden>
      <header class="dialog-header">
        <h2>Accounts</h2>
        <button id="admin-logout" class="plain-button" type="button">Log out</button>
      </header>
      <form id="account-form" class="admin-form" autocomplete="off">
        <label>
          <span>New account</span>
          <input id="account-name" name="name" maxlength="32" required placeholder="Alex">
        </label>
        <button type="submit">Add account</button>
      </form>
      <div id="account-list" class="account-list"></div>
    </section>
  </dialog>

  <script src="/app.js"></script>
</body>
</html>
"#;

const STYLES_CSS: &str = r#":root {
  color-scheme: light;
  --ink: #1d2328;
  --muted: #64707a;
  --line: #d9e0e4;
  --panel: #ffffff;
  --page: #f4f6f2;
  --accent: #2f6f63;
  --accent-strong: #1f554c;
  --sun: #f1b84b;
  --rose: #d75b62;
  --blue: #4b7fb8;
  --shadow: 0 16px 50px rgba(29, 35, 40, 0.12);
  font-family: Inter, ui-sans-serif, system-ui, -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif;
}

* {
  box-sizing: border-box;
}

html,
body {
  min-height: 100%;
}

body {
  margin: 0;
  background: var(--page);
  color: var(--ink);
  overscroll-behavior: none;
}

button,
input,
select {
  font: inherit;
}

button {
  border: 0;
  cursor: pointer;
}

.app-shell {
  min-height: 100vh;
  display: grid;
  grid-template-rows: auto 1fr auto;
  gap: 18px;
  padding: clamp(16px, 3vw, 32px);
}

.topbar {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 16px;
}

.eyebrow,
.week-range,
.host-date,
label span,
.task-meta,
.admin-message {
  color: var(--muted);
}

.eyebrow {
  margin: 0 0 4px;
  font-size: 0.86rem;
  font-weight: 700;
  text-transform: uppercase;
}

h1,
h2 {
  margin: 0;
  letter-spacing: 0;
}

h1 {
  font-size: clamp(2rem, 5vw, 4.5rem);
  line-height: 0.95;
}

h2 {
  font-size: 1.6rem;
}

.week-range {
  margin: 8px 0 0;
  font-size: clamp(1rem, 1.8vw, 1.35rem);
}

.clock-panel {
  min-width: 150px;
  display: grid;
  justify-items: end;
  gap: 2px;
}

.host-time {
  font-size: clamp(1.7rem, 3vw, 3rem);
  font-weight: 900;
  line-height: 1;
}

.host-date {
  font-size: clamp(0.92rem, 1.3vw, 1.12rem);
  font-weight: 800;
  text-align: right;
}

.week-actions {
  display: grid;
  grid-template-columns: 64px auto 64px 64px;
  align-items: center;
  gap: 10px;
}

.icon-button,
.text-button,
.quick-add button,
.admin-form button {
  min-height: 56px;
  border-radius: 8px;
  background: var(--ink);
  color: #fff;
  font-weight: 800;
  box-shadow: var(--shadow);
}

.icon-button {
  width: 64px;
  font-size: 2.6rem;
  line-height: 1;
}

.text-button,
.admin-form button {
  padding: 0 22px;
}

.secondary {
  background: var(--accent-strong);
}

.admin-cog {
  font-size: 1.8rem;
}

.planner-stage {
  min-height: 0;
  display: grid;
  grid-template-columns: minmax(92px, 130px) 1fr;
  gap: 12px;
}

.member-rail {
  display: grid;
  grid-template-rows: repeat(4, minmax(0, 1fr));
  gap: 10px;
}

.member {
  display: grid;
  place-items: center;
  border-radius: 8px;
  background: var(--panel);
  border: 1px solid var(--line);
  box-shadow: var(--shadow);
  text-align: center;
  padding: 10px;
}

.avatar {
  width: 54px;
  aspect-ratio: 1;
  display: grid;
  place-items: center;
  border-radius: 50%;
  color: #fff;
  font-weight: 900;
  font-size: 1.45rem;
}

.member strong {
  margin-top: 8px;
  font-size: clamp(0.85rem, 1.3vw, 1.05rem);
}

.week-grid {
  min-width: 0;
  display: grid;
  grid-template-columns: repeat(7, minmax(140px, 1fr));
  gap: 10px;
  touch-action: pan-y;
}

.day-column {
  min-height: 56vh;
  background: var(--panel);
  border: 1px solid var(--line);
  border-radius: 8px;
  box-shadow: var(--shadow);
  display: grid;
  grid-template-rows: auto 1fr;
  overflow: hidden;
}

.day-header {
  padding: 16px;
  border-bottom: 1px solid var(--line);
}

.day-header strong {
  display: block;
  font-size: clamp(1.05rem, 1.6vw, 1.35rem);
}

.date-label {
  color: var(--muted);
  font-weight: 700;
}

.task-list {
  display: grid;
  align-content: start;
  gap: 10px;
  padding: 12px;
}

.task-card {
  border-left: 6px solid var(--accent);
  border-radius: 8px;
  background: #f8faf7;
  padding: 12px;
  min-height: 92px;
  display: grid;
  gap: 9px;
}

.task-card[data-tone="1"] {
  border-color: var(--blue);
}

.task-card[data-tone="2"] {
  border-color: var(--sun);
}

.task-card[data-tone="3"] {
  border-color: var(--rose);
}

.task-title {
  margin: 0;
  font-size: clamp(1rem, 1.4vw, 1.18rem);
  font-weight: 850;
  overflow-wrap: anywhere;
}

.task-meta {
  display: flex;
  flex-wrap: wrap;
  gap: 6px;
  font-size: 0.9rem;
  font-weight: 700;
}

.chip {
  display: inline-flex;
  align-items: center;
  min-height: 28px;
  padding: 0 8px;
  border-radius: 999px;
  background: #e8eeee;
}

.task-actions {
  display: flex;
  justify-content: flex-end;
}

.delete-task {
  width: 38px;
  min-height: 34px;
  border-radius: 8px;
  background: #edf1ef;
  color: var(--muted);
  font-size: 1.35rem;
  line-height: 1;
}

.empty-day {
  min-height: 88px;
  display: grid;
  place-items: center;
  color: #99a3aa;
  border: 1px dashed var(--line);
  border-radius: 8px;
  font-weight: 750;
}

.quick-add,
.admin-dialog {
  background: var(--panel);
  border: 1px solid var(--line);
  border-radius: 8px;
  box-shadow: var(--shadow);
}

.quick-add {
  padding: 12px;
}

form {
  display: grid;
  grid-template-columns: minmax(180px, 2fr) repeat(3, minmax(120px, 1fr)) auto;
  gap: 10px;
  align-items: end;
}

label {
  display: grid;
  gap: 6px;
  font-weight: 750;
}

input,
select {
  width: 100%;
  min-height: 52px;
  border: 1px solid var(--line);
  border-radius: 8px;
  background: #f9fbf8;
  color: var(--ink);
  padding: 0 12px;
}

.quick-add button,
.admin-form button {
  padding: 0 24px;
  background: var(--accent);
}

.admin-dialog {
  width: min(560px, calc(100vw - 28px));
  padding: 18px;
}

.admin-dialog[open] {
  position: fixed;
  inset: 50% auto auto 50%;
  transform: translate(-50%, -50%);
  z-index: 20;
}

.admin-dialog::backdrop {
  background: rgba(29, 35, 40, 0.34);
}

.dialog-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 12px;
  margin-bottom: 14px;
}

.plain-button {
  min-height: 44px;
  border-radius: 8px;
  padding: 0 14px;
  background: #edf1ef;
  color: var(--ink);
  font-weight: 800;
}

.admin-form {
  grid-template-columns: 1fr auto;
}

.admin-form .dialog-header,
.admin-form .admin-message {
  grid-column: 1 / -1;
}

.account-list {
  display: grid;
  gap: 8px;
  margin-top: 14px;
}

.account-row {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 10px;
  min-height: 52px;
  padding: 8px 10px;
  border: 1px solid var(--line);
  border-radius: 8px;
  background: #f9fbf8;
  font-weight: 800;
}

.account-row button {
  min-height: 38px;
  border-radius: 8px;
  padding: 0 12px;
  background: #edf1ef;
  color: var(--muted);
  font-weight: 800;
}

@media (max-width: 980px) {
  .app-shell {
    gap: 14px;
    padding: 14px;
  }

  .topbar {
    display: grid;
    grid-template-columns: 1fr auto;
    align-items: end;
  }

  .week-actions {
    grid-column: 1 / -1;
  }

  .planner-stage {
    grid-template-columns: 1fr;
  }

  .member-rail {
    grid-template-columns: repeat(4, minmax(0, 1fr));
    grid-template-rows: none;
  }

  .member {
    min-height: 92px;
  }

  .avatar {
    width: 42px;
    font-size: 1.15rem;
  }

  .week-grid {
    grid-auto-flow: column;
    grid-auto-columns: minmax(78vw, 1fr);
    grid-template-columns: none;
    overflow-x: auto;
    scroll-snap-type: x mandatory;
    padding-bottom: 8px;
  }

  .day-column {
    min-height: 52vh;
    scroll-snap-align: start;
  }

  form {
    grid-template-columns: 1fr 1fr;
  }

  form label:first-child,
  .quick-add button {
    grid-column: 1 / -1;
  }
}

@media (max-width: 640px) {
  .topbar {
    display: grid;
  }

  .week-actions {
    grid-template-columns: 58px auto 58px 58px;
  }

  .icon-button {
    width: 58px;
  }

  .clock-panel {
    justify-items: start;
  }

  .host-date {
    text-align: left;
  }

  .member-rail {
    grid-template-columns: repeat(2, minmax(0, 1fr));
  }

  form,
  .admin-form {
    grid-template-columns: 1fr;
  }
}
"#;

const APP_JS: &str = r##"const STORAGE_KEY = "splanner.tasks.v1";
const ACCOUNT_COLORS = ["#2f6f63", "#4b7fb8", "#d75b62", "#f1b84b", "#7a6fbe", "#bf6b45"];

const state = {
  weekStart: startOfWeek(new Date()),
  tasks: loadTasks(),
  accounts: [],
  adminPassword: "",
  hostClockOffsetMs: 0,
};

const grid = document.querySelector("#week-grid");
const weekTitle = document.querySelector("#week-title");
const weekRange = document.querySelector("#week-range");
const hostTime = document.querySelector("#host-time");
const hostDate = document.querySelector("#host-date");
const taskDay = document.querySelector("#task-day");
const taskAssignee = document.querySelector("#task-assignee");
const taskRequester = document.querySelector("#task-requester");
const form = document.querySelector("#task-form");
const memberRail = document.querySelector("#member-rail");
const adminDialog = document.querySelector("#admin-dialog");
const adminLogin = document.querySelector("#admin-login");
const adminPanel = document.querySelector("#admin-panel");
const adminPassword = document.querySelector("#admin-password");
const adminMessage = document.querySelector("#admin-message");
const accountForm = document.querySelector("#account-form");
const accountName = document.querySelector("#account-name");
const accountList = document.querySelector("#account-list");

document.querySelector("#prev-week").addEventListener("click", () => shiftWeek(-1));
document.querySelector("#next-week").addEventListener("click", () => shiftWeek(1));
document.querySelector("#today").addEventListener("click", () => {
  state.weekStart = startOfWeek(hostNow());
  render();
});

document.querySelector("#admin-open").addEventListener("click", () => {
  adminMessage.textContent = "";
  if (typeof adminDialog.showModal === "function") {
    adminDialog.showModal();
  } else {
    adminDialog.setAttribute("open", "");
  }
  if (!state.adminPassword) adminPassword.focus();
});

document.querySelector("#admin-close").addEventListener("click", () => {
  if (typeof adminDialog.close === "function") {
    adminDialog.close();
  } else {
    adminDialog.removeAttribute("open");
  }
});

document.querySelector("#admin-logout").addEventListener("click", () => {
  state.adminPassword = "";
  adminPassword.value = "";
  setAdminMode(false);
});

adminLogin.addEventListener("submit", async (event) => {
  event.preventDefault();
  const password = adminPassword.value;
  const response = await apiPost("/api/admin/login", { password });
  if (!response.ok) {
    adminMessage.textContent = response.error || "Wrong password";
    return;
  }
  state.adminPassword = password;
  adminMessage.textContent = "";
  setAdminMode(true);
});

accountForm.addEventListener("submit", async (event) => {
  event.preventDefault();
  const response = await apiPost("/api/accounts", {
    password: state.adminPassword,
    name: accountName.value,
  });
  if (!response.ok) {
    adminMessage.textContent = response.error || "Could not add account";
    return;
  }
  accountName.value = "";
  await loadAccounts(response.accounts);
});

accountList.addEventListener("click", async (event) => {
  const button = event.target.closest("[data-account-delete]");
  if (!button) return;
  const response = await apiPost("/api/accounts/delete", {
    password: state.adminPassword,
    name: button.dataset.accountDelete,
  });
  if (!response.ok) {
    adminMessage.textContent = response.error || "Could not delete account";
    return;
  }
  await loadAccounts(response.accounts);
});

form.addEventListener("submit", (event) => {
  event.preventDefault();
  const data = new FormData(form);
  const title = data.get("title").trim();
  if (!title) return;

  state.tasks.push({
    id: createId(),
    title,
    date: data.get("day"),
    assignee: data.get("assignee"),
    requester: data.get("requester"),
    createdAt: new Date().toISOString(),
  });

  saveTasks();
  form.reset();
  taskDay.value = toDateKey(hostNow());
  render();
});

grid.addEventListener("click", (event) => {
  const button = event.target.closest("[data-delete]");
  if (!button) return;
  state.tasks = state.tasks.filter((task) => task.id !== button.dataset.delete);
  saveTasks();
  render();
});

let swipeStartX = 0;
let swipeStartY = 0;

grid.addEventListener("pointerdown", (event) => {
  swipeStartX = event.clientX;
  swipeStartY = event.clientY;
});

grid.addEventListener("pointerup", (event) => {
  const xDelta = event.clientX - swipeStartX;
  const yDelta = event.clientY - swipeStartY;
  if (Math.abs(xDelta) < 110 || Math.abs(xDelta) < Math.abs(yDelta) * 1.3) return;
  shiftWeek(xDelta < 0 ? 1 : -1);
});

async function init() {
  renderClock();
  await syncHostTime();
  state.weekStart = startOfWeek(hostNow());
  renderClock();
  setInterval(renderClock, 1000);
  setInterval(syncHostTime, 5 * 60 * 1000);
  await loadAccounts();
  render();
}

async function syncHostTime() {
  try {
    const response = await fetch("/api/now");
    const data = await response.json();
    state.hostClockOffsetMs = data.nowMs - Date.now();
  } catch {
    state.hostClockOffsetMs = 0;
  }
}

function hostNow() {
  return new Date(Date.now() + state.hostClockOffsetMs);
}

function renderClock() {
  const now = hostNow();
  hostTime.textContent = formatDate(now, { hour: "2-digit", minute: "2-digit" });
  hostDate.textContent = formatDate(now, { weekday: "long", month: "long", day: "numeric", year: "numeric" });
}

async function loadAccounts(accounts = null) {
  state.accounts = accounts || await fetch("/api/accounts").then((response) => response.json());
  renderMembers();
  renderAccountControls();
  renderPersonOptions();
  renderWeekGrid();
}

function render() {
  renderMembers();
  renderWeekHeading();
  renderDayOptions();
  renderPersonOptions();
  renderWeekGrid();
  renderAccountControls();
}

function renderMembers() {
  memberRail.innerHTML = state.accounts.map((name, index) => `
    <article class="member">
      <span class="avatar" style="background:${ACCOUNT_COLORS[index % ACCOUNT_COLORS.length]}">${name.slice(0, 1)}</span>
      <strong>${escapeHtml(name)}</strong>
    </article>
  `).join("");
}

function renderAccountControls() {
  accountList.innerHTML = state.accounts.map((name) => `
    <div class="account-row">
      <span>${escapeHtml(name)}</span>
      <button type="button" data-account-delete="${escapeHtml(name)}">Delete</button>
    </div>
  `).join("");
}

function renderPersonOptions() {
  const assignee = taskAssignee.value;
  const requester = taskRequester.value;
  const options = state.accounts.map((name) => `<option value="${escapeHtml(name)}">${escapeHtml(name)}</option>`).join("");
  taskAssignee.innerHTML = `<option value="">Anyone</option>${options}`;
  taskRequester.innerHTML = `<option value="">Family</option>${options}`;
  taskAssignee.value = state.accounts.includes(assignee) ? assignee : "";
  taskRequester.value = state.accounts.includes(requester) ? requester : "";
}

function setAdminMode(isLoggedIn) {
  adminLogin.hidden = isLoggedIn;
  adminPanel.hidden = !isLoggedIn;
  renderAccountControls();
}

function renderWeekHeading() {
  const days = getWeekDays();
  const todayStart = startOfWeek(hostNow());
  const weekOffset = Math.round((state.weekStart - todayStart) / (7 * 24 * 60 * 60 * 1000));
  weekTitle.textContent = weekOffset === 0 ? "This week" : weekOffset === 1 ? "Next week" : weekOffset === -1 ? "Last week" : `${Math.abs(weekOffset)} weeks ${weekOffset > 0 ? "ahead" : "back"}`;
  weekRange.textContent = `${formatDate(days[0], { month: "long", day: "numeric" })} - ${formatDate(days[6], { month: "long", day: "numeric", year: "numeric" })}`;
}

function renderDayOptions() {
  const current = taskDay.value || toDateKey(hostNow());
  taskDay.innerHTML = getWeekDays().map((day) => `
    <option value="${toDateKey(day)}">${formatDate(day, { weekday: "long", month: "short", day: "numeric" })}</option>
  `).join("");
  taskDay.value = getWeekDays().some((day) => toDateKey(day) === current) ? current : toDateKey(getWeekDays()[0]);
}

function renderWeekGrid() {
  grid.innerHTML = getWeekDays().map((day) => {
    const key = toDateKey(day);
    const tasks = state.tasks
      .filter((task) => task.date === key)
      .sort((a, b) => a.createdAt.localeCompare(b.createdAt));
    return `
      <article class="day-column">
        <header class="day-header">
          <strong>${formatDate(day, { weekday: "short" })}</strong>
          <span class="date-label">${formatDate(day, { month: "short", day: "numeric" })}</span>
        </header>
        <div class="task-list">
          ${tasks.length ? tasks.map(renderTask).join("") : `<div class="empty-day">Open</div>`}
        </div>
      </article>
    `;
  }).join("");
}

function renderTask(task, index) {
  const assignee = task.assignee || "Anyone";
  const requester = task.requester ? `Asked by ${task.requester}` : "Family task";
  return `
    <article class="task-card" data-tone="${index % 4}">
      <p class="task-title">${escapeHtml(task.title)}</p>
      <div class="task-meta">
        <span class="chip">${escapeHtml(assignee)}</span>
        <span class="chip">${escapeHtml(requester)}</span>
      </div>
      <div class="task-actions">
        <button class="delete-task" type="button" data-delete="${task.id}" aria-label="Remove ${escapeHtml(task.title)}">&times;</button>
      </div>
    </article>
  `;
}

async function apiPost(url, payload) {
  const response = await fetch(url, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify(payload),
  });
  const data = await response.json();
  return { ...data, ok: response.ok && data.ok };
}

function shiftWeek(amount) {
  state.weekStart = addDays(state.weekStart, amount * 7);
  render();
}

function getWeekDays() {
  return Array.from({ length: 7 }, (_, index) => addDays(state.weekStart, index));
}

function startOfWeek(date) {
  const copy = new Date(date);
  copy.setHours(0, 0, 0, 0);
  const day = copy.getDay();
  const mondayOffset = day === 0 ? -6 : 1 - day;
  return addDays(copy, mondayOffset);
}

function addDays(date, amount) {
  const copy = new Date(date);
  copy.setDate(copy.getDate() + amount);
  return copy;
}

function toDateKey(date) {
  const year = date.getFullYear();
  const month = String(date.getMonth() + 1).padStart(2, "0");
  const day = String(date.getDate()).padStart(2, "0");
  return `${year}-${month}-${day}`;
}

function formatDate(date, options) {
  return new Intl.DateTimeFormat(undefined, options).format(date);
}

function loadTasks() {
  try {
    return JSON.parse(localStorage.getItem(STORAGE_KEY)) || seedTasks();
  } catch {
    return seedTasks();
  }
}

function saveTasks() {
  localStorage.setItem(STORAGE_KEY, JSON.stringify(state.tasks));
}

function createId() {
  if (window.crypto && typeof window.crypto.randomUUID === "function") {
    return window.crypto.randomUUID();
  }
  return `task-${Date.now()}-${Math.random().toString(16).slice(2)}`;
}

function seedTasks() {
  const days = Array.from({ length: 7 }, (_, index) => addDays(startOfWeek(new Date()), index));
  return [
    {
      id: createId(),
      title: "Set dinner table",
      date: toDateKey(days[0]),
      assignee: "Sam",
      requester: "Mum",
      createdAt: new Date().toISOString(),
    },
    {
      id: createId(),
      title: "Bring sports bag",
      date: toDateKey(days[2]),
      assignee: "Maya",
      requester: "Dad",
      createdAt: new Date(Date.now() + 1).toISOString(),
    },
  ];
}

function escapeHtml(value) {
  return String(value)
    .replaceAll("&", "&amp;")
    .replaceAll("<", "&lt;")
    .replaceAll(">", "&gt;")
    .replaceAll('"', "&quot;")
    .replaceAll("'", "&#039;");
}

init();
"##;
