#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use arboard::Clipboard;
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use globset::{Glob, GlobSet, GlobSetBuilder};
use ignore::gitignore::GitignoreBuilder;
use notify::{Config as NotifyConfig, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use reqwest::blocking::Client;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    collections::{HashMap, HashSet},
    fs,
    io::Write,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::{
        atomic::{AtomicBool, Ordering},
        Mutex,
    },
    thread,
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use tauri::{
    menu::{Menu, MenuItem, PredefinedMenuItem, Submenu},
    tray::{MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent},
    AppHandle, Emitter, Manager, WebviewUrl, WebviewWindow, WebviewWindowBuilder, WindowEvent,
};
use tempfile::TempDir;

type CommandResult<T> = Result<T, String>;

const VALID_THEMES: &[&str] = &["sky", "default", "grey"];
const EMPTY_TREE_HASH: &str = "4b825dc642cb6eb9a060e54bf8d69288fbee4904";
const SQUASH_CHUNK_WINDOW_MS: i64 = 2 * 60 * 1000;
const MAX_SQUASH_PROMPT_CHARS: usize = 25_000;
const MAX_SQUASH_PROMPT_MESSAGE_CHARS: usize = 400;
const MAX_SQUASH_NAME_STATUS_CHARS: usize = 6_000;
const MAX_SQUASH_DIFFSTAT_CHARS: usize = 4_000;
const MAX_SQUASH_COMMIT_MESSAGE_CHARS: usize = 160;

const TAURI_BUILD_IGNORES: &[&str] = &["dist-tauri", "src-tauri/target", "src-tauri/gen"];

const MONITOR_DEFAULT_IGNORES: &[&str] = &[
    ".git",
    "node_modules",
    ".venv",
    "venv",
    "__pycache__",
    ".mypy_cache",
    ".pytest_cache",
    "dist",
    "dist-tauri",
    "build",
    "out",
    ".next",
    ".nuxt",
    ".turbo",
    ".parcel-cache",
    ".cache",
    "target",
    "src-tauri/target",
    "src-tauri/gen",
    "coverage",
    "logs",
    "tmp",
    "temp",
    "output",
    "tmp*",
    "*.log",
    "*.tmp",
    "*.swp",
];

const IGNORED_NAMES: &[&str] = &[
    ".DS_Store",
    "Thumbs.db",
    "desktop.ini",
    ".AppleDouble",
    ".LSOverride",
    "Icon\r",
    ".git",
    ".gitattributes",
    "node_modules",
    "npm-debug.log*",
    "yarn-error.log",
    "yarn-debug.log*",
    "pnpm-debug.log*",
    "package-lock.json",
    "yarn.lock",
    "pnpm-lock.yaml",
    "tsconfig.tsbuildinfo",
    "dist",
    "dist-tauri",
    "build",
    ".cache",
    "out",
    ".next",
    ".turbo",
    ".venv",
    "venv",
    "__pycache__",
    "*.py[cod]",
    ".mypy_cache",
    ".pytest_cache",
    ".tox",
    "*.egg-info",
    ".coverage",
    "htmlcov",
    ".env",
    ".env.*",
    "target",
    "*.class",
    "*.jar",
    "*.war",
    "*.ear",
    "*.zip",
    "*.tar.gz",
    "*.rar",
    "*.log",
    "*.iml",
    ".idea",
    ".project",
    ".classpath",
    ".settings",
    "*.o",
    "*.obj",
    "*.so",
    "*.dylib",
    "*.dll",
    "*.exe",
    "*.out",
    "*.app",
    "CMakeFiles",
    "CMakeCache.txt",
    "Debug",
    "Release",
    "bin",
    "pkg",
    "vendor",
    "Cargo.lock",
    "*.gem",
    ".bundle",
    "vendor/bundle",
    "log",
    "tmp",
    "coverage",
    "composer.lock",
    "*.cache",
    "*.session",
    "obj",
    "TestResults",
    ".vs",
    ".vscode",
    ".history",
    "*.code-workspace",
    "*.sublime-project",
    "*.sublime-workspace",
    "*.swp",
    "*.swo",
    "*.tmp",
    "*.bak",
    "*~",
    "logs",
    "test-results",
    "lcov-report",
    "*.sqlite3",
    "*.sqlite3-journal",
    "*.db",
    "*.db-journal",
    "docker-compose.override.yml",
    ".docker",
    "*.pid",
    "*.seed",
    "*.pid.lock",
    ".terraform",
    "*.tfstate",
    "*.tfstate.backup",
    ".terraform.lock.hcl",
    ".serverless",
    ".aws-sam",
    ".gradle",
    ".meteor/local",
    ".expo",
    ".nuxt",
    ".parcel-cache",
    "reports",
    "*.apk",
    "*.aab",
    ".android",
    ".flutter-plugins",
    ".flutter-plugins-dependencies",
    ".packages",
    "*.xcworkspace",
    "xcuserdata",
    "DerivedData",
    "*.ipa",
    "*.dSYM",
    "Library",
    "Temp",
    "Obj",
    "Build",
    "Builds",
    "Binaries",
    "DerivedDataCache",
    "Intermediate",
    "Saved",
    "*.lock",
    "*.7z",
];

const CODE_EXTS: &[&str] = &[
    "js", "jsx", "ts", "tsx", "py", "sh", "rb", "pl", "php", "java", "c", "cpp", "h", "cs", "go",
    "rs", "json", "yml", "yaml", "toml", "md", "html", "css", "txt",
];

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct FolderObj {
    path: String,
    #[serde(default)]
    monitoring: bool,
    #[serde(default)]
    needs_relocation: bool,
    #[serde(default)]
    lines_changed: i64,
    #[serde(default)]
    llm_candidates: Vec<String>,
    #[serde(default)]
    llm_buffer: Vec<String>,
    #[serde(default)]
    first_candidate_birthday: Option<i64>,
    #[serde(default)]
    last_head_hash: Option<String>,
    #[serde(default)]
    rewrite_in_progress: bool,
    #[serde(default)]
    rewrite_started_at: Option<i64>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct StoreData {
    #[serde(default)]
    folders: Vec<FolderObj>,
    #[serde(default)]
    selected: Option<String>,
    #[serde(default = "default_theme")]
    theme: String,
    #[serde(default = "default_true")]
    skymode: bool,
    #[serde(default = "default_true")]
    skip_git_prompt: bool,
    #[serde(default = "default_intelligent_threshold")]
    intelligent_commit_threshold: i64,
    #[serde(default = "default_minutes_threshold")]
    minutes_commit_threshold: i64,
    #[serde(default)]
    autostart: bool,
    #[serde(default = "default_true")]
    close_to_tray: bool,
    #[serde(default)]
    needs_relocation: bool,
    #[serde(default)]
    daily_commit_stats: HashMap<String, i64>,
    #[serde(default)]
    gitea_token: String,
    #[serde(default)]
    rewrite_in_progress: bool,
    #[serde(default)]
    rewrite_started_at: Option<i64>,
    #[serde(default)]
    llm_buffer: Vec<String>,
    #[serde(default)]
    commit_model: Option<String>,
    #[serde(default)]
    readme_model: Option<String>,
    #[serde(default)]
    author: Option<String>,
    #[serde(default)]
    license: Option<String>,
}

impl Default for StoreData {
    fn default() -> Self {
        Self {
            folders: Vec::new(),
            selected: None,
            theme: default_theme(),
            skymode: true,
            skip_git_prompt: true,
            intelligent_commit_threshold: default_intelligent_threshold(),
            minutes_commit_threshold: default_minutes_threshold(),
            autostart: false,
            close_to_tray: true,
            needs_relocation: false,
            daily_commit_stats: HashMap::new(),
            gitea_token: String::new(),
            rewrite_in_progress: false,
            rewrite_started_at: None,
            llm_buffer: Vec::new(),
            commit_model: None,
            readme_model: None,
            author: None,
            license: None,
        }
    }
}

fn default_theme() -> String {
    "sky".to_string()
}

fn default_true() -> bool {
    true
}

fn default_intelligent_threshold() -> i64 {
    20
}

fn default_minutes_threshold() -> i64 {
    5
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct CommitPage {
    head: Option<String>,
    commits: Vec<CommitSummary>,
    total: usize,
    page: usize,
    page_size: usize,
    pages: usize,
}

#[derive(Debug, Serialize)]
struct CommitSummary {
    hash: String,
    date: String,
    message: String,
}

#[derive(Debug, Serialize)]
struct TreeNode {
    name: String,
    #[serde(rename = "type")]
    node_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    children: Option<Vec<TreeNode>>,
}

#[derive(Default)]
struct PendingChanges {
    names: HashSet<String>,
    paths: HashSet<PathBuf>,
}

#[derive(Clone)]
enum MenuAction {
    TrayToggle(String),
    TrayRemove(String),
    TrayAdd,
    TrayOpen,
    TrayStartAll,
    TrayStopAll,
    Quit,
    Settings,
    ContextOpen(PathBuf),
    ContextCopy(PathBuf),
    ContextGitignore { root: PathBuf, rel: String },
}

struct AppState {
    store: Mutex<StoreData>,
    store_path: PathBuf,
    watchers: Mutex<HashMap<String, RecommendedWatcher>>,
    pending: Mutex<HashMap<String, PendingChanges>>,
    active: Mutex<HashSet<String>>,
    menu_actions: Mutex<HashMap<String, MenuAction>>,
    quitting: AtomicBool,
    tray: Mutex<Option<TrayIcon>>,
}

#[derive(Debug)]
struct CommandOutput {
    stdout: String,
}

#[derive(Default)]
struct GitStatus {
    not_added: Vec<String>,
    created: Vec<String>,
    modified: Vec<String>,
    deleted: Vec<String>,
    renamed: Vec<(String, String)>,
}

#[derive(Clone, Debug)]
struct SquashCommit {
    hash: String,
    tree: String,
    parents: Vec<String>,
    timestamp_ms: i64,
    author_name: String,
    author_email: String,
    author_date: String,
    committer_name: String,
    committer_email: String,
    committer_date: String,
    message: String,
}

#[derive(Clone, Debug)]
struct SquashPlanEntry {
    commits: Vec<SquashCommit>,
    message: String,
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}

fn debug(msg: impl AsRef<str>) {
    println!("[DEBUG {}] {}", now_ms(), msg.as_ref());
}

fn store_path() -> PathBuf {
    let base = dirs::data_dir()
        .or_else(dirs::config_dir)
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));
    base.join("Auto-Git").join("config.json")
}

fn load_store(path: &Path) -> StoreData {
    let mut store = fs::read_to_string(path)
        .ok()
        .and_then(|raw| serde_json::from_str::<StoreData>(&raw).ok())
        .unwrap_or_default();
    normalize_store(&mut store);
    store
}

fn normalize_store(store: &mut StoreData) {
    if !VALID_THEMES.contains(&store.theme.as_str()) {
        store.theme = if store.skymode { "sky" } else { "default" }.to_string();
    }
    store.skymode = store.theme == "sky";

    for folder in &mut store.folders {
        let repo_exists = Path::new(&folder.path).join(".git").exists();
        let path_exists = Path::new(&folder.path).exists();
        folder.needs_relocation = !path_exists;
        if !repo_exists || folder.needs_relocation {
            folder.monitoring = false;
        }
    }
}

fn save_store(state: &AppState) -> CommandResult<()> {
    let store = state.store.lock().map_err(|e| e.to_string())?.clone();
    if let Some(parent) = state.store_path.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let data = serde_json::to_vec_pretty(&store).map_err(|e| e.to_string())?;
    fs::write(&state.store_path, data).map_err(|e| e.to_string())
}

fn run_process(
    program: &str,
    args: &[String],
    cwd: Option<&Path>,
    env: Option<&HashMap<String, String>>,
    input: Option<&str>,
) -> CommandResult<CommandOutput> {
    let mut cmd = Command::new(program);
    cmd.args(args);
    if let Some(cwd) = cwd {
        cmd.current_dir(cwd);
    }
    if let Some(env) = env {
        cmd.envs(env);
    }
    if input.is_some() {
        cmd.stdin(Stdio::piped());
    }
    cmd.stdout(Stdio::piped()).stderr(Stdio::piped());
    let mut child = cmd.spawn().map_err(|e| e.to_string())?;
    if let Some(input) = input {
        if let Some(stdin) = child.stdin.as_mut() {
            stdin
                .write_all(input.as_bytes())
                .map_err(|e| e.to_string())?;
        }
    }
    let output = child.wait_with_output().map_err(|e| e.to_string())?;
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    if output.status.success() {
        Ok(CommandOutput { stdout })
    } else {
        Err(if stderr.trim().is_empty() {
            stdout.trim().to_string()
        } else {
            stderr.trim().to_string()
        })
    }
}

fn run_git(repo: &str, args: &[&str]) -> CommandResult<String> {
    let owned: Vec<String> = args.iter().map(|s| s.to_string()).collect();
    Ok(run_process("git", &owned, Some(Path::new(repo)), None, None)?.stdout)
}

fn run_git_owned(
    repo: &str,
    args: &[String],
    env: Option<&HashMap<String, String>>,
    input: Option<&str>,
) -> CommandResult<String> {
    Ok(run_process("git", args, Some(Path::new(repo)), env, input)?.stdout)
}

fn git_status(repo: &str) -> CommandResult<GitStatus> {
    let raw = run_git(repo, &["status", "--porcelain"])?;
    let mut status = GitStatus::default();
    for line in raw.lines() {
        if line.len() < 3 {
            continue;
        }
        let code = &line[0..2];
        let path = line[3..].trim().to_string();
        if code.contains('?') {
            status.not_added.push(path);
        } else if code.contains('R') {
            if let Some((from, to)) = path.split_once(" -> ") {
                status.renamed.push((from.to_string(), to.to_string()));
            }
        } else if code.contains('A') {
            status.created.push(path);
        } else if code.contains('D') {
            status.deleted.push(path);
        } else if code.contains('M') {
            status.modified.push(path);
        }
    }
    Ok(status)
}

fn has_status_changes(status: &GitStatus) -> bool {
    !status.not_added.is_empty()
        || !status.created.is_empty()
        || !status.modified.is_empty()
        || !status.deleted.is_empty()
        || !status.renamed.is_empty()
}

fn build_commit_message_from_status(status: &GitStatus, prefix: &str) -> String {
    let mut changes = Vec::new();
    for f in &status.not_added {
        changes.push(format!("[add] {f}"));
    }
    for f in &status.created {
        changes.push(format!("[add] {f}"));
    }
    for f in &status.modified {
        changes.push(format!("[change] {f}"));
    }
    for f in &status.deleted {
        changes.push(format!("[unlink] {f}"));
    }
    for (from, to) in &status.renamed {
        changes.push(format!("[rename] {from} -> {to}"));
    }
    format!("{prefix}\n {}", changes.join("\n "))
}

fn is_rebase_in_progress(repo_path: &str) -> bool {
    let git_dir = Path::new(repo_path).join(".git");
    git_dir.join("rebase-merge").exists() || git_dir.join("rebase-apply").exists()
}

fn is_git_repo_path(folder_path: &str) -> bool {
    Path::new(folder_path).join(".git").exists()
}

fn ensure_gitignore_defaults(folder_path: &str) -> CommandResult<()> {
    let gitignore_path = Path::new(folder_path).join(".gitignore");
    let mut existing = HashSet::new();
    if gitignore_path.exists() {
        for line in fs::read_to_string(&gitignore_path)
            .map_err(|e| e.to_string())?
            .lines()
        {
            let trimmed = line.trim();
            if !trimmed.is_empty() {
                existing.insert(trimmed.to_string());
            }
        }
    }
    let mut changed = !gitignore_path.exists();
    for entry in MONITOR_DEFAULT_IGNORES {
        if existing.insert((*entry).to_string()) {
            changed = true;
        }
    }
    if changed {
        let mut lines: Vec<_> = existing.into_iter().collect();
        lines.sort();
        fs::write(gitignore_path, lines.join("\n") + "\n").map_err(|e| e.to_string())?;
    }
    Ok(())
}

fn init_git_repo_internal(folder: &str) -> CommandResult<()> {
    if !Path::new(folder).join(".git").exists() {
        run_git(folder, &["init"])?;
        ensure_gitignore_defaults(folder)?;
        run_git(folder, &["add", ".gitignore"])?;
        run_git(folder, &["commit", "--allow-empty", "-m", "initial commit"])?;
    }
    Ok(())
}

fn short_hash(hash: &str) -> String {
    hash.chars().take(7).collect()
}

fn truncate_text(text: impl AsRef<str>, max_chars: usize) -> String {
    let normalized = text.as_ref().trim();
    if normalized.chars().count() <= max_chars {
        return normalized.to_string();
    }
    let mut out: String = normalized
        .chars()
        .take(max_chars.saturating_sub(1))
        .collect();
    out = out.trim_end().to_string();
    out.push('…');
    out
}

fn normalize_single_line(text: impl AsRef<str>) -> String {
    text.as_ref()
        .replace("```json", "")
        .replace("```markdown", "")
        .replace("```", "")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .trim()
        .to_string()
}

fn truncate_prompt_block(text: impl AsRef<str>, max_chars: usize) -> String {
    let normalized = text.as_ref().trim();
    if normalized.is_empty() {
        return String::new();
    }
    if normalized.chars().count() <= max_chars {
        return normalized.to_string();
    }
    let mut out: String = normalized.chars().take(max_chars).collect();
    out = out.trim_end().to_string();
    out.push_str("\n…");
    out
}

fn emit(app: &AppHandle, event: &str, payload: impl Serialize + Clone) {
    if let Err(err) = app.emit(event, payload) {
        eprintln!("[AutoGit] failed to emit {event}: {err}");
    }
}

fn ensure_ollama_running() -> CommandResult<()> {
    let client = Client::builder()
        .timeout(Duration::from_millis(700))
        .build()
        .map_err(|e| e.to_string())?;
    if client.get("http://127.0.0.1:11434/").send().is_ok() {
        return Ok(());
    }

    #[cfg(unix)]
    {
        if let Ok(output) = run_process(
            "lsof",
            &["-i".into(), ":11434".into(), "-t".into()],
            None,
            None,
            None,
        ) {
            for pid in output.stdout.lines().filter(|line| !line.trim().is_empty()) {
                let _ = run_process("kill", &["-9".into(), pid.trim().into()], None, None, None);
            }
        }
    }

    let _ = Command::new("ollama")
        .arg("serve")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| e.to_string())?;

    for _ in 0..10 {
        thread::sleep(Duration::from_millis(500));
        if client.get("http://127.0.0.1:11434/").send().is_ok() {
            return Ok(());
        }
    }
    Err("[AutoGit] ollama serve could not be reached after 5 seconds".to_string())
}

