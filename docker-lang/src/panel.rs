//! Docker images panel: a sidebar view (`docker.images`) and a full page
//! (`docker.images.page`), drawn by the IDE from the JSON views built here.
//!
//! The `docker` CLI runs on background threads; the IDE polls the view while
//! the panel is shown, so results appear as they arrive. Events and views are
//! described in the IDE's `src/extension/ui_host.rs`.

use std::process::Command;
use std::sync::{Mutex, MutexGuard, OnceLock};
use std::time::{Duration, Instant};

use serde_json::{json, Value};

pub const SIDEBAR: &str = "docker.images";
pub const PAGE: &str = "docker.images.page";

/// Images are listed again after this long while a panel is shown.
const AUTO_REFRESH: Duration = Duration::from_secs(30);

#[derive(Debug, Clone, PartialEq)]
pub struct Image {
    pub id: String,
    pub repository: String,
    pub tag: String,
    pub size: String,
    pub created: String,
}

impl Image {
    fn is_dangling(&self) -> bool {
        self.repository == "<none>"
    }

    /// Name to pass to `docker`: `repo:tag`, or the id for an untagged image.
    pub fn reference(&self) -> String {
        if self.is_dangling() {
            self.id.clone()
        } else if self.tag == "<none>" {
            self.repository.clone()
        } else {
            format!("{}:{}", self.repository, self.tag)
        }
    }

    fn title(&self) -> String {
        if self.is_dangling() {
            "<untagged>".into()
        } else {
            self.reference()
        }
    }
}

/// Work for the `docker` CLI.
#[derive(Debug, Clone, PartialEq)]
pub enum Job {
    List,
    Pull(String),
    Remove(String),
    Prune,
}

#[derive(Default)]
pub struct State {
    pub images: Vec<Image>,
    pub loading: bool,
    pub loaded: Option<Instant>,
    pub error: Option<String>,
    /// Image references being pulled or removed, with the verb shown.
    pub busy: Vec<(String, &'static str)>,
    pub pruning: bool,
    /// Toasts for the IDE, sent with the next view.
    pub toasts: Vec<String>,
    /// Bumped to clear the pull input.
    pub input_revision: u64,
}

fn state() -> MutexGuard<'static, State> {
    static STATE: OnceLock<Mutex<State>> = OnceLock::new();
    STATE
        .get_or_init(Default::default)
        .lock()
        .unwrap_or_else(|e| e.into_inner())
}

// ── Pure logic (tested) ───────────────────────────────────────────────────────

/// Parse `docker image ls --format "{{json .}}"` (one object per line).
pub fn parse_images(stdout: &str) -> Vec<Image> {
    stdout
        .lines()
        .filter_map(|l| serde_json::from_str::<Value>(l.trim()).ok())
        .map(|v| {
            let field = |k: &str| v[k].as_str().unwrap_or("").to_string();
            Image {
                id: field("ID"),
                repository: field("Repository"),
                tag: field("Tag"),
                size: field("Size"),
                created: field("CreatedSince"),
            }
        })
        .collect()
}

/// Apply a UI event: the jobs to start and the actions for the IDE.
pub fn handle_event(state: &mut State, event: &Value) -> (Vec<Job>, Vec<Value>) {
    let mut jobs = Vec::new();
    let mut actions = Vec::new();
    if event["type"] != "click" {
        return (jobs, actions);
    }
    let row = event["row"].as_str().unwrap_or("");
    let image = state.images.iter().find(|i| i.id == row).cloned();
    match event["id"].as_str().unwrap_or("") {
        "refresh" => jobs.push(Job::List),
        "pull" => {
            let name = event["inputs"]["image"].as_str().unwrap_or("").trim();
            if name.is_empty() {
                state
                    .toasts
                    .push("Type the image to pull, e.g. nginx:latest".into());
            } else if !state.busy.iter().any(|(r, _)| r == name) {
                state.busy.push((name.to_string(), "Pulling"));
                state.input_revision += 1;
                jobs.push(Job::Pull(name.to_string()));
            }
        }
        "remove" => {
            if let Some(image) = image {
                let reference = image.reference();
                if !state.busy.iter().any(|(r, _)| *r == reference) {
                    state.busy.push((reference.clone(), "Removing"));
                    jobs.push(Job::Remove(reference));
                }
            }
        }
        "run" => {
            if let Some(image) = image {
                actions.push(json!({
                    "type": "terminal",
                    "command": format!("docker run --rm -it {}", image.reference()),
                }));
            }
        }
        "prune" if !state.pruning => {
            state.pruning = true;
            jobs.push(Job::Prune);
        }
        "open_page" => actions.push(json!({ "type": "open_panel", "panel": PAGE })),
        "open_containers" => actions.push(json!({
            "type": "open_panel",
            "panel": crate::containers::CONTAINERS,
        })),
        _ => {}
    }
    (jobs, actions)
}

