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
    io::{Read, Write},
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
    utils::config::Color,
    AppHandle, Emitter, Manager, TitleBarStyle, WebviewUrl, WebviewWindow, WebviewWindowBuilder,
    WindowEvent,
};
use tempfile::{NamedTempFile, TempDir};
use wait_timeout::ChildExt;

type CommandResult<T> = Result<T, String>;

const VALID_THEMES: &[&str] = &["sky", "default", "grey"];
const VALID_REWORD_MODES: &[&str] = &["ask", "auto", "manual"];
const EMPTY_TREE_HASH: &str = "4b825dc642cb6eb9a060e54bf8d69288fbee4904";
const SQUASH_CHUNK_WINDOW_MS: i64 = 2 * 60 * 1000;
const MAX_SQUASH_PROMPT_CHARS: usize = 25_000;
const MAX_SQUASH_PROMPT_MESSAGE_CHARS: usize = 400;
const MAX_SQUASH_NAME_STATUS_CHARS: usize = 6_000;
const MAX_SQUASH_DIFFSTAT_CHARS: usize = 4_000;
const MAX_SQUASH_COMMIT_MESSAGE_CHARS: usize = 160;
const OLLAMA_BASE_URL: &str = "http://127.0.0.1:11434";
const ROSE_TITLEBAR_COLOR: Color = Color(255, 241, 242, 255);
const GIT_PROCESS_TIMEOUT: Duration = Duration::from_secs(5 * 60);
// Automatic rewriting is safe to enable because all history mutation happens in an isolated
// worktree and the real branch is updated only after tree validation and a compare-and-swap.
const AUTO_HISTORY_REWRITE_ENABLED: bool = true;
const TRANSACTIONAL_SQUASH_ENABLED: bool = false;
const VISIBLE_HISTORY_REVISION: &str = "--branches";

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
    #[serde(default = "default_reword_mode")]
    reword_mode: String,
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
            reword_mode: default_reword_mode(),
            author: None,
            license: None,
        }
    }
}

fn default_theme() -> String {
    "sky".to_string()
}

fn default_reword_mode() -> String {
    "ask".to_string()
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
    pending_rewrite_count: usize,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct CommitSummary {
    hash: String,
    date: String,
    message: String,
    needs_rewrite: bool,
    can_reword: bool,
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
    repo_operations: Mutex<HashMap<String, String>>,
    menu_actions: Mutex<HashMap<String, MenuAction>>,
    quitting: AtomicBool,
    tray: Mutex<Option<TrayIcon>>,
}

struct RepoOperationGuard<'a> {
    state: &'a AppState,
    key: String,
}

impl Drop for RepoOperationGuard<'_> {
    fn drop(&mut self) {
        if let Ok(mut operations) = self.state.repo_operations.lock() {
            operations.remove(&self.key);
        }
    }
}

fn repo_key(repo_path: &str) -> String {
    fs::canonicalize(repo_path)
        .unwrap_or_else(|_| PathBuf::from(repo_path))
        .to_string_lossy()
        .to_string()
}

fn begin_repo_operation<'a>(
    state: &'a AppState,
    repo_path: &str,
    operation: &str,
) -> CommandResult<RepoOperationGuard<'a>> {
    let key = repo_key(repo_path);
    let mut operations = state.repo_operations.lock().map_err(|e| e.to_string())?;
    if let Some(active) = operations.get(&key) {
        return Err(format!(
            "Repository is busy with another Auto-Git operation: {active}"
        ));
    }
    operations.insert(key.clone(), operation.to_string());
    drop(operations);
    Ok(RepoOperationGuard { state, key })
}

fn repo_is_busy(state: &AppState, repo_path: &str) -> bool {
    let key = repo_key(repo_path);
    state
        .repo_operations
        .lock()
        .map(|operations| operations.contains_key(&key))
        .unwrap_or(true)
}

#[derive(Debug)]
struct CommandOutput {
    stdout: String,
}

#[derive(Clone, Debug)]
struct RepoSnapshot {
    branch_ref: String,
    head: String,
    tree: String,
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
    if !VALID_REWORD_MODES.contains(&store.reword_mode.as_str()) {
        store.reword_mode = default_reword_mode();
    }

    for folder in &mut store.folders {
        let repo_exists = Path::new(&folder.path).join(".git").exists();
        let path_exists = Path::new(&folder.path).exists();
        folder.needs_relocation = !path_exists;
        if !repo_exists || folder.needs_relocation {
            folder.monitoring = false;
        }
        // A persisted flag can only describe a process from a previous app lifetime. Never resume
        // a history mutation automatically after a crash or force quit.
        if folder.rewrite_in_progress {
            folder.rewrite_in_progress = false;
            folder.rewrite_started_at = None;
            folder.monitoring = false;
        }
    }
}

fn save_store(state: &AppState) -> CommandResult<()> {
    let store = state.store.lock().map_err(|e| e.to_string())?.clone();
    let parent = state
        .store_path
        .parent()
        .ok_or_else(|| "Configuration path has no parent directory.".to_string())?;
    fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    let data = serde_json::to_vec_pretty(&store).map_err(|e| e.to_string())?;
    let mut temp = NamedTempFile::new_in(parent).map_err(|e| e.to_string())?;
    temp.write_all(&data).map_err(|e| e.to_string())?;
    temp.flush().map_err(|e| e.to_string())?;
    temp.as_file().sync_all().map_err(|e| e.to_string())?;
    temp.persist(&state.store_path)
        .map(|_| ())
        .map_err(|e| e.error.to_string())
}

fn run_process(
    program: &str,
    args: &[String],
    cwd: Option<&Path>,
    env: Option<&HashMap<String, String>>,
    input: Option<&str>,
) -> CommandResult<CommandOutput> {
    run_process_with_timeout(program, args, cwd, env, input, GIT_PROCESS_TIMEOUT)
}

