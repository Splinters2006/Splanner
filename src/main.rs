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
        return json_response("403 Forbidden", r#"{"ok":false,"error":"Not allowed to delete this task"}"#);
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
        return json_response("400 Bad Request", r#"{"ok":false,"error":"Message required"}"#);
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
        &format!(r#"{{"ok":true,"broadcasts":{}}}"#, broadcasts_json(&broadcasts)),
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
        return json_response("404 Not Found", r#"{"ok":false,"error":"Unknown broadcast"}"#);
    };

    if !broadcast.seen_by.iter().any(|name| same_name(name, &requester)) {
        broadcast.seen_by.push(requester);
    }

    if let Err(error) = write_broadcasts(&broadcasts) {
        return server_error(&error.to_string());
    }

    json_response(
        "200 OK",
        &format!(r#"{{"ok":true,"broadcasts":{}}}"#, broadcasts_json(&broadcasts)),
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
        return json_response("403 Forbidden", r#"{"ok":false,"error":"Not allowed to delete this event"}"#);
    }

    let remaining = events
        .into_iter()
        .filter(|event| event.id != id)
        .collect::<Vec<_>>();

    if let Err(error) = write_events(&remaining) {
        return server_error(&error.to_string());
    }

    json_response(
        "200 OK",
        &format!(r#"{{"ok":true,"events":{}}}"#, events_json(&remaining)),
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
    let note = parts.get(7).map(|value| sanitize_note_text(value, 2000)).unwrap_or_default();

    if id.is_empty() || title.is_empty() || !is_valid_date_key(&date) || !is_valid_time_value(&time) {
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
    fs::write(
        EVENTS_FILE,
        if body.is_empty() {
            body
        } else {
            format!("{body}\n")
        },
    )
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
        .map(|character| if matches!(character, '\t' | '\n' | '\r') { ' ' } else { character })
        .take(max_len)
        .collect()
}

fn sanitize_broadcast_text(value: &str, max_len: usize) -> String {
    value
        .trim()
        .chars()
        .map(|character| if matches!(character, '\t' | '\n' | '\r') { ' ' } else { character })
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
        .map(|group| group.members.iter().any(|member| same_name(member, account_name)))
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

const INDEX_HTML: &str = r#"<!doctype html>
<html lang="en">
<head>
  <meta charset="utf-8">
  <meta name="viewport" content="width=device-width, initial-scale=1, viewport-fit=cover">
  <title>Splanner</title>
  <link rel="stylesheet" href="/styles.css">
</head>
<body>
  <section id="login-screen" class="login-screen" aria-label="Sign in">
    <form id="user-login" class="login-form" autocomplete="off">
      <h1>Splanner</h1>
      <label>
        <span>Name</span>
        <select id="login-name" name="name" required></select>
      </label>
      <label>
        <span>PIN</span>
        <input id="login-pin" name="pin" type="password" inputmode="numeric" autocomplete="off" required>
      </label>
      <button type="submit">Open planner</button>
      <button id="login-admin-open" class="plain-button" type="button">Admin settings</button>
      <p id="login-message" class="admin-message" role="status"></p>
    </form>
  </section>

  <main id="app-shell" class="app-shell" hidden>
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
      <section class="filter-panel" aria-label="Task filter">
        <label>
          <span>Show tasks for</span>
          <select id="task-filter"></select>
        </label>
      </section>
      <nav class="week-actions" aria-label="Week navigation">
        <button id="prev-week" class="icon-button" type="button" aria-label="Previous week">&lsaquo;</button>
        <button id="today" class="text-button" type="button">Today</button>
        <button id="next-week" class="icon-button" type="button" aria-label="Next week">&rsaquo;</button>
        <button id="admin-open" class="icon-button secondary admin-cog" type="button" aria-label="Admin settings">&#9881;</button>
      </nav>
    </header>

    <section id="broadcast-panel" class="broadcast-panel" aria-label="Family broadcasts">
      <div id="broadcast-list" class="broadcast-list"></div>
    </section>

    <section class="planner-stage" aria-label="Weekly planner">
      <div class="member-rail" id="member-rail" aria-label="Family members"></div>
      <section id="week-grid" class="week-grid" aria-live="polite"></section>
    </section>

    <button id="create-open" class="floating-add" type="button" aria-label="Create">+</button>

    <dialog id="create-dialog" class="create-dialog">
      <header class="dialog-header">
        <h2>Create</h2>
        <button id="create-close" class="plain-button" type="button" aria-label="Close create menu">Close</button>
      </header>

      <label class="create-type-field">
        <span>Type</span>
        <select id="create-type">
          <option value="task">Task</option>
          <option value="broadcast">Broadcast</option>
          <option value="event">Event</option>
        </select>
      </label>

      <form id="task-form" class="create-form" autocomplete="off">
        <label>
          <span>Task name</span>
          <input id="task-title" name="title" required maxlength="100">
        </label>
        <label>
          <span>By date</span>
          <select id="task-day" name="day"></select>
        </label>
        <label>
          <span>By hour</span>
          <input id="task-hour" name="hour" inputmode="numeric" pattern="[0-9]{1,2}" maxlength="2">
        </label>
        <label>
          <span>By minute</span>
          <select id="task-minute" name="minute"></select>
        </label>
        <fieldset class="person-field">
          <legend>For</legend>
          <div id="task-assignees" class="person-options"></div>
        </fieldset>
        <label class="note-field">
          <span>Note</span>
          <textarea id="task-note" name="note" maxlength="2000"></textarea>
        </label>
        <button type="submit">Add task</button>
      </form>

      <form id="broadcast-form" class="create-form" autocomplete="off" hidden>
        <label class="wide-field">
          <span>Broadcast message</span>
          <textarea id="broadcast-message" name="message" maxlength="500" required></textarea>
        </label>
        <fieldset class="person-field">
          <legend>To</legend>
          <div id="broadcast-targets" class="person-options"></div>
        </fieldset>
        <button type="submit">Send broadcast</button>
      </form>

      <form id="event-form" class="create-form" autocomplete="off" hidden>
        <label>
          <span>Event name</span>
          <input id="event-title" name="title" required maxlength="100">
        </label>
        <label>
          <span>Start date</span>
          <select id="event-start-day" name="startDay"></select>
        </label>
        <label>
          <span>Start hour</span>
          <input id="event-start-hour" name="startHour" inputmode="numeric" pattern="[0-9]{1,2}" maxlength="2">
        </label>
        <label>
          <span>Start minute</span>
          <select id="event-start-minute" name="startMinute"></select>
        </label>
        <label>
          <span>End date</span>
          <select id="event-end-day" name="endDay"></select>
        </label>
        <label>
          <span>End hour</span>
          <input id="event-end-hour" name="endHour" inputmode="numeric" pattern="[0-9]{1,2}" maxlength="2">
        </label>
        <label>
          <span>End minute</span>
          <select id="event-end-minute" name="endMinute"></select>
        </label>
        <fieldset class="person-field">
          <legend>For</legend>
          <div id="event-assignees" class="person-options"></div>
        </fieldset>
        <label class="note-field">
          <span>Note</span>
          <textarea id="event-note" name="note" maxlength="2000"></textarea>
        </label>
        <button type="submit">Add event</button>
      </form>
    </dialog>
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
          <input id="account-name" name="name" maxlength="32" required>
        </label>
        <label>
          <span>PIN</span>
          <input id="account-pin" name="pin" type="password" inputmode="numeric" autocomplete="off" pattern="[0-9]{4}" maxlength="4" required>
        </label>
        <button type="submit">Add account</button>
      </form>
      <div id="account-list" class="account-list"></div>

      <section class="group-admin">
        <h3>Groups</h3>
        <form id="group-form" class="admin-form" autocomplete="off">
          <label>
            <span>New group</span>
            <input id="group-name" name="name" maxlength="32" required>
          </label>
          <button type="submit">Add group</button>
        </form>
        <div id="group-list" class="group-list"></div>
      </section>
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

[hidden] {
  display: none !important;
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

.login-screen {
  min-height: 100vh;
  display: grid;
  place-items: center;
  padding: 18px;
}

.login-form {
  width: min(420px, 100%);
  display: grid;
  grid-template-columns: 1fr;
  gap: 14px;
  background: var(--panel);
  border: 1px solid var(--line);
  border-radius: 8px;
  box-shadow: var(--shadow);
  padding: 22px;
}

.login-form h1 {
  font-size: 3rem;
}

.app-shell {
  min-height: 100vh;
  display: grid;
  grid-template-rows: auto 1fr auto;
  gap: 18px;
  padding: clamp(16px, 3vw, 32px);
}

.viewer-mode {
  grid-template-rows: auto 1fr;
}

.viewer-mode .quick-add,
.viewer-mode .member-rail,
.viewer-mode .task-actions,
.viewer-mode .broadcast-form,
.viewer-mode .admin-cog {
  display: none;
}

.viewer-mode .planner-stage {
  grid-template-columns: 1fr;
}

.viewer-mode .day-column {
  min-height: 70vh;
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
h2,
h3 {
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

h3 {
  font-size: 1.15rem;
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
.login-form button,
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

.time-strip {
  display: grid;
  grid-template-columns: repeat(4, 1fr);
  gap: 4px;
  margin-top: 10px;
}

.time-strip span {
  min-height: 24px;
  display: grid;
  place-items: center;
  border-radius: 6px;
  background: #eef3ef;
  color: var(--muted);
  font-size: 0.75rem;
  font-weight: 850;
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

.time-chip {
  background: var(--ink);
  color: #fff;
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
  grid-template-columns: minmax(180px, 2fr) minmax(120px, 1fr) minmax(100px, 0.7fr) minmax(100px, 0.7fr) minmax(220px, 2fr) auto;
  gap: 10px;
  align-items: end;
}

label,
.person-field {
  display: grid;
  gap: 6px;
  font-weight: 750;
}

fieldset {
  min-width: 0;
  margin: 0;
  padding: 0;
  border: 0;
}

legend {
  padding: 0;
  color: var(--muted);
}

.person-options {
  display: flex;
  min-height: 52px;
  align-items: center;
  gap: 8px;
  flex-wrap: wrap;
  border: 1px solid var(--line);
  border-radius: 8px;
  background: #f9fbf8;
  padding: 7px;
}

.person-option {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  min-height: 36px;
  border-radius: 999px;
  background: #e8eeee;
  padding: 0 10px;
}

.person-option input {
  width: 18px;
  min-height: 18px;
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

.account-list,
.group-list {
  display: grid;
  gap: 8px;
  margin-top: 14px;
}

.account-row,
.group-row {
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

.account-row button,
.group-row button {
  min-height: 38px;
  border-radius: 8px;
  padding: 0 12px;
  background: #edf1ef;
  color: var(--muted);
  font-weight: 800;
}

.group-admin {
  margin-top: 22px;
}

.group-row {
  display: grid;
  align-items: start;
}

.group-row header {
  display: flex;
  justify-content: space-between;
  gap: 10px;
}

.group-members {
  display: flex;
  flex-wrap: wrap;
  gap: 8px;
  margin-top: 10px;
}



.filter-panel {
  min-width: min(260px, 100%);
}

.filter-panel label {
  display: grid;
  gap: 6px;
}

.member.current-member {
  border-color: var(--accent);
  box-shadow: 0 0 0 3px rgba(47, 111, 99, 0.22), var(--shadow);
}

.note-field {
  grid-column: 1 / -1;
}

textarea {
  width: 100%;
  min-height: 84px;
  border: 1px solid var(--line);
  border-radius: 8px;
  background: #f9fbf8;
  color: var(--ink);
  padding: 10px 12px;
  resize: vertical;
  font: inherit;
}

.task-card {
  min-width: 0;
  height: auto;
}

.task-card.has-note {
  cursor: default;
}

.task-meta,
.task-actions {
  min-width: 0;
}

.chip {
  max-width: 100%;
  white-space: normal;
  overflow-wrap: anywhere;
}

.task-actions {
  gap: 8px;
}

.note-toggle {
  min-width: 38px;
  min-height: 34px;
  border-radius: 8px;
  background: #edf1ef;
  color: var(--ink);
  font-size: 1.1rem;
  font-weight: 900;
  line-height: 1;
}

.task-note-panel {
  border-radius: 8px;
  background: #eef3ef;
  color: var(--ink);
  padding: 10px;
  overflow-wrap: anywhere;
  white-space: pre-wrap;
  font-size: 0.95rem;
  line-height: 1.35;
}

.task-note-panel a {
  color: var(--accent-strong);
  font-weight: 800;
}



.broadcast-panel {
  display: grid;
  gap: 10px;
}

.broadcast-list {
  display: grid;
  gap: 10px;
}

.broadcast-list:empty {
  display: none;
}

.broadcast-card {
  display: grid;
  gap: 8px;
  padding: 12px 14px;
  border-radius: 8px;
  border: 1px solid rgba(31, 85, 76, 0.25);
  background: #fff8df;
  box-shadow: var(--shadow);
}

.broadcast-card header {
  display: flex;
  justify-content: space-between;
  gap: 10px;
  align-items: start;
}

.broadcast-card strong {
  font-weight: 900;
}

.broadcast-message {
  margin: 0;
  overflow-wrap: anywhere;
  white-space: pre-wrap;
  line-height: 1.35;
}

.broadcast-message a {
  color: var(--accent-strong);
  font-weight: 850;
}

.broadcast-meta {
  display: flex;
  flex-wrap: wrap;
  gap: 6px;
  color: var(--muted);
  font-weight: 750;
}

.broadcast-seen {
  min-height: 38px;
  border-radius: 8px;
  padding: 0 14px;
  background: var(--accent);
  color: #fff;
  font-weight: 850;
}

.broadcast-form {
  grid-template-columns: minmax(220px, 2fr) minmax(220px, 2fr) auto;
  background: var(--panel);
  border: 1px solid var(--line);
  border-radius: 8px;
  box-shadow: var(--shadow);
  padding: 12px;
}

.broadcast-form button {
  min-height: 52px;
  border-radius: 8px;
  padding: 0 22px;
  background: var(--accent-strong);
  color: #fff;
  font-weight: 850;
}

.broadcast-target-field {
  min-width: 0;
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

  .week-actions,
  .filter-panel {
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

  form,
  .broadcast-form {
    grid-template-columns: 1fr 1fr;
  }

  form label:first-child,
  .broadcast-message-field,
  .broadcast-target-field,
  .person-field,
  .quick-add button,
  .broadcast-form button {
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
/* Floating creation menu */
.floating-add {
  position: fixed;
  right: max(22px, env(safe-area-inset-right));
  bottom: max(22px, env(safe-area-inset-bottom));
  z-index: 60;
  width: 68px;
  min-height: 68px;
  border-radius: 999px;
  background: var(--accent);
  color: #fff;
  font-size: 2.4rem;
  font-weight: 900;
  box-shadow: var(--shadow);
}

.create-dialog {
  width: min(820px, calc(100vw - 28px));
  max-height: min(840px, calc(100vh - 28px));
  overflow: auto;
  padding: 18px;
  background: var(--panel);
  border: 1px solid var(--line);
  border-radius: 12px;
  box-shadow: var(--shadow);
}

.create-dialog[open] {
  position: fixed;
  inset: 50% auto auto 50%;
  transform: translate(-50%, -50%);
  z-index: 70;
}

.create-dialog::backdrop {
  background: rgba(29, 35, 40, 0.34);
}

.create-type-field {
  margin-bottom: 12px;
}

.create-form {
  display: grid;
  grid-template-columns: repeat(4, minmax(0, 1fr));
  gap: 10px;
  align-items: end;
}

.create-form .person-field,
.create-form .note-field,
.create-form .wide-field,
.create-form button {
  grid-column: 1 / -1;
}

.create-form textarea {
  width: 100%;
  min-height: 92px;
  border: 1px solid var(--line);
  border-radius: 8px;
  background: #f9fbf8;
  color: var(--ink);
  padding: 12px;
  resize: vertical;
  font: inherit;
}

.create-form button {
  min-height: 56px;
  border-radius: 8px;
  background: var(--accent);
  color: #fff;
  font-weight: 800;
}

.task-card,
.event-card,
.broadcast-card,
.chip {
  min-width: 0;
  overflow-wrap: anywhere;
  word-break: break-word;
}

.task-meta .chip,
.event-meta .chip,
.broadcast-meta .chip {
  max-width: 100%;
  white-space: normal;
}

.event-card {
  border-left: 6px solid var(--blue);
  border-radius: 8px;
  background: #f6f9fc;
  padding: 12px;
  min-height: 92px;
  display: grid;
  gap: 9px;
}

.event-title {
  margin: 0;
  font-size: clamp(1rem, 1.4vw, 1.18rem);
  font-weight: 850;
  overflow-wrap: anywhere;
}

.event-meta {
  display: flex;
  flex-wrap: wrap;
  gap: 6px;
  color: var(--muted);
  font-size: 0.9rem;
  font-weight: 700;
}

.event-actions {
  display: flex;
  gap: 8px;
  justify-content: flex-end;
}

.delete-event,
.note-toggle,
.event-note-toggle {
  width: 38px;
  min-height: 34px;
  border-radius: 8px;
  background: #edf1ef;
  color: var(--muted);
  font-size: 1.35rem;
  line-height: 1;
}

.event-note-panel {
  border-top: 1px solid var(--line);
  padding-top: 8px;
  color: var(--ink);
}

.broadcast-seen {
  width: 38px;
  min-height: 34px;
  border-radius: 8px;
  background: #edf1ef;
  color: var(--muted);
  font-size: 1.35rem;
  line-height: 1;
}

@media (max-width: 640px) {
  .create-form {
    grid-template-columns: 1fr 1fr;
  }
}

"#;

const APP_JS: &str = r##"const LEGACY_STORAGE_KEY = "splanner.tasks.v1";
const ACCOUNT_COLORS = ["#2f6f63", "#4b7fb8", "#d75b62", "#f1b84b", "#7a6fbe", "#bf6b45"];

const state = {
  weekStart: startOfWeek(new Date()),
  tasks: [],
  events: [],
  broadcasts: [],
  accounts: [],
  groups: [],
  adminPassword: "",
  currentUser: sessionStorage.getItem("splanner.currentUser") || "",
  isViewer: false,
  hostClockOffsetMs: 0,
  overviewResetTimer: null,
  taskFilter: sessionStorage.getItem("splanner.taskFilter") || "all",
  lastActivityAt: Date.now(),
  idleRefreshInFlight: false,
};

const IDLE_REFRESH_MS = 60 * 1000;

const loginScreen = document.querySelector("#login-screen");
const appShell = document.querySelector("#app-shell");
const userLogin = document.querySelector("#user-login");
const loginName = document.querySelector("#login-name");
const loginPin = document.querySelector("#login-pin");
const loginMessage = document.querySelector("#login-message");
const grid = document.querySelector("#week-grid");
const weekTitle = document.querySelector("#week-title");
const weekRange = document.querySelector("#week-range");
const hostTime = document.querySelector("#host-time");
const hostDate = document.querySelector("#host-date");
const taskDay = document.querySelector("#task-day");
const taskHour = document.querySelector("#task-hour");
const taskMinute = document.querySelector("#task-minute");
const taskNote = document.querySelector("#task-note");
const eventForm = document.querySelector("#event-form");
const eventTitle = document.querySelector("#event-title");
const eventStartDay = document.querySelector("#event-start-day");
const eventStartHour = document.querySelector("#event-start-hour");
const eventStartMinute = document.querySelector("#event-start-minute");
const eventEndDay = document.querySelector("#event-end-day");
const eventEndHour = document.querySelector("#event-end-hour");
const eventEndMinute = document.querySelector("#event-end-minute");
const eventAssignees = document.querySelector("#event-assignees");
const eventNote = document.querySelector("#event-note");
const createOpen = document.querySelector("#create-open");
const createDialog = document.querySelector("#create-dialog");
const createClose = document.querySelector("#create-close");
const createType = document.querySelector("#create-type");
const taskFilter = document.querySelector("#task-filter");
const broadcastList = document.querySelector("#broadcast-list");
const broadcastForm = document.querySelector("#broadcast-form");
const broadcastMessage = document.querySelector("#broadcast-message");
const broadcastTargets = document.querySelector("#broadcast-targets");
const taskAssignees = document.querySelector("#task-assignees");
const form = document.querySelector("#task-form");
const memberRail = document.querySelector("#member-rail");
const adminDialog = document.querySelector("#admin-dialog");
const adminLogin = document.querySelector("#admin-login");
const adminPanel = document.querySelector("#admin-panel");
const adminPassword = document.querySelector("#admin-password");
const adminMessage = document.querySelector("#admin-message");
const accountForm = document.querySelector("#account-form");
const accountName = document.querySelector("#account-name");
const accountPin = document.querySelector("#account-pin");
const accountList = document.querySelector("#account-list");
const groupForm = document.querySelector("#group-form");
const groupName = document.querySelector("#group-name");
const groupList = document.querySelector("#group-list");

userLogin.addEventListener("submit", async (event) => {
  event.preventDefault();
  const response = await apiPost("/api/user/login", {
    name: loginName.value,
    pin: loginPin.value,
  });
  if (!response.ok) {
    loginMessage.textContent = response.error || "Wrong name or PIN";
    return;
  }
  state.currentUser = response.name;
  state.isViewer = isViewerAccount(response.name);
  sessionStorage.setItem("splanner.currentUser", state.currentUser);
  loginPin.value = "";
  loginMessage.textContent = "";
  await loadTasks();
  await loadEvents();
  await loadBroadcasts();
  showPlanner();
});

document.querySelector("#prev-week").addEventListener("click", () => shiftWeek(-1));
document.querySelector("#next-week").addEventListener("click", () => shiftWeek(1));
document.querySelector("#today").addEventListener("click", () => {
  markOverviewInteraction();
  state.weekStart = startOfWeek(hostNow());
  render();
});

taskFilter.addEventListener("change", () => {
  state.taskFilter = taskFilter.value || "all";
  sessionStorage.setItem("splanner.taskFilter", state.taskFilter);
  markOverviewInteraction();
  renderWeekGrid();
});

createOpen.addEventListener("click", () => openCreateDialog());
createClose.addEventListener("click", () => closeCreateDialog());
createType.addEventListener("change", renderCreateType);

function openCreateDialog() {
  renderCreateType();
  if (typeof createDialog.showModal === "function") {
    createDialog.showModal();
  } else {
    createDialog.setAttribute("open", "");
  }
}

function closeCreateDialog() {
  if (typeof createDialog.close === "function") {
    createDialog.close();
  } else {
    createDialog.removeAttribute("open");
  }
}

function renderCreateType() {
  const type = createType.value || "task";
  form.hidden = type !== "task";
  broadcastForm.hidden = type !== "broadcast";
  eventForm.hidden = type !== "event";
}

broadcastForm.addEventListener("submit", async (event) => {
  event.preventDefault();
  if (state.isViewer || !state.currentUser) return;
  const message = broadcastMessage.value.trim();
  if (!message) return;

  const response = await apiPost("/api/broadcasts", {
    message,
    targets: getSelectedBroadcastTargets().join(","),
    requester: state.currentUser,
    createdAt: new Date().toISOString(),
  });
  if (!response.ok) {
    alert(response.error || "Could not send broadcast");
    return;
  }
  state.broadcasts = normalizeBroadcasts(response.broadcasts);
  broadcastForm.reset();
  closeCreateDialog();
  renderBroadcasts();
});

broadcastList.addEventListener("click", async (event) => {
  const button = event.target.closest("[data-broadcast-seen]");
  if (!button || state.isViewer || !state.currentUser) return;
  const response = await apiPost("/api/broadcasts/seen", {
    id: button.dataset.broadcastSeen,
    requester: state.currentUser,
  });
  if (!response.ok) {
    alert(response.error || "Could not mark broadcast seen");
    return;
  }
  state.broadcasts = normalizeBroadcasts(response.broadcasts);
  renderBroadcasts();
});

document.querySelector("#admin-open").addEventListener("click", () => openAdmin());
document.querySelector("#login-admin-open").addEventListener("click", () => openAdmin());

document.addEventListener("keydown", (event) => {
  const target = event.target;
  const tagName = target && target.tagName ? target.tagName.toLowerCase() : "";
  if (["input", "textarea", "select", "button"].includes(tagName) || (target && target.isContentEditable)) return;

  if (event.key === "ArrowLeft") {
    event.preventDefault();
    shiftWeek(-1);
  } else if (event.key === "ArrowRight") {
    event.preventDefault();
    shiftWeek(1);
  }
});

["pointerdown", "keydown", "input", "change", "submit"].forEach((eventName) => {
  document.addEventListener(eventName, recordActivity, true);
});

function openAdmin() {
  adminMessage.textContent = "";
  if (typeof adminDialog.showModal === "function") {
    adminDialog.showModal();
  } else {
    adminDialog.setAttribute("open", "");
  }
  if (!state.adminPassword) adminPassword.focus();
}

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
    pin: accountPin.value,
  });
  if (!response.ok) {
    adminMessage.textContent = response.error || "Could not add account";
    return;
  }
  accountName.value = "";
  accountPin.value = "";
  await loadAccounts(response.accounts);
  await loadGroups();
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
  await loadGroups();
});

groupForm.addEventListener("submit", async (event) => {
  event.preventDefault();
  const response = await apiPost("/api/groups", {
    password: state.adminPassword,
    name: groupName.value,
  });
  if (!response.ok) {
    adminMessage.textContent = response.error || "Could not add group";
    return;
  }
  groupName.value = "";
  await loadGroups(response.groups);
});

groupList.addEventListener("click", async (event) => {
  const deleteButton = event.target.closest("[data-group-delete]");
  if (deleteButton) {
    const response = await apiPost("/api/groups/delete", {
      password: state.adminPassword,
      name: deleteButton.dataset.groupDelete,
    });
    if (!response.ok) {
      adminMessage.textContent = response.error || "Could not delete group";
      return;
    }
    await loadGroups(response.groups);
    return;
  }

  const memberButton = event.target.closest("[data-group-member]");
  if (!memberButton) return;
  const response = await apiPost("/api/groups/member", {
    password: state.adminPassword,
    group: memberButton.dataset.groupName,
    member: memberButton.dataset.groupMember,
    action: memberButton.dataset.groupAction,
  });
  if (!response.ok) {
    adminMessage.textContent = response.error || "Could not update group";
    return;
  }
  await loadGroups(response.groups);
});

form.addEventListener("submit", async (event) => {
  event.preventDefault();
  if (state.isViewer) return;
  const data = new FormData(form);
  const title = String(data.get("title") || "").trim();
  if (!title) return;
  const time = selectedTimeValue(taskHour, taskMinute, "by");
  if (time === null) return;

  const response = await apiPost("/api/tasks", {
    title,
    date: data.get("day"),
    time,
    assignees: getSelectedAssignees().join(","),
    requester: state.currentUser,
    createdAt: new Date().toISOString(),
    note: String(data.get("note") || "").trim(),
  });
  if (!response.ok) {
    alert(response.error || "Could not add task");
    return;
  }
  state.tasks = normalizeTasks(response.tasks);

  form.reset();
  taskDay.value = toDateKey(hostNow());
  setDefaultTaskTime();
  closeCreateDialog();
  render();
});

eventForm.addEventListener("submit", async (event) => {
  event.preventDefault();
  if (state.isViewer || !state.currentUser) return;
  const data = new FormData(eventForm);
  const title = String(data.get("title") || "").trim();
  if (!title) return;
  const startTime = selectedTimeValue(eventStartHour, eventStartMinute, "start");
  const endTime = selectedTimeValue(eventEndHour, eventEndMinute, "end");
  if (startTime === null || endTime === null || !startTime || !endTime) return;

  const response = await apiPost("/api/events", {
    title,
    startDate: data.get("startDay"),
    startTime,
    endDate: data.get("endDay"),
    endTime,
    assignees: getSelectedEventAssignees().join(","),
    requester: state.currentUser,
    createdAt: new Date().toISOString(),
    note: String(data.get("note") || "").trim(),
  });
  if (!response.ok) {
    alert(response.error || "Could not add event");
    return;
  }
  state.events = normalizeEvents(response.events);
  eventForm.reset();
  setDefaultEventTime();
  closeCreateDialog();
  render();
});

grid.addEventListener("click", async (event) => {
  const noteButton = event.target.closest("[data-note-toggle]");
  if (noteButton) {
    const panel = document.querySelector(`#${CSS.escape(noteButton.dataset.noteToggle)}`);
    if (panel) panel.hidden = !panel.hidden;
    return;
  }

  const eventNoteButton = event.target.closest("[data-event-note-toggle]");
  if (eventNoteButton) {
    const panel = document.querySelector(`#${CSS.escape(eventNoteButton.dataset.eventNoteToggle)}`);
    if (panel) panel.hidden = !panel.hidden;
    return;
  }

  const eventDeleteButton = event.target.closest("[data-event-delete]");
  if (eventDeleteButton) {
    const response = await apiPost("/api/events/delete", {
      id: eventDeleteButton.dataset.eventDelete,
      requester: state.currentUser,
    });
    if (!response.ok) {
      alert(response.error || "You cannot delete this event");
      return;
    }
    state.events = normalizeEvents(response.events);
    render();
    return;
  }

  const button = event.target.closest("[data-delete]");
  if (!button) return;
  const response = await apiPost("/api/tasks/delete", {
    id: button.dataset.delete,
    requester: state.currentUser,
  });
  if (!response.ok) {
    alert(response.error || "You cannot delete this task");
    return;
  }
  state.tasks = normalizeTasks(response.tasks);
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
  setInterval(refreshPlannerDataIfIdle, 5 * 1000);
  renderTimeSelectors();
  await loadAccounts();
  await loadGroups();
  await loadTasks();
  await loadEvents();
  await loadBroadcasts();
  if (state.currentUser && state.accounts.includes(state.currentUser)) {
    state.isViewer = isViewerAccount(state.currentUser);
    showPlanner();
  } else {
    showLogin();
  }
  render();
}

async function loadTasks(shouldRender = true) {
  try {
    state.tasks = normalizeTasks(await fetch("/api/tasks").then((response) => response.json()));
    if (!state.tasks.length) {
      await migrateLegacyTasks();
    }
  } catch {
    state.tasks = [];
  }
  if (shouldRender) renderWeekGrid();
}

async function loadEvents(shouldRender = true) {
  try {
    state.events = normalizeEvents(await fetch("/api/events").then((response) => response.json()));
  } catch {
    state.events = [];
  }
  if (shouldRender) renderWeekGrid();
}

async function loadBroadcasts(shouldRender = true) {
  try {
    state.broadcasts = normalizeBroadcasts(await fetch("/api/broadcasts").then((response) => response.json()));
  } catch {
    state.broadcasts = [];
  }
  if (shouldRender) renderBroadcasts();
}

async function refreshPlannerDataIfIdle() {
  if (!state.currentUser || appShell.hidden || state.idleRefreshInFlight) return;
  if (document.hidden) return;
  if (Date.now() - state.lastActivityAt < IDLE_REFRESH_MS) return;

  state.idleRefreshInFlight = true;
  try {
    await loadTasks(false);
    await loadEvents(false);
    await loadBroadcasts(false);
    renderWeekGrid();
    renderBroadcasts();
    state.lastActivityAt = Date.now();
  } finally {
    state.idleRefreshInFlight = false;
  }
}

function recordActivity() {
  state.lastActivityAt = Date.now();
}

async function migrateLegacyTasks() {
  let legacyTasks = [];
  try {
    legacyTasks = JSON.parse(localStorage.getItem(LEGACY_STORAGE_KEY) || "[]") || [];
  } catch {
    legacyTasks = [];
  }
  if (!legacyTasks.length) return;

  for (const task of legacyTasks) {
    await apiPost("/api/tasks", {
      title: task.title || "Untitled",
      date: task.date || toDateKey(hostNow()),
      time: task.time || "",
      assignees: normalizeAssignees(task).join(","),
      requester: task.requester && !isViewerAccount(task.requester) ? task.requester : state.currentUser,
      createdAt: task.createdAt || new Date().toISOString(),
      note: task.note || task.notes || "",
    });
  }
  state.tasks = normalizeTasks(await fetch("/api/tasks").then((response) => response.json()));
  localStorage.setItem(`${LEGACY_STORAGE_KEY}.migrated`, new Date().toISOString());
}

async function loadGroups(groups = null) {
  state.groups = groups || await fetch("/api/groups").then((response) => response.json());
  renderGroupControls();
  renderPersonOptions();
  renderBroadcastTargets();
  renderTaskFilter();
  renderWeekGrid();
  renderBroadcasts();
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
  hostTime.textContent = formatDate(now, { hour: "2-digit", minute: "2-digit", hour12: false });
  hostDate.textContent = formatDate(now, { weekday: "long", month: "long", day: "numeric", year: "numeric" });
}

async function loadAccounts(accounts = null) {
  state.accounts = accounts || await fetch("/api/accounts").then((response) => response.json());
  if (!state.accounts.includes(state.currentUser)) {
    state.currentUser = "";
    state.isViewer = false;
    sessionStorage.removeItem("splanner.currentUser");
  }
  renderLoginOptions();
  renderMembers();
  renderAccountControls();
  renderGroupControls();
  renderPersonOptions();
  renderBroadcastTargets();
  renderTaskFilter();
  renderWeekGrid();
  renderBroadcasts();
}

function render() {
  renderMembers();
  renderWeekHeading();
  renderDayOptions();
  renderPersonOptions();
  renderBroadcastTargets();
  renderTaskFilter();
  renderWeekGrid();
  renderBroadcasts();
  renderAccountControls();
  renderGroupControls();
}

function showLogin() {
  loginScreen.hidden = false;
  appShell.hidden = true;
  renderLoginOptions();
}

function showPlanner() {
  state.isViewer = isViewerAccount(state.currentUser);
  appShell.classList.toggle("viewer-mode", state.isViewer);
  loginScreen.hidden = false;
  loginScreen.hidden = true;
  appShell.hidden = false;
  render();
}

function isViewerAccount(name) {
  const normalized = String(name).trim().toLowerCase();
  return normalized === "viewer" || normalized === "overview";
}

function isSystemAccount(name) {
  return String(name).trim().toLowerCase() === "overview";
}

function taskAccounts() {
  return state.accounts.filter((name) => !isSystemAccount(name));
}

function renderLoginOptions() {
  loginName.innerHTML = state.accounts.length
    ? state.accounts.map((name) => `<option value="${escapeHtml(name)}">${escapeHtml(name)}</option>`).join("")
    : `<option value="">Ask admin to add accounts</option>`;
  loginName.value = state.accounts.includes(state.currentUser) ? state.currentUser : state.accounts[0] || "";
}

function renderMembers() {
  memberRail.innerHTML = taskAccounts().map((name, index) => `
    <article class="member ${sameName(name, state.currentUser) ? "current-member" : ""}">
      <span class="avatar" style="background:${ACCOUNT_COLORS[index % ACCOUNT_COLORS.length]}">${escapeHtml(name.slice(0, 1))}</span>
      <strong>${escapeHtml(name)}</strong>
    </article>
  `).join("");
}

function renderAccountControls() {
  accountList.innerHTML = state.accounts.map((name) => `
    <div class="account-row">
      <span>${escapeHtml(name)}</span>
      ${isSystemAccount(name)
        ? `<span class="chip">Built in</span>`
        : `<button type="button" data-account-delete="${escapeHtml(name)}">Delete</button>`}
    </div>
  `).join("");
}

function renderGroupControls() {
  groupList.innerHTML = state.groups.length
    ? state.groups.map((group) => `
      <article class="group-row">
        <header>
          <strong>${escapeHtml(group.name)}</strong>
          <button type="button" data-group-delete="${escapeHtml(group.name)}">Delete</button>
        </header>
        <div class="group-members">
          ${taskAccounts().map((name) => {
            const isMember = group.members.includes(name);
            return `
              <button type="button"
                data-group-name="${escapeHtml(group.name)}"
                data-group-member="${escapeHtml(name)}"
                data-group-action="${isMember ? "remove" : "add"}">
                ${isMember ? "Remove" : "Add"} ${escapeHtml(name)}
              </button>
            `;
          }).join("")}
        </div>
      </article>
    `).join("")
    : `<div class="empty-day">No groups yet</div>`;
}

function renderPersonOptions() {
  const personOptions = taskAccounts().map((name) => `
      <label class="person-option">
        <input type="checkbox" value="${escapeHtml(name)}">
        <span>${escapeHtml(name)}</span>
      </label>
    `).join("");
  const groupOptions = state.groups.map((group) => `
      <label class="person-option">
        <input type="checkbox" value="@${escapeHtml(group.name)}">
        <span>${escapeHtml(group.name)}</span>
      </label>
    `).join("");
  const optionsHtml = personOptions || groupOptions
    ? `${personOptions}${groupOptions}`
    : `<span class="chip">No accounts or groups yet</span>`;
  taskAssignees.innerHTML = optionsHtml;
  if (eventAssignees) eventAssignees.innerHTML = optionsHtml;
}

function renderBroadcastTargets() {
  if (!broadcastTargets) return;
  const personOptions = taskAccounts().map((name) => `
      <label class="person-option">
        <input type="checkbox" value="${escapeHtml(name)}">
        <span>${escapeHtml(name)}</span>
      </label>
    `).join("");
  const groupOptions = state.groups.map((group) => `
      <label class="person-option">
        <input type="checkbox" value="@${escapeHtml(group.name)}">
        <span>${escapeHtml(group.name)}</span>
      </label>
    `).join("");
  broadcastTargets.innerHTML = personOptions || groupOptions
    ? `${personOptions}${groupOptions}`
    : `<span class="chip">No accounts or groups yet</span>`;
}

function getSelectedBroadcastTargets() {
  return Array.from(broadcastTargets.querySelectorAll("input:checked")).map((input) => input.value);
}

function renderTaskFilter() {
  const options = [{ value: "all", label: "Everyone" }];
  if (!state.isViewer && state.currentUser) {
    options.push({ value: `person:${state.currentUser}`, label: state.currentUser });
  }
  if (state.isViewer) {
    taskAccounts().forEach((name) => options.push({ value: `person:${name}`, label: name }));
    state.groups.forEach((group) => options.push({ value: `group:${group.name}`, label: group.name }));
  } else {
    state.groups
      .filter((group) => group.members.some((member) => sameName(member, state.currentUser)))
      .forEach((group) => options.push({ value: `group:${group.name}`, label: group.name }));
  }

  if (!options.some((option) => option.value === state.taskFilter)) {
    state.taskFilter = "all";
  }
  taskFilter.innerHTML = options.map((option) => `
    <option value="${escapeHtml(option.value)}">${escapeHtml(option.label)}</option>
  `).join("");
  taskFilter.value = state.taskFilter;
}

function getSelectedAssignees() {
  return Array.from(taskAssignees.querySelectorAll("input:checked")).map((input) => input.value);
}

function getSelectedEventAssignees() {
  return Array.from(eventAssignees.querySelectorAll("input:checked")).map((input) => input.value);
}

function setAdminMode(isLoggedIn) {
  adminLogin.hidden = isLoggedIn;
  adminPanel.hidden = !isLoggedIn;
  renderAccountControls();
}

function renderBroadcasts() {
  if (!broadcastList) return;
  const broadcasts = visibleBroadcasts();
  broadcastList.innerHTML = broadcasts.map(renderBroadcast).join("");
}

function renderBroadcast(broadcast) {
  const targets = normalizeBroadcastTargets(broadcast);
  const targetLabel = targets.length ? targets.join(", ") : "Everyone";
  const canClose = !state.isViewer && broadcastAppliesToPerson(broadcast, state.currentUser) && !hasSeenBroadcast(broadcast, state.currentUser);
  return `
    <article class="broadcast-card">
      <header>
        <strong>${escapeHtml(broadcast.requester || "Someone")} broadcasts</strong>
        ${canClose ? `<button class="broadcast-seen" type="button" data-broadcast-seen="${escapeHtml(broadcast.id)}" aria-label="Close broadcast">&times;</button>` : ""}
      </header>
      <p class="broadcast-message">${linkifyNote(broadcast.message || "")}</p>
      <div class="broadcast-meta">
        <span class="chip">To: ${escapeHtml(targetLabel)}</span>
      </div>
    </article>
  `;
}

function visibleBroadcasts() {
  return state.broadcasts.filter((broadcast) => {
    if (state.isViewer) return !broadcastIsComplete(broadcast);
    return broadcastAppliesToPerson(broadcast, state.currentUser) && !hasSeenBroadcast(broadcast, state.currentUser);
  });
}

function renderWeekHeading() {
  const days = getWeekDays();
  const todayStart = startOfWeek(hostNow());
  const weekOffset = Math.round((state.weekStart - todayStart) / (7 * 24 * 60 * 60 * 1000));
  weekTitle.textContent = weekOffset === 0 ? "This week" : weekOffset === 1 ? "Next week" : weekOffset === -1 ? "Last week" : `${Math.abs(weekOffset)} weeks ${weekOffset > 0 ? "ahead" : "back"}`;
  weekRange.textContent = `${formatDate(days[0], { month: "long", day: "numeric" })} - ${formatDate(days[6], { month: "long", day: "numeric", year: "numeric" })}`;
}

function renderDayOptions() {
  const options = getWeekDays().map((day) => `
    <option value="${toDateKey(day)}">${formatDate(day, { weekday: "long", month: "short", day: "numeric" })}</option>
  `).join("");
  const currentTask = taskDay.value || toDateKey(hostNow());
  taskDay.innerHTML = options;
  taskDay.value = getWeekDays().some((day) => toDateKey(day) === currentTask) ? currentTask : toDateKey(getWeekDays()[0]);

  [eventStartDay, eventEndDay].forEach((select) => {
    if (!select) return;
    const current = select.value || toDateKey(hostNow());
    select.innerHTML = options;
    select.value = getWeekDays().some((day) => toDateKey(day) === current) ? current : toDateKey(getWeekDays()[0]);
  });
}

function renderWeekGrid() {
  grid.innerHTML = getWeekDays().map((day) => {
    const key = toDateKey(day);
    const tasks = state.tasks
      .filter((task) => task.date === key)
      .filter(taskMatchesCurrentFilter)
      .sort(sortTasks);
    const events = state.events
      .filter((event) => eventOverlapsDay(event, key))
      .filter(eventMatchesCurrentFilter)
      .sort(sortEvents);
    const itemsHtml = [
      ...events.map(renderEvent),
      ...tasks.map(renderTask),
    ].join("");
    return `
      <article class="day-column">
        <header class="day-header">
          <strong>${formatDate(day, { weekday: "short" })}</strong>
          <span class="date-label">${formatDate(day, { month: "short", day: "numeric" })}</span>
          <div class="time-strip" aria-hidden="true">
            <span>8</span>
            <span>12</span>
            <span>16</span>
            <span>20</span>
          </div>
        </header>
        <div class="task-list">
          ${itemsHtml || `<div class="empty-day">Open</div>`}
        </div>
      </article>
    `;
  }).join("");
}

function renderTask(task, index) {
  const assignees = normalizeAssignees(task);
  const assignee = assignees.length ? assignees.join(", ") : "Anyone";
  const requester = task.requester ? `by: ${task.requester}` : "by: unknown";
  const note = String(task.note || task.notes || "").trim();
  const noteId = `task-note-${String(task.id).replace(/[^a-zA-Z0-9_-]/g, "-")}`;
  const canDelete = canDeleteTask(task);
  return `
    <article class="task-card ${note ? "has-note" : ""}" data-tone="${index % 4}">
      <p class="task-title">${escapeHtml(task.title)}</p>
      <div class="task-meta">
        ${task.time ? `<span class="chip time-chip">${formatTaskTime(task.time)}</span>` : ""}
        <span class="chip">${escapeHtml(assignee)}</span>
        <span class="chip">${escapeHtml(requester)}</span>
      </div>
      <div class="task-actions">
        <button class="note-toggle" type="button" data-note-toggle="${escapeHtml(noteId)}" aria-label="Show note for ${escapeHtml(task.title)}">&#8942;</button>
        ${canDelete ? `<button class="delete-task" type="button" data-delete="${escapeHtml(task.id)}" aria-label="Remove ${escapeHtml(task.title)}">&times;</button>` : ""}
      </div>
      <div id="${escapeHtml(noteId)}" class="task-note-panel" hidden>${note ? linkifyNote(note) : ""}</div>
    </article>
  `;
}

function renderEvent(event, index) {
  const assignees = normalizeAssignees(event);
  const assignee = assignees.length ? assignees.join(", ") : "Anyone";
  const requester = event.requester ? `by: ${event.requester}` : "by: unknown";
  const note = String(event.note || event.notes || "").trim();
  const noteId = `event-note-${String(event.id).replace(/[^a-zA-Z0-9_-]/g, "-")}`;
  const canDelete = canDeleteEvent(event);
  return `
    <article class="event-card ${note ? "has-note" : ""}" data-tone="${index % 4}">
      <p class="event-title">${escapeHtml(event.title)}</p>
      <div class="event-meta">
        <span class="chip time-chip">${escapeHtml(formatEventTime(event))}</span>
        <span class="chip">${escapeHtml(assignee)}</span>
        <span class="chip">${escapeHtml(requester)}</span>
      </div>
      <div class="event-actions">
        <button class="event-note-toggle" type="button" data-event-note-toggle="${escapeHtml(noteId)}" aria-label="Show note for ${escapeHtml(event.title)}">&#8942;</button>
        ${canDelete ? `<button class="delete-event" type="button" data-event-delete="${escapeHtml(event.id)}" aria-label="Remove ${escapeHtml(event.title)}">&times;</button>` : ""}
      </div>
      <div id="${escapeHtml(noteId)}" class="event-note-panel" hidden>${note ? linkifyNote(note) : ""}</div>
    </article>
  `;
}

function normalizeEvents(events) {
  return Array.isArray(events) ? events.map((event) => ({
    ...event,
    assignees: normalizeAssignees(event),
    note: event.note || event.notes || "",
  })) : [];
}

function normalizeBroadcasts(broadcasts) {
  return Array.isArray(broadcasts) ? broadcasts.map((broadcast) => ({
    ...broadcast,
    targets: normalizeBroadcastTargets(broadcast),
    seenBy: normalizeSeenBy(broadcast),
  })) : [];
}

function normalizeBroadcastTargets(broadcast) {
  if (Array.isArray(broadcast.targets)) return broadcast.targets;
  if (typeof broadcast.targets === "string") return broadcast.targets.split(",").map((item) => item.trim()).filter(Boolean);
  return [];
}

function normalizeSeenBy(broadcast) {
  if (Array.isArray(broadcast.seenBy)) return broadcast.seenBy;
  if (Array.isArray(broadcast.seen_by)) return broadcast.seen_by;
  if (typeof broadcast.seenBy === "string") return broadcast.seenBy.split(",").map((item) => item.trim()).filter(Boolean);
  return [];
}

function hasSeenBroadcast(broadcast, name) {
  return normalizeSeenBy(broadcast).some((seenName) => sameName(seenName, name));
}

function broadcastTargetPeople(broadcast) {
  const targets = normalizeBroadcastTargets(broadcast);
  const people = [];
  const addPerson = (name) => {
    if (!name || isSystemAccount(name)) return;
    if (!people.some((existing) => sameName(existing, name))) people.push(name);
  };

  if (!targets.length) {
    taskAccounts().forEach(addPerson);
    return people;
  }

  targets.forEach((target) => {
    if (target.startsWith("@")) {
      const group = state.groups.find((candidate) => sameName(candidate.name, target.slice(1)));
      if (group) group.members.forEach(addPerson);
    } else {
      addPerson(target);
    }
  });
  return people;
}

function broadcastAppliesToPerson(broadcast, person) {
  return broadcastTargetPeople(broadcast).some((name) => sameName(name, person));
}

function broadcastIsComplete(broadcast) {
  const people = broadcastTargetPeople(broadcast);
  return people.length > 0 && people.every((name) => hasSeenBroadcast(broadcast, name));
}

function taskMatchesCurrentFilter(task) {
  const filter = state.taskFilter || "all";
  if (filter === "all") return true;
  const assignees = normalizeAssignees(task);
  if (filter.startsWith("person:")) {
    const person = filter.slice("person:".length);
    return taskAppliesToPerson(task, person);
  }
  if (filter.startsWith("group:")) {
    const group = filter.slice("group:".length);
    return assignees.some((assignee) => sameName(assignee, `@${group}`));
  }
  return true;
}

function taskAppliesToPerson(task, person) {
  const assignees = normalizeAssignees(task);
  if (!assignees.length) return true;
  return assignees.some((assignee) => {
    if (assignee.startsWith("@")) {
      const group = state.groups.find((candidate) => sameName(candidate.name, assignee.slice(1)));
      return group ? group.members.some((member) => sameName(member, person)) : false;
    }
    return sameName(assignee, person);
  });
}

function canDeleteTask(task) {
  if (state.isViewer || !state.currentUser) return false;
  if (sameName(task.requester, state.currentUser)) return true;
  const assignees = normalizeAssignees(task);
  if (!assignees.length) return true;
  return taskAppliesToPerson(task, state.currentUser);
}

function eventMatchesCurrentFilter(event) {
  return taskMatchesCurrentFilter(event);
}

function canDeleteEvent(event) {
  if (state.isViewer || !state.currentUser) return false;
  if (sameName(event.requester, state.currentUser)) return true;
  const assignees = normalizeAssignees(event);
  if (!assignees.length) return true;
  return taskAppliesToPerson(event, state.currentUser);
}

function eventOverlapsDay(event, dateKey) {
  const start = String(event.startDate || "");
  const end = String(event.endDate || start);
  return start <= dateKey && dateKey <= end;
}

function sortEvents(a, b) {
  const aTime = a.startTime || "";
  const bTime = b.startTime || "";
  if (aTime && bTime && aTime !== bTime) return aTime.localeCompare(bTime);
  return String(a.createdAt || "").localeCompare(String(b.createdAt || ""));
}

function sortTasks(a, b) {
  if (a.time && b.time && a.time !== b.time) return a.time.localeCompare(b.time);
  if (a.time && !b.time) return -1;
  if (!a.time && b.time) return 1;
  return String(a.createdAt || "").localeCompare(String(b.createdAt || ""));
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
  markOverviewInteraction();
  state.weekStart = addDays(state.weekStart, amount * 7);
  render();
}

function markOverviewInteraction() {
  if (!state.isViewer) return;
  if (state.overviewResetTimer) {
    clearTimeout(state.overviewResetTimer);
  }
  state.overviewResetTimer = setTimeout(() => {
    if (!state.isViewer) return;
    state.weekStart = startOfWeek(hostNow());
    render();
  }, 10 * 60 * 1000);
}

function renderTimeSelectors() {
  [taskMinute, eventStartMinute, eventEndMinute].forEach((select) => {
    if (!select) return;
    select.innerHTML = Array.from({ length: 12 }, (_, index) => {
      const value = String(index * 5).padStart(2, "0");
      return `<option value="${value}">${value}</option>`;
    }).join("");
  });
}

function selectedTimeValue(hourInput, minuteSelect, label) {
  if (!hourInput || !minuteSelect) return "";
  const rawHour = hourInput.value.trim();
  if (!rawHour) {
    alert(`Use a ${label} hour between 0 and 23.`);
    hourInput.focus();
    return null;
  }
  if (!/^\d{1,2}$/.test(rawHour)) {
    alert(`Use a ${label} hour between 0 and 23.`);
    hourInput.focus();
    return null;
  }
  const hour = Number(rawHour);
  if (hour < 0 || hour > 23) {
    alert(`Use a ${label} hour between 0 and 23.`);
    hourInput.focus();
    return null;
  }
  return `${String(hour).padStart(2, "0")}:${minuteSelect.value || "00"}`;
}

function setDefaultTaskTime() {
  if (taskHour) taskHour.value = "";
  if (taskMinute) taskMinute.value = "00";
  if (taskNote) taskNote.value = "";
}

function setDefaultEventTime() {
  if (eventStartHour) eventStartHour.value = "";
  if (eventEndHour) eventEndHour.value = "";
  if (eventStartMinute) eventStartMinute.value = "00";
  if (eventEndMinute) eventEndMinute.value = "00";
  if (eventNote) eventNote.value = "";
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

function formatTaskTime(value) {
  const [hour, minute] = String(value).split(":").map(Number);
  const date = new Date();
  date.setHours(hour, minute || 0, 0, 0);
  return formatDate(date, { hour: "2-digit", minute: "2-digit", hour12: false });
}

function formatEventTime(event) {
  const startDate = event.startDate || "";
  const endDate = event.endDate || startDate;
  const start = `${startDate} ${event.startTime || ""}`.trim();
  const end = `${endDate} ${event.endTime || ""}`.trim();
  return start === end ? start : `${start} - ${end}`;
}

function normalizeTasks(tasks) {
  return Array.isArray(tasks) ? tasks.map((task) => ({
    ...task,
    assignees: normalizeAssignees(task),
    note: task.note || task.notes || "",
  })) : [];
}

function normalizeAssignees(task) {
  if (Array.isArray(task.assignees)) return task.assignees;
  if (typeof task.assignees === "string") return task.assignees.split(",").map((item) => item.trim()).filter(Boolean);
  if (task.assignee) return [task.assignee];
  return [];
}

function linkifyNote(value) {
  const escaped = escapeHtml(value);
  return escaped.replace(/(https?:\/\/[^\s<]+)/g, '<a href="$1" target="_blank" rel="noopener noreferrer">$1</a>');
}

function sameName(left, right) {
  return String(left || "").trim().toLowerCase() === String(right || "").trim().toLowerCase();
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