fn stream_ollama(
    prompt: &str,
    model: &str,
    temperature: f64,
    app: &AppHandle,
) -> CommandResult<String> {
    ensure_ollama_running()?;
    emit(app, "cat-begin", ());

    let client = Client::builder()
        .timeout(Duration::from_secs(120))
        .build()
        .map_err(|e| e.to_string())?;
    let mut response = client
        .post("http://127.0.0.1:11434/api/generate")
        .json(&json!({
            "model": model,
            "prompt": prompt,
            "stream": true,
            "options": { "temperature": temperature }
        }))
        .send()
        .map_err(|e| e.to_string())?;

    if !response.status().is_success() {
        emit(app, "cat-end", ());
        return Err(format!("Ollama request failed: {}", response.status()));
    }

    let mut raw = String::new();
    response
        .read_to_string(&mut raw)
        .map_err(|e| e.to_string())?;
    let mut full_output = String::new();
    for line in raw.lines().filter(|line| !line.trim().is_empty()) {
        if let Ok(obj) = serde_json::from_str::<Value>(line) {
            if let Some(chunk) = obj.get("response").and_then(Value::as_str) {
                full_output.push_str(chunk);
                emit(app, "cat-chunk", chunk.to_string());
            }
        }
    }
    emit(app, "cat-end", ());
    Ok(full_output)
}

trait ReadToString {
    fn read_to_string(&mut self, out: &mut String) -> std::io::Result<usize>;
}

impl ReadToString for reqwest::blocking::Response {
    fn read_to_string(&mut self, out: &mut String) -> std::io::Result<usize> {
        use std::io::Read;
        let mut buf = Vec::new();
        let len = self.read_to_end(&mut buf)?;
        out.push_str(&String::from_utf8_lossy(&buf));
        Ok(len)
    }
}

fn parse_llm_commit_messages(raw_output: &str) -> CommandResult<HashMap<String, String>> {
    let cleaned = raw_output
        .trim()
        .trim_start_matches("```json")
        .trim_start_matches("```")
        .trim_end_matches("```")
        .trim();
    if cleaned.starts_with('{') {
        let obj: Value = serde_json::from_str(cleaned)
            .map_err(|e| format!("Could not parse LLM output: {e}\n{raw_output}"))?;
        let mut map = HashMap::new();
        if let Some(entries) = obj.as_object() {
            for (hash, message) in entries {
                if let Some(message) = message.as_str() {
                    map.insert(hash.clone(), message.to_string());
                }
            }
        }
        return Ok(map);
    }
    if cleaned.starts_with('[') {
        let arr: Vec<Value> = serde_json::from_str(cleaned)
            .map_err(|e| format!("Could not parse LLM output: {e}\n{raw_output}"))?;
        let mut map = HashMap::new();
        for item in arr {
            if let (Some(hash), Some(message)) = (
                item.get("commit").and_then(Value::as_str),
                item.get("newMessage")
                    .or_else(|| item.get("new_message"))
                    .and_then(Value::as_str),
            ) {
                map.insert(hash.to_string(), message.to_string());
            }
        }
        return Ok(map);
    }
    Err(format!("Could not parse LLM output:\n{raw_output}"))
}

fn get_commits_for_llm(folder_path: &str, hashes: &[String]) -> CommandResult<Vec<Value>> {
    let mut commits = Vec::new();
    for hash in hashes {
        let diff = run_git(folder_path, &["diff", &format!("{hash}^!")])?;
        let msg = run_git(folder_path, &["show", "-s", "--format=%B", hash])?;
        commits.push(json!({
            "hash": short_hash(hash),
            "message": msg.trim(),
            "diff": diff
        }));
    }
    Ok(commits)
}

fn generate_llm_commit_prompt(folder_path: &str, hashes: &[String]) -> CommandResult<String> {
    let commits = get_commits_for_llm(folder_path, hashes)?;
    let prompt = format!(
        r#"Analyze the following git commits. For each commit, generate a concise commit message summarizing the actual change.
- ONLY output a JSON object mapping each commit hash to its new message.
- Do NOT add any explanations, greetings, or extra text.

Example Output:
{{
  "1a2b3c4": "Fix bug in user registration",
  "2b3c4d5": "Refactor login logic"
}}

COMMITS (as JSON):

{}"#,
        serde_json::to_string_pretty(&commits).map_err(|e| e.to_string())?
    );
    if prompt.len() > 200_000 {
        return Err(format!(
            "LLM prompt too large ({} chars) for {folder_path}",
            prompt.len()
        ));
    }
    Ok(prompt)
}

fn reword_commits_sequentially(
    repo_path: &str,
    commit_message_map: &HashMap<String, String>,
    hashes: &[String],
) -> CommandResult<()> {
    let status = git_status(repo_path)?;
    let mut stashed = false;
    if has_status_changes(&status) {
        let _ = run_git(
            repo_path,
            &["stash", "push", "--include-untracked", "--keep-index"],
        );
        stashed = true;
    }

    let all_raw = run_git(repo_path, &["log", "--format=%H"])?;
    let all_commits: Vec<String> = all_raw.lines().map(|s| s.to_string()).collect();
    let mut full_hashes: Vec<String> = hashes
        .iter()
        .filter_map(|h| all_commits.iter().find(|full| full.starts_with(h)).cloned())
        .collect();
    full_hashes.sort_by_key(|h| {
        all_commits
            .iter()
            .position(|full| full == h)
            .unwrap_or(usize::MAX)
    });
    full_hashes.reverse();

    let temp_dir = TempDir::new().map_err(|e| e.to_string())?;
    let sequence_path = temp_dir.path().join(if cfg!(windows) {
        "sequence-editor.cmd"
    } else {
        "sequence-editor.sh"
    });
    let message_path = temp_dir.path().join(if cfg!(windows) {
        "message-editor.cmd"
    } else {
        "message-editor.sh"
    });

    if cfg!(windows) {
        fs::write(
            &sequence_path,
            "@echo off\r\npowershell -NoProfile -Command \"(Get-Content %1) -replace '^pick ', 'reword ' | Set-Content %1\"\r\n",
        )
        .map_err(|e| e.to_string())?;
    } else {
        fs::write(
            &sequence_path,
            "#!/bin/sh\nsed -i.bak '1s/^pick /reword /' \"$1\"\n",
        )
        .map_err(|e| e.to_string())?;
        set_executable(&sequence_path)?;
    }

    for full_hash in full_hashes {
        let short = short_hash(&full_hash);
        let new_msg = commit_message_map
            .get(&full_hash)
            .or_else(|| commit_message_map.get(&short))
            .cloned();
        let Some(new_msg) = new_msg else {
            continue;
        };

        let _ = run_git(repo_path, &["rebase", "--abort"]);
        let msg_file = temp_dir.path().join("commit-message.txt");
        fs::write(&msg_file, new_msg.trim().to_string() + "\n").map_err(|e| e.to_string())?;
        if cfg!(windows) {
            fs::write(
                &message_path,
                format!(
                    "@echo off\r\ncopy /Y \"{}\" %1 >NUL\r\n",
                    msg_file.display()
                ),
            )
            .map_err(|e| e.to_string())?;
        } else {
            fs::write(
                &message_path,
                format!("#!/bin/sh\ncat \"{}\" > \"$1\"\n", msg_file.display()),
            )
            .map_err(|e| e.to_string())?;
            set_executable(&message_path)?;
        }

        let mut env = HashMap::new();
        env.insert(
            "GIT_SEQUENCE_EDITOR".to_string(),
            sequence_path.to_string_lossy().to_string(),
        );
        env.insert(
            "GIT_EDITOR".to_string(),
            message_path.to_string_lossy().to_string(),
        );
        run_git_owned(
            repo_path,
            &["rebase".into(), "-i".into(), format!("{full_hash}^")],
            Some(&env),
            None,
        )?;
    }

    if stashed {
        let _ = run_git(repo_path, &["stash", "pop"]);
    }
    Ok(())
}