fn run_process_with_timeout(
    program: &str,
    args: &[String],
    cwd: Option<&Path>,
    env: Option<&HashMap<String, String>>,
    input: Option<&str>,
    timeout: Duration,
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
    drop(child.stdin.take());
    let stdout = child.stdout.take().map(|mut pipe| {
        thread::spawn(move || {
            let mut bytes = Vec::new();
            pipe.read_to_end(&mut bytes).map(|_| bytes)
        })
    });
    let stderr = child.stderr.take().map(|mut pipe| {
        thread::spawn(move || {
            let mut bytes = Vec::new();
            pipe.read_to_end(&mut bytes).map(|_| bytes)
        })
    });
    let status = match child.wait_timeout(timeout).map_err(|e| e.to_string())? {
        Some(status) => status,
        None => {
            let _ = child.kill();
            let _ = child.wait();
            return Err(format!(
                "{program} timed out after {} seconds",
                timeout.as_secs()
            ));
        }
    };
    let collect = |reader: Option<thread::JoinHandle<std::io::Result<Vec<u8>>>>| {
        reader
            .map(|handle| {
                handle
                    .join()
                    .map_err(|_| "process output reader panicked".to_string())?
                    .map_err(|e| e.to_string())
            })
            .transpose()
            .map(|bytes| bytes.unwrap_or_default())
    };
    let stdout = String::from_utf8_lossy(&collect(stdout)?).to_string();
    let stderr = String::from_utf8_lossy(&collect(stderr)?).to_string();
    if status.success() {
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

fn git_dir_path(repo_path: &str) -> CommandResult<PathBuf> {
    let raw = run_git(repo_path, &["rev-parse", "--absolute-git-dir"])?;
    Ok(PathBuf::from(raw.trim()))
}

fn active_git_operation(repo_path: &str) -> CommandResult<Option<String>> {
    let git_dir = git_dir_path(repo_path)?;
    let markers = [
        ("rebase-merge", "rebase"),
        ("rebase-apply", "rebase"),
        ("MERGE_HEAD", "merge"),
        ("CHERRY_PICK_HEAD", "cherry-pick"),
        ("REVERT_HEAD", "revert"),
        ("BISECT_LOG", "bisect"),
    ];
    Ok(markers
        .iter()
        .find(|(marker, _)| git_dir.join(marker).exists())
        .map(|(_, name)| (*name).to_string()))
}

fn validate_repo_state(repo_path: &str, require_clean: bool) -> CommandResult<()> {
    if let Some(operation) = active_git_operation(repo_path)? {
        return Err(format!(
            "Git {operation} is already in progress. Auto-Git left it untouched."
        ));
    }
    let unmerged = run_git(repo_path, &["diff", "--name-only", "--diff-filter=U"])?;
    if !unmerged.trim().is_empty() {
        return Err(
            "Repository has unresolved conflicts. Auto-Git left them untouched.".to_string(),
        );
    }
    if require_clean {
        let status = run_git(repo_path, &["status", "--porcelain"])?;
        if !status.trim().is_empty() {
            return Err(
                "This operation requires a clean worktree and index; Auto-Git did not stash anything."
                    .to_string(),
            );
        }
    }
    Ok(())
}

fn safe_repo_snapshot(repo_path: &str, require_clean: bool) -> CommandResult<RepoSnapshot> {
    validate_repo_state(repo_path, require_clean)?;
    let branch_ref = run_git(repo_path, &["symbolic-ref", "--quiet", "HEAD"])
        .map_err(|_| "HEAD is detached. Auto-Git will not change branches or commit.".to_string())?
        .trim()
        .to_string();
    let head = run_git(repo_path, &["rev-parse", "HEAD"])?
        .trim()
        .to_string();
    let tree = run_git(repo_path, &["rev-parse", "HEAD^{tree}"])?
        .trim()
        .to_string();
    Ok(RepoSnapshot {
        branch_ref,
        head,
        tree,
    })
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

fn changed_paths_for_monitoring(repo: &str) -> CommandResult<Vec<String>> {
    let raw = run_git(
        repo,
        &["status", "--porcelain=v1", "-z", "--untracked-files=all"],
    )?;
    let records: Vec<&str> = raw
        .split('\0')
        .filter(|record| !record.is_empty())
        .collect();
    let mut paths = Vec::new();
    let mut index = 0;
    while index < records.len() {
        let record = records[index];
        let bytes = record.as_bytes();
        if bytes.len() < 4 || bytes[2] != b' ' {
            index += 1;
            continue;
        }
        let status = &record[..2];
        paths.push(record[3..].to_string());
        if status.contains('R') || status.contains('C') {
            index += 1;
            if let Some(original_path) = records.get(index) {
                paths.push((*original_path).to_string());
            }
        }
        index += 1;
    }
    paths.sort();
    paths.dedup();
    Ok(paths)
}

fn has_status_changes(status: &GitStatus) -> bool {
    !status.not_added.is_empty()
        || !status.created.is_empty()
        || !status.modified.is_empty()
        || !status.deleted.is_empty()
        || !status.renamed.is_empty()
}

fn is_git_operation_in_progress(repo_path: &str) -> bool {
    active_git_operation(repo_path).ok().flatten().is_some()
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
        update_gitignore_from_existing_project(folder)?;
        run_git(folder, &["add", "-A"])?;
        run_git(folder, &["commit", "--allow-empty", "-m", "initial commit"])?;
    }
    Ok(())
}

fn short_hash(hash: &str) -> String {
    hash.chars().take(7).collect()
}

fn is_auto_git_message(message: &str) -> bool {
    message.trim_start().starts_with("auto-git")
}

fn resolve_commit_hash(repo_path: &str, hash: &str) -> Option<String> {
    run_git(
        repo_path,
        &["rev-parse", "--verify", &format!("{hash}^{{commit}}")],
    )
    .ok()
    .map(|raw| raw.trim().to_string())
    .filter(|resolved| !resolved.is_empty())
}

fn is_commit_on_current_history(repo_path: &str, hash: &str) -> bool {
    run_git(repo_path, &["merge-base", "--is-ancestor", hash, "HEAD"]).is_ok()
}

fn pending_rewrite_hashes(repo_path: &str, queued_hashes: &[String]) -> Vec<String> {
    let mut seen = HashSet::new();
    queued_hashes
        .iter()
        .filter_map(|hash| resolve_commit_hash(repo_path, hash))
        .filter(|hash| is_commit_on_current_history(repo_path, hash))
        .filter(|hash| seen.insert(hash.clone()))
        .collect()
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
    if client.get(OLLAMA_BASE_URL).send().is_ok() {
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
        if client.get(OLLAMA_BASE_URL).send().is_ok() {
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
    stream_ollama_request(prompt, model, temperature, app, false)
}

fn stream_ollama_json(
    prompt: &str,
    model: &str,
    temperature: f64,
    app: &AppHandle,
) -> CommandResult<String> {
    stream_ollama_request(prompt, model, temperature, app, true)
}

fn stream_ollama_request(
    prompt: &str,
    model: &str,
    temperature: f64,
    app: &AppHandle,
    json_mode: bool,
) -> CommandResult<String> {
    ensure_ollama_running()?;
    emit(app, "cat-begin", ());

    let client = Client::builder()
        .timeout(Duration::from_secs(120))
        .build()
        .map_err(|e| e.to_string())?;
    let mut payload = json!({
        "model": model,
        "prompt": prompt,
        "stream": true,
        "options": { "temperature": temperature }
    });
    if json_mode {
        payload["format"] = json!("json");
    }
    let mut response = client
        .post(format!("{OLLAMA_BASE_URL}/api/generate"))
        .json(&payload)
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

fn parse_llm_commit_messages(raw_output: &str) -> CommandResult<HashMap<String, String>> {
    serde_json::from_str::<HashMap<String, String>>(raw_output.trim())
        .map_err(|e| format!("Could not parse schema-constrained LLM output: {e}"))
}

fn validate_llm_commit_messages(
    parsed: HashMap<String, String>,
    hashes: &[String],
) -> CommandResult<HashMap<String, String>> {
    if parsed.len() != hashes.len() {
        return Err(format!(
            "LLM returned {} commit messages; expected {}.",
            parsed.len(),
            hashes.len()
        ));
    }

    let mut validated = HashMap::new();
    for hash in hashes {
        let short = short_hash(hash);
        let message = parsed
            .get(hash)
            .or_else(|| parsed.get(&short))
            .ok_or_else(|| format!("LLM response did not contain commit {short}."))?
            .trim();
        if message.is_empty() {
            return Err(format!("LLM returned an empty message for commit {short}."));
        }
        if message.lines().count() != 1 {
            return Err(format!(
                "LLM returned a multi-line message for commit {short}."
            ));
        }
        if is_auto_git_message(message) {
            return Err(format!(
                "LLM did not replace the generic message for commit {short}."
            ));
        }
        validated.insert(hash.clone(), message.to_string());
    }
    Ok(validated)
}

fn generate_llm_message_for_commit(
    app: &AppHandle,
    folder_path: &str,
    hash: &str,
    model: &str,
) -> CommandResult<String> {
    let hashes = vec![hash.to_string()];
    let prompt = generate_llm_commit_prompt(folder_path, &hashes)?;
    let llm_raw = stream_ollama_json(&prompt, model, 0.3, app)?;
    let parsed = parse_llm_commit_messages(&llm_raw)?;
    let validated = validate_llm_commit_messages(parsed, &hashes)?;
    validated.get(hash).cloned().ok_or_else(|| {
        format!(
            "No validated message was returned for {}.",
            short_hash(hash)
        )
    })
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

fn shell_single_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\"'\"'"))
}

fn reword_commits_transactionally(
    repo_path: &str,
    commit_message_map: &HashMap<String, String>,
    hashes: &[String],
) -> CommandResult<()> {
    if cfg!(windows) {
        return Err(
            "Transactional history rewriting is not enabled on Windows yet; no Git changes were made."
                .to_string(),
        );
    }
    if hashes.is_empty() {
        return Ok(());
    }
    if hashes.len() > 50 {
        return Err("Refusing to rewrite more than 50 commits in one transaction.".to_string());
    }
    let snapshot = safe_repo_snapshot(repo_path, true)?;
    let all_raw = run_git(repo_path, &["log", "--format=%H"])?;
    let all_commits: Vec<String> = all_raw.lines().map(|s| s.to_string()).collect();
    let full_hashes = resolve_reword_hashes_newest_first(&all_commits, hashes);
    if full_hashes.len() != hashes.len() {
        return Err("One or more queued commits are not in the current HEAD history.".to_string());
    }
    let oldest = full_hashes
        .last()
        .ok_or_else(|| "No rewrite candidates resolved.".to_string())?;
    let parent = run_git(repo_path, &["rev-parse", "--verify", &format!("{oldest}^")])
        .ok()
        .map(|raw| raw.trim().to_string());
    let merge_range = parent
        .as_ref()
        .map(|parent| format!("{parent}..HEAD"))
        .unwrap_or_else(|| "HEAD".to_string());
    if !run_git(repo_path, &["rev-list", "--merges", &merge_range])?
        .trim()
        .is_empty()
    {
        return Err(
            "The rewrite range contains merge commits. Auto-Git left history untouched."
                .to_string(),
        );
    }

    let temp_dir = TempDir::new().map_err(|e| e.to_string())?;
    let sequence_path = temp_dir.path().join("sequence-editor.sh");
    let mut sequence_script = String::from(
        "#!/bin/sh\nset -eu\ntodo=$1\ntmp=\"${todo}.auto-git\"\nwhile IFS= read -r line || [ -n \"$line\" ]; do\n  printf '%s\\n' \"$line\"\n  case \"$line\" in\n",
    );
    for (index, full_hash) in full_hashes.iter().enumerate() {
        let short = short_hash(full_hash);
        let new_msg = commit_message_map
            .get(full_hash)
            .or_else(|| commit_message_map.get(&short))
            .ok_or_else(|| format!("No validated message exists for commit {short}."))?;
        let msg_file = temp_dir.path().join(format!("commit-message-{index}.txt"));
        fs::write(&msg_file, new_msg.trim().to_string() + "\n").map_err(|e| e.to_string())?;
        let command = format!(
            "exec git commit --amend --no-verify -F {}",
            shell_single_quote(&msg_file.to_string_lossy())
        );
        sequence_script.push_str(&format!(
            "    \"pick {full_hash} \"*) printf '%s\\n' {} ;;\n",
            shell_single_quote(&command)
        ));
    }
    sequence_script.push_str("  esac\ndone < \"$todo\" > \"$tmp\"\nmv \"$tmp\" \"$todo\"\n");
    fs::write(&sequence_path, sequence_script).map_err(|e| e.to_string())?;
    set_executable(&sequence_path)?;

    let worktree_path = temp_dir.path().join("worktree");
    let worktree_arg = worktree_path.to_string_lossy().to_string();
    run_git(
        repo_path,
        &["worktree", "add", "--detach", &worktree_arg, &snapshot.head],
    )?;
    let result = (|| -> CommandResult<()> {
        let mut env = HashMap::new();
        env.insert(
            "GIT_SEQUENCE_EDITOR".to_string(),
            sequence_path.to_string_lossy().to_string(),
        );
        env.insert("GIT_EDITOR".to_string(), "true".to_string());
        let rebase_args = if let Some(parent) = &parent {
            vec![
                "-c".into(),
                "core.abbrev=40".into(),
                "-c".into(),
                "rebase.abbreviateCommands=false".into(),
                "rebase".into(),
                "-i".into(),
                parent.clone(),
            ]
        } else {
            vec![
                "-c".into(),
                "core.abbrev=40".into(),
                "-c".into(),
                "rebase.abbreviateCommands=false".into(),
                "rebase".into(),
                "-i".into(),
                "--root".into(),
            ]
        };
        run_git_owned(&worktree_arg, &rebase_args, Some(&env), None)?;
        let new_head = run_git(&worktree_arg, &["rev-parse", "HEAD"])?
            .trim()
            .to_string();
        let new_tree = run_git(&worktree_arg, &["rev-parse", "HEAD^{tree}"])?
            .trim()
            .to_string();
        if new_tree != snapshot.tree {
            return Err("Rewrite validation failed: final tree changed.".to_string());
        }
        run_git(
            repo_path,
            &[
                "update-ref",
                &snapshot.branch_ref,
                &new_head,
                &snapshot.head,
            ],
        )?;
        Ok(())
    })();
    let _ = run_git(repo_path, &["worktree", "remove", "--force", &worktree_arg]);
    result
}

fn resolve_reword_hashes_newest_first(all_commits: &[String], hashes: &[String]) -> Vec<String> {
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
    full_hashes
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
struct CommitIdentity {
    tree: String,
    author_time: String,
    author_name: String,
    author_email: String,
    subject: String,
}

fn parse_commit_identity_line(line: &str) -> Option<(String, CommitIdentity)> {
    let mut parts = line.splitn(6, '\x1f');
    let hash = parts.next()?.to_string();
    Some((
        hash,
        CommitIdentity {
            tree: parts.next()?.to_string(),
            author_time: parts.next()?.to_string(),
            author_name: parts.next()?.to_string(),
            author_email: parts.next()?.to_string(),
            subject: parts.next().unwrap_or_default().to_string(),
        },
    ))
}

fn current_commit_identities(repo_path: &str) -> HashMap<CommitIdentity, String> {
    let raw = run_git(
        repo_path,
        &["log", "--format=%H%x1f%T%x1f%at%x1f%an%x1f%ae%x1f%s"],
    )
    .unwrap_or_default();
    raw.lines()
        .filter_map(parse_commit_identity_line)
        .map(|(hash, identity)| (identity, hash))
        .collect()
}

fn identities_for_hashes(repo_path: &str, hashes: &[String]) -> HashMap<String, CommitIdentity> {
    let wanted: HashSet<String> = hashes
        .iter()
        .filter_map(|hash| resolve_commit_hash(repo_path, hash))
        .collect();
    current_commit_identities(repo_path)
        .into_iter()
        .filter_map(|(identity, hash)| {
            if wanted.contains(&hash) {
                Some((hash, identity))
            } else {
                None
            }
        })
        .collect()
}

#[allow(clippy::too_many_arguments)]
fn emit_rewrite_progress(
    app: &AppHandle,
    folder_path: &str,
    scope: &str,
    status: &str,
    current: usize,
    total: usize,
    hash: Option<&str>,
    error: Option<&str>,
) {
    emit(
        app,
        "rewrite-progress",
        json!({
            "folderPath": folder_path,
            "scope": scope,
            "status": status,
            "current": current,
            "total": total,
            "hash": hash.map(short_hash),
            "error": error,
        }),
    );
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

fn run_llm_commit_rewrite(app: AppHandle, folder_path: String) -> CommandResult<()> {
    let state = app.state::<AppState>();
    let _operation = begin_repo_operation(&state, &folder_path, "history rewrite")?;
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
        if folder.llm_candidates.len() > 50 {
            return Err(
                "More than 50 commits are queued. Rewrite a smaller explicit batch.".to_string(),
            );
        }
        if folder.rewrite_in_progress {
            return Err("Another rewrite is already running for this repository.".to_string());
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
        let llm_raw = stream_ollama_json(&prompt, &model, 0.3, &app)?;
        let message_map =
            validate_llm_commit_messages(parse_llm_commit_messages(&llm_raw)?, &hashes)?;
        reword_commits_transactionally(&folder_path, &message_map, &hashes)?;
        emit(&app, "repo-updated", folder_path.clone());
        Ok(())
    })();

    if let Err(err) = result {
        error = Some(err.clone());
        eprintln!("[runLLMCommitRewrite] Rewrite failed: {err}");
    }

    let retry_candidates = if error.is_some() {
        let mut candidates = hashes.clone();
        let buffer = {
            let store = state.store.lock().map_err(|e| e.to_string())?;
            store
                .folders
                .iter()
                .find(|f| f.path == folder_path)
                .map(|f| f.llm_buffer.clone())
                .unwrap_or_default()
        };
        candidates.extend(buffer);
        pending_rewrite_hashes(&folder_path, &candidates)
    } else {
        Vec::new()
    };

    {
        let mut store = state.store.lock().map_err(|e| e.to_string())?;
        if let Some(folder) = store.folders.iter_mut().find(|f| f.path == folder_path) {
            let buffer = folder.llm_buffer.clone();
            if error.is_some() {
                folder.llm_candidates = retry_candidates;
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
    emit(&app, "repo-updated", folder_path.clone());

    error.map_or(Ok(()), Err)
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

fn automatic_rewrite_eligible(folder: &FolderObj) -> bool {
    AUTO_HISTORY_REWRITE_ENABLED
        && folder.monitoring
        && !folder.rewrite_in_progress
        && !folder.llm_candidates.is_empty()
}

fn line_rewrite_due(folder: &FolderObj, threshold: i64) -> bool {
    automatic_rewrite_eligible(folder) && folder.lines_changed >= threshold
}

fn time_rewrite_due(folder: &FolderObj, minutes_threshold: i64, now: i64) -> bool {
    automatic_rewrite_eligible(folder)
        && folder
            .first_candidate_birthday
            .map(|birthday| ((now - birthday) as f64 / 1000.0 / 60.0) >= minutes_threshold as f64)
            .unwrap_or(false)
}

#[derive(Debug)]
struct SelectedCommit {
    head: String,
    changed_lines: i64,
    staged_paths: Vec<String>,
}

fn commit_selected_paths(
    repo_path: &str,
    paths: &[String],
) -> CommandResult<Option<SelectedCommit>> {
    let snapshot = safe_repo_snapshot(repo_path, false)?;
    if run_git(repo_path, &["diff", "--cached", "--quiet"]).is_err() {
        return Err(
            "The Git index already contains staged changes. Auto-Git left them untouched."
                .to_string(),
        );
    }
    let mut add_args = vec!["add".to_string(), "-A".to_string(), "--".to_string()];
    add_args.extend(paths.iter().cloned());
    run_git_owned(repo_path, &add_args, None, None)?;
    let staged_raw = run_git(repo_path, &["diff", "--cached", "--name-only", "-z"])?;
    let staged_paths: Vec<String> = staged_raw
        .split('\0')
        .filter(|path| !path.is_empty())
        .map(str::to_string)
        .collect();
    if staged_paths.is_empty() {
        return Ok(None);
    }
    let allowed = |staged_path: &str| {
        paths.iter().any(|path| {
            staged_path == path
                || staged_path.starts_with(&format!("{}/", path.trim_end_matches('/')))
        })
    };
    if staged_paths.iter().any(|path| !allowed(path)) {
        let mut reset_args = vec![
            "reset".to_string(),
            "--quiet".to_string(),
            "HEAD".to_string(),
            "--".to_string(),
        ];
        reset_args.extend(paths.iter().cloned());
        let _ = run_git_owned(repo_path, &reset_args, None, None);
        return Err("Auto-Git refused to commit files outside the watcher event set.".to_string());
    }
    if run_git(repo_path, &["rev-parse", "HEAD"])?.trim() != snapshot.head {
        return Err(
            "HEAD changed while Auto-Git was preparing a commit; nothing was committed."
                .to_string(),
        );
    }
    let diff_output = run_git(repo_path, &["diff", "--cached", "--numstat"])?;
    let changed_lines = diff_output.lines().map(parse_numstat_added_deleted).sum();
    let message = format!(
        "auto-git:\n {}",
        staged_paths
            .iter()
            .map(|path| format!("[change] {path}"))
            .collect::<Vec<_>>()
            .join("\n ")
    );
    run_git(repo_path, &["commit", "-m", &message])?;
    let head = run_git(repo_path, &["rev-parse", "HEAD"])?
        .trim()
        .to_string();
    Ok(Some(SelectedCommit {
        head,
        changed_lines,
        staged_paths,
    }))
}

fn auto_commit(app: AppHandle, folder_path: String, mut paths: Vec<String>) -> CommandResult<bool> {
    let state = app.state::<AppState>();
    let _operation = begin_repo_operation(&state, &folder_path, "automatic commit")?;
    let monitoring = {
        let store = state.store.lock().map_err(|e| e.to_string())?;
        store
            .folders
            .iter()
            .find(|f| f.path == folder_path)
            .map(|f| f.monitoring && !f.rewrite_in_progress)
            .unwrap_or(false)
    };
    if !monitoring {
        return Ok(false);
    }
    paths.retain(|path| {
        !path.is_empty()
            && !Path::new(path).is_absolute()
            && !Path::new(path)
                .components()
                .any(|part| matches!(part, std::path::Component::ParentDir))
            && !is_default_ignored(&folder_path, &Path::new(&folder_path).join(path))
    });
    paths.sort();
    paths.dedup();
    if paths.is_empty() {
        return Ok(false);
    }
    let Some(commit) = commit_selected_paths(&folder_path, &paths)? else {
        return Ok(false);
    };
    debug(format!(
        "[MONITOR] committed {} watcher paths for {folder_path}",
        commit.staged_paths.len()
    ));
    let changed_lines = commit.changed_lines;
    let new_head = commit.head;

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
        line_rewrite_due(folder, threshold)
    };
    save_store(&state)?;

    if should_rewrite {
        let app_clone = app.clone();
        let folder_clone = folder_path.clone();
        thread::spawn(move || {
            if let Err(err) = run_llm_commit_rewrite(app_clone, folder_clone) {
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

fn update_gitignore_from_existing_project(folder_path: &str) -> CommandResult<()> {
    let mut stack = vec![PathBuf::from(folder_path)];
    let mut patterns = HashSet::new();

    while let Some(current) = stack.pop() {
        let entries = fs::read_dir(&current).map_err(|e| e.to_string())?;
        for entry in entries {
            let entry = entry.map_err(|e| e.to_string())?;
            let path = entry.path();
            let file_type = entry.file_type().map_err(|e| e.to_string())?;
            let name = entry.file_name().to_string_lossy().to_string();

            if let Some(pattern) = tauri_build_ignore_for_path(folder_path, &path) {
                patterns.insert(pattern.to_string());
            }
            let matched_patterns = IGNORED_NAMES
                .iter()
                .copied()
                .filter(|pattern| file_name_matches_ignore(&name, pattern))
                .collect::<Vec<_>>();
            for pattern in &matched_patterns {
                patterns.insert((*pattern).to_string());
            }

            if file_type.is_dir()
                && matched_patterns.is_empty()
                && !should_ignore_path(folder_path, &path)
            {
                stack.push(path);
            }
        }
    }

    let mut patterns = patterns.into_iter().collect::<Vec<_>>();
    patterns.sort();
    for pattern in patterns {
        ensure_in_gitignore(folder_path, &pattern)?;
    }
    Ok(())
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

fn start_monitoring_watcher(app: AppHandle, folder_path: String) -> CommandResult<()> {
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
                        if should_ignore_path(&watched_folder, &path) {
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
    debug(format!("[MONITOR] Watcher active for {folder_path}"));
    Ok(())
}

fn queue_monitoring_paths(
    app: &AppHandle,
    folder_path: &str,
    relative_paths: Vec<String>,
) -> CommandResult<bool> {
    let state = app.state::<AppState>();
    let root = Path::new(folder_path);
    let mut queued = false;
    {
        let mut pending = state.pending.lock().map_err(|e| e.to_string())?;
        let entry = pending.entry(folder_path.to_string()).or_default();
        for relative_path in relative_paths {
            let path = root.join(&relative_path);
            if relative_path.is_empty()
                || Path::new(&relative_path).is_absolute()
                || Path::new(&relative_path)
                    .components()
                    .any(|part| matches!(part, std::path::Component::ParentDir))
                || should_ignore_path(folder_path, &path)
            {
                continue;
            }
            if let Some(name) = path.file_name().and_then(|name| name.to_str()) {
                entry.names.insert(name.to_string());
            }
            entry.paths.insert(path);
            queued = true;
        }
    }
    if queued {
        schedule_pending_processing(app.clone(), folder_path.to_string());
    }
    Ok(queued)
}

fn start_monitoring_and_reconcile(
    app: &AppHandle,
    folder_path: &str,
    mut reconciliation_paths: Vec<String>,
) -> CommandResult<bool> {
    start_monitoring_watcher(app.clone(), folder_path.to_string())?;
    let watcher_active = app
        .state::<AppState>()
        .watchers
        .lock()
        .map_err(|e| e.to_string())?
        .contains_key(folder_path);
    if !watcher_active {
        return Ok(false);
    }
    reconciliation_paths.extend(changed_paths_for_monitoring(folder_path)?);
    reconciliation_paths.sort();
    reconciliation_paths.dedup();
    queue_monitoring_paths(app, folder_path, reconciliation_paths)?;
    Ok(true)
}

fn stop_monitoring_watcher(state: &AppState, folder_path: &str) {
    if let Ok(mut watchers) = state.watchers.lock() {
        watchers.remove(folder_path);
    }
    if let Ok(mut pending) = state.pending.lock() {
        pending.remove(folder_path);
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
            emit(
                &app,
                "monitoring-error",
                json!({ "path": folder_path, "code": "COMMIT_FAILED", "message": err }),
            );
        }
        let state = app.state::<AppState>();
        if let Ok(mut active) = state.active.lock() {
            active.remove(&folder_path);
        };
        let still_pending = state
            .pending
            .lock()
            .map(|pending| {
                pending
                    .get(&folder_path)
                    .map(|changes| !changes.paths.is_empty())
                    .unwrap_or(false)
            })
            .unwrap_or(false);
        if still_pending {
            schedule_pending_processing(app, folder_path);
        }
    });
}

fn process_pending_changes(app: AppHandle, folder_path: String) -> CommandResult<()> {
    let state = app.state::<AppState>();
    // Keep the pending paths queued while a rewrite or another commit owns the repository. The
    // debounce worker will reschedule itself after the operation guard is released.
    if repo_is_busy(&state, &folder_path) {
        return Ok(());
    }
    let pending = {
        let mut pending_map = state.pending.lock().map_err(|e| e.to_string())?;
        pending_map.remove(&folder_path).unwrap_or_default()
    };
    let PendingChanges { paths, .. } = pending;
    if paths.is_empty() {
        return Ok(());
    }
    let root = Path::new(&folder_path);
    let mut relative_paths: Vec<String> = paths
        .into_iter()
        .filter(|path| !should_ignore_path(&folder_path, path))
        .filter_map(|path| path.strip_prefix(root).ok().map(Path::to_path_buf))
        .filter(|path| {
            !path.as_os_str().is_empty()
                && !path
                    .components()
                    .any(|part| matches!(part, std::path::Component::ParentDir))
        })
        .map(|path| path.to_string_lossy().replace('\\', "/"))
        .collect();
    relative_paths.sort();
    relative_paths.dedup();
    if relative_paths.is_empty() {
        return Ok(());
    }

    if is_git_repo_path(&folder_path)
        && auto_commit(app.clone(), folder_path.clone(), relative_paths)?
    {
        emit(&app, "repo-updated", folder_path);
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
            folder.monitoring = true;
            folder.llm_buffer = folder.llm_buffer.clone();
            folder.llm_candidates = folder.llm_candidates.clone();
        } else {
            store.folders.push(FolderObj {
                path: new_folder.clone(),
                monitoring: true,
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
        let _operation = begin_repo_operation(state, &new_folder, "automatic monitoring start")?;
        enable_monitoring(app, state, &new_folder)?;
    } else {
        update_tray_menu(app)?;
    }
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
            if time_rewrite_due(folder, minutes_threshold, now) {
                let app_clone = app.clone();
                let folder_path = folder.path.clone();
                thread::spawn(move || {
                    let _ = run_llm_commit_rewrite(app_clone, folder_path);
                });
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
    let builder =
        WebviewWindowBuilder::new(app, "settings", WebviewUrl::App("settings.html".into()))
            .title("Einstellungen")
            .inner_size(600.0, 550.0)
            .resizable(false)
            .background_color(ROSE_TITLEBAR_COLOR);
    #[cfg(target_os = "macos")]
    let builder = builder
        .title_bar_style(TitleBarStyle::Transparent)
        .hidden_title(true);
    builder.build().map_err(|e| e.to_string())?;
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
    let raw = run_git(
        &folder_obj.path,
        &["rev-list", VISIBLE_HISTORY_REVISION, "--count"],
    )
    .unwrap_or_default();
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
    state: tauri::State<'_, AppState>,
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
            pending_rewrite_count: 0,
        });
    }
    let queued_hashes = state
        .store
        .lock()
        .map_err(|e| e.to_string())?
        .folders
        .iter()
        .find(|folder| folder.path == folder_obj.path)
        .map(|folder| folder.llm_candidates.clone())
        .unwrap_or_default();
    let queued_full: HashSet<String> = queued_hashes
        .iter()
        .filter_map(|hash| resolve_commit_hash(&folder_obj.path, hash))
        .collect();
    let current_history: HashSet<String> = run_git(&folder_obj.path, &["rev-list", "HEAD"])
        .unwrap_or_default()
        .lines()
        .map(str::to_string)
        .collect();
    let pending_rewrite_count = pending_rewrite_hashes(&folder_obj.path, &queued_hashes).len();
    let total = run_git(
        &folder_obj.path,
        &["rev-list", VISIBLE_HISTORY_REVISION, "--count"],
    )
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
            pending_rewrite_count: 0,
        });
    }
    let skip = (page - 1) * page_size;
    let raw = run_git(
        &folder_obj.path,
        &[
            "log",
            VISIBLE_HISTORY_REVISION,
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
                needs_rewrite: is_auto_git_message(message) || queued_full.contains(hash),
                can_reword: current_history.contains(hash),
            })
        })
        .collect();
    let head = run_git(&folder_obj.path, &["rev-parse", "--verify", "HEAD"])
        .ok()
        .map(|raw| short_hash(raw.trim()));
    let pages = total.div_ceil(page_size);
    Ok(CommitPage {
        head,
        commits,
        total,
        page,
        page_size,
        pages: pages.max(1),
        pending_rewrite_count,
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
fn revert_commit(
    state: tauri::State<'_, AppState>,
    folder_obj: FolderObj,
    hash: String,
) -> CommandResult<()> {
    if !folder_obj.needs_relocation && Path::new(&folder_obj.path).exists() {
        let _operation = begin_repo_operation(&state, &folder_obj.path, "revert")?;
        safe_repo_snapshot(&folder_obj.path, false)?;
        run_git(&folder_obj.path, &["revert", &hash, "--no-edit"])?;
    }
    Ok(())
}

#[tauri::command]
fn checkout_commit(
    state: tauri::State<'_, AppState>,
    folder_obj: FolderObj,
    hash: String,
) -> CommandResult<()> {
    if !folder_obj.needs_relocation && Path::new(&folder_obj.path).exists() {
        let _operation = begin_repo_operation(&state, &folder_obj.path, "checkout")?;
        checkout_commit_internal(&folder_obj.path, &hash)?;
    }
    Ok(())
}

fn checkout_commit_internal(repo_path: &str, hash: &str) -> CommandResult<()> {
    validate_repo_state(repo_path, true)?;
    if run_git(repo_path, &["branch", "--contains", hash])?
        .trim()
        .is_empty()
    {
        return Err("The selected commit is not reachable from a local branch.".to_string());
    }
    let pointed_branches = run_git(
        repo_path,
        &[
            "for-each-ref",
            &format!("--points-at={hash}"),
            "--format=%(refname:short)",
            "refs/heads",
        ],
    )?;
    let branch_names: Vec<&str> = pointed_branches.lines().collect();
    if let [branch_name] = branch_names.as_slice() {
        run_git(repo_path, &["checkout", branch_name])?;
    } else {
        run_git(repo_path, &["checkout", "--detach", hash])?;
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
fn commit_current_folder(
    state: tauri::State<'_, AppState>,
    folder_obj: FolderObj,
    message: Option<String>,
) -> CommandResult<Value> {
    if folder_obj.needs_relocation || !Path::new(&folder_obj.path).exists() {
        return Ok(json!({}));
    }
    let _operation = begin_repo_operation(&state, &folder_obj.path, "manual commit")?;
    let _snapshot = safe_repo_snapshot(&folder_obj.path, false)?;
    let status = git_status(&folder_obj.path)?;
    if !has_status_changes(&status) {
        return Ok(json!({ "success": false, "error": "Nichts zu committen." }));
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
    let _operation = begin_repo_operation(&state, &folder_path, "monitoring toggle")?;
    if monitoring {
        if !is_git_repo_path(&folder_path) {
            {
                let mut store = state.store.lock().map_err(|e| e.to_string())?;
                let Some(folder) = store.folders.iter_mut().find(|f| f.path == folder_path) else {
                    return Ok(false);
                };
                if folder.needs_relocation || !Path::new(&folder_path).exists() {
                    return Ok(false);
                }
                folder.monitoring = true;
            }
            save_store(&state)?;
            update_tray_menu(&app)?;
            return Ok(true);
        }
        return enable_monitoring(&app, &state, &folder_path);
    }
    {
        let mut store = state.store.lock().map_err(|e| e.to_string())?;
        let Some(folder) = store.folders.iter_mut().find(|f| f.path == folder_path) else {
            return Ok(false);
        };
        folder.monitoring = false;
    }
    save_store(&state)?;
    stop_monitoring_watcher(&state, &folder_path);
    update_tray_menu(&app)?;
    monitoring = false;
    Ok(monitoring)
}

fn enable_monitoring(app: &AppHandle, state: &AppState, folder_path: &str) -> CommandResult<bool> {
    {
        let store = state.store.lock().map_err(|e| e.to_string())?;
        let Some(folder) = store.folders.iter().find(|f| f.path == folder_path) else {
            return Ok(false);
        };
        if folder.needs_relocation || !is_git_repo_path(folder_path) {
            return Ok(false);
        }
    }
    safe_repo_snapshot(folder_path, false)?;
    let reconciliation_paths = changed_paths_for_monitoring(folder_path)?;
    {
        let mut store = state.store.lock().map_err(|e| e.to_string())?;
        let Some(folder) = store.folders.iter_mut().find(|f| f.path == folder_path) else {
            return Ok(false);
        };
        folder.monitoring = true;
    }
    save_store(state)?;
    let monitoring = match start_monitoring_and_reconcile(app, folder_path, reconciliation_paths) {
        Ok(watcher_active) => watcher_active,
        Err(err) => {
            stop_monitoring_watcher(state, folder_path);
            if let Ok(mut store) = state.store.lock() {
                if let Some(folder) = store.folders.iter_mut().find(|f| f.path == folder_path) {
                    folder.monitoring = false;
                }
            }
            let _ = save_store(state);
            let _ = update_tray_menu(app);
            return Err(err);
        }
    };
    if !monitoring {
        let mut store = state.store.lock().map_err(|e| e.to_string())?;
        if let Some(folder) = store.folders.iter_mut().find(|f| f.path == folder_path) {
            folder.monitoring = false;
        }
        drop(store);
        save_store(state)?;
    } else {
        emit(app, "repo-updated", folder_path.to_string());
    }
    update_tray_menu(app)?;
    Ok(monitoring)
}

#[tauri::command]
fn ollama_list() -> CommandResult<Value> {
    if let Ok(models) = ollama_list_from_api() {
        return Ok(json!({ "status": "ok", "models": models }));
    }

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

fn ollama_list_from_api() -> CommandResult<Vec<Value>> {
    let client = Client::builder()
        .timeout(Duration::from_secs(2))
        .build()
        .map_err(|e| e.to_string())?;
    let response = client
        .get(format!("{OLLAMA_BASE_URL}/api/tags"))
        .send()
        .map_err(|e| e.to_string())?;
    let status = response.status();
    if !status.is_success() {
        return Err(format!("Ollama tags request failed: {status}"));
    }
    let payload: Value = response.json().map_err(|e| e.to_string())?;
    Ok(payload
        .get("models")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default())
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
    if let Ok(value) = ollama_pull_from_api(&model) {
        return Ok(value);
    }

    match run_process("ollama", &["pull".into(), model], None, None, None) {
        Ok(out) => Ok(json!({ "status": "ok", "msg": out.stdout })),
        Err(err) => Ok(json!({ "status": "error", "msg": err })),
    }
}

fn ollama_pull_from_api(model: &str) -> CommandResult<Value> {
    let client = Client::builder()
        .timeout(Duration::from_secs(30 * 60))
        .build()
        .map_err(|e| e.to_string())?;
    let response = client
        .post(format!("{OLLAMA_BASE_URL}/api/pull"))
        .json(&json!({ "name": model, "stream": false }))
        .send()
        .map_err(|e| e.to_string())?;
    let status = response.status();
    if !status.is_success() {
        return Err(format!("Ollama pull request failed: {status}"));
    }
    let payload: Value = response.json().unwrap_or_else(|_| json!({}));
    Ok(json!({
        "status": "ok",
        "msg": payload.get("status").and_then(Value::as_str).unwrap_or("model pulled")
    }))
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
fn get_reword_mode(state: tauri::State<'_, AppState>) -> CommandResult<String> {
    Ok(state
        .store
        .lock()
        .map_err(|e| e.to_string())?
        .reword_mode
        .clone())
}

#[tauri::command]
fn set_reword_mode(state: tauri::State<'_, AppState>, val: String) -> CommandResult<()> {
    if !VALID_REWORD_MODES.contains(&val.as_str()) {
        return Err("Invalid Reword button behavior.".to_string());
    }
    state.store.lock().map_err(|e| e.to_string())?.reword_mode = val;
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
    let _operation = begin_repo_operation(&state, &folder_path, "repository initialization")?;
    let should_monitor = state
        .store
        .lock()
        .map_err(|e| e.to_string())?
        .folders
        .iter()
        .find(|folder| folder.path == folder_path)
        .map(|folder| folder.monitoring)
        .unwrap_or(true);
    match init_git_repo_internal(&folder_path) {
        Ok(()) => {
            let last_head_hash = run_git(&folder_path, &["rev-parse", "HEAD"])
                .ok()
                .map(|s| s.trim().to_string());
            {
                let mut store = state.store.lock().map_err(|e| e.to_string())?;
                for folder in &mut store.folders {
                    if folder.path == folder_path {
                        folder.monitoring = should_monitor;
                        folder.needs_relocation = false;
                        folder.last_head_hash = last_head_hash.clone();
                    }
                }
            }
            save_store(&state)?;
            let monitoring = if should_monitor {
                enable_monitoring(&app, &state, &folder_path)?
            } else {
                update_tray_menu(&app)?;
                false
            };
            Ok(json!({ "success": true, "monitoring": monitoring }))
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
    Ok(run_git(
        &folder_obj.path,
        &["log", VISIBLE_HISTORY_REVISION, "--format=%H"],
    )
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
        if folder.rewrite_in_progress {
            return Ok(json!({
                "success": false,
                "error": "Another rewrite is already running for this repository."
            }));
        }
        folder.lines_changed = 0;
        folder.first_candidate_birthday = Some(now_ms());
    }
    save_store(&state)?;
    thread::spawn(move || {
        let _ = run_llm_commit_rewrite(app, folder_path);
    });
    Ok(json!({ "success": true }))
}

fn run_manual_rewrite_job(
    app: AppHandle,
    folder_path: String,
    hashes: Vec<String>,
    scope: &'static str,
    supplied_messages: Option<HashMap<String, String>>,
) {
    let state = app.state::<AppState>();
    if hashes.len() > 50 {
        if let Ok(mut store) = state.store.lock() {
            if let Some(folder) = store.folders.iter_mut().find(|f| f.path == folder_path) {
                folder.rewrite_in_progress = false;
                folder.rewrite_started_at = None;
            }
        }
        let _ = save_store(&state);
        emit_rewrite_progress(
            &app,
            &folder_path,
            scope,
            "failed",
            0,
            hashes.len(),
            None,
            Some("Refusing to rewrite more than 50 commits in one transaction."),
        );
        return;
    }
    let _operation = match begin_repo_operation(&state, &folder_path, "history rewrite") {
        Ok(operation) => operation,
        Err(err) => {
            if let Ok(mut store) = state.store.lock() {
                if let Some(folder) = store.folders.iter_mut().find(|f| f.path == folder_path) {
                    folder.rewrite_in_progress = false;
                    folder.rewrite_started_at = None;
                }
            }
            let _ = save_store(&state);
            emit_rewrite_progress(
                &app,
                &folder_path,
                scope,
                "failed",
                0,
                hashes.len(),
                None,
                Some(&err),
            );
            return;
        }
    };
    let total = hashes.len();
    let (model, queued_before, original_birthday) = match state.store.lock() {
        Ok(store) => {
            let Some(folder) = store
                .folders
                .iter()
                .find(|folder| folder.path == folder_path)
            else {
                return;
            };
            (
                store
                    .commit_model
                    .clone()
                    .unwrap_or_else(|| "qwen2.5-coder:7b".to_string()),
                folder.llm_candidates.clone(),
                folder.first_candidate_birthday,
            )
        }
        Err(err) => {
            eprintln!("[manualRewrite] Could not lock store: {err}");
            return;
        }
    };

    let mut identity_hashes = queued_before.clone();
    identity_hashes.extend(hashes.clone());
    let identities_before = identities_for_hashes(&folder_path, &identity_hashes);
    let attempted_identities: HashMap<String, CommitIdentity> = hashes
        .iter()
        .filter_map(|hash| {
            identities_before
                .get(hash)
                .cloned()
                .map(|identity| (hash.clone(), identity))
        })
        .collect();
    let queued_identities: HashSet<CommitIdentity> = queued_before
        .iter()
        .filter_map(|hash| resolve_commit_hash(&folder_path, hash))
        .filter_map(|hash| identities_before.get(&hash).cloned())
        .collect();
    let queued_outside_current_history: Vec<String> = queued_before
        .iter()
        .filter_map(|hash| {
            let resolved = resolve_commit_hash(&folder_path, hash)?;
            if identities_before.contains_key(&resolved) {
                None
            } else {
                Some(resolved)
            }
        })
        .collect();

    let mut successful_messages = supplied_messages.unwrap_or_default();
    let mut llm_failures: Vec<(String, String)> = Vec::new();
    let uses_llm = successful_messages.is_empty();
    match if uses_llm {
        ensure_ollama_running()
    } else {
        Ok(())
    } {
        Ok(()) => {
            if !uses_llm {
                emit_rewrite_progress(
                    &app,
                    &folder_path,
                    scope,
                    "running",
                    total,
                    total,
                    hashes.first().map(String::as_str),
                    None,
                );
            } else {
                for (index, hash) in hashes.iter().enumerate() {
                    emit_rewrite_progress(
                        &app,
                        &folder_path,
                        scope,
                        "running",
                        index,
                        total,
                        Some(hash),
                        None,
                    );
                    match generate_llm_message_for_commit(&app, &folder_path, hash, &model) {
                        Ok(message) => {
                            successful_messages.insert(hash.clone(), message);
                        }
                        Err(err) => {
                            eprintln!("[manualRewrite] {} failed: {err}", short_hash(hash));
                            llm_failures.push((hash.clone(), err));
                        }
                    }
                    emit_rewrite_progress(
                        &app,
                        &folder_path,
                        scope,
                        "running",
                        index + 1,
                        total,
                        Some(hash),
                        None,
                    );
                }
            }
        }
        Err(err) => {
            for hash in &hashes {
                llm_failures.push((hash.clone(), err.clone()));
            }
            emit_rewrite_progress(
                &app,
                &folder_path,
                scope,
                "running",
                total,
                total,
                None,
                None,
            );
        }
    }

    let successful_hashes: Vec<String> = hashes
        .iter()
        .filter(|hash| successful_messages.contains_key(*hash))
        .cloned()
        .collect();
    let history_error = if successful_hashes.is_empty() {
        None
    } else {
        match reword_commits_transactionally(&folder_path, &successful_messages, &successful_hashes)
        {
            Ok(()) => None,
            Err(err) => {
                eprintln!("[manualRewrite] Git rewrite failed: {err}");
                Some(err)
            }
        }
    };

    let mut desired_identities = queued_identities;
    if history_error.is_none() {
        for hash in &successful_hashes {
            if let Some(identity) = attempted_identities.get(hash) {
                desired_identities.remove(identity);
            }
        }
        for (hash, _) in &llm_failures {
            if let Some(identity) = attempted_identities.get(hash) {
                desired_identities.insert(identity.clone());
            }
        }
    } else {
        desired_identities.extend(attempted_identities.values().cloned());
    }

    let identities_after = current_commit_identities(&folder_path);
    let mut candidates: Vec<String> = desired_identities
        .iter()
        .filter_map(|identity| identities_after.get(identity).cloned())
        .collect();
    candidates.extend(queued_outside_current_history.clone());
    if scope == "pending" || history_error.is_some() {
        candidates = pending_rewrite_hashes(&folder_path, &candidates);
        candidates.extend(queued_outside_current_history);
    }
    candidates.sort();
    candidates.dedup();

    if let Ok(mut store) = state.store.lock() {
        if let Some(folder) = store
            .folders
            .iter_mut()
            .find(|folder| folder.path == folder_path)
        {
            candidates.extend(folder.llm_buffer.clone());
            candidates.sort();
            candidates.dedup();
            folder.llm_candidates = candidates;
            folder.llm_buffer.clear();
            folder.rewrite_in_progress = false;
            folder.rewrite_started_at = None;
            folder.first_candidate_birthday = if folder.llm_candidates.is_empty() {
                None
            } else {
                original_birthday.or_else(|| Some(now_ms()))
            };
        }
    }
    if let Err(err) = save_store(&state) {
        eprintln!("[manualRewrite] Could not save rewrite state: {err}");
    }
    emit(&app, "repo-updated", folder_path.clone());

    let errors: Vec<String> = llm_failures
        .iter()
        .map(|(hash, err)| format!("{}: {err}", short_hash(hash)))
        .chain(history_error.iter().map(|err| format!("Git: {err}")))
        .collect();
    let status = if history_error.is_some() || successful_hashes.is_empty() {
        "failed"
    } else if llm_failures.is_empty() {
        "succeeded"
    } else {
        "partial"
    };
    let error_text = if errors.is_empty() {
        None
    } else {
        Some(errors.join("\n"))
    };
    emit_rewrite_progress(
        &app,
        &folder_path,
        scope,
        status,
        total,
        total,
        None,
        error_text.as_deref(),
    );
}

#[tauri::command]
fn rewrite_commit(
    app: AppHandle,
    state: tauri::State<'_, AppState>,
    folder_path: String,
    hash: String,
) -> CommandResult<Value> {
    let full_hash = resolve_commit_hash(&folder_path, &hash)
        .ok_or_else(|| format!("Commit {hash} was not found."))?;
    if !is_commit_on_current_history(&folder_path, &full_hash) {
        return Ok(json!({
            "success": false,
            "error": "This commit is not in the current HEAD history. Jump to its branch/history first."
        }));
    }
    if is_git_operation_in_progress(&folder_path) {
        return Ok(
            json!({ "success": false, "error": "Another Git operation is already in progress." }),
        );
    }
    {
        let mut store = state.store.lock().map_err(|e| e.to_string())?;
        let Some(folder) = store
            .folders
            .iter_mut()
            .find(|folder| folder.path == folder_path)
        else {
            return Ok(json!({ "success": false, "error": "folder not found" }));
        };
        if folder.needs_relocation {
            return Ok(json!({ "success": false, "error": "needs relocation" }));
        }
        if folder.rewrite_in_progress {
            return Ok(json!({
                "success": false,
                "error": "Another rewrite is already running for this repository."
            }));
        }
        folder.rewrite_in_progress = true;
        folder.rewrite_started_at = Some(now_ms());
    }
    save_store(&state)?;
    thread::spawn(move || {
        run_manual_rewrite_job(app, folder_path, vec![full_hash], "single", None)
    });
    Ok(json!({ "success": true, "count": 1 }))
}

#[tauri::command]
fn rewrite_commit_with_message(
    app: AppHandle,
    state: tauri::State<'_, AppState>,
    folder_path: String,
    hash: String,
    message: String,
) -> CommandResult<Value> {
    let message = message.trim().to_string();
    if message.is_empty() {
        return Ok(json!({ "success": false, "error": "The commit message cannot be empty." }));
    }
    if message.lines().count() != 1 {
        return Ok(
            json!({ "success": false, "error": "Please enter a single-line commit message." }),
        );
    }
    let full_hash = resolve_commit_hash(&folder_path, &hash)
        .ok_or_else(|| format!("Commit {hash} was not found."))?;
    if !is_commit_on_current_history(&folder_path, &full_hash) {
        return Ok(json!({
            "success": false,
            "error": "This commit is not in the current HEAD history. Jump to its branch/history first."
        }));
    }
    if is_git_operation_in_progress(&folder_path) {
        return Ok(
            json!({ "success": false, "error": "Another Git operation is already in progress." }),
        );
    }
    {
        let mut store = state.store.lock().map_err(|e| e.to_string())?;
        let Some(folder) = store
            .folders
            .iter_mut()
            .find(|folder| folder.path == folder_path)
        else {
            return Ok(json!({ "success": false, "error": "folder not found" }));
        };
        if folder.needs_relocation {
            return Ok(json!({ "success": false, "error": "needs relocation" }));
        }
        if folder.rewrite_in_progress {
            return Ok(json!({
                "success": false,
                "error": "Another rewrite is already running for this repository."
            }));
        }
        folder.rewrite_in_progress = true;
        folder.rewrite_started_at = Some(now_ms());
    }
    save_store(&state)?;
    thread::spawn(move || {
        let mut messages = HashMap::new();
        messages.insert(full_hash.clone(), message);
        run_manual_rewrite_job(app, folder_path, vec![full_hash], "typed", Some(messages));
    });
    Ok(json!({ "success": true, "count": 1 }))
}

#[tauri::command]
fn rewrite_pending_commits(
    app: AppHandle,
    state: tauri::State<'_, AppState>,
    folder_path: String,
) -> CommandResult<Value> {
    if is_git_operation_in_progress(&folder_path) {
        return Ok(
            json!({ "success": false, "error": "Another Git operation is already in progress." }),
        );
    }
    let hashes = {
        let mut store = state.store.lock().map_err(|e| e.to_string())?;
        let Some(folder) = store
            .folders
            .iter_mut()
            .find(|folder| folder.path == folder_path)
        else {
            return Ok(json!({ "success": false, "error": "folder not found" }));
        };
        if folder.needs_relocation {
            return Ok(json!({ "success": false, "error": "needs relocation" }));
        }
        if folder.rewrite_in_progress {
            return Ok(json!({
                "success": false,
                "error": "Another rewrite is already running for this repository."
            }));
        }
        let hashes = pending_rewrite_hashes(&folder_path, &folder.llm_candidates);
        if hashes.is_empty() {
            return Ok(
                json!({ "success": false, "error": "There are no pending commits to rewrite." }),
            );
        }
        if hashes.len() > 50 {
            return Ok(json!({
                "success": false,
                "error": "More than 50 commits are queued. Rewrite a smaller explicit batch."
            }));
        }
        folder.rewrite_in_progress = true;
        folder.rewrite_started_at = Some(now_ms());
        hashes
    };
    let count = hashes.len();
    save_store(&state)?;
    thread::spawn(move || run_manual_rewrite_job(app, folder_path, hashes, "pending", None));
    Ok(json!({ "success": true, "count": count }))
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
    if !TRANSACTIONAL_SQUASH_ENABLED {
        return Err(
            "Squashing is temporarily disabled until it uses the transactional worktree engine."
                .to_string(),
        );
    }
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
    if is_git_operation_in_progress(&folder_path) {
        return Err(
            "Another Git operation is already in progress for this repository.".to_string(),
        );
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

fn slugify_repository_name(name: &str) -> String {
    let mut slug = String::new();
    let mut needs_separator = false;
    for character in name.chars() {
        if character.is_ascii_alphanumeric() {
            if needs_separator && !slug.is_empty() {
                slug.push('-');
            }
            slug.push(character.to_ascii_lowercase());
            needs_separator = false;
        } else {
            needs_separator = !slug.is_empty();
        }
    }
    if slug.is_empty() {
        "repository".to_string()
    } else {
        slug
    }
}

fn gitea_response_error(context: &str, response: reqwest::blocking::Response) -> String {
    let status = response.status();
    let body = response.text().unwrap_or_default();
    let detail = serde_json::from_str::<Value>(&body)
        .ok()
        .and_then(|value| {
            let message = value.get("message").and_then(Value::as_str);
            let errors = value.get("errors").filter(|errors| !errors.is_null());
            match (message, errors) {
                (Some(message), Some(errors)) => Some(format!("{message}; {errors}")),
                (Some(message), None) => Some(message.to_string()),
                (None, Some(errors)) => Some(errors.to_string()),
                (None, None) => None,
            }
        })
        .or_else(|| (!body.trim().is_empty()).then(|| body.trim().to_string()))
        .unwrap_or_else(|| "Gitea returned no error details".to_string());
    format!(
        "{context} failed ({status}): {}",
        truncate_text(detail, 1000)
    )
}

#[tauri::command]
fn push_to_gitea(
    app: AppHandle,
    state: tauri::State<'_, AppState>,
    folder_path: String,
    allow_dirty: bool,
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
    if has_status_changes(&git_status(&folder_path)?) && !allow_dirty {
        return Ok(json!({
            "success": false,
            "code": "DIRTY_WORKTREE",
            "error": "The repository contains uncommitted or untracked files. Confirm the warning before pushing."
        }));
    }
    let result = (|| -> CommandResult<Value> {
        let folder_name = Path::new(&folder_path)
            .file_name()
            .and_then(|s| s.to_str())
            .ok_or_else(|| "invalid repository path".to_string())?;
        let repo_name = slugify_repository_name(folder_name);
        let base = "https://giers10.uber.space/api/v1";
        let description = generate_repo_description(&app, &state, &folder_path)?;
        let client = Client::new();
        let user_resp = client
            .get(format!("{base}/user"))
            .header("Authorization", format!("token {token}"))
            .send()
            .map_err(|e| e.to_string())?;
        if !user_resp.status().is_success() {
            return Err(gitea_response_error("Gitea user request", user_resp));
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
            let created_response = client
                .post(format!("{base}/user/repos"))
                .header("Authorization", format!("token {token}"))
                .json(&json!({
                    "name": repo_name,
                    "description": description,
                    "private": false,
                    "auto_init": false
                }))
                .send()
                .map_err(|e| e.to_string())?;
            if !created_response.status().is_success() {
                return Err(gitea_response_error(
                    "Creating the Gitea repository",
                    created_response,
                ));
            }
            let created: Value = created_response.json().map_err(|e| e.to_string())?;
            created
                .get("clone_url")
                .and_then(Value::as_str)
                .ok_or_else(|| "Gitea create response missing clone_url".to_string())?
                .to_string()
        } else if check.status().is_success() {
            let update_response = client
                .patch(format!("{base}/repos/{username}/{repo_name}"))
                .header("Authorization", format!("token {token}"))
                .json(&json!({ "description": description }))
                .send()
                .map_err(|e| e.to_string())?;
            if !update_response.status().is_success() {
                return Err(gitea_response_error(
                    "Updating the Gitea repository description",
                    update_response,
                ));
            }
            let existing: Value = check.json().map_err(|e| e.to_string())?;
            existing
                .get("clone_url")
                .and_then(Value::as_str)
                .ok_or_else(|| "Gitea repo response missing clone_url".to_string())?
                .to_string()
        } else {
            return Err(gitea_response_error("Checking the Gitea repository", check));
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
        repo_operations: Mutex::new(HashMap::new()),
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
                    let folder_path = folder.path;
                    if !is_git_repo_path(&folder_path) {
                        continue;
                    }
                    let result = safe_repo_snapshot(&folder_path, false)
                        .and_then(|_| changed_paths_for_monitoring(&folder_path))
                        .and_then(|paths| {
                            start_monitoring_and_reconcile(&app_handle, &folder_path, paths)
                        });
                    if let Err(err) = result {
                        let state = app_handle.state::<AppState>();
                        stop_monitoring_watcher(&state, &folder_path);
                        if let Ok(mut store) = state.store.lock() {
                            if let Some(folder) =
                                store.folders.iter_mut().find(|f| f.path == folder_path)
                            {
                                folder.monitoring = false;
                            }
                        }
                        let _ = save_store(&state);
                        emit(
                            &app_handle,
                            "monitoring-error",
                            json!({
                                "path": folder_path,
                                "code": "STARTUP_RECONCILIATION_FAILED",
                                "message": err
                            }),
                        );
                    }
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
            get_reword_mode,
            set_reword_mode,
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
            rewrite_commit,
            rewrite_commit_with_message,
            rewrite_pending_commits,
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

#[cfg(test)]
mod tests {
    use super::*;

    fn init_test_repo() -> TempDir {
        let temp = TempDir::new().unwrap();
        let path = temp.path().to_str().unwrap();
        run_git(path, &["init"]).unwrap();
        run_git(path, &["config", "user.name", "Auto Git Test"]).unwrap();
        run_git(path, &["config", "user.email", "auto-git@example.test"]).unwrap();
        temp
    }

    fn test_commit(repo_path: &str, filename: &str, contents: &str, message: &str) -> String {
        fs::write(Path::new(repo_path).join(filename), contents).unwrap();
        run_git(repo_path, &["add", filename]).unwrap();
        run_git(repo_path, &["commit", "-m", message]).unwrap();
        run_git(repo_path, &["rev-parse", "HEAD"])
            .unwrap()
            .trim()
            .to_string()
    }

    #[test]
    fn repository_names_are_slugified_for_gitea() {
        assert_eq!(
            slugify_repository_name("EP-133 Sample Tool Offline"),
            "ep-133-sample-tool-offline"
        );
        assert_eq!(slugify_repository_name("  Fragile___App  "), "fragile-app");
        assert_eq!(slugify_repository_name("☃"), "repository");
    }

    #[test]
    fn reword_button_mode_defaults_to_ask_and_invalid_values_are_normalized() {
        let mut store = StoreData::default();
        assert_eq!(store.reword_mode, "ask");

        store.reword_mode = "unexpected".to_string();
        normalize_store(&mut store);
        assert_eq!(store.reword_mode, "ask");
    }

    #[test]
    fn scheduled_rewrites_use_thresholds_not_the_reword_button_mode() {
        let mut folder = FolderObj {
            path: "/tmp/example".to_string(),
            monitoring: true,
            needs_relocation: false,
            lines_changed: 12,
            llm_candidates: vec!["a".repeat(40)],
            llm_buffer: Vec::new(),
            first_candidate_birthday: Some(60_000),
            last_head_hash: None,
            rewrite_in_progress: false,
            rewrite_started_at: None,
        };

        assert!(line_rewrite_due(&folder, 10));
        assert!(time_rewrite_due(&folder, 1, 120_000));
        folder.rewrite_in_progress = true;
        assert!(!line_rewrite_due(&folder, 10));
        assert!(!time_rewrite_due(&folder, 1, 120_000));
    }

    #[test]
    fn commit_summary_uses_the_javascript_field_names() {
        let summary = CommitSummary {
            hash: "abcdef0".to_string(),
            date: "2026-07-12T00:00:00Z".to_string(),
            message: "Test message".to_string(),
            needs_rewrite: true,
            can_reword: true,
        };

        let serialized = serde_json::to_value(summary).unwrap();
        assert_eq!(serialized["needsRewrite"], true);
        assert_eq!(serialized["canReword"], true);
        assert!(serialized.get("needs_rewrite").is_none());
        assert!(serialized.get("can_reword").is_none());
    }

    #[test]
    fn existing_project_ignore_rules_are_written_before_staging() {
        let temp = TempDir::new().unwrap();
        fs::write(temp.path().join(".DS_Store"), "metadata").unwrap();
        fs::create_dir(temp.path().join("Release.app")).unwrap();
        fs::write(temp.path().join("Release.app").join("binary"), "binary").unwrap();

        update_gitignore_from_existing_project(temp.path().to_str().unwrap()).unwrap();

        let gitignore = fs::read_to_string(temp.path().join(".gitignore")).unwrap();
        assert!(gitignore.lines().any(|line| line == ".DS_Store"));
        assert!(gitignore.lines().any(|line| line == "*.app"));
    }

    #[test]
    fn reword_hashes_are_processed_newest_first() {
        let all_commits = vec![
            "ccccccc333333333333333333333333333333333".to_string(),
            "bbbbbbb222222222222222222222222222222222".to_string(),
            "aaaaaaa111111111111111111111111111111111".to_string(),
        ];
        let hashes = vec![
            "aaaaaaa".to_string(),
            "ccccccc".to_string(),
            "bbbbbbb".to_string(),
        ];

        assert_eq!(
            resolve_reword_hashes_newest_first(&all_commits, &hashes),
            all_commits
        );
    }

    #[test]
    fn only_explicitly_queued_commits_are_pending() {
        let temp = init_test_repo();
        let path = temp.path().to_str().unwrap();
        let generic = test_commit(path, "one.txt", "one", "auto-git: [change] one.txt");
        let queued = test_commit(path, "two.txt", "two", "Improve the second file");
        let ignored = test_commit(path, "three.txt", "three", "Document the third file");

        let pending = pending_rewrite_hashes(path, std::slice::from_ref(&queued));

        assert_eq!(pending, vec![queued]);
        assert!(!pending.contains(&generic));
        assert!(!pending.contains(&ignored));
    }

    #[test]
    fn llm_commit_messages_must_be_complete_and_non_generic() {
        let hash = "aaaaaaa111111111111111111111111111111111".to_string();
        let mut valid = HashMap::new();
        valid.insert(
            "aaaaaaa".to_string(),
            "Explain the actual change".to_string(),
        );
        assert_eq!(
            validate_llm_commit_messages(valid, std::slice::from_ref(&hash))
                .unwrap()
                .get(&hash)
                .unwrap(),
            "Explain the actual change"
        );

        let mut generic = HashMap::new();
        generic.insert("aaaaaaa".to_string(), "auto-git: unchanged".to_string());
        assert!(validate_llm_commit_messages(generic, std::slice::from_ref(&hash)).is_err());
        assert!(validate_llm_commit_messages(HashMap::new(), &[hash]).is_err());
        assert!(parse_llm_commit_messages("```json\n{\"aaaaaaa\":\"message\"}\n```").is_err());
    }

    #[test]
    fn root_commit_can_be_reworded_without_a_parent_revision() {
        let temp = init_test_repo();
        let path = temp.path().to_str().unwrap();
        let root = test_commit(path, "root.txt", "root", "auto-git: [create] root.txt");
        let mut messages = HashMap::new();
        messages.insert(root.clone(), "Create the root fixture".to_string());

        reword_commits_transactionally(path, &messages, &[root]).unwrap();

        assert_eq!(
            run_git(path, &["show", "-s", "--format=%s", "HEAD"])
                .unwrap()
                .trim(),
            "Create the root fixture"
        );
        assert!(run_git(
            path,
            &[
                "for-each-ref",
                "--format=%(refname)",
                "refs/auto-git/rewrite-backups"
            ]
        )
        .unwrap()
        .trim()
        .is_empty());
    }

    #[test]
    fn jump_here_keeps_branch_history_visible_and_can_jump_again() {
        let temp = init_test_repo();
        let path = temp.path().to_str().unwrap();
        let older = test_commit(path, "older.txt", "older", "Create older fixture");
        let newer = test_commit(path, "newer.txt", "newer", "Create newer fixture");

        checkout_commit_internal(path, &older).unwrap();

        let visible = run_git(path, &["rev-list", VISIBLE_HISTORY_REVISION]).unwrap();
        assert!(visible.lines().any(|hash| hash == older));
        assert!(visible.lines().any(|hash| hash == newer));

        checkout_commit_internal(path, &newer).unwrap();
        assert_eq!(run_git(path, &["rev-parse", "HEAD"]).unwrap().trim(), newer);
        assert!(run_git(path, &["symbolic-ref", "--quiet", "HEAD"])
            .unwrap()
            .starts_with("refs/heads/"));
    }

    #[test]
    fn pending_descendant_identity_survives_an_older_commit_reword() {
        let temp = init_test_repo();
        let path = temp.path().to_str().unwrap();
        let older = test_commit(path, "older.txt", "older", "auto-git: [create] older.txt");
        let newer = test_commit(path, "newer.txt", "newer", "auto-git: [create] newer.txt");
        let identity = identities_for_hashes(path, std::slice::from_ref(&newer))
            .remove(&newer)
            .unwrap();
        let mut messages = HashMap::new();
        messages.insert(older.clone(), "Create the older fixture".to_string());

        reword_commits_transactionally(path, &messages, &[older]).unwrap();

        let remapped = current_commit_identities(path).remove(&identity).unwrap();
        assert_ne!(remapped, newer);
        assert_eq!(
            run_git(path, &["show", "-s", "--format=%s", &remapped])
                .unwrap()
                .trim(),
            "auto-git: [create] newer.txt"
        );
    }

    #[test]
    fn rewrite_refuses_dirty_worktrees_without_stashing() {
        let temp = init_test_repo();
        let path = temp.path().to_str().unwrap();
        let head = test_commit(path, "file.txt", "original", "auto-git: original");
        fs::write(temp.path().join("file.txt"), "dirty").unwrap();
        let mut messages = HashMap::new();
        messages.insert(head.clone(), "Improve the original message".to_string());

        let result = reword_commits_transactionally(path, &messages, std::slice::from_ref(&head));

        assert!(result.is_err());
        assert_eq!(run_git(path, &["rev-parse", "HEAD"]).unwrap().trim(), head);
        assert_eq!(
            fs::read_to_string(temp.path().join("file.txt")).unwrap(),
            "dirty"
        );
        assert!(!run_git(path, &["status", "--porcelain"])
            .unwrap()
            .trim()
            .is_empty());
    }

    #[test]
    fn existing_git_operations_are_never_aborted() {
        let temp = init_test_repo();
        let path = temp.path().to_str().unwrap();
        let head = test_commit(path, "file.txt", "content", "auto-git: original");
        let git_dir = git_dir_path(path).unwrap();
        fs::create_dir(git_dir.join("rebase-merge")).unwrap();
        let mut messages = HashMap::new();
        messages.insert(head.clone(), "Improve the original message".to_string());

        let result = reword_commits_transactionally(path, &messages, std::slice::from_ref(&head));

        assert!(result.is_err());
        assert!(git_dir.join("rebase-merge").exists());
        assert_eq!(run_git(path, &["rev-parse", "HEAD"]).unwrap().trim(), head);
    }

    #[test]
    fn monitored_commit_only_includes_event_paths() {
        let temp = init_test_repo();
        let path = temp.path().to_str().unwrap();
        fs::write(temp.path().join("watched.txt"), "before").unwrap();
        fs::write(temp.path().join("other.txt"), "before").unwrap();
        run_git(path, &["add", "watched.txt", "other.txt"]).unwrap();
        run_git(path, &["commit", "-m", "base"]).unwrap();
        fs::write(temp.path().join("watched.txt"), "after").unwrap();
        fs::write(temp.path().join("other.txt"), "unrelated").unwrap();

        let committed = commit_selected_paths(path, &["watched.txt".to_string()])
            .unwrap()
            .unwrap();

        assert_eq!(committed.staged_paths, vec!["watched.txt"]);
        assert_eq!(
            run_git(path, &["show", "--format=", "--name-only", "HEAD"])
                .unwrap()
                .trim(),
            "watched.txt"
        );
        let status = run_git(path, &["status", "--porcelain"]).unwrap();
        assert!(status.contains("other.txt"));
        assert!(!status.contains("watched.txt"));
    }

    #[test]
    fn monitoring_start_reconciles_changes_made_while_paused() {
        let temp = init_test_repo();
        let path = temp.path().to_str().unwrap();
        fs::write(temp.path().join("modified.txt"), "before").unwrap();
        fs::write(temp.path().join("deleted.txt"), "delete me").unwrap();
        run_git(path, &["add", "modified.txt", "deleted.txt"]).unwrap();
        run_git(path, &["commit", "-m", "base"]).unwrap();

        fs::write(temp.path().join("modified.txt"), "after").unwrap();
        fs::remove_file(temp.path().join("deleted.txt")).unwrap();
        fs::write(temp.path().join("created with spaces.txt"), "new").unwrap();

        let paths = changed_paths_for_monitoring(path).unwrap();
        assert_eq!(
            paths,
            vec![
                "created with spaces.txt".to_string(),
                "deleted.txt".to_string(),
                "modified.txt".to_string(),
            ]
        );

        let committed = commit_selected_paths(path, &paths).unwrap().unwrap();
        assert_eq!(committed.staged_paths, paths);
        assert!(run_git(path, &["status", "--porcelain"])
            .unwrap()
            .trim()
            .is_empty());
    }

    #[test]
    fn monitored_commit_preserves_an_existing_staged_index() {
        let temp = init_test_repo();
        let path = temp.path().to_str().unwrap();
        fs::write(temp.path().join("watched.txt"), "before").unwrap();
        fs::write(temp.path().join("staged.txt"), "before").unwrap();
        run_git(path, &["add", "watched.txt", "staged.txt"]).unwrap();
        run_git(path, &["commit", "-m", "base"]).unwrap();
        let original_head = run_git(path, &["rev-parse", "HEAD"]).unwrap();
        fs::write(temp.path().join("watched.txt"), "after").unwrap();
        fs::write(temp.path().join("staged.txt"), "staged").unwrap();
        run_git(path, &["add", "staged.txt"]).unwrap();

        let result = commit_selected_paths(path, &["watched.txt".to_string()]);

        assert!(result.is_err());
        assert_eq!(
            run_git(path, &["rev-parse", "HEAD"]).unwrap(),
            original_head
        );
        assert_eq!(
            run_git(path, &["diff", "--cached", "--name-only"])
                .unwrap()
                .trim(),
            "staged.txt"
        );
    }

    #[test]
    fn build_outputs_are_always_ignored_by_monitoring() {
        let temp = TempDir::new().unwrap();
        let root = temp.path().to_str().unwrap();
        assert!(is_default_ignored(
            root,
            &temp.path().join("src-tauri/target/debug/app")
        ));
        assert!(is_default_ignored(
            root,
            &temp.path().join("dist-tauri/renderer.js")
        ));
        assert!(is_default_ignored(
            root,
            &temp.path().join("node_modules/package/index.js")
        ));
    }

    #[cfg(unix)]
    #[test]
    fn subprocesses_are_killed_after_their_deadline() {
        let result = run_process_with_timeout(
            "sh",
            &["-c".to_string(), "while :; do :; done".to_string()],
            None,
            None,
            None,
            Duration::from_millis(100),
        );

        assert!(result.err().unwrap().contains("timed out"));
    }
}