/// Record the outcome of a finished job.
pub fn finish_job(state: &mut State, job: &Job, result: Result<String, String>) {
    match job {
        Job::List => {
            state.loading = false;
            state.loaded = Some(Instant::now());
            match result {
                Ok(out) => {
                    state.images = parse_images(&out);
                    state.error = None;
                }
                Err(e) => state.error = Some(e),
            }
        }
        Job::Pull(name) | Job::Remove(name) => {
            state.busy.retain(|(r, _)| r != name);
            let verb = if matches!(job, Job::Pull(_)) {
                "Pulled"
            } else {
                "Removed"
            };
            state.toasts.push(match result {
                Ok(_) => format!("{verb} {name}"),
                Err(e) => format!("{name}: {e}"),
            });
        }
        Job::Prune => {
            state.pruning = false;
            state.toasts.push(match result {
                Ok(out) => out
                    .lines()
                    .rev()
                    .find(|l| !l.trim().is_empty())
                    .unwrap_or("Unused images removed")
                    .to_string(),
                Err(e) => format!("Prune: {e}"),
            });
        }
    }
}

/// Whether the image list is due for a refresh.
pub fn needs_refresh(state: &State) -> bool {
    !state.loading && state.loaded.is_none_or(|t| t.elapsed() >= AUTO_REFRESH)
}

/// Row buttons of an image (disabled while it is being pulled or removed).
fn image_actions(image: &Image, busy: bool) -> Value {
    json!([
        { "id": "run", "label": "Run", "icon": "play", "enabled": !busy,
          "tooltip": format!("docker run --rm -it {}", image.reference()) },
        { "id": "remove", "label": "Remove", "icon": "trash", "style": "danger",
          "enabled": !busy, "confirm": format!("Remove image {}?", image.title()) },
    ])
}

/// The JSON view of `panel`; drains the pending toasts.
pub fn build_view(state: &mut State, panel: &str) -> Value {
    let mut children = vec![
        json!({ "type": "row", "children": [
            { "type": "input", "id": "image", "hint": "Image to pull, e.g. nginx:latest",
              "value": "", "revision": state.input_revision, "submit": "pull" },
            { "type": "button", "id": "pull", "label": "Pull", "icon": "download", "style": "primary" },
        ]}),
        json!({ "type": "row", "children": [
            { "type": "button", "id": "refresh", "label": "Refresh", "icon": "refresh",
              "enabled": !state.loading },
            { "type": "button", "id": "prune", "label": "Prune", "icon": "broom",
              "enabled": !state.pruning,
              "tooltip": "Remove dangling (untagged) images",
              "confirm": "Remove all dangling (untagged) images?" },
        ]}),
    ];
    if panel == SIDEBAR {
        children[1]["children"].as_array_mut().expect("row").push(
            json!({ "type": "button", "id": "open_page", "label": "Full view",
                          "icon": "external" }),
        );
        children.insert(
            0,
            json!({ "type": "button", "id": "open_containers", "label": "Containers & logs",
                    "icon": "container", "tooltip": "Start, stop and read the logs of containers" }),
        );
    }
    if let Some(e) = &state.error {
        children.push(json!({ "type": "text", "text": e, "style": "error" }));
    }
    for (reference, verb) in &state.busy {
        children.push(json!({ "type": "spinner", "text": format!("{verb} {reference}…") }));
    }
    if state.pruning {
        children.push(json!({ "type": "spinner", "text": "Pruning dangling images…" }));
    }
    if state.loading && state.images.is_empty() {
        children.push(json!({ "type": "spinner", "text": "Loading images…" }));
    }
    if state.error.is_none() && state.loaded.is_some() {
        children.push(json!({ "type": "text", "style": "muted",
            "text": format!("{} image(s)", state.images.len()) }));
    }
    let empty = "No images. Pull one above.";
    let busy = |i: &Image| state.busy.iter().any(|(r, _)| *r == i.reference());
    if panel == PAGE {
        children.insert(0, json!({ "type": "heading", "text": "Docker images" }));
        let rows: Vec<Value> = state
            .images
            .iter()
            .map(|i| {
                json!({ "id": i.id, "cells": [i.repository, i.tag, i.id, i.created, i.size],
                        "actions": image_actions(i, busy(i)) })
            })
            .collect();
        children.push(json!({ "type": "table", "empty": empty,
            "columns": ["Repository", "Tag", "Image ID", "Created", "Size"], "rows": rows }));
    } else {
        let items: Vec<Value> = state
            .images
            .iter()
            .map(|i| {
                json!({ "id": i.id, "title": i.title(),
                        "subtitle": format!("{} · {}", i.size, i.created),
                        "detail": i.id, "actions": image_actions(i, busy(i)) })
            })
            .collect();
        children.push(json!({ "type": "list", "items": items, "empty": empty }));
    }
    let toasts: Vec<Value> = state
        .toasts
        .drain(..)
        .map(|t| json!({ "type": "toast", "text": t }))
        .collect();
    let working = state.loading || state.pruning || !state.busy.is_empty();
    json!({
        "poll_ms": if working { 500 } else { 3000 },
        "actions": toasts,
        "children": children,
    })
}

