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
const GROUPS_FILE: &str = "data/groups.txt";
const TASKS_FILE: &str = "data/tasks.txt";
const BROADCASTS_FILE: &str = "data/broadcasts.txt";
const EVENTS_FILE: &str = "data/events.txt";
const HASH_SALT: &str = "splanner-local-admin-v1";
const OVERVIEW_ACCOUNT: &str = "overview";

#[derive(Clone)]
struct Account {
    name: String,
    pin_hash: String,
}

#[derive(Clone)]
struct Group {
    name: String,
    members: Vec<String>,
}

#[derive(Clone)]
struct Task {
    id: String,
    title: String,
    date: String,
    time: String,
    assignees: Vec<String>,
    requester: String,
    created_at: String,
    note: String,
}

#[derive(Clone)]
struct Broadcast {
    id: String,
    message: String,
    targets: Vec<String>,
    requester: String,
    created_at: String,
    seen_by: Vec<String>,
}

#[derive(Clone)]
struct EventPlan {
    id: String,
    title: String,
    start_date: String,
    start_time: String,
    end_date: String,
    end_time: String,
    assignees: Vec<String>,
    requester: String,
    created_at: String,
    note: String,
}

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
        ensure_groups_file()?;
        ensure_tasks_file()?;
        ensure_broadcasts_file()?;
        ensure_events_file()?;
        println!("Admin password saved.");
        return Ok(());
    }

    ensure_accounts_file()?;
    ensure_groups_file()?;
    ensure_tasks_file()?;
    ensure_broadcasts_file()?;
    ensure_events_file()?;
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
        ("GET", "/api/groups") => (
            "200 OK",
            "application/json; charset=utf-8",
            groups_json(&read_groups()),
        ),
        ("GET", "/api/now") => (
            "200 OK",
            "application/json; charset=utf-8",
            host_time_json(),
        ),
        ("GET", "/api/tasks") => (
            "200 OK",
            "application/json; charset=utf-8",
            tasks_json(&read_tasks()),
        ),
        ("GET", "/api/broadcasts") => (
            "200 OK",
            "application/json; charset=utf-8",
            broadcasts_json(&read_broadcasts()),
        ),
        ("GET", "/api/events") => (
            "200 OK",
            "application/json; charset=utf-8",
            events_json(&read_events()),
        ),
        ("POST", "/api/admin/login") => handle_admin_login(&parsed.body),
        ("POST", "/api/user/login") => handle_user_login(&parsed.body),
        ("POST", "/api/accounts") => handle_add_account(&parsed.body),
        ("POST", "/api/accounts/delete") => handle_delete_account(&parsed.body),
        ("POST", "/api/groups") => handle_add_group(&parsed.body),
        ("POST", "/api/groups/delete") => handle_delete_group(&parsed.body),
        ("POST", "/api/groups/member") => handle_group_member(&parsed.body),
        ("POST", "/api/tasks") => handle_add_task(&parsed.body),
        ("POST", "/api/tasks/delete") => handle_delete_task(&parsed.body),
        ("POST", "/api/broadcasts") => handle_add_broadcast(&parsed.body),
        ("POST", "/api/broadcasts/seen") => handle_mark_broadcast_seen(&parsed.body),
        ("POST", "/api/events") => handle_add_event(&parsed.body),
        ("POST", "/api/events/delete") => handle_delete_event(&parsed.body),
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
    if is_overview_account(&name) {
        return json_response(
            "400 Bad Request",
            r#"{"ok":false,"error":"Overview is a built-in account"}"#,
        );
    }
    let pin = json_field(body, "pin").unwrap_or_default();
    if !is_valid_pin(&pin) {
        return json_response(
            "400 Bad Request",
            r#"{"ok":false,"error":"Use a 4 digit PIN"}"#,
        );
    }

    let mut accounts = read_accounts();
    if let Some(account) = accounts
        .iter_mut()
        .find(|account| same_name(&account.name, &name))
    {
        account.pin_hash = password_hash(&pin);
    } else {
        accounts.push(Account {
            name,
            pin_hash: password_hash(&pin),
        });
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

fn handle_delete_account(body: &str) -> (&'static str, &'static str, String) {
    let password = json_field(body, "password").unwrap_or_default();
    if !verify_admin_password(&password) {
        return json_response(
            "401 Unauthorized",
            r#"{"ok":false,"error":"Wrong password"}"#,
        );
    }

    let name = sanitize_account_name(&json_field(body, "name").unwrap_or_default());
    if is_overview_account(&name) {
        return json_response(
            "200 OK",
            &format!(
                r#"{{"ok":true,"accounts":{}}}"#,
                accounts_json(&read_accounts())
            ),
        );
    }
    let mut accounts = read_accounts();
    accounts.retain(|account| !same_name(&account.name, &name));
    remove_account_from_groups(&name);

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

fn handle_add_group(body: &str) -> (&'static str, &'static str, String) {
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

    let mut groups = read_groups();
    if !groups.iter().any(|group| same_name(&group.name, &name)) {
        groups.push(Group {
            name,
            members: Vec::new(),
        });
        if let Err(error) = write_groups(&groups) {
            return server_error(&error.to_string());
        }
    }

    json_response(
        "200 OK",
        &format!(r#"{{"ok":true,"groups":{}}}"#, groups_json(&groups)),
    )
}

fn handle_delete_group(body: &str) -> (&'static str, &'static str, String) {
    let password = json_field(body, "password").unwrap_or_default();
    if !verify_admin_password(&password) {
        return json_response(
            "401 Unauthorized",
            r#"{"ok":false,"error":"Wrong password"}"#,
        );
    }

    let name = sanitize_account_name(&json_field(body, "name").unwrap_or_default());
    let mut groups = read_groups();
    groups.retain(|group| !same_name(&group.name, &name));
    if let Err(error) = write_groups(&groups) {
        return server_error(&error.to_string());
    }

    json_response(
        "200 OK",
        &format!(r#"{{"ok":true,"groups":{}}}"#, groups_json(&groups)),
    )
}

fn handle_group_member(body: &str) -> (&'static str, &'static str, String) {
    let password = json_field(body, "password").unwrap_or_default();
    if !verify_admin_password(&password) {
        return json_response(
            "401 Unauthorized",
            r#"{"ok":false,"error":"Wrong password"}"#,
        );
    }

    let group_name = sanitize_account_name(&json_field(body, "group").unwrap_or_default());
    let member = sanitize_account_name(&json_field(body, "member").unwrap_or_default());
    let action = json_field(body, "action").unwrap_or_default();
    if group_name.is_empty() || member.is_empty() {
        return json_response(
            "400 Bad Request",
            r#"{"ok":false,"error":"Group and member required"}"#,
        );
    }
    if !read_accounts()
        .iter()
        .any(|account| same_name(&account.name, &member))
    {
        return json_response(
            "400 Bad Request",
            r#"{"ok":false,"error":"Unknown account"}"#,
        );
    }

    let mut groups = read_groups();
    let Some(group) = groups
        .iter_mut()
        .find(|group| same_name(&group.name, &group_name))
    else {
        return json_response("404 Not Found", r#"{"ok":false,"error":"Unknown group"}"#);
    };

    if action == "remove" {
        group.members.retain(|name| !same_name(name, &member));
    } else if !group.members.iter().any(|name| same_name(name, &member)) {
        group.members.push(member);
    }

    if let Err(error) = write_groups(&groups) {
        return server_error(&error.to_string());
    }

    json_response(
        "200 OK",
        &format!(r#"{{"ok":true,"groups":{}}}"#, groups_json(&groups)),
    )
}

fn handle_user_login(body: &str) -> (&'static str, &'static str, String) {
    let name = sanitize_account_name(&json_field(body, "name").unwrap_or_default());
    let pin = json_field(body, "pin").unwrap_or_default();
    if is_overview_account(&name) {
        if verify_admin_password(&pin) {
            return json_response(
                "200 OK",
                &format!(r#"{{"ok":true,"name":"{}"}}"#, OVERVIEW_ACCOUNT),
            );
        }
        return json_response(
            "401 Unauthorized",
            r#"{"ok":false,"error":"Wrong name or PIN"}"#,
        );
    }

    let is_valid = read_accounts()
        .iter()
        .any(|account| same_name(&account.name, &name) && account.pin_hash == password_hash(&pin));

    if is_valid {
        json_response(
            "200 OK",
            &format!(r#"{{"ok":true,"name":"{}"}}"#, escape_json(&name)),
        )
    } else {
        json_response(
            "401 Unauthorized",
            r#"{"ok":false,"error":"Wrong name or PIN"}"#,
        )
    }
}

fn handle_add_task(body: &str) -> (&'static str, &'static str, String) {
    let requester = sanitize_account_name(&json_field(body, "requester").unwrap_or_default());
    if !is_valid_task_requester(&requester) || is_overview_account(&requester) {
        return json_response("401 Unauthorized", r#"{"ok":false,"error":"Unknown user"}"#);
    }

    let title = sanitize_task_text(&json_field(body, "title").unwrap_or_default(), 100);
    let date = json_field(body, "date").unwrap_or_default();
    let time = json_field(body, "time").unwrap_or_default();
    let created_at = sanitize_task_text(&json_field(body, "createdAt").unwrap_or_default(), 40);
    let note = sanitize_note_text(&json_field(body, "note").unwrap_or_default(), 2000);
    let assignees = parse_assignees_field(&json_field(body, "assignees").unwrap_or_default());

    if title.is_empty() || !is_valid_date_key(&date) || !is_valid_time_value(&time) {
        return json_response("400 Bad Request", r#"{"ok":false,"error":"Invalid task"}"#);
    }

    let mut tasks = read_tasks();
    tasks.push(Task {
        id: create_server_id(),
        title,
        date,
        time,
        assignees,
        requester,
        created_at,
        note,
    });

    if let Err(error) = write_tasks(&tasks) {
        return server_error(&error.to_string());
    }

    json_response(
        "200 OK",
        &format!(r#"{{"ok":true,"tasks":{}}}"#, tasks_json(&tasks)),
    )
}

fn handle_delete_task(body: &str) -> (&'static str, &'static str, String) {
    let requester = sanitize_account_name(&json_field(body, "requester").unwrap_or_default());
    if !is_valid_task_requester(&requester) || is_overview_account(&requester) {
        return json_response("401 Unauthorized", r#"{"ok":false,"error":"Unknown user"}"#);
    }

    let id = sanitize_task_text(&json_field(body, "id").unwrap_or_default(), 100);
    let groups = read_groups();
    let tasks = read_tasks();
    let Some(task) = tasks.iter().find(|task| task.id == id) else {
        return json_response("404 Not Found", r#"{"ok":false,"error":"Unknown task"}"#);
    };

    if !can_delete_task(task, &requester, &groups) {
        return json_response(
            "403 Forbidden",
            r#"{"ok":false,"error":"Not allowed to delete this task"}"#,
        );
    }

    let remaining = tasks
        .into_iter()
        .filter(|task| task.id != id)
        .collect::<Vec<_>>();

    if let Err(error) = write_tasks(&remaining) {
        return server_error(&error.to_string());
    }

    json_response(
        "200 OK",
        &format!(r#"{{"ok":true,"tasks":{}}}"#, tasks_json(&remaining)),
    )
}

fn handle_add_broadcast(body: &str) -> (&'static str, &'static str, String) {
    let requester = sanitize_account_name(&json_field(body, "requester").unwrap_or_default());
    if !is_valid_task_requester(&requester) || is_overview_account(&requester) {
        return json_response("401 Unauthorized", r#"{"ok":false,"error":"Unknown user"}"#);
    }

    let message = sanitize_broadcast_text(&json_field(body, "message").unwrap_or_default(), 500);
    let created_at = sanitize_task_text(&json_field(body, "createdAt").unwrap_or_default(), 40);
    let targets = parse_assignees_field(&json_field(body, "targets").unwrap_or_default());

    if message.is_empty() {
        return json_response(
            "400 Bad Request",
            r#"{"ok":false,"error":"Message required"}"#,
        );
    }

    let mut broadcasts = read_broadcasts();
    broadcasts.push(Broadcast {
        id: create_broadcast_id(),
        message,
        targets,
        requester,
        created_at,
        seen_by: Vec::new(),
    });

    if let Err(error) = write_broadcasts(&broadcasts) {
        return server_error(&error.to_string());
    }

    json_response(
        "200 OK",
        &format!(
            r#"{{"ok":true,"broadcasts":{}}}"#,
            broadcasts_json(&broadcasts)
        ),
    )
}

fn handle_mark_broadcast_seen(body: &str) -> (&'static str, &'static str, String) {
    let requester = sanitize_account_name(&json_field(body, "requester").unwrap_or_default());
    if !is_valid_task_requester(&requester) || is_overview_account(&requester) {
        return json_response("401 Unauthorized", r#"{"ok":false,"error":"Unknown user"}"#);
    }

    let id = sanitize_task_text(&json_field(body, "id").unwrap_or_default(), 100);
    let mut broadcasts = read_broadcasts();
    let Some(broadcast) = broadcasts.iter_mut().find(|broadcast| broadcast.id == id) else {
        return json_response(
            "404 Not Found",
            r#"{"ok":false,"error":"Unknown broadcast"}"#,
        );
    };

    if !broadcast
        .seen_by
        .iter()
        .any(|name| same_name(name, &requester))
    {
        broadcast.seen_by.push(requester);
    }

    if let Err(error) = write_broadcasts(&broadcasts) {
        return server_error(&error.to_string());
    }

    json_response(
        "200 OK",
        &format!(
            r#"{{"ok":true,"broadcasts":{}}}"#,
            broadcasts_json(&broadcasts)
        ),
    )
}

fn handle_add_event(body: &str) -> (&'static str, &'static str, String) {
    let requester = sanitize_account_name(&json_field(body, "requester").unwrap_or_default());
    if !is_valid_task_requester(&requester) || is_overview_account(&requester) {
        return json_response("401 Unauthorized", r#"{"ok":false,"error":"Unknown user"}"#);
    }

    let title = sanitize_task_text(&json_field(body, "title").unwrap_or_default(), 100);
    let start_date = json_field(body, "startDate").unwrap_or_default();
    let start_time = json_field(body, "startTime").unwrap_or_default();
    let end_date = json_field(body, "endDate").unwrap_or_default();
    let end_time = json_field(body, "endTime").unwrap_or_default();
    let created_at = sanitize_task_text(&json_field(body, "createdAt").unwrap_or_default(), 40);
    let note = sanitize_note_text(&json_field(body, "note").unwrap_or_default(), 2000);
    let assignees = parse_assignees_field(&json_field(body, "assignees").unwrap_or_default());

    if title.is_empty()
        || !is_valid_date_key(&start_date)
        || !is_valid_time_value(&start_time)
        || !is_valid_date_key(&end_date)
        || !is_valid_time_value(&end_time)
        || start_time.is_empty()
        || end_time.is_empty()
    {
        return json_response("400 Bad Request", r#"{"ok":false,"error":"Invalid event"}"#);
    }

    let mut events = read_events();
    events.push(EventPlan {
        id: create_event_id(),
        title,
        start_date,
        start_time,
        end_date,
        end_time,
        assignees,
        requester,
        created_at,
        note,
    });

    if let Err(error) = write_events(&events) {
        return server_error(&error.to_string());
    }

    json_response(
        "200 OK",
        &format!(r#"{{"ok":true,"events":{}}}"#, events_json(&events)),
    )
}

fn handle_delete_event(body: &str) -> (&'static str, &'static str, String) {
    let requester = sanitize_account_name(&json_field(body, "requester").unwrap_or_default());
    if !is_valid_task_requester(&requester) || is_overview_account(&requester) {
        return json_response("401 Unauthorized", r#"{"ok":false,"error":"Unknown user"}"#);
    }

    let id = sanitize_task_text(&json_field(body, "id").unwrap_or_default(), 100);
    let groups = read_groups();
    let events = read_events();
    let Some(event) = events.iter().find(|event| event.id == id) else {
        return json_response("404 Not Found", r#"{"ok":false,"error":"Unknown event"}"#);
    };

    if !can_delete_event(event, &requester, &groups) {
        return json_response(
            "403 Forbidden",
            r#"{"ok":false,"error":"Not allowed to delete this event"}"#,
        );
    }

    let remaining = events
        .into_iter()
        .filter(|event| event.id != id)
        .collect::<Vec<_>>();

    if let Err(error) = write_events(&remaining) {
        return server_error(&error.to_string());
    }

    let events = read_events();
    json_response(
        "200 OK",
        &format!(r#"{{"ok":true,"events":{}}}"#, events_json(&events)),
    )
}

fn json_response(status: &'static str, body: &str) -> (&'static str, &'static str, String) {
    (status, "application/json; charset=utf-8", body.to_string())
}

fn server_error(error: &str) -> (&'static str, &'static str, String) {
    json_response(
        "500 Internal Server Error",
        &format!(r#"{{"ok":false,"error":"{}"}}"#, escape_json(error)),
    )
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
        fs::write(ACCOUNTS_FILE, "")?;
    }
    Ok(())
}

fn ensure_groups_file() -> std::io::Result<()> {
    ensure_data_dir()?;
    if !Path::new(GROUPS_FILE).exists() {
        fs::write(GROUPS_FILE, "")?;
    }
    Ok(())
}

fn ensure_tasks_file() -> std::io::Result<()> {
    ensure_data_dir()?;
    if !Path::new(TASKS_FILE).exists() {
        fs::write(TASKS_FILE, "")?;
    }
    Ok(())
}

fn ensure_broadcasts_file() -> std::io::Result<()> {
    ensure_data_dir()?;
    if !Path::new(BROADCASTS_FILE).exists() {
        fs::write(BROADCASTS_FILE, "")?;
    }
    Ok(())
}

fn ensure_events_file() -> std::io::Result<()> {
    ensure_data_dir()?;
    if !Path::new(EVENTS_FILE).exists() {
        fs::write(EVENTS_FILE, "")?;
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

fn read_accounts() -> Vec<Account> {
    fs::read_to_string(ACCOUNTS_FILE)
        .unwrap_or_default()
        .lines()
        .filter_map(parse_account_line)
        .fold(Vec::<Account>::new(), |mut list, account| {
            if !list
                .iter()
                .any(|existing| same_name(&existing.name, &account.name))
            {
                list.push(account);
            }
            list
        })
}

fn write_accounts(accounts: &[Account]) -> std::io::Result<()> {
    ensure_data_dir()?;
    let body = accounts
        .iter()
        .map(|account| format!("{}\t{}", account.name, account.pin_hash))
        .collect::<Vec<_>>()
        .join("\n");
    fs::write(
        ACCOUNTS_FILE,
        if body.is_empty() {
            body
        } else {
            format!("{body}\n")
        },
    )
}

fn parse_account_line(line: &str) -> Option<Account> {
    let (name, pin_hash) = line.split_once('\t')?;
    let name = sanitize_account_name(name);
    let pin_hash = pin_hash.trim().to_string();
    if name.is_empty() || pin_hash.is_empty() {
        None
    } else {
        Some(Account { name, pin_hash })
    }
}

fn read_groups() -> Vec<Group> {
    let accounts = read_accounts();
    fs::read_to_string(GROUPS_FILE)
        .unwrap_or_default()
        .lines()
        .filter_map(|line| parse_group_line(line, &accounts))
        .fold(Vec::<Group>::new(), |mut list, group| {
            if !list
                .iter()
                .any(|existing| same_name(&existing.name, &group.name))
            {
                list.push(group);
            }
            list
        })
}

fn write_groups(groups: &[Group]) -> std::io::Result<()> {
    ensure_data_dir()?;
    let body = groups
        .iter()
        .map(|group| format!("{}\t{}", group.name, group.members.join(",")))
        .collect::<Vec<_>>()
        .join("\n");
    fs::write(
        GROUPS_FILE,
        if body.is_empty() {
            body
        } else {
            format!("{body}\n")
        },
    )
}

fn parse_group_line(line: &str, accounts: &[Account]) -> Option<Group> {
    let (name, members) = line.split_once('\t').unwrap_or((line, ""));
    let name = sanitize_account_name(name);
    if name.is_empty() {
        return None;
    }
    let members = members
        .split(',')
        .map(sanitize_account_name)
        .filter(|member| {
            !member.is_empty()
                && accounts
                    .iter()
                    .any(|account| same_name(&account.name, member))
        })
        .fold(Vec::<String>::new(), |mut list, member| {
            if !list.iter().any(|existing| same_name(existing, &member)) {
                list.push(member);
            }
            list
        });
    Some(Group { name, members })
}

fn read_tasks() -> Vec<Task> {
    fs::read_to_string(TASKS_FILE)
        .unwrap_or_default()
        .lines()
        .filter_map(parse_task_line)
        .collect()
}

fn write_tasks(tasks: &[Task]) -> std::io::Result<()> {
    ensure_data_dir()?;
    if Path::new(TASKS_FILE).exists() {
        let _ = fs::copy(TASKS_FILE, "data/tasks.backup.txt");
    }
    let body = tasks
        .iter()
        .map(|task| {
            format!(
                "{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}",
                task.id,
                task.title,
                task.date,
                task.time,
                task.assignees.join(","),
                task.requester,
                task.created_at,
                task.note
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    fs::write(
        TASKS_FILE,
        if body.is_empty() {
            body
        } else {
            format!("{body}\n")
        },
    )
}

fn parse_task_line(line: &str) -> Option<Task> {
    let parts = line.split('\t').collect::<Vec<_>>();
    if parts.len() != 7 && parts.len() != 8 {
        return None;
    }
    let id = sanitize_task_text(parts[0], 100);
    let title = sanitize_task_text(parts[1], 100);
    let date = parts[2].to_string();
    let time = parts[3].to_string();
    let assignees = parse_assignees_field(parts[4]);
    let requester = sanitize_account_name(parts[5]);
    let created_at = sanitize_task_text(parts[6], 40);
    let note = parts
        .get(7)
        .map(|value| sanitize_note_text(value, 2000))
        .unwrap_or_default();

    if id.is_empty() || title.is_empty() || !is_valid_date_key(&date) || !is_valid_time_value(&time)
    {
        None
    } else {
        Some(Task {
            id,
            title,
            date,
            time,
            assignees,
            requester,
            created_at,
            note,
        })
    }
}

fn tasks_json(tasks: &[Task]) -> String {
    format!(
        "[{}]",
        tasks
            .iter()
            .map(|task| {
                format!(
                    r#"{{"id":"{}","title":"{}","date":"{}","time":"{}","assignees":{},"requester":"{}","createdAt":"{}","note":"{}"}}"#,
                    escape_json(&task.id),
                    escape_json(&task.title),
                    escape_json(&task.date),
                    escape_json(&task.time),
                    string_array_json(&task.assignees),
                    escape_json(&task.requester),
                    escape_json(&task.created_at),
                    escape_json(&task.note)
                )
            })
            .collect::<Vec<_>>()
            .join(",")
    )
}

fn read_broadcasts() -> Vec<Broadcast> {
    fs::read_to_string(BROADCASTS_FILE)
        .unwrap_or_default()
        .lines()
        .filter_map(parse_broadcast_line)
        .collect()
}

fn write_broadcasts(broadcasts: &[Broadcast]) -> std::io::Result<()> {
    ensure_data_dir()?;
    if Path::new(BROADCASTS_FILE).exists() {
        let _ = fs::copy(BROADCASTS_FILE, "data/broadcasts.backup.txt");
    }
    let body = broadcasts
        .iter()
        .map(|broadcast| {
            format!(
                "{}\t{}\t{}\t{}\t{}\t{}",
                broadcast.id,
                broadcast.message,
                broadcast.targets.join(","),
                broadcast.requester,
                broadcast.created_at,
                broadcast.seen_by.join(",")
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    fs::write(
        BROADCASTS_FILE,
        if body.is_empty() {
            body
        } else {
            format!("{body}\n")
        },
    )
}

fn parse_broadcast_line(line: &str) -> Option<Broadcast> {
    let parts = line.split('\t').collect::<Vec<_>>();
    if parts.len() != 6 {
        return None;
    }
    let id = sanitize_task_text(parts[0], 100);
    let message = sanitize_broadcast_text(parts[1], 500);
    let targets = parse_assignees_field(parts[2]);
    let requester = sanitize_account_name(parts[3]);
    let created_at = sanitize_task_text(parts[4], 40);
    let seen_by = parts[5]
        .split(',')
        .map(sanitize_account_name)
        .filter(|name| !name.is_empty())
        .fold(Vec::<String>::new(), |mut list, name| {
            if !list.iter().any(|existing| same_name(existing, &name)) {
                list.push(name);
            }
            list
        });

    if id.is_empty() || message.is_empty() {
        None
    } else {
        Some(Broadcast {
            id,
            message,
            targets,
            requester,
            created_at,
            seen_by,
        })
    }
}

fn broadcasts_json(broadcasts: &[Broadcast]) -> String {
    format!(
        "[{}]",
        broadcasts
            .iter()
            .map(|broadcast| {
                format!(
                    r#"{{"id":"{}","message":"{}","targets":{},"requester":"{}","createdAt":"{}","seenBy":{}}}"#,
                    escape_json(&broadcast.id),
                    escape_json(&broadcast.message),
                    string_array_json(&broadcast.targets),
                    escape_json(&broadcast.requester),
                    escape_json(&broadcast.created_at),
                    string_array_json(&broadcast.seen_by)
                )
            })
            .collect::<Vec<_>>()
            .join(",")
    )
}

fn read_events() -> Vec<EventPlan> {
    fs::read_to_string(EVENTS_FILE)
        .unwrap_or_default()
        .lines()
        .filter_map(parse_event_line)
        .collect()
}

fn write_events(events: &[EventPlan]) -> std::io::Result<()> {
    ensure_data_dir()?;
    if Path::new(EVENTS_FILE).exists() {
        let _ = fs::copy(EVENTS_FILE, "data/events.backup.txt");
    }
    let body = events
        .iter()
        .map(|event| {
            format!(
                "{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}",
                event.id,
                event.title,
                event.start_date,
                event.start_time,
                event.end_date,
                event.end_time,
                event.assignees.join(","),
                event.requester,
                event.created_at,
                event.note
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    let body = if body.is_empty() {
        body
    } else {
        format!("{body}\n")
    };
    let temp_file = format!("{EVENTS_FILE}.tmp");
    fs::write(&temp_file, body)?;
    fs::rename(temp_file, EVENTS_FILE)
}

fn parse_event_line(line: &str) -> Option<EventPlan> {
    let parts = line.split('\t').collect::<Vec<_>>();
    if parts.len() != 10 {
        return None;
    }
    let id = sanitize_task_text(parts[0], 100);
    let title = sanitize_task_text(parts[1], 100);
    let start_date = parts[2].to_string();
    let start_time = parts[3].to_string();
    let end_date = parts[4].to_string();
    let end_time = parts[5].to_string();
    let assignees = parse_assignees_field(parts[6]);
    let requester = sanitize_account_name(parts[7]);
    let created_at = sanitize_task_text(parts[8], 40);
    let note = sanitize_note_text(parts[9], 2000);

    if id.is_empty()
        || title.is_empty()
        || !is_valid_date_key(&start_date)
        || !is_valid_time_value(&start_time)
        || !is_valid_date_key(&end_date)
        || !is_valid_time_value(&end_time)
        || start_time.is_empty()
        || end_time.is_empty()
    {
        None
    } else {
        Some(EventPlan {
            id,
            title,
            start_date,
            start_time,
            end_date,
            end_time,
            assignees,
            requester,
            created_at,
            note,
        })
    }
}

fn events_json(events: &[EventPlan]) -> String {
    format!(
        "[{}]",
        events
            .iter()
            .map(|event| {
                format!(
                    r#"{{"id":"{}","title":"{}","startDate":"{}","startTime":"{}","endDate":"{}","endTime":"{}","assignees":{},"requester":"{}","createdAt":"{}","note":"{}"}}"#,
                    escape_json(&event.id),
                    escape_json(&event.title),
                    escape_json(&event.start_date),
                    escape_json(&event.start_time),
                    escape_json(&event.end_date),
                    escape_json(&event.end_time),
                    string_array_json(&event.assignees),
                    escape_json(&event.requester),
                    escape_json(&event.created_at),
                    escape_json(&event.note)
                )
            })
            .collect::<Vec<_>>()
            .join(",")
    )
}

fn parse_assignees_field(value: &str) -> Vec<String> {
    value
        .split(',')
        .map(|name| {
            if let Some(group_name) = name.trim().strip_prefix('@') {
                format!("@{}", sanitize_account_name(group_name))
            } else {
                sanitize_account_name(name)
            }
        })
        .filter(|name| !name.is_empty() && name != "@")
        .fold(Vec::<String>::new(), |mut list, name| {
            if !list.iter().any(|existing| same_name(existing, &name)) {
                list.push(name);
            }
            list
        })
}

fn sanitize_task_text(value: &str, max_len: usize) -> String {
    value
        .trim()
        .chars()
        .filter(|character| !matches!(character, '\t' | '\n' | '\r'))
        .take(max_len)
        .collect()
}

fn sanitize_note_text(value: &str, max_len: usize) -> String {
    value
        .trim()
        .chars()
        .map(|character| {
            if matches!(character, '\t' | '\n' | '\r') {
                ' '
            } else {
                character
            }
        })
        .take(max_len)
        .collect()
}

fn sanitize_broadcast_text(value: &str, max_len: usize) -> String {
    value
        .trim()
        .chars()
        .map(|character| {
            if matches!(character, '\t' | '\n' | '\r') {
                ' '
            } else {
                character
            }
        })
        .take(max_len)
        .collect()
}

fn is_valid_task_requester(name: &str) -> bool {
    is_overview_account(name)
        || read_accounts()
            .iter()
            .any(|account| same_name(&account.name, name))
}

fn is_valid_date_key(value: &str) -> bool {
    let bytes = value.as_bytes();
    bytes.len() == 10
        && bytes[4] == b'-'
        && bytes[7] == b'-'
        && bytes
            .iter()
            .enumerate()
            .all(|(index, byte)| matches!(index, 4 | 7) || byte.is_ascii_digit())
}

fn is_valid_time_value(value: &str) -> bool {
    if value.is_empty() {
        return true;
    }
    let Some((hour, minute)) = value.split_once(':') else {
        return false;
    };
    let Ok(hour) = hour.parse::<u8>() else {
        return false;
    };
    let Ok(minute) = minute.parse::<u8>() else {
        return false;
    };
    hour < 24 && minute < 60
}

fn create_server_id() -> String {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or_default();
    format!("task-{now}")
}

fn create_broadcast_id() -> String {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or_default();
    format!("broadcast-{now}")
}

fn create_event_id() -> String {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or_default();
    format!("event-{now}")
}

fn can_delete_task(task: &Task, requester: &str, groups: &[Group]) -> bool {
    if same_name(&task.requester, requester) {
        return true;
    }
    if task.assignees.is_empty() {
        return true;
    }
    task.assignees.iter().any(|assignee| {
        if let Some(group_name) = assignee.strip_prefix('@') {
            is_member_of_group(requester, group_name, groups)
        } else {
            same_name(assignee, requester)
        }
    })
}

fn can_delete_event(event: &EventPlan, requester: &str, groups: &[Group]) -> bool {
    if same_name(&event.requester, requester) {
        return true;
    }
    if event.assignees.is_empty() {
        return true;
    }
    event.assignees.iter().any(|assignee| {
        if let Some(group_name) = assignee.strip_prefix('@') {
            is_member_of_group(requester, group_name, groups)
        } else {
            same_name(assignee, requester)
        }
    })
}

fn is_member_of_group(account_name: &str, group_name: &str, groups: &[Group]) -> bool {
    groups
        .iter()
        .find(|group| same_name(&group.name, group_name))
        .map(|group| {
            group
                .members
                .iter()
                .any(|member| same_name(member, account_name))
        })
        .unwrap_or(false)
}

fn remove_account_from_groups(account_name: &str) {
    let mut groups = read_groups();
    for group in &mut groups {
        group
            .members
            .retain(|member| !same_name(member, account_name));
    }
    if let Err(error) = write_groups(&groups) {
        eprintln!("failed to update groups: {error}");
    }
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

fn accounts_json(accounts: &[Account]) -> String {
    let mut names = vec![OVERVIEW_ACCOUNT.to_string()];
    names.extend(accounts.iter().map(|account| account.name.clone()));
    string_array_json(&names)
}

fn is_overview_account(name: &str) -> bool {
    same_name(name, OVERVIEW_ACCOUNT)
}

fn groups_json(groups: &[Group]) -> String {
    format!(
        "[{}]",
        groups
            .iter()
            .map(|group| {
                format!(
                    r#"{{"name":"{}","members":{}}}"#,
                    escape_json(&group.name),
                    string_array_json(&group.members)
                )
            })
            .collect::<Vec<_>>()
            .join(",")
    )
}

fn string_array_json(values: &[String]) -> String {
    format!(
        "[{}]",
        values
            .iter()
            .map(|value| format!(r#""{}""#, escape_json(value)))
            .collect::<Vec<_>>()
            .join(",")
    )
}

fn is_valid_pin(pin: &str) -> bool {
    pin.len() == 4 && pin.chars().all(|character| character.is_ascii_digit())
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

const INDEX_HTML: &str = include_str!("index.html");

const STYLES_CSS: &str = include_str!("styles.css");

const APP_JS: &str = include_str!("app.js");