#[cfg(unix)]
fn set_executable(path: &Path) -> CommandResult<()> {
    use std::os::unix::fs::PermissionsExt;
    let mut permissions = fs::metadata(path).map_err(|e| e.to_string())?.permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(path, permissions).map_err(|e| e.to_string())
}

#[cfg(not(unix))]
fn set_executable(_path: &Path) -> CommandResult<()> {
    Ok(())
}

fn run_llm_commit_rewrite(app: AppHandle, folder_path: String, force: bool) -> CommandResult<()> {
    let state = app.state::<AppState>();
    let (hashes, original_birthday) = {
        let mut store = state.store.lock().map_err(|e| e.to_string())?;
        let Some(folder) = store.folders.iter_mut().find(|f| f.path == folder_path) else {
            return Ok(());
        };
        if folder.needs_relocation {
            return Ok(());
        }
        if folder.llm_candidates.is_empty() {
            return Ok(());
        }
        if folder.rewrite_in_progress && !force {
            return Ok(());
        }
        if folder.rewrite_in_progress && force {
            folder.rewrite_in_progress = false;
            folder.rewrite_started_at = None;
        }
        let hashes = folder.llm_candidates.clone();
        let original_birthday = folder.first_candidate_birthday;
        folder.rewrite_in_progress = true;
        folder.rewrite_started_at = Some(now_ms());
        (hashes, original_birthday)
    };
    save_store(&state)?;

    let mut error: Option<String> = None;
    let result = (|| -> CommandResult<()> {
        let prompt = generate_llm_commit_prompt(&folder_path, &hashes)?;
        let model = {
            let store = state.store.lock().map_err(|e| e.to_string())?;
            store
                .commit_model
                .clone()
                .unwrap_or_else(|| "qwen2.5-coder:7b".to_string())
        };
        let llm_raw = stream_ollama(&prompt, &model, 0.3, &app)?;
        let message_map = parse_llm_commit_messages(&llm_raw)?;
        reword_commits_sequentially(&folder_path, &message_map, &hashes)?;
        emit(&app, "repo-updated", folder_path.clone());
        Ok(())
    })();

    if let Err(err) = result {
        error = Some(err.clone());
        eprintln!("[runLLMCommitRewrite] Rewrite failed: {err}");
        let fallback = (|| -> CommandResult<()> {
            let mut map = HashMap::new();
            for h in &hashes {
                let msg = run_git(&folder_path, &["show", "-s", "--format=%B", h])?;
                map.insert(h.clone(), msg.trim().to_string());
            }
            reword_commits_sequentially(&folder_path, &map, &hashes)?;
            emit(&app, "repo-updated", folder_path.clone());
            Ok(())
        })();
        if fallback.is_ok() {
            error = None;
        }
    }

    {
        let mut store = state.store.lock().map_err(|e| e.to_string())?;
        if let Some(folder) = store.folders.iter_mut().find(|f| f.path == folder_path) {
            let buffer = folder.llm_buffer.clone();
            if error.is_some() {
                folder.llm_candidates = hashes.clone();
                folder.llm_candidates.extend(buffer);
                folder.first_candidate_birthday = original_birthday.or_else(|| {
                    if folder.llm_candidates.is_empty() {
                        None
                    } else {
                        Some(now_ms())
                    }
                });
            } else {
                folder.llm_candidates = buffer;
                folder.first_candidate_birthday = if folder.llm_candidates.is_empty() {
                    None
                } else {
                    Some(now_ms())
                };
                folder.lines_changed = 0;
            }
            folder.llm_buffer.clear();
            folder.rewrite_in_progress = false;
            folder.rewrite_started_at = None;
        }
    }
    save_store(&state)?;

    if let Some(err) = error {
        return Err(err);
    }

    if let Ok(status) = git_status(&folder_path) {
        if has_status_changes(&status) {
            let msg = build_commit_message_from_status(&status, "auto-git: ");
            let _ = auto_commit(app, folder_path, msg);
        }
    }
    Ok(())
}

fn parse_numstat_added_deleted(line: &str) -> i64 {
    let cols: Vec<_> = line.split_whitespace().collect();
    if cols.len() < 2 {
        return 0;
    }
    let added = cols[0].parse::<i64>().unwrap_or(0);
    let deleted = cols[1].parse::<i64>().unwrap_or(0);
    added + deleted
}

fn auto_commit(app: AppHandle, folder_path: String, message: String) -> CommandResult<bool> {
    let state = app.state::<AppState>();
    let rewrite_active = {
        let store = state.store.lock().map_err(|e| e.to_string())?;
        store
            .folders
            .iter()
            .find(|f| f.path == folder_path)
            .map(|f| f.rewrite_in_progress)
            .unwrap_or(false)
    };
    if rewrite_active {
        return Ok(false);
    }

    if is_rebase_in_progress(&folder_path) {
        let _ = run_git(&folder_path, &["rebase", "--abort"]);
    }

    let status = git_status(&folder_path)?;
    if !has_status_changes(&status) {
        return Ok(false);
    }

    let current_branch = run_git(&folder_path, &["rev-parse", "--abbrev-ref", "HEAD"])
        .ok()
        .map(|s| s.trim().to_string());

    if current_branch.as_deref() == Some("HEAD") || current_branch.is_none() {
        let head_commit = run_git(&folder_path, &["rev-parse", "HEAD"])?
            .trim()
            .to_string();
        let master_commit = run_git(&folder_path, &["rev-parse", "refs/heads/master"])
            .ok()
            .map(|s| s.trim().to_string());
        match master_commit {
            Some(master) if master == head_commit => {
                run_git(&folder_path, &["checkout", "master"])?;
            }
            Some(_) => {
                let backup = format!("backup-master-{}", now_ms());
                run_git(&folder_path, &["branch", "-m", "master", &backup])?;
                run_git(&folder_path, &["checkout", "-b", "master"])?;
            }
            None => {
                run_git(&folder_path, &["checkout", "-b", "master"])?;
            }
        }
    }

    let diff_output = run_git(&folder_path, &["diff", "--numstat"])?;
    let changed_lines: i64 = diff_output.lines().map(parse_numstat_added_deleted).sum();
    run_git(&folder_path, &["add", "-A"])?;
    run_git(&folder_path, &["commit", "-m", &message])?;
    let new_head = run_git(&folder_path, &["rev-parse", "HEAD"])?
        .trim()
        .to_string();

    let should_rewrite = {
        let mut store = state.store.lock().map_err(|e| e.to_string())?;
        let threshold = store.intelligent_commit_threshold;
        let today = current_date_string();
        *store.daily_commit_stats.entry(today).or_insert(0) += 1;
        let Some(folder) = store.folders.iter_mut().find(|f| f.path == folder_path) else {
            return Ok(true);
        };
        folder.lines_changed += changed_lines;
        if folder.rewrite_in_progress {
            folder.llm_buffer.push(new_head.clone());
        } else {
            folder.llm_candidates.push(new_head.clone());
            if folder.llm_candidates.len() == 1 {
                folder.first_candidate_birthday = Some(now_ms());
            }
        }
        folder.last_head_hash = Some(new_head);
        !folder.rewrite_in_progress && folder.lines_changed >= threshold
    };
    save_store(&state)?;

    if should_rewrite {
        let app_clone = app.clone();
        let folder_clone = folder_path.clone();
        thread::spawn(move || {
            if let Err(err) = run_llm_commit_rewrite(app_clone, folder_clone, false) {
                eprintln!("[autoCommit] rewrite failed: {err}");
            }
        });
    }
    Ok(true)
}

fn current_date_string() -> String {
    let output = Command::new("date")
        .arg("+%Y-%m-%d")
        .output()
        .ok()
        .and_then(|out| String::from_utf8(out.stdout).ok())
        .unwrap_or_else(|| "1970-01-01".to_string());
    output.trim().to_string()
}

fn ignored_globset() -> GlobSet {
    let mut builder = GlobSetBuilder::new();
    for pat in IGNORED_NAMES.iter().chain(MONITOR_DEFAULT_IGNORES.iter()) {
        let pattern = if pat.contains('/') || pat.contains('*') {
            (*pat).to_string()
        } else {
            format!("**/{pat}")
        };
        if let Ok(glob) = Glob::new(&pattern) {
            builder.add(glob);
        }
    }
    builder
        .build()
        .unwrap_or_else(|_| GlobSetBuilder::new().build().unwrap())
}

fn normalized_relative_path(folder_path: &str, path: &Path) -> Option<String> {
    let rel = path.strip_prefix(folder_path).unwrap_or(path);
    let rel_str = rel.to_string_lossy().replace('\\', "/");
    let rel_str = rel_str
        .trim_start_matches("./")
        .trim_matches('/')
        .to_string();
    if rel_str.is_empty() {
        None
    } else {
        Some(rel_str)
    }
}

fn matches_repo_relative_path(rel_path: &str, pattern: &str) -> bool {
    let pattern = pattern.trim_matches('/');
    rel_path == pattern || rel_path.starts_with(&format!("{pattern}/"))
}

fn tauri_build_ignore_for_path(folder_path: &str, path: &Path) -> Option<&'static str> {
    let rel_path = normalized_relative_path(folder_path, path)?;
    TAURI_BUILD_IGNORES.iter().copied().find(|pattern| {
        if pattern.contains('/') {
            matches_repo_relative_path(&rel_path, pattern)
        } else {
            rel_path.split('/').any(|part| part == *pattern)
        }
    })
}

fn is_default_ignored(folder_path: &str, path: &Path) -> bool {
    let rel = path.strip_prefix(folder_path).unwrap_or(path);
    let rel_str = rel.to_string_lossy();
    if rel_str.is_empty() {
        return false;
    }
    if tauri_build_ignore_for_path(folder_path, path).is_some() {
        return true;
    }
    if rel.components().any(|c| {
        let c = c.as_os_str().to_string_lossy();
        c == ".git" || c == "node_modules" || c == "dist-tauri" || c == "target"
    }) {
        return true;
    }
    ignored_globset().is_match(rel)
}

fn gitignore_ignores(folder_path: &str, path: &Path) -> bool {
    let mut builder = GitignoreBuilder::new(folder_path);
    let gitignore = Path::new(folder_path).join(".gitignore");
    if gitignore.exists() {
        let _ = builder.add(gitignore);
    }
    let Ok(ig) = builder.build() else {
        return false;
    };
    let rel = path.strip_prefix(folder_path).unwrap_or(path);
    ig.matched_path_or_any_parents(rel, path.is_dir())
        .is_ignore()
}

fn should_ignore_path(folder_path: &str, path: &Path) -> bool {
    is_default_ignored(folder_path, path) || gitignore_ignores(folder_path, path)
}

fn ensure_in_gitignore(folder_path: &str, pattern: &str) -> CommandResult<bool> {
    let gitignore_path = Path::new(folder_path).join(".gitignore");
    let mut lines = if gitignore_path.exists() {
        fs::read_to_string(&gitignore_path)
            .map_err(|e| e.to_string())?
            .lines()
            .map(|s| s.to_string())
            .collect::<Vec<_>>()
    } else {
        Vec::new()
    };
    if lines.iter().any(|line| line.trim() == pattern) {
        return Ok(false);
    }
    lines.push(pattern.to_string());
    fs::write(gitignore_path, lines.join("\n") + "\n").map_err(|e| e.to_string())?;
    Ok(true)
}

fn file_name_matches_ignore(name: &str, pattern: &str) -> bool {
    let normalized = pattern.trim_end_matches('/');
    if normalized.contains('*') {
        Glob::new(normalized)
            .ok()
            .and_then(|g| g.compile_matcher().is_match(name).then_some(()))
            .is_some()
    } else {
        name == normalized
    }
}

fn exceeds_file_limit(folder_path: &str, limit: usize) -> bool {
    let mut count = 0usize;
    let mut stack = vec![PathBuf::from(folder_path)];
    while let Some(current) = stack.pop() {
        let Ok(entries) = fs::read_dir(&current) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if should_ignore_path(folder_path, &path) {
                continue;
            }
            if path.is_dir() {
                stack.push(path);
            } else {
                count += 1;
                if count > limit {
                    return true;
                }
            }
        }
    }
    false
}