// ── Docker CLI ────────────────────────────────────────────────────────────────

fn docker(args: &[&str]) -> Result<String, String> {
    docker_output(args).map(|(out, _)| out)
}

/// Run the `docker` CLI: (stdout, stderr) on success, else the first line of
/// stderr.
pub(crate) fn docker_output(args: &[&str]) -> Result<(String, String), String> {
    let mut cmd = Command::new("docker");
    cmd.args(args);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    let out = cmd.output().map_err(|e| {
        if e.kind() == std::io::ErrorKind::NotFound {
            "Docker CLI not found: install Docker and put `docker` on PATH.".to_string()
        } else {
            format!("docker: {e}")
        }
    })?;
    if out.status.success() {
        Ok((
            String::from_utf8_lossy(&out.stdout).into_owned(),
            String::from_utf8_lossy(&out.stderr).into_owned(),
        ))
    } else {
        let err = String::from_utf8_lossy(&out.stderr);
        let line = err
            .lines()
            .find(|l| !l.trim().is_empty())
            .unwrap_or("docker failed")
            .trim()
            .to_string();
        Err(line)
    }
}

fn run_job(job: &Job) -> Result<String, String> {
    match job {
        Job::List => docker(&["image", "ls", "--format", "{{json .}}"]),
        Job::Pull(name) => docker(&["pull", name]),
        Job::Remove(reference) => docker(&["image", "rm", reference]),
        Job::Prune => docker(&["image", "prune", "-f"]),
    }
}

fn start(job: Job) {
    if job == Job::List {
        let mut s = state();
        if s.loading {
            return;
        }
        s.loading = true;
    }
    std::thread::spawn(move || {
        let result = run_job(&job);
        let refresh = job != Job::List;
        finish_job(&mut state(), &job, result);
        if refresh {
            start(Job::List);
        }
    });
}

// ── Entry points used by the FFI layer ───────────────────────────────────────

pub fn view(panel: &str) -> Option<String> {
    if panel != SIDEBAR && panel != PAGE {
        return None;
    }
    let refresh = needs_refresh(&state());
    if refresh {
        start(Job::List);
    }
    let mut view = build_view(&mut state(), panel);
    if panel == SIDEBAR {
        // Running containers right under the toolbar (button + 2 rows).
        let section = crate::containers::sidebar_section();
        if let Some(children) = view["children"].as_array_mut() {
            let at = 3.min(children.len());
            children.splice(at..at, section);
        }
    }
    Some(view.to_string())
}