fn start_monitoring_watcher(
    app: AppHandle,
    folder_path: String,
    skip_initial_check: bool,
) -> CommandResult<()> {
    let state = app.state::<AppState>();
    if state
        .watchers
        .lock()
        .map_err(|e| e.to_string())?
        .contains_key(&folder_path)
    {
        return Ok(());
    }
    if !is_git_repo_path(&folder_path) {
        return Ok(());
    }
    if exceeds_file_limit(&folder_path, 20_000) {
        {
            let mut store = state.store.lock().map_err(|e| e.to_string())?;
            for folder in &mut store.folders {
                if folder.path == folder_path {
                    folder.monitoring = false;
                }
            }
        }
        save_store(&state)?;
        emit(
            &app,
            "monitoring-error",
            json!({ "path": folder_path, "code": "TOO_MANY_FILES" }),
        );
        return Ok(());
    }

    let app_for_watcher = app.clone();
    let watched_folder = folder_path.clone();
    let mut watcher = RecommendedWatcher::new(
        move |result: notify::Result<notify::Event>| match result {
            Ok(event) => {
                if !matches!(
                    event.kind,
                    EventKind::Create(_) | EventKind::Modify(_) | EventKind::Remove(_)
                ) {
                    return;
                }
                let mut relevant = false;
                let state = app_for_watcher.state::<AppState>();
                {
                    let mut pending = match state.pending.lock() {
                        Ok(guard) => guard,
                        Err(_) => return,
                    };
                    let entry = pending.entry(watched_folder.clone()).or_default();
                    for path in event.paths {
                        let is_tauri_build_path =
                            tauri_build_ignore_for_path(&watched_folder, &path).is_some();
                        if !is_tauri_build_path && should_ignore_path(&watched_folder, &path) {
                            continue;
                        }
                        if let Some(name) = path.file_name().and_then(|s| s.to_str()) {
                            entry.names.insert(name.to_string());
                        }
                        entry.paths.insert(path);
                        relevant = true;
                    }
                }
                if relevant {
                    schedule_pending_processing(app_for_watcher.clone(), watched_folder.clone());
                }
            }
            Err(err) => {
                eprintln!("[MONITOR] watcher error for {watched_folder}: {err}");
            }
        },
        NotifyConfig::default(),
    )
    .map_err(|e| e.to_string())?;
    watcher
        .watch(Path::new(&folder_path), RecursiveMode::Recursive)
        .map_err(|e| e.to_string())?;
    state
        .watchers
        .lock()
        .map_err(|e| e.to_string())?
        .insert(folder_path.clone(), watcher);

    if !skip_initial_check {
        schedule_pending_processing(app.clone(), folder_path.clone());
    }
    debug(format!("[MONITOR] Watcher active for {folder_path}"));
    Ok(())
}

fn stop_monitoring_watcher(state: &AppState, folder_path: &str) {
    if let Ok(mut watchers) = state.watchers.lock() {
        watchers.remove(folder_path);
    }
}

fn schedule_pending_processing(app: AppHandle, folder_path: String) {
    let state = app.state::<AppState>();
    {
        let mut active = match state.active.lock() {
            Ok(guard) => guard,
            Err(_) => return,
        };
        if active.contains(&folder_path) {
            return;
        }
        active.insert(folder_path.clone());
    }
    thread::spawn(move || {
        thread::sleep(Duration::from_millis(550));
        if let Err(err) = process_pending_changes(app.clone(), folder_path.clone()) {
            eprintln!("[MONITOR] process pending failed for {folder_path}: {err}");
        }
        let state = app.state::<AppState>();
        {
            if let Ok(mut active) = state.active.lock() {
                active.remove(&folder_path);
            };
        }
    });
}

fn process_pending_changes(app: AppHandle, folder_path: String) -> CommandResult<()> {
    let state = app.state::<AppState>();
    let pending = {
        let mut pending_map = state.pending.lock().map_err(|e| e.to_string())?;
        pending_map.remove(&folder_path).unwrap_or_default()
    };
    let PendingChanges { names, paths } = pending;

    let mut tauri_patterns = HashSet::new();
    for path in &paths {
        if let Some(pattern) = tauri_build_ignore_for_path(&folder_path, path) {
            tauri_patterns.insert(pattern);
        }
    }
    for pattern in &tauri_patterns {
        let _ = ensure_in_gitignore(&folder_path, pattern);
    }

    let has_tauri_target = tauri_patterns.contains(&"src-tauri/target");

    for name in names {
        if has_tauri_target && name == "target" {
            continue;
        }
        for pattern in IGNORED_NAMES {
            if file_name_matches_ignore(&name, pattern) {
                let _ = ensure_in_gitignore(&folder_path, pattern);
            }
        }
    }

    if is_git_repo_path(&folder_path) {
        let status = git_status(&folder_path)?;
        if has_status_changes(&status) {
            let msg = build_commit_message_from_status(&status, "auto-git: ");
            if auto_commit(app.clone(), folder_path.clone(), msg)? {
                emit(&app, "repo-updated", folder_path);
            }
        }
    }
    Ok(())
}

fn add_folder_by_path_internal(
    app: &AppHandle,
    state: &AppState,
    new_folder: String,
) -> CommandResult<Vec<FolderObj>> {
    let is_repo = is_git_repo_path(&new_folder);
    let last_head_hash = if is_repo {
        run_git(&new_folder, &["rev-parse", "HEAD"])
            .ok()
            .map(|s| s.trim().to_string())
    } else {
        None
    };

    {
        let mut store = state.store.lock().map_err(|e| e.to_string())?;
        if let Some(folder) = store.folders.iter_mut().find(|f| f.path == new_folder) {
            folder.last_head_hash = last_head_hash;
            folder.monitoring = folder.monitoring && is_repo;
            folder.llm_buffer = folder.llm_buffer.clone();
            folder.llm_candidates = folder.llm_candidates.clone();
        } else {
            store.folders.push(FolderObj {
                path: new_folder.clone(),
                monitoring: is_repo,
                needs_relocation: false,
                lines_changed: 0,
                llm_candidates: Vec::new(),
                llm_buffer: Vec::new(),
                first_candidate_birthday: None,
                last_head_hash,
                rewrite_in_progress: false,
                rewrite_started_at: None,
            });
        }
        store.selected = Some(new_folder.clone());
    }
    save_store(state)?;
    if is_repo {
        start_monitoring_watcher(app.clone(), new_folder, false)?;
    }
    update_tray_menu(app)?;
    Ok(state
        .store
        .lock()
        .map_err(|e| e.to_string())?
        .folders
        .clone())
}

fn update_folders_listener(app: &AppHandle) -> CommandResult<()> {
    let state = app.state::<AppState>();
    let mut changed_folders = Vec::new();
    let now = now_ms();
    {
        let mut store = state.store.lock().map_err(|e| e.to_string())?;
        let minutes_threshold = store.minutes_commit_threshold;
        for folder in &mut store.folders {
            if folder.rewrite_in_progress {
                let in_rebase = is_rebase_in_progress(&folder.path);
                let started = folder.rewrite_started_at.unwrap_or(0);
                if !in_rebase && now - started > 2 * 60 * 1000 {
                    let mut merged = folder.llm_candidates.clone();
                    merged.extend(folder.llm_buffer.clone());
                    folder.rewrite_in_progress = false;
                    folder.rewrite_started_at = None;
                    folder.llm_buffer.clear();
                    folder.llm_candidates = merged;
                    folder.first_candidate_birthday = if folder.llm_candidates.is_empty() {
                        None
                    } else {
                        Some(now_ms())
                    };
                }
            }
            if let Some(birthday) = folder.first_candidate_birthday {
                if !folder.rewrite_in_progress
                    && ((now - birthday) as f64 / 1000.0 / 60.0) >= minutes_threshold as f64
                {
                    let app_clone = app.clone();
                    let folder_path = folder.path.clone();
                    thread::spawn(move || {
                        let _ = run_llm_commit_rewrite(app_clone, folder_path, false);
                    });
                }
            }

            let was_relocated = folder.needs_relocation;
            let now_exists = Path::new(&folder.path).exists();
            if was_relocated && now_exists {
                let hash_found = folder
                    .last_head_hash
                    .as_ref()
                    .and_then(|hash| run_git(&folder.path, &["branch", "--contains", hash]).ok())
                    .map(|raw| !raw.trim().is_empty())
                    .unwrap_or(false);
                if hash_found {
                    folder.needs_relocation = false;
                    changed_folders.push(folder.clone());
                } else {
                    folder.needs_relocation = true;
                }
            } else if !now_exists && !was_relocated {
                folder.needs_relocation = true;
                folder.monitoring = false;
                changed_folders.push(folder.clone());
            }
        }
    }

    if !changed_folders.is_empty() {
        save_store(&state)?;
        for folder in changed_folders {
            if folder.needs_relocation {
                stop_monitoring_watcher(&state, &folder.path);
            }
            emit(app, "folders-location-updated", folder);
        }
        update_tray_menu(app)?;
    }
    Ok(())
}