pub fn event(panel: &str, event: &str) -> Option<String> {
    if panel != SIDEBAR && panel != PAGE {
        return None;
    }
    let event: Value = serde_json::from_str(event).ok()?;
    if event["id"]
        .as_str()
        .is_some_and(|id| id.starts_with("container_"))
    {
        let actions = crate::containers::sidebar_event(&event);
        return Some(Value::Array(actions).to_string());
    }
    let (jobs, actions) = handle_event(&mut state(), &event);
    for job in jobs {
        start(job);
    }
    Some(Value::Array(actions).to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    const LS: &str = r#"{"Containers":"N/A","CreatedSince":"3 weeks ago","ID":"a1b2c3","Repository":"nginx","Size":"187MB","Tag":"latest"}
{"CreatedSince":"2 months ago","ID":"d4e5f6","Repository":"<none>","Size":"5MB","Tag":"<none>"}
not json
"#;

    fn loaded() -> State {
        State {
            images: parse_images(LS),
            loaded: Some(Instant::now()),
            ..Default::default()
        }
    }

    fn click(id: &str, row: Option<&str>, image: &str) -> Value {
        let mut e = json!({ "type": "click", "id": id, "inputs": { "image": image } });
        if let Some(r) = row {
            e["row"] = r.into();
        }
        e
    }

    #[test]
    fn parses_image_lines_and_skips_noise() {
        let images = parse_images(LS);
        assert_eq!(images.len(), 2);
        assert_eq!(images[0].reference(), "nginx:latest");
        assert_eq!(images[0].size, "187MB");
        assert_eq!(images[1].reference(), "d4e5f6", "untagged: by id");
        assert_eq!(images[1].title(), "<untagged>");
    }

    #[test]
    fn pull_starts_a_job_and_clears_the_input() {
        let mut s = loaded();
        let (jobs, actions) = handle_event(&mut s, &click("pull", None, " redis:7 "));
        assert_eq!(jobs, [Job::Pull("redis:7".into())]);
        assert!(actions.is_empty());
        assert_eq!(s.input_revision, 1);
        assert_eq!(s.busy, [("redis:7".to_string(), "Pulling")]);
        // Same image again while pulling: ignored.
        assert!(handle_event(&mut s, &click("pull", None, "redis:7"))
            .0
            .is_empty());
        // Nothing typed: a hint, no job.
        assert!(handle_event(&mut s, &click("pull", None, "")).0.is_empty());
        assert_eq!(s.toasts.len(), 1);
    }

    #[test]
    fn row_actions_target_the_clicked_image() {
        let mut s = loaded();
        let (jobs, _) = handle_event(&mut s, &click("remove", Some("a1b2c3"), ""));
        assert_eq!(jobs, [Job::Remove("nginx:latest".into())]);
        let (jobs, actions) = handle_event(&mut s, &click("run", Some("a1b2c3"), ""));
        assert!(jobs.is_empty());
        assert_eq!(actions[0]["type"], "terminal");
        assert_eq!(actions[0]["command"], "docker run --rm -it nginx:latest");
        assert!(handle_event(&mut s, &click("remove", Some("gone"), ""))
            .0
            .is_empty());
        let (_, actions) = handle_event(&mut s, &click("open_page", None, ""));
        assert_eq!(actions[0], json!({ "type": "open_panel", "panel": PAGE }));
        let (_, actions) = handle_event(&mut s, &click("open_containers", None, ""));
        assert_eq!(actions[0]["panel"], crate::containers::CONTAINERS);
        let (jobs, _) = handle_event(&mut s, &json!({ "type": "submit", "id": "image" }));
        assert!(jobs.is_empty(), "the submit's click does the pull");
    }

    #[test]
    fn finished_jobs_update_the_state_and_toast() {
        let mut s = State {
            loading: true,
            ..Default::default()
        };
        finish_job(
            &mut s,
            &Job::List,
            Err("Cannot connect to the Docker daemon".into()),
        );
        assert!(!s.loading && s.error.is_some());
        finish_job(&mut s, &Job::List, Ok(LS.into()));
        assert!(s.error.is_none());
        assert_eq!(s.images.len(), 2);

        s.busy.push(("redis".into(), "Removing"));
        finish_job(&mut s, &Job::Remove("redis".into()), Err("conflict".into()));
        assert!(s.busy.is_empty());
        assert_eq!(s.toasts, ["redis: conflict"]);

        s.pruning = true;
        finish_job(
            &mut s,
            &Job::Prune,
            Ok("Deleted Images:\nTotal reclaimed space: 5MB\n".into()),
        );
        assert!(!s.pruning);
        assert_eq!(s.toasts[1], "Total reclaimed space: 5MB");
    }

    #[test]
    fn views_list_images_and_drain_toasts() {
        let mut s = loaded();
        s.toasts.push("hello".into());
        let v = build_view(&mut s, SIDEBAR);
        assert_eq!(v["actions"][0]["text"], "hello");
        assert!(s.toasts.is_empty());
        let list = v["children"]
            .as_array()
            .unwrap()
            .iter()
            .find(|c| c["type"] == "list")
            .unwrap();
        assert_eq!(list["items"][0]["title"], "nginx:latest");
        assert_eq!(list["items"][0]["actions"][1]["style"], "danger");
        assert!(v.to_string().contains("open_page"));

        let page = build_view(&mut s, PAGE);
        assert_eq!(page["children"][0]["type"], "heading");
        let table = page["children"]
            .as_array()
            .unwrap()
            .iter()
            .find(|c| c["type"] == "table")
            .unwrap();
        assert_eq!(table["rows"][1]["cells"][0], "<none>");
        assert!(!page.to_string().contains("open_page"));
        assert_eq!(page["poll_ms"], 3000);

        s.loading = true;
        assert_eq!(build_view(&mut s, PAGE)["poll_ms"], 500);
    }

    #[test]
    fn refresh_is_due_when_never_loaded_or_stale() {
        let mut s = State::default();
        assert!(needs_refresh(&s));
        s.loading = true;
        assert!(!needs_refresh(&s));
        let s = loaded();
        assert!(!needs_refresh(&s));
    }

    /// Against the local Docker, with a running container named by
    /// `CU_E2E_CONTAINER`: listed in the sidebar, its logs read, then stopped
    /// from the sidebar's Stop button.
    #[test]
    #[ignore = "needs docker and a running container"]
    fn sidebar_running_containers_e2e() {
        let name = std::env::var("CU_E2E_CONTAINER").unwrap();
        let wait = |cond: &dyn Fn(&str) -> bool| -> String {
            for _ in 0..60 {
                let v = view(SIDEBAR).unwrap();
                if cond(&v) {
                    return v;
                }
                std::thread::sleep(Duration::from_millis(250));
            }
            panic!("timeout");
        };
        let v = wait(&|v| v.contains(&format!("\"title\":\"{name}\"")));
        println!("listed {name}");
        let id = {
            let v: Value = serde_json::from_str(&v).unwrap();
            let list = v["children"]
                .as_array()
                .unwrap()
                .iter()
                .find(|c| c["type"] == "list" && c.to_string().contains("container_logs"))
                .unwrap()
                .clone();
            list["items"]
                .as_array()
                .unwrap()
                .iter()
                .find(|i| i["title"] == name.as_str())
                .unwrap()["id"]
                .as_str()
                .unwrap()
                .to_string()
        };
        let click = |id_: &str| {
            event(
                SIDEBAR,
                &json!({ "type": "click", "id": id_, "row": id }).to_string(),
            )
            .unwrap()
        };
        let actions = click("container_logs");
        println!("logs click -> {actions}");
        assert!(actions.contains("docker.logs"));
        let logs = (0..40)
            .find_map(|_| {
                let v = crate::containers::view(crate::containers::LOGS).unwrap();
                if v.contains("hello from e2e") {
                    return Some(v);
                }
                std::thread::sleep(Duration::from_millis(250));
                None
            })
            .expect("logs");
        assert!(logs.contains("hello from e2e"));
        println!("logs read");
        click("container_stop");
        wait(&|v| !v.contains(&format!("\"title\":\"{name}\"")));
        println!("stopped and gone from the section");
    }

    #[test]
    fn unknown_panels_are_refused() {
        assert!(view("other").is_none());
        assert!(event("other", "{}").is_none());
        assert!(event(SIDEBAR, "not json").is_none());
    }
}