fn build_app_menu(app: &tauri::App) -> tauri::Result<()> {
    let settings = MenuItem::with_id(app, "settings", "Settings", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
    let app_menu = Submenu::with_items(app, "Auto-Git", true, &[&settings, &quit])?;
    let undo = PredefinedMenuItem::undo(app, None)?;
    let redo = PredefinedMenuItem::redo(app, None)?;
    let separator = PredefinedMenuItem::separator(app)?;
    let cut = PredefinedMenuItem::cut(app, None)?;
    let copy = PredefinedMenuItem::copy(app, None)?;
    let paste = PredefinedMenuItem::paste(app, None)?;
    let select_all = PredefinedMenuItem::select_all(app, None)?;
    let edit_submenu = Submenu::with_items(
        app,
        "Edit",
        true,
        &[&undo, &redo, &separator, &cut, &copy, &paste, &select_all],
    )?;
    let menu = Menu::with_items(app, &[&app_menu, &edit_submenu])?;
    app.set_menu(menu)?;
    Ok(())
}

fn menu_id(prefix: &str, path: &str) -> String {
    format!("{prefix}_{}", URL_SAFE_NO_PAD.encode(path))
}

fn update_tray_menu(app: &AppHandle) -> CommandResult<()> {
    let state = app.state::<AppState>();
    let folders = state
        .store
        .lock()
        .map_err(|e| e.to_string())?
        .folders
        .clone();
    let mut actions = HashMap::new();
    actions.insert("tray_open".to_string(), MenuAction::TrayOpen);
    actions.insert("tray_add".to_string(), MenuAction::TrayAdd);
    actions.insert("tray_start_all".to_string(), MenuAction::TrayStartAll);
    actions.insert("tray_stop_all".to_string(), MenuAction::TrayStopAll);
    actions.insert("tray_quit".to_string(), MenuAction::Quit);

    let mut items: Vec<Box<dyn tauri::menu::IsMenuItem<tauri::Wry>>> = Vec::new();
    items.push(Box::new(
        MenuItem::with_id(app, "tray_open", "Auto-Git öffnen", true, None::<&str>)
            .map_err(|e| e.to_string())?,
    ));
    items.push(Box::new(
        PredefinedMenuItem::separator(app).map_err(|e| e.to_string())?,
    ));

    for folder in &folders {
        let label = format!(
            "{} {}",
            if folder.monitoring { "●" } else { "○" },
            Path::new(&folder.path)
                .file_name()
                .and_then(|s| s.to_str())
                .unwrap_or(&folder.path)
        );
        let toggle_id = menu_id("tray_toggle", &folder.path);
        let remove_id = menu_id("tray_remove", &folder.path);
        actions.insert(
            toggle_id.clone(),
            MenuAction::TrayToggle(folder.path.clone()),
        );
        actions.insert(
            remove_id.clone(),
            MenuAction::TrayRemove(folder.path.clone()),
        );
        let toggle = MenuItem::with_id(
            app,
            toggle_id,
            if folder.monitoring {
                "Monitoring stoppen"
            } else {
                "Monitoring starten"
            },
            !folder.needs_relocation,
            None::<&str>,
        )
        .map_err(|e| e.to_string())?;
        let remove = MenuItem::with_id(app, remove_id, "Ordner entfernen", true, None::<&str>)
            .map_err(|e| e.to_string())?;
        let submenu = Submenu::with_items(app, label, true, &[&toggle, &remove])
            .map_err(|e| e.to_string())?;
        items.push(Box::new(submenu));
    }

    items.push(Box::new(
        PredefinedMenuItem::separator(app).map_err(|e| e.to_string())?,
    ));
    items.push(Box::new(
        MenuItem::with_id(
            app,
            "tray_add",
            "Neuen Ordner hinzufügen",
            true,
            None::<&str>,
        )
        .map_err(|e| e.to_string())?,
    ));
    items.push(Box::new(
        MenuItem::with_id(
            app,
            "tray_start_all",
            "Alle Monitorings starten",
            true,
            None::<&str>,
        )
        .map_err(|e| e.to_string())?,
    ));
    items.push(Box::new(
        MenuItem::with_id(
            app,
            "tray_stop_all",
            "Alle Monitorings stoppen",
            true,
            None::<&str>,
        )
        .map_err(|e| e.to_string())?,
    ));
    items.push(Box::new(
        PredefinedMenuItem::separator(app).map_err(|e| e.to_string())?,
    ));
    items.push(Box::new(
        MenuItem::with_id(app, "tray_quit", "Beenden", true, None::<&str>)
            .map_err(|e| e.to_string())?,
    ));

    let refs: Vec<&dyn tauri::menu::IsMenuItem<tauri::Wry>> =
        items.iter().map(|item| item.as_ref()).collect();
    let menu = Menu::with_items(app, &refs).map_err(|e| e.to_string())?;
    *state.menu_actions.lock().map_err(|e| e.to_string())? = actions;

    let mut tray_guard = state.tray.lock().map_err(|e| e.to_string())?;
    if let Some(tray) = tray_guard.as_ref() {
        tray.set_menu(Some(menu)).map_err(|e| e.to_string())?;
    } else {
        let mut builder = TrayIconBuilder::with_id("main-tray")
            .tooltip("Auto-Git läuft im Hintergrund")
            .menu(&menu)
            .show_menu_on_left_click(false);
        if let Some(icon) = app.default_window_icon() {
            builder = builder.icon(icon.clone());
        }
        let tray = builder
            .on_tray_icon_event(|tray: &TrayIcon<tauri::Wry>, event| {
                if let TrayIconEvent::Click {
                    button: MouseButton::Left,
                    button_state: MouseButtonState::Up,
                    ..
                } = event
                {
                    if let Some(app) = tray.app_handle().get_webview_window("main") {
                        let _ = app.show();
                        let _ = app.set_focus();
                    }
                }
            })
            .build(app)
            .map_err(|e| e.to_string())?;
        *tray_guard = Some(tray);
    }
    Ok(())
}

fn open_settings_window(app: &AppHandle) -> CommandResult<()> {
    if let Some(win) = app.get_webview_window("settings") {
        win.show().map_err(|e| e.to_string())?;
        win.set_focus().map_err(|e| e.to_string())?;
        return Ok(());
    }
    WebviewWindowBuilder::new(app, "settings", WebviewUrl::App("settings.html".into()))
        .title("Einstellungen")
        .inner_size(600.0, 500.0)
        .resizable(false)
        .build()
        .map_err(|e| e.to_string())?;
    Ok(())
}

fn handle_menu_action(app: &AppHandle, action: MenuAction) {
    let state = app.state::<AppState>();
    match action {
        MenuAction::TrayOpen => {
            if let Some(win) = app.get_webview_window("main") {
                let _ = win.show();
                let _ = win.set_focus();
            }
        }
        MenuAction::TrayToggle(path) => emit(app, "tray-toggle-monitoring", path),
        MenuAction::TrayRemove(path) => emit(app, "tray-remove-folder", path),
        MenuAction::TrayAdd => emit(app, "tray-add-folder", ()),
        MenuAction::TrayStartAll => {
            if let Ok(store) = state.store.lock() {
                for folder in &store.folders {
                    if !folder.monitoring && !folder.needs_relocation {
                        emit(app, "tray-toggle-monitoring", folder.path.clone());
                    }
                }
            }
        }
        MenuAction::TrayStopAll => {
            if let Ok(store) = state.store.lock() {
                for folder in &store.folders {
                    if folder.monitoring {
                        emit(app, "tray-toggle-monitoring", folder.path.clone());
                    }
                }
            }
        }
        MenuAction::Quit => {
            state.quitting.store(true, Ordering::SeqCst);
            app.exit(0);
        }
        MenuAction::Settings => {
            let _ = open_settings_window(app);
        }
        MenuAction::ContextOpen(path) => {
            let _ = open::that(path);
        }
        MenuAction::ContextCopy(path) => {
            if let Ok(mut clipboard) = Clipboard::new() {
                let _ = clipboard.set_text(path.to_string_lossy().to_string());
            }
        }
        MenuAction::ContextGitignore { root, rel } => {
            let gitignore = root.join(".gitignore");
            if let Err(err) = fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(gitignore)
                .and_then(|mut file| writeln!(file, "\n{rel}"))
            {
                eprintln!("Konnte nicht zu .gitignore hinzufügen: {err}");
            }
        }
    }
}

fn get_selected_internal(state: &AppState) -> CommandResult<Option<FolderObj>> {
    let store = state.store.lock().map_err(|e| e.to_string())?;
    Ok(store
        .selected
        .as_ref()
        .and_then(|selected| store.folders.iter().find(|f| &f.path == selected).cloned()))
}

#[tauri::command]
fn get_selected(state: tauri::State<'_, AppState>) -> CommandResult<Option<FolderObj>> {
    get_selected_internal(&state)
}

#[tauri::command]
fn set_selected(
    state: tauri::State<'_, AppState>,
    folder_obj_or_path: Value,
) -> CommandResult<Option<FolderObj>> {
    let folder_path = if let Some(path) = folder_obj_or_path.as_str() {
        path.to_string()
    } else {
        folder_obj_or_path
            .get("path")
            .and_then(Value::as_str)
            .ok_or_else(|| "missing folder path".to_string())?
            .to_string()
    };
    {
        let mut store = state.store.lock().map_err(|e| e.to_string())?;
        store.selected = Some(folder_path.clone());
    }
    save_store(&state)?;
    get_selected_internal(&state)
}

#[tauri::command]
fn get_folders(state: tauri::State<'_, AppState>) -> CommandResult<Vec<FolderObj>> {
    Ok(state
        .store
        .lock()
        .map_err(|e| e.to_string())?
        .folders
        .clone())
}

#[tauri::command]
fn add_folder(app: AppHandle, state: tauri::State<'_, AppState>) -> CommandResult<Vec<FolderObj>> {
    let Some(path) = rfd::FileDialog::new().pick_folder() else {
        return Ok(state
            .store
            .lock()
            .map_err(|e| e.to_string())?
            .folders
            .clone());
    };
    add_folder_by_path_internal(&app, &state, path.to_string_lossy().to_string())
}

#[tauri::command]
fn add_folder_by_path(
    app: AppHandle,
    state: tauri::State<'_, AppState>,
    folder_path: String,
) -> CommandResult<Vec<FolderObj>> {
    add_folder_by_path_internal(&app, &state, folder_path)
}

#[tauri::command]
fn remove_folder(
    app: AppHandle,
    state: tauri::State<'_, AppState>,
    folder_obj: FolderObj,
) -> CommandResult<Vec<FolderObj>> {
    {
        let mut store = state.store.lock().map_err(|e| e.to_string())?;
        store.folders.retain(|f| f.path != folder_obj.path);
        if store.selected.as_deref() == Some(&folder_obj.path) {
            store.selected = None;
        }
    }
    stop_monitoring_watcher(&state, &folder_obj.path);
    save_store(&state)?;
    update_tray_menu(&app)?;
    Ok(state
        .store
        .lock()
        .map_err(|e| e.to_string())?
        .folders
        .clone())
}

#[tauri::command]
fn get_commit_count(folder_obj: FolderObj) -> CommandResult<usize> {
    if folder_obj.needs_relocation || !Path::new(&folder_obj.path).join(".git").exists() {
        return Ok(0);
    }
    let raw = run_git(&folder_obj.path, &["rev-list", "--all", "--count"]).unwrap_or_default();
    Ok(raw.trim().parse::<usize>().unwrap_or(0))
}

#[tauri::command]
fn has_diffs(folder_obj: FolderObj) -> CommandResult<bool> {
    if folder_obj.needs_relocation || !is_git_repo_path(&folder_obj.path) {
        return Ok(false);
    }
    Ok(has_status_changes(&git_status(&folder_obj.path)?))
}

#[tauri::command]
fn remove_git_folder(folder_obj: FolderObj) -> CommandResult<()> {
    if !folder_obj.needs_relocation {
        let git_dir = Path::new(&folder_obj.path).join(".git");
        if git_dir.exists() {
            fs::remove_dir_all(git_dir).map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

#[tauri::command]
fn get_commits(
    folder_obj: FolderObj,
    page: Option<usize>,
    page_size: Option<usize>,
) -> CommandResult<CommitPage> {
    let page = page.unwrap_or(1).max(1);
    let page_size = page_size.unwrap_or(50).max(1);
    if folder_obj.needs_relocation || !Path::new(&folder_obj.path).exists() {
        return Ok(CommitPage {
            head: None,
            commits: Vec::new(),
            total: 0,
            page: 1,
            page_size,
            pages: 1,
        });
    }
    let total = run_git(&folder_obj.path, &["rev-list", "--all", "--count"])
        .ok()
        .and_then(|raw| raw.trim().parse::<usize>().ok())
        .unwrap_or(0);
    if total == 0 {
        return Ok(CommitPage {
            head: None,
            commits: Vec::new(),
            total: 0,
            page: 1,
            page_size,
            pages: 1,
        });
    }
    let skip = (page - 1) * page_size;
    let raw = run_git(
        &folder_obj.path,
        &[
            "log",
            "--all",
            &format!("--skip={skip}"),
            &format!("--max-count={page_size}"),
            "--date=iso-strict",
            "--format=%H%x1f%aI%x1f%s",
        ],
    )
    .unwrap_or_default();
    let commits = raw
        .lines()
        .filter_map(|line| {
            let mut parts = line.splitn(3, '\x1f');
            let hash = parts.next()?;
            let date = parts.next()?;
            let message = parts.next().unwrap_or("");
            Some(CommitSummary {
                hash: short_hash(hash),
                date: date.to_string(),
                message: message.to_string(),
            })
        })
        .collect();
    let head = run_git(&folder_obj.path, &["rev-parse", "--verify", "HEAD"])
        .ok()
        .map(|raw| short_hash(raw.trim()));
    let pages = (total + page_size - 1) / page_size;
    Ok(CommitPage {
        head,
        commits,
        total,
        page,
        page_size,
        pages: pages.max(1),
    })
}

#[tauri::command]
fn diff_commit(folder_obj: FolderObj, hash: String) -> CommandResult<Option<String>> {
    if folder_obj.needs_relocation || !Path::new(&folder_obj.path).exists() {
        return Ok(None);
    }
    Ok(Some(run_git(
        &folder_obj.path,
        &["diff", &format!("{hash}^!")],
    )?))
}

#[tauri::command]
fn revert_commit(folder_obj: FolderObj, hash: String) -> CommandResult<()> {
    if !folder_obj.needs_relocation && Path::new(&folder_obj.path).exists() {
        run_git(&folder_obj.path, &["revert", &hash, "--no-edit"])?;
    }
    Ok(())
}

#[tauri::command]
fn checkout_commit(folder_obj: FolderObj, hash: String) -> CommandResult<()> {
    if !folder_obj.needs_relocation && Path::new(&folder_obj.path).exists() {
        run_git(&folder_obj.path, &["checkout", &hash, "--force"])?;
    }
    Ok(())
}

#[tauri::command]
fn snapshot_commit(folder_obj: FolderObj, hash: String) -> CommandResult<Option<String>> {
    if folder_obj.needs_relocation || !Path::new(&folder_obj.path).exists() {
        return Ok(None);
    }
    let Some(out_dir) = rfd::FileDialog::new()
        .set_title("Ordner auswählen zum Speichern des Snapshots")
        .pick_folder()
    else {
        return Ok(None);
    };
    let base = Path::new(&folder_obj.path)
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("snapshot");
    let file_path = out_dir.join(format!("{base}-{hash}.zip"));
    run_process(
        "git",
        &[
            "-C".into(),
            folder_obj.path,
            "archive".into(),
            "--format".into(),
            "zip".into(),
            "--output".into(),
            file_path.to_string_lossy().to_string(),
            hash,
        ],
        None,
        None,
        None,
    )?;
    Ok(Some(file_path.to_string_lossy().to_string()))
}

#[tauri::command]
fn get_theme(state: tauri::State<'_, AppState>) -> CommandResult<String> {
    Ok(state.store.lock().map_err(|e| e.to_string())?.theme.clone())
}

#[tauri::command]
fn set_theme(app: AppHandle, state: tauri::State<'_, AppState>, val: String) -> CommandResult<()> {
    let theme = if VALID_THEMES.contains(&val.as_str()) {
        val
    } else {
        "default".to_string()
    };
    {
        let mut store = state.store.lock().map_err(|e| e.to_string())?;
        store.theme = theme.clone();
        store.skymode = theme == "sky";
    }
    save_store(&state)?;
    emit(&app, "theme-changed", theme);
    Ok(())
}

#[tauri::command]
fn get_skip_git_prompt(state: tauri::State<'_, AppState>) -> CommandResult<bool> {
    Ok(state
        .store
        .lock()
        .map_err(|e| e.to_string())?
        .skip_git_prompt)
}

#[tauri::command]
fn set_skip_git_prompt(state: tauri::State<'_, AppState>, val: bool) -> CommandResult<()> {
    state
        .store
        .lock()
        .map_err(|e| e.to_string())?
        .skip_git_prompt = val;
    save_store(&state)
}

#[tauri::command]
fn get_folder_tree(folder_path: String) -> CommandResult<Vec<TreeNode>> {
    fn walk(base: &Path, rel: &Path) -> Vec<TreeNode> {
        let full = base.join(rel);
        let mut nodes = Vec::new();
        let Ok(entries) = fs::read_dir(full) else {
            return nodes;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let name = entry.file_name().to_string_lossy().to_string();
            if [
                ".DS_Store",
                "node_modules",
                ".git",
                "dist",
                "dist-tauri",
                "build",
                ".cache",
                "out",
                ".venv",
                ".mypy_cache",
                "__pycache__",
                "package-lock.json",
            ]
            .contains(&name.as_str())
            {
                continue;
            }
            if path.is_dir() {
                nodes.push(TreeNode {
                    name: name.clone(),
                    node_type: "dir".to_string(),
                    children: Some(walk(base, &rel.join(&name))),
                });
            } else {
                nodes.push(TreeNode {
                    name,
                    node_type: "file".to_string(),
                    children: None,
                });
            }
        }
        nodes
    }
    Ok(walk(Path::new(&folder_path), Path::new(".")))
}

#[tauri::command]
fn commit_current_folder(folder_obj: FolderObj, message: Option<String>) -> CommandResult<Value> {
    if folder_obj.needs_relocation || !Path::new(&folder_obj.path).exists() {
        return Ok(json!({}));
    }
    let status = git_status(&folder_obj.path)?;
    if !has_status_changes(&status) {
        return Ok(json!({ "success": false, "error": "Nichts zu committen." }));
    }
    if is_rebase_in_progress(&folder_obj.path) {
        let _ = run_git(&folder_obj.path, &["rebase", "--abort"]);
    }
    run_git(&folder_obj.path, &["add", "-A"])?;
    run_git(
        &folder_obj.path,
        &["commit", "-m", message.as_deref().unwrap_or("test")],
    )?;
    Ok(json!({ "success": true }))
}

#[tauri::command]
fn set_monitoring(
    app: AppHandle,
    state: tauri::State<'_, AppState>,
    folder_path: String,
    mut monitoring: bool,
) -> CommandResult<bool> {
    {
        let mut store = state.store.lock().map_err(|e| e.to_string())?;
        let Some(folder) = store.folders.iter_mut().find(|f| f.path == folder_path) else {
            return Ok(false);
        };
        if folder.needs_relocation {
            return Ok(false);
        }
        if monitoring && !is_git_repo_path(&folder_path) {
            monitoring = false;
        }
        folder.monitoring = monitoring;
    }
    save_store(&state)?;
    if monitoring {
        start_monitoring_watcher(app.clone(), folder_path, false)?;
    } else {
        stop_monitoring_watcher(&state, &folder_path);
    }
    update_tray_menu(&app)?;
    Ok(monitoring)
}

#[tauri::command]
fn ollama_list() -> CommandResult<Value> {
    let output = run_process(
        "ollama",
        &["list".into(), "--json".into()],
        None,
        None,
        None,
    );
    match output {
        Ok(out) => {
            let mut models = Vec::new();
            for line in out.stdout.lines().filter(|l| !l.trim().is_empty()) {
                if let Ok(value) = serde_json::from_str::<Value>(line) {
                    models.push(value);
                }
            }
            if !models.is_empty() {
                return Ok(json!({ "status": "ok", "models": models }));
            }
            parse_ollama_list_plain()
        }
        Err(_) => parse_ollama_list_plain(),
    }
}

fn parse_ollama_list_plain() -> CommandResult<Value> {
    match run_process("ollama", &["list".into()], None, None, None) {
        Ok(out) => {
            let models: Vec<Value> = out
                .stdout
                .lines()
                .skip(1)
                .filter_map(|line| {
                    let name = line.split_whitespace().next()?;
                    Some(json!({ "name": name }))
                })
                .collect();
            Ok(json!({ "status": "ok", "models": models }))
        }
        Err(err) if err.contains("No such file") || err.contains("not found") => {
            Ok(json!({ "status": "no-cli" }))
        }
        Err(err) => Ok(json!({ "status": "error", "msg": err })),
    }
}

#[tauri::command]
fn ollama_pull(model: String) -> CommandResult<Value> {
    match run_process("ollama", &["pull".into(), model], None, None, None) {
        Ok(out) => Ok(json!({ "status": "ok", "msg": out.stdout })),
        Err(err) => Ok(json!({ "status": "error", "msg": err })),
    }
}

#[tauri::command]
fn get_commit_model(state: tauri::State<'_, AppState>) -> CommandResult<String> {
    Ok(state
        .store
        .lock()
        .map_err(|e| e.to_string())?
        .commit_model
        .clone()
        .unwrap_or_else(|| "qwen2.5-coder:7b".to_string()))
}

#[tauri::command]
fn set_commit_model(state: tauri::State<'_, AppState>, val: String) -> CommandResult<()> {
    state.store.lock().map_err(|e| e.to_string())?.commit_model = Some(val);
    save_store(&state)
}

#[tauri::command]
fn get_readme_model(state: tauri::State<'_, AppState>) -> CommandResult<String> {
    Ok(state
        .store
        .lock()
        .map_err(|e| e.to_string())?
        .readme_model
        .clone()
        .unwrap_or_else(|| "qwen2.5-coder:32b".to_string()))
}

#[tauri::command]
fn set_readme_model(state: tauri::State<'_, AppState>, val: String) -> CommandResult<()> {
    state.store.lock().map_err(|e| e.to_string())?.readme_model = Some(val);
    save_store(&state)
}

#[tauri::command]
fn get_intelligent_commit_threshold(state: tauri::State<'_, AppState>) -> CommandResult<i64> {
    Ok(state
        .store
        .lock()
        .map_err(|e| e.to_string())?
        .intelligent_commit_threshold)
}

#[tauri::command]
fn set_intelligent_commit_threshold(
    state: tauri::State<'_, AppState>,
    value: i64,
) -> CommandResult<()> {
    state
        .store
        .lock()
        .map_err(|e| e.to_string())?
        .intelligent_commit_threshold = value;
    save_store(&state)
}

#[tauri::command]
fn get_minutes_commit_threshold(state: tauri::State<'_, AppState>) -> CommandResult<i64> {
    Ok(state
        .store
        .lock()
        .map_err(|e| e.to_string())?
        .minutes_commit_threshold)
}

#[tauri::command]
fn set_minutes_commit_threshold(
    state: tauri::State<'_, AppState>,
    value: i64,
) -> CommandResult<()> {
    state
        .store
        .lock()
        .map_err(|e| e.to_string())?
        .minutes_commit_threshold = value;
    save_store(&state)
}

#[tauri::command]
fn get_autostart(state: tauri::State<'_, AppState>) -> CommandResult<bool> {
    Ok(state.store.lock().map_err(|e| e.to_string())?.autostart)
}

#[tauri::command]
fn set_autostart(state: tauri::State<'_, AppState>, enabled: bool) -> CommandResult<()> {
    state.store.lock().map_err(|e| e.to_string())?.autostart = enabled;
    save_store(&state)?;
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let auto = auto_launch::AutoLaunchBuilder::new()
        .set_app_name("Auto-Git")
        .set_app_path(exe.to_string_lossy().as_ref())
        .build()
        .map_err(|e| e.to_string())?;
    if enabled {
        auto.enable().map_err(|e| e.to_string())?;
    } else {
        auto.disable().map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[tauri::command]
fn get_close_to_tray(state: tauri::State<'_, AppState>) -> CommandResult<bool> {
    Ok(state.store.lock().map_err(|e| e.to_string())?.close_to_tray)
}

#[tauri::command]
fn set_close_to_tray(state: tauri::State<'_, AppState>, val: bool) -> CommandResult<()> {
    state.store.lock().map_err(|e| e.to_string())?.close_to_tray = val;
    save_store(&state)
}

#[tauri::command]
fn close_settings(app: AppHandle) -> CommandResult<()> {
    if let Some(win) = app.get_webview_window("settings") {
        win.close().map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[tauri::command]
fn is_git_repo(folder_path: String) -> CommandResult<bool> {
    Ok(is_git_repo_path(&folder_path))
}

#[tauri::command]
fn init_repo(
    app: AppHandle,
    state: tauri::State<'_, AppState>,
    folder_path: String,
) -> CommandResult<Value> {
    match init_git_repo_internal(&folder_path) {
        Ok(()) => {
            let last_head_hash = run_git(&folder_path, &["rev-parse", "HEAD"])
                .ok()
                .map(|s| s.trim().to_string());
            {
                let mut store = state.store.lock().map_err(|e| e.to_string())?;
                for folder in &mut store.folders {
                    if folder.path == folder_path {
                        folder.monitoring = true;
                        folder.needs_relocation = false;
                        folder.last_head_hash = last_head_hash.clone();
                    }
                }
            }
            save_store(&state)?;
            start_monitoring_watcher(app.clone(), folder_path, true)?;
            update_tray_menu(&app)?;
            Ok(json!({ "success": true }))
        }
        Err(err) => Ok(json!({ "success": false, "error": err })),
    }
}

#[tauri::command]
fn relocate_folder(
    state: tauri::State<'_, AppState>,
    old_path: String,
    new_path: String,
) -> CommandResult<Option<FolderObj>> {
    let mut updated = None;
    {
        let mut store = state.store.lock().map_err(|e| e.to_string())?;
        for folder in &mut store.folders {
            if folder.path == old_path {
                folder.path = new_path.clone();
                folder.needs_relocation = false;
                updated = Some(folder.clone());
            }
        }
    }
    save_store(&state)?;
    Ok(updated)
}

#[tauri::command]
fn pick_folder() -> CommandResult<Option<Vec<String>>> {
    Ok(rfd::FileDialog::new()
        .pick_folder()
        .map(|p| vec![p.to_string_lossy().to_string()]))
}

#[tauri::command]
fn repo_has_commit(repo_path: String, commit_hash: String) -> CommandResult<bool> {
    Ok(run_git(&repo_path, &["branch", "--contains", &commit_hash])
        .map(|raw| !raw.trim().is_empty())
        .unwrap_or(false))
}

#[tauri::command]
fn get_daily_commit_stats(
    state: tauri::State<'_, AppState>,
) -> CommandResult<HashMap<String, i64>> {
    Ok(state
        .store
        .lock()
        .map_err(|e| e.to_string())?
        .daily_commit_stats
        .clone())
}

#[tauri::command]
fn get_all_commit_hashes(folder_obj: FolderObj) -> CommandResult<Vec<String>> {
    if folder_obj.needs_relocation || !Path::new(&folder_obj.path).exists() {
        return Ok(Vec::new());
    }
    Ok(run_git(&folder_obj.path, &["log", "--all", "--format=%H"])
        .unwrap_or_default()
        .lines()
        .map(|s| s.to_string())
        .collect())
}

#[tauri::command]
fn trigger_rewrite_now(
    app: AppHandle,
    state: tauri::State<'_, AppState>,
    folder_path: String,
) -> CommandResult<Value> {
    {
        let mut store = state.store.lock().map_err(|e| e.to_string())?;
        let Some(folder) = store.folders.iter_mut().find(|f| f.path == folder_path) else {
            return Ok(json!({ "success": false, "error": "folder not found" }));
        };
        if folder.needs_relocation {
            return Ok(json!({ "success": false, "error": "needs relocation" }));
        }
        if folder.llm_candidates.is_empty() {
            return Ok(json!({ "success": false, "error": "no candidates" }));
        }
        folder.rewrite_in_progress = false;
        folder.rewrite_started_at = None;
        folder.lines_changed = 0;
        folder.first_candidate_birthday = Some(now_ms());
    }
    save_store(&state)?;
    thread::spawn(move || {
        let _ = run_llm_commit_rewrite(app, folder_path, true);
    });
    Ok(json!({ "success": true }))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct TreeContextInfo {
    abs_path: String,
    rel_path: String,
    root: String,
    #[serde(rename = "type")]
    node_type: String,
}

#[tauri::command]
fn show_folder_context_menu(
    app: AppHandle,
    window: WebviewWindow,
    state: tauri::State<'_, AppState>,
    folder_path: String,
) -> CommandResult<()> {
    let open_id = menu_id("ctx_open", &folder_path);
    let copy_id = menu_id("ctx_copy", &folder_path);
    {
        let mut actions = state.menu_actions.lock().map_err(|e| e.to_string())?;
        actions.insert(
            open_id.clone(),
            MenuAction::ContextOpen(PathBuf::from(&folder_path)),
        );
        actions.insert(
            copy_id.clone(),
            MenuAction::ContextCopy(PathBuf::from(&folder_path)),
        );
    }
    let open = MenuItem::with_id(&app, open_id, "Open Folder", true, None::<&str>)
        .map_err(|e| e.to_string())?;
    let copy = MenuItem::with_id(&app, copy_id, "Copy Folder Path", true, None::<&str>)
        .map_err(|e| e.to_string())?;
    let menu = Menu::with_items(&app, &[&open, &copy]).map_err(|e| e.to_string())?;
    window.popup_menu(&menu).map_err(|e| e.to_string())
}

#[tauri::command]
fn show_tree_context_menu(
    app: AppHandle,
    window: WebviewWindow,
    state: tauri::State<'_, AppState>,
    info: TreeContextInfo,
) -> CommandResult<()> {
    let open_id = menu_id("ctx_tree_open", &info.abs_path);
    let copy_id = menu_id("ctx_tree_copy", &info.abs_path);
    let ignore_id = menu_id("ctx_tree_ignore", &(info.root.clone() + &info.rel_path));
    {
        let mut actions = state.menu_actions.lock().map_err(|e| e.to_string())?;
        actions.insert(
            open_id.clone(),
            MenuAction::ContextOpen(PathBuf::from(&info.abs_path)),
        );
        actions.insert(
            copy_id.clone(),
            MenuAction::ContextCopy(PathBuf::from(&info.abs_path)),
        );
        actions.insert(
            ignore_id.clone(),
            MenuAction::ContextGitignore {
                root: PathBuf::from(&info.root),
                rel: info.rel_path.clone(),
            },
        );
    }
    let open_label = if info.node_type == "dir" {
        "Open Folder"
    } else {
        "Open File"
    };
    let copy_label = if info.node_type == "dir" {
        "Copy Folder Path"
    } else {
        "Copy File Path"
    };
    let open = MenuItem::with_id(&app, open_id, open_label, true, None::<&str>)
        .map_err(|e| e.to_string())?;
    let copy = MenuItem::with_id(&app, copy_id, copy_label, true, None::<&str>)
        .map_err(|e| e.to_string())?;
    let add_ignore = MenuItem::with_id(&app, ignore_id, "Add to .gitignore", true, None::<&str>)
        .map_err(|e| e.to_string())?;
    let sep = PredefinedMenuItem::separator(&app).map_err(|e| e.to_string())?;
    let menu =
        Menu::with_items(&app, &[&open, &copy, &sep, &add_ignore]).map_err(|e| e.to_string())?;
    window.popup_menu(&menu).map_err(|e| e.to_string())
}

fn has_readme_internal(folder_path: &str) -> bool {
    Path::new(folder_path).join("README.md").exists()
}

#[tauri::command]
fn has_readme(folder_path: String) -> CommandResult<bool> {
    Ok(has_readme_internal(&folder_path))
}

fn is_text_file(file_path: &Path) -> bool {
    let ext = file_path
        .extension()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_lowercase();
    if CODE_EXTS.contains(&ext.as_str()) {
        return true;
    }
    let Ok(meta) = fs::metadata(file_path) else {
        return false;
    };
    if meta.len() > 200 * 1024 {
        return false;
    }
    let Ok(bytes) = fs::read(file_path) else {
        return false;
    };
    !bytes.iter().take(400).any(|b| *b == 0)
}

fn gitignore_filter_ignores(base: &Path, path: &Path) -> bool {
    let mut builder = GitignoreBuilder::new(base);
    let gitignore = base.join(".gitignore");
    if gitignore.exists() {
        let _ = builder.add(gitignore);
    }
    let Ok(ig) = builder.build() else {
        return false;
    };
    let rel = path.strip_prefix(base).unwrap_or(path);
    ig.matched_path_or_any_parents(rel, path.is_dir())
        .is_ignore()
}

fn relevance_score(file_path: &Path, rel_path: &Path, content: &str) -> i64 {
    let base = file_path
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_lowercase();
    let rel = rel_path.to_string_lossy().to_lowercase();
    let mut score = 0;
    if base.starts_with("main.")
        || base.starts_with("index.")
        || base.starts_with("app.")
        || base.starts_with("server.")
    {
        score += 20;
    }
    if [
        "package.json",
        "requirements.txt",
        "pyproject.toml",
        "makefile",
        "cargo.toml",
    ]
    .contains(&base.as_str())
    {
        score += 20;
    }
    if rel_path.parent() == Some(Path::new("")) || rel_path.parent() == Some(Path::new(".")) {
        score += 10;
    }
    if rel.contains("test")
        || rel.contains("mock")
        || rel.contains("example")
        || rel.contains("spec")
        || rel.contains("demo")
    {
        score -= 30;
    }
    score += content.matches("export ").count() as i64 * 2;
    score += content.matches("module.exports").count() as i64 * 2;
    score += content.matches("function ").count() as i64;
    score += content.matches("class ").count() as i64;
    score += content.matches("\ndef ").count() as i64;
    if content.lines().count() < 20 {
        score -= 5;
    }
    if content.len() > 1500 {
        score += 2;
    }
    score
}

fn get_relevant_files(folder_path: &str, max_size: u64) -> Vec<PathBuf> {
    #[derive(Clone)]
    struct Candidate {
        path: PathBuf,
        size: u64,
        score: i64,
    }
    fn walk(base: &Path, current: &Path, out: &mut Vec<Candidate>) {
        let Ok(entries) = fs::read_dir(current) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if gitignore_filter_ignores(base, &path) {
                continue;
            }
            let name = entry.file_name().to_string_lossy().to_string();
            if path.is_dir() {
                if name.starts_with('.')
                    || ["node_modules", "dist", "dist-tauri", "target", "build"]
                        .contains(&name.as_str())
                {
                    continue;
                }
                walk(base, &path, out);
            } else if is_text_file(&path) {
                let size = fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
                let content = fs::read_to_string(&path).unwrap_or_default();
                let rel = path.strip_prefix(base).unwrap_or(&path).to_path_buf();
                let score = relevance_score(&path, &rel, &content);
                out.push(Candidate { path, size, score });
            }
        }
    }
    let base = Path::new(folder_path);
    let mut candidates = Vec::new();
    walk(base, base, &mut candidates);
    candidates.sort_by(|a, b| b.score.cmp(&a.score).then(a.size.cmp(&b.size)));
    let mut selected = Vec::new();
    let mut total = 0;
    for c in candidates {
        if total + c.size > max_size {
            break;
        }
        total += c.size;
        selected.push(c.path);
    }
    selected
}

#[tauri::command]
fn generate_readme(
    app: AppHandle,
    state: tauri::State<'_, AppState>,
    folder_path: String,
) -> CommandResult<String> {
    let (author, license, model) = {
        let store = state.store.lock().map_err(|e| e.to_string())?;
        (
            store
                .author
                .clone()
                .unwrap_or_else(|| "Unknown".to_string()),
            store.license.clone().unwrap_or_else(|| "MIT".to_string()),
            store
                .readme_model
                .clone()
                .unwrap_or_else(|| "qwen2.5-coder:32b".to_string()),
        )
    };
    let repo_name = Path::new(&folder_path)
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("Project")
        .to_string();
    let mut prompt = format!(
        r#"You are a tool that generates README.md files in markdown format.
Do not review, suggest, or improve the code.
Your only job is to create a clear and concise README in markdown, suitable for immediate use on GitHub.

IMPORTANT: The LICENSE is {license}.
NEVER add Contact Details.

Now write a similar README.md for the following project:

Author: {author}

Source Code:
"#
    );
    for file in get_relevant_files(&folder_path, 100 * 1024) {
        let rel = file.strip_prefix(&folder_path).unwrap_or(&file);
        let content = fs::read_to_string(&file).unwrap_or_default();
        prompt.push_str(&format!("\n---\nFile: {}\n{}\n", rel.display(), content));
    }
    prompt.push_str(&format!(
        "\n---\nWrite ONLY the complete README.md in markdown format. Remember, the license is {license}!"
    ));
    let result = stream_ollama(&prompt, &model, 0.4, &app)?;
    let cleaned = result
        .replace("```markdown", "")
        .replace("```md", "")
        .replace("```", "")
        .trim()
        .to_string();
    let disclaimer = "> ⚠️ **This README.md has been automatically generated using AI and might contain hallucinations or inaccuracies. Please proceed with caution!**\n\n";
    let final_text = format!("# {repo_name}\n\n**Author:** {author}\n\n{disclaimer}{cleaned}");
    fs::write(Path::new(&folder_path).join("README.md"), &final_text).map_err(|e| e.to_string())?;
    Ok(final_text)
}

fn get_commit_history_for_squash(repo_path: &str) -> CommandResult<Vec<SquashCommit>> {
    let merge_commits =
        run_git(repo_path, &["rev-list", "--min-parents=2", "HEAD"]).unwrap_or_default();
    if !merge_commits.trim().is_empty() {
        return Err("Smart squash currently only supports linear commit history.".to_string());
    }
    let raw = run_git(
        repo_path,
        &[
            "log",
            "--reverse",
            "--format=%H%x1f%T%x1f%P%x1f%ct%x1f%an%x1f%ae%x1f%aI%x1f%cn%x1f%ce%x1f%cI%x1f%B%x1e",
            "HEAD",
        ],
    )?;
    let mut commits = Vec::new();
    for entry in raw.split('\x1e').map(str::trim).filter(|s| !s.is_empty()) {
        let parts: Vec<_> = entry.split('\x1f').collect();
        if parts.len() < 11 {
            continue;
        }
        commits.push(SquashCommit {
            hash: parts[0].to_string(),
            tree: parts[1].to_string(),
            parents: parts[2].split_whitespace().map(|s| s.to_string()).collect(),
            timestamp_ms: parts[3].parse::<i64>().unwrap_or(0) * 1000,
            author_name: parts[4].to_string(),
            author_email: parts[5].to_string(),
            author_date: parts[6].to_string(),
            committer_name: parts[7].to_string(),
            committer_email: parts[8].to_string(),
            committer_date: parts[9].to_string(),
            message: parts[10..].join("\x1f").trim_end().to_string(),
        });
    }
    Ok(commits)
}

fn detect_squash_chunks(commits: &[SquashCommit]) -> Vec<Vec<SquashCommit>> {
    if commits.is_empty() {
        return Vec::new();
    }
    let mut chunks: Vec<Vec<SquashCommit>> = vec![vec![commits[0].clone()]];
    for pair in commits.windows(2) {
        let previous = &pair[0];
        let current = &pair[1];
        if (current.timestamp_ms - previous.timestamp_ms).abs() <= SQUASH_CHUNK_WINDOW_MS {
            chunks.last_mut().unwrap().push(current.clone());
        } else {
            chunks.push(vec![current.clone()]);
        }
    }
    chunks
}

fn build_squash_fallback_message(commits: &[SquashCommit]) -> String {
    let hashes = commits
        .iter()
        .map(|c| short_hash(&c.hash))
        .collect::<Vec<_>>();
    truncate_text(
        format!("auto-git: [squash] {}", hashes.join(", ")),
        MAX_SQUASH_COMMIT_MESSAGE_CHARS,
    )
}

fn sanitize_squash_commit_message(raw: &str, commits: &[SquashCommit]) -> String {
    let mut cleaned = normalize_single_line(raw)
        .trim_matches(|c| c == '"' || c == '\'' || c == '`')
        .to_string();
    let lower = cleaned.to_lowercase();
    if lower.starts_with("commit message:") {
        cleaned = cleaned["commit message:".len()..].trim().to_string();
    }
    if cleaned.is_empty() {
        cleaned = build_squash_fallback_message(commits);
    }
    truncate_text(cleaned, MAX_SQUASH_COMMIT_MESSAGE_CHARS)
}

fn generate_squash_commit_message_prompt(
    repo_path: &str,
    commits: &[SquashCommit],
) -> CommandResult<String> {
    let oldest = commits.first().ok_or_else(|| "no commits".to_string())?;
    let newest = commits.last().ok_or_else(|| "no commits".to_string())?;
    let diff_base = oldest
        .parents
        .first()
        .cloned()
        .unwrap_or_else(|| EMPTY_TREE_HASH.to_string());
    let name_status = run_git(
        repo_path,
        &["diff", "--name-status", &diff_base, &newest.hash],
    )?;
    let diff_stat = run_git(
        repo_path,
        &[
            "diff",
            "--stat",
            "--compact-summary",
            &diff_base,
            &newest.hash,
        ],
    )?;
    let mut omitted = 0;
    let commits_for_prompt: Vec<Value> = commits
        .iter()
        .map(|commit| {
            let normalized = normalize_single_line(&commit.message);
            if !normalized.is_empty() && normalized.len() <= MAX_SQUASH_PROMPT_MESSAGE_CHARS {
                json!({ "hash": short_hash(&commit.hash), "message": normalized })
            } else {
                if !normalized.is_empty() {
                    omitted += 1;
                }
                json!({ "hash": short_hash(&commit.hash) })
            }
        })
        .collect();
    let omission_note = if omitted > 0 {
        format!(
            "Some original commit messages were omitted because they were too long ({omitted})."
        )
    } else {
        String::new()
    };
    let prompt = format!(
        r#"Analyze the following git commits that will be squashed into one commit.
Generate one concise commit message summarizing the combined actual change.
- Output ONLY the literal commit message text.
- Do NOT add markdown, quotes, bullet points, or explanations.
- Keep it under 140 characters.

COMMITS:
{}

{}

CHANGED FILES:
{}

DIFF STAT:
{}"#,
        serde_json::to_string_pretty(&commits_for_prompt).map_err(|e| e.to_string())?,
        omission_note,
        {
            let block = truncate_prompt_block(name_status, MAX_SQUASH_NAME_STATUS_CHARS);
            if block.is_empty() {
                "(none)".to_string()
            } else {
                block
            }
        },
        {
            let block = truncate_prompt_block(diff_stat, MAX_SQUASH_DIFFSTAT_CHARS);
            if block.is_empty() {
                "(none)".to_string()
            } else {
                block
            }
        },
    );
    if prompt.len() > MAX_SQUASH_PROMPT_CHARS {
        return Err(format!(
            "Squash prompt too large ({} chars) for {repo_path}",
            prompt.len()
        ));
    }
    Ok(prompt)
}

fn generate_squash_commit_message(
    repo_path: &str,
    commits: &[SquashCommit],
    model: &str,
    app: &AppHandle,
) -> CommandResult<String> {
    let prompt = generate_squash_commit_message_prompt(repo_path, commits)?;
    let raw = stream_ollama(&prompt, model, 0.3, app)?;
    Ok(sanitize_squash_commit_message(&raw, commits))
}

fn build_squash_plan(
    repo_path: &str,
    chunks: Vec<Vec<SquashCommit>>,
    model: &str,
    app: &AppHandle,
) -> Vec<SquashPlanEntry> {
    chunks
        .into_iter()
        .map(|chunk| {
            if chunk.len() == 1 {
                SquashPlanEntry {
                    message: chunk[0].message.clone(),
                    commits: chunk,
                }
            } else {
                let message = generate_squash_commit_message(repo_path, &chunk, model, app)
                    .unwrap_or_else(|_| build_squash_fallback_message(&chunk));
                SquashPlanEntry {
                    commits: chunk,
                    message,
                }
            }
        })
        .collect()
}

fn smart_squash_commits(app: AppHandle, folder_path: String) -> CommandResult<Value> {
    if !Path::new(&folder_path).exists() {
        return Err("Folder not found.".to_string());
    }
    let state = app.state::<AppState>();
    let folder_obj = {
        let store = state.store.lock().map_err(|e| e.to_string())?;
        store
            .folders
            .iter()
            .find(|f| f.path == folder_path)
            .cloned()
            .ok_or_else(|| "Folder is not tracked by auto-git.".to_string())?
    };
    if folder_obj.needs_relocation {
        return Err("This folder needs to be relocated before squashing commits.".to_string());
    }
    if folder_obj.rewrite_in_progress {
        return Err("Another rewrite is already running for this repository.".to_string());
    }
    if is_rebase_in_progress(&folder_path) {
        return Err("A git rebase is already in progress for this repository.".to_string());
    }
    let current_branch = run_git(&folder_path, &["rev-parse", "--abbrev-ref", "HEAD"])?
        .trim()
        .to_string();
    if current_branch.is_empty() || current_branch == "HEAD" {
        return Err("Cannot squash commits while HEAD is detached.".to_string());
    }
    let commits = get_commit_history_for_squash(&folder_path)?;
    if commits.len() < 2 {
        return Ok(
            json!({ "success": true, "squashedChunks": 0, "removedCommits": 0, "message": "Not enough commits to squash." }),
        );
    }
    let chunks = detect_squash_chunks(&commits);
    let squashable = chunks.iter().filter(|chunk| chunk.len() > 1).count();
    if squashable == 0 {
        return Ok(
            json!({ "success": true, "squashedChunks": 0, "removedCommits": 0, "message": "No quick-succession commit chunks found." }),
        );
    }
    let original_head = run_git(&folder_path, &["rev-parse", "HEAD"])?
        .trim()
        .to_string();
    let status = git_status(&folder_path)?;
    let mut stashed = false;
    let mut stash_warning: Option<String> = None;
    if has_status_changes(&status) {
        run_git(&folder_path, &["stash", "push", "--include-untracked"])?;
        stashed = true;
    }

    let model = {
        let store = state.store.lock().map_err(|e| e.to_string())?;
        store
            .commit_model
            .clone()
            .unwrap_or_else(|| "qwen2.5-coder:7b".to_string())
    };
    let result = (|| -> CommandResult<String> {
        let plan = build_squash_plan(&folder_path, chunks, &model, &app);
        let mut new_head: Option<String> = None;
        for entry in plan {
            let last = entry.commits.last().unwrap();
            let mut env = HashMap::new();
            env.insert("GIT_AUTHOR_NAME".to_string(), last.author_name.clone());
            env.insert("GIT_AUTHOR_EMAIL".to_string(), last.author_email.clone());
            env.insert("GIT_AUTHOR_DATE".to_string(), last.author_date.clone());
            env.insert(
                "GIT_COMMITTER_NAME".to_string(),
                if last.committer_name.is_empty() {
                    last.author_name.clone()
                } else {
                    last.committer_name.clone()
                },
            );
            env.insert(
                "GIT_COMMITTER_EMAIL".to_string(),
                if last.committer_email.is_empty() {
                    last.author_email.clone()
                } else {
                    last.committer_email.clone()
                },
            );
            env.insert(
                "GIT_COMMITTER_DATE".to_string(),
                last.committer_date.clone(),
            );
            let mut args = vec!["commit-tree".to_string(), last.tree.clone()];
            if let Some(parent) = &new_head {
                args.push("-p".to_string());
                args.push(parent.clone());
            }
            let stdout = run_git_owned(
                &folder_path,
                &args,
                Some(&env),
                Some(&(entry.message + "\n")),
            )?;
            new_head = Some(stdout.trim().to_string());
        }
        let new_head =
            new_head.ok_or_else(|| "git commit-tree did not return a commit hash.".to_string())?;
        run_git(&folder_path, &["reset", "--hard", &new_head])?;
        Ok(new_head)
    })();

    match result {
        Ok(new_head) => {
            if stashed {
                if let Err(err) = run_git(&folder_path, &["stash", "pop"]) {
                    stash_warning = Some(err);
                }
            }
            {
                let mut store = state.store.lock().map_err(|e| e.to_string())?;
                if let Some(folder) = store.folders.iter_mut().find(|f| f.path == folder_path) {
                    folder.rewrite_in_progress = false;
                    folder.rewrite_started_at = None;
                    folder.llm_candidates.clear();
                    folder.llm_buffer.clear();
                    folder.lines_changed = 0;
                    folder.first_candidate_birthday = None;
                    folder.last_head_hash = Some(new_head);
                }
            }
            save_store(&state)?;
            emit(&app, "repo-updated", folder_path.clone());
            Ok(json!({
                "success": true,
                "squashedChunks": squashable,
                "removedCommits": commits.len() - build_squash_plan_for_removed_count(&commits),
                "warning": stash_warning
            }))
        }
        Err(err) => {
            let _ = run_git(&folder_path, &["reset", "--hard", &original_head]);
            if stashed {
                let _ = run_git(&folder_path, &["stash", "pop"]);
            }
            Err(err)
        }
    }
}

fn build_squash_plan_for_removed_count(commits: &[SquashCommit]) -> usize {
    detect_squash_chunks(commits)
        .iter()
        .map(|chunk| if chunk.is_empty() { 0 } else { 1 })
        .sum::<usize>()
}

#[tauri::command]
fn squash_commits(app: AppHandle, folder_path: String) -> CommandResult<Value> {
    match smart_squash_commits(app, folder_path) {
        Ok(value) => Ok(value),
        Err(err) => Ok(json!({ "success": false, "error": err })),
    }
}

fn generate_repo_description(
    app: &AppHandle,
    state: &AppState,
    folder_path: &str,
) -> CommandResult<String> {
    let repo_name = Path::new(folder_path)
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("repository");
    let top_files = get_relevant_files(folder_path, 100 * 1024)
        .into_iter()
        .take(5)
        .filter_map(|f| {
            f.strip_prefix(folder_path)
                .ok()
                .map(|p| p.to_string_lossy().to_string())
        })
        .collect::<Vec<_>>();
    let prompt = format!(
        "You are an assistant that writes a very short (<255 chars) description for a new Git repository.\nDo NOT exceed 255 characters and do NOT add markdown or commentary.\n\nProject name: {repo_name}\n\nKey files:\n{}\n\nWrite one concise sentence or two under 255 chars.",
        top_files.iter().map(|f| format!("- {f}")).collect::<Vec<_>>().join("\n")
    );
    let model = state
        .store
        .lock()
        .map_err(|e| e.to_string())?
        .readme_model
        .clone()
        .unwrap_or_else(|| "qwen2.5-coder:32b".to_string());
    let raw = stream_ollama(&prompt, &model, 0.3, app)?;
    Ok(truncate_text(
        raw.replace("```markdown", "").replace("```", ""),
        255,
    ))
}

#[tauri::command]
fn push_to_gitea(
    app: AppHandle,
    state: tauri::State<'_, AppState>,
    folder_path: String,
) -> CommandResult<Value> {
    let token = state
        .store
        .lock()
        .map_err(|e| e.to_string())?
        .gitea_token
        .clone();
    if token.is_empty() {
        return Ok(
            json!({ "success": false, "error": "No Gitea API token configured – open Settings and enter it first" }),
        );
    }
    let result = (|| -> CommandResult<Value> {
        let repo_name = Path::new(&folder_path)
            .file_name()
            .and_then(|s| s.to_str())
            .ok_or_else(|| "invalid repository path".to_string())?;
        let base = "https://giers10.uber.space/api/v1";
        let description = generate_repo_description(&app, &state, &folder_path)?;
        let client = Client::new();
        let user_resp = client
            .get(format!("{base}/user"))
            .header("Authorization", format!("token {token}"))
            .send()
            .map_err(|e| e.to_string())?;
        if !user_resp.status().is_success() {
            return Err(format!("/user request failed: {}", user_resp.status()));
        }
        let user: Value = user_resp.json().map_err(|e| e.to_string())?;
        let username = user
            .get("login")
            .and_then(Value::as_str)
            .ok_or_else(|| "Gitea user response missing login".to_string())?;
        let check = client
            .get(format!("{base}/repos/{username}/{repo_name}"))
            .header("Authorization", format!("token {token}"))
            .send()
            .map_err(|e| e.to_string())?;
        let repo_url = if check.status().as_u16() == 404 {
            let created: Value = client
                .post(format!("{base}/user/repos"))
                .header("Authorization", format!("token {token}"))
                .json(&json!({
                    "name": repo_name,
                    "description": description,
                    "private": false,
                    "auto_init": false
                }))
                .send()
                .map_err(|e| e.to_string())?
                .json()
                .map_err(|e| e.to_string())?;
            created
                .get("clone_url")
                .and_then(Value::as_str)
                .ok_or_else(|| "Gitea create response missing clone_url".to_string())?
                .to_string()
        } else if check.status().is_success() {
            let _ = client
                .patch(format!("{base}/repos/{username}/{repo_name}"))
                .header("Authorization", format!("token {token}"))
                .json(&json!({ "description": description }))
                .send();
            let existing: Value = client
                .get(format!("{base}/repos/{username}/{repo_name}"))
                .header("Authorization", format!("token {token}"))
                .send()
                .map_err(|e| e.to_string())?
                .json()
                .map_err(|e| e.to_string())?;
            existing
                .get("clone_url")
                .and_then(Value::as_str)
                .ok_or_else(|| "Gitea repo response missing clone_url".to_string())?
                .to_string()
        } else {
            return Err(format!("Error checking repo: {}", check.status()));
        };
        let _ = run_git(&folder_path, &["remote", "remove", "origin"]);
        run_git(&folder_path, &["remote", "add", "origin", &repo_url])?;
        let branch = run_git(&folder_path, &["rev-parse", "--abbrev-ref", "HEAD"])?
            .trim()
            .to_string();
        run_git(
            &folder_path,
            &["push", "-u", "origin", &branch, "--force", "--tags"],
        )?;
        Ok(json!({ "success": true, "repoUrl": repo_url }))
    })();
    Ok(result.unwrap_or_else(|err| json!({ "success": false, "error": err })))
}

#[tauri::command]
fn get_gitea_token(state: tauri::State<'_, AppState>) -> CommandResult<String> {
    Ok(state
        .store
        .lock()
        .map_err(|e| e.to_string())?
        .gitea_token
        .clone())
}

#[tauri::command]
fn set_gitea_token(state: tauri::State<'_, AppState>, token: String) -> CommandResult<()> {
    state.store.lock().map_err(|e| e.to_string())?.gitea_token = token;
    save_store(&state)
}

fn main() {
    let path = store_path();
    let store = load_store(&path);
    let state = AppState {
        store: Mutex::new(store),
        store_path: path,
        watchers: Mutex::new(HashMap::new()),
        pending: Mutex::new(HashMap::new()),
        active: Mutex::new(HashSet::new()),
        menu_actions: Mutex::new(HashMap::new()),
        quitting: AtomicBool::new(false),
        tray: Mutex::new(None),
    };

    tauri::Builder::default()
        .manage(state)
        .setup(|app| {
            build_app_menu(app)?;
            app.on_menu_event(|app_handle, event| {
                let id = event.id().0.as_str().to_string();
                let action = if id == "settings" {
                    Some(MenuAction::Settings)
                } else if id == "quit" {
                    Some(MenuAction::Quit)
                } else {
                    app_handle
                        .state::<AppState>()
                        .menu_actions
                        .lock()
                        .ok()
                        .and_then(|actions| actions.get(&id).cloned())
                };
                if let Some(action) = action {
                    handle_menu_action(app_handle, action);
                }
            });

            let app_handle = app.handle().clone();
            if let Err(err) = update_tray_menu(&app_handle) {
                eprintln!("[AutoGit] failed to build tray menu: {err}");
            }
            let folders = app
                .state::<AppState>()
                .store
                .lock()
                .map(|store| store.folders.clone())
                .unwrap_or_default();
            for folder in folders {
                if folder.monitoring {
                    let _ = start_monitoring_watcher(app_handle.clone(), folder.path, false);
                }
            }
            let monitor_app = app_handle.clone();
            thread::spawn(move || loop {
                let _ = update_folders_listener(&monitor_app);
                thread::sleep(Duration::from_secs(3));
            });
            Ok(())
        })
        .on_window_event(|window, event| {
            if window.label() == "main" {
                if let WindowEvent::CloseRequested { api, .. } = event {
                    let state = window.state::<AppState>();
                    let close_to_tray = state
                        .store
                        .lock()
                        .map(|store| store.close_to_tray)
                        .unwrap_or(false);
                    if close_to_tray && !state.quitting.load(Ordering::SeqCst) {
                        api.prevent_close();
                        let _ = window.hide();
                    }
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            get_selected,
            set_selected,
            get_folders,
            add_folder,
            add_folder_by_path,
            remove_folder,
            get_commit_count,
            has_diffs,
            remove_git_folder,
            get_commits,
            diff_commit,
            revert_commit,
            checkout_commit,
            snapshot_commit,
            get_theme,
            set_theme,
            get_skip_git_prompt,
            set_skip_git_prompt,
            get_folder_tree,
            commit_current_folder,
            set_monitoring,
            ollama_list,
            ollama_pull,
            get_commit_model,
            set_commit_model,
            get_readme_model,
            set_readme_model,
            get_intelligent_commit_threshold,
            set_intelligent_commit_threshold,
            get_minutes_commit_threshold,
            set_minutes_commit_threshold,
            get_autostart,
            set_autostart,
            get_close_to_tray,
            set_close_to_tray,
            close_settings,
            is_git_repo,
            init_repo,
            relocate_folder,
            pick_folder,
            repo_has_commit,
            get_daily_commit_stats,
            get_all_commit_hashes,
            trigger_rewrite_now,
            show_folder_context_menu,
            show_tree_context_menu,
            has_readme,
            generate_readme,
            squash_commits,
            push_to_gitea,
            get_gitea_token,
            set_gitea_token
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
