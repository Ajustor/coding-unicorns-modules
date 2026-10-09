//! Docker containers page (`docker.containers`) and logs page
//! (`docker.logs`), both opened as editor tabs.
//!
//! Containers can be started, stopped, restarted and removed; *Logs* shows
//! the last lines of a container in the logs page, fetched again every
//! [`LOGS_REFRESH`] while it is shown (short `docker logs --tail` runs, so no
//! `docker logs -f` process outlives the IDE).

use std::sync::{Mutex, MutexGuard, OnceLock};
use std::time::{Duration, Instant};

use serde_json::{json, Value};

use crate::panel::docker_output;

pub const CONTAINERS: &str = "docker.containers";
pub const LOGS: &str = "docker.logs";

/// The container list is fetched again after this long while shown.
const LIST_REFRESH: Duration = Duration::from_secs(5);
/// Logs are fetched again after this long while following.
const LOGS_REFRESH: Duration = Duration::from_secs(2);
/// Line counts offered for the logs.
const TAILS: [u32; 3] = [200, 1000, 5000];

#[derive(Debug, Clone, PartialEq)]
pub struct Container {
    pub id: String,
    pub name: String,
    pub image: String,
    pub status: String,
    /// `running`, `exited`, `paused`, `created`…
    pub state: String,
    pub ports: String,
}

impl Container {
    pub fn is_running(&self) -> bool {
        self.state == "running"
    }
}

/// Work for the `docker` CLI.
#[derive(Debug, Clone, PartialEq)]
pub enum Job {
    List {
        all: bool,
    },
    /// `docker <verb> <id>`: `start`, `stop`, `restart` or `rm`.
    Act {
        verb: &'static str,
        id: String,
        name: String,
    },
    Logs {
        id: String,
        tail: u32,
    },
}

pub struct LogsState {
    pub id: String,
    pub name: String,
    pub text: String,
    pub error: Option<String>,
    pub loading: bool,
    pub loaded: Option<Instant>,
    pub follow: bool,
    pub timestamps: bool,
    pub tail: u32,
}

#[derive(Default)]
pub struct State {
    pub containers: Vec<Container>,
    /// Stopped containers are listed too.
    pub show_all: bool,
    pub loading: bool,
    pub loaded: Option<Instant>,
    pub error: Option<String>,
    /// Container ids with an action in progress, and its verb.
    pub busy: Vec<(String, &'static str)>,
    pub logs: Option<LogsState>,
    pub toasts: Vec<String>,
}

fn state() -> MutexGuard<'static, State> {
    static STATE: OnceLock<Mutex<State>> = OnceLock::new();
    STATE
        .get_or_init(|| {
            Mutex::new(State {
                show_all: true,
                ..Default::default()
            })
        })
        .lock()
        .unwrap_or_else(|e| e.into_inner())
}

// ── Pure logic (tested) ───────────────────────────────────────────────────────

/// Parse `docker ps --format "{{json .}}"` (one object per line).
pub fn parse_containers(stdout: &str) -> Vec<Container> {
    stdout
        .lines()
        .filter_map(|l| serde_json::from_str::<Value>(l.trim()).ok())
        .map(|v| {
            let field = |k: &str| v[k].as_str().unwrap_or("").to_string();
            Container {
                id: field("ID"),
                name: field("Names"),
                image: field("Image"),
                status: field("Status"),
                state: field("State"),
                ports: field("Ports"),
            }
        })
        .collect()
}

/// Merge `docker logs --timestamps` stdout and stderr by time (each line
/// starts with an RFC 3339 timestamp), dropping the timestamps unless asked.
pub fn merge_logs(stdout: &str, stderr: &str, timestamps: bool) -> String {
    let mut lines: Vec<&str> = stdout.lines().chain(stderr.lines()).collect();
    // Same timestamp format everywhere: text order is time order. Stable, so
    // lines of one stream keep their order.
    lines.sort_by(|a, b| timestamp(a).cmp(timestamp(b)));
    let lines: Vec<&str> = lines
        .into_iter()
        .map(|l| {
            if timestamps {
                l
            } else {
                l.split_once(' ').map(|(_, rest)| rest).unwrap_or(l)
            }
        })
        .collect();
    lines.join("\n")
}

fn timestamp(line: &str) -> &str {
    line.split_once(' ').map(|(t, _)| t).unwrap_or(line)
}

/// Apply a UI event: the jobs to start and the actions for the IDE.
pub fn handle_event(state: &mut State, event: &Value) -> (Vec<Job>, Vec<Value>) {
    let mut jobs = Vec::new();
    let mut actions = Vec::new();
    let id = event["id"].as_str().unwrap_or("");
    if event["type"] == "toggle" {
        let checked = event["checked"].as_bool().unwrap_or(false);
        match id {
            "all" => {
                state.show_all = checked;
                jobs.push(Job::List { all: checked });
            }
            "follow" => {
                if let Some(l) = &mut state.logs {
                    l.follow = checked;
                }
            }
            "timestamps" => {
                if let Some(l) = &mut state.logs {
                    l.timestamps = checked;
                    jobs.push(Job::Logs {
                        id: l.id.clone(),
                        tail: l.tail,
                    });
                }
            }
            _ => {}
        }
        return (jobs, actions);
    }
    if event["type"] != "click" {
        return (jobs, actions);
    }
    let row = event["row"].as_str().unwrap_or("");
    let container = state.containers.iter().find(|c| c.id == row).cloned();
    match (id, container) {
        ("refresh", _) => jobs.push(Job::List {
            all: state.show_all,
        }),
        ("logs", Some(c)) => {
            let (follow, timestamps, tail) = state
                .logs
                .as_ref()
                .map(|l| (l.follow, l.timestamps, l.tail))
                .unwrap_or((true, false, TAILS[0]));
            state.logs = Some(LogsState {
                id: c.id.clone(),
                name: c.name.clone(),
                text: String::new(),
                error: None,
                loading: false,
                loaded: None,
                follow,
                timestamps,
                tail,
            });
            actions.push(json!({ "type": "open_panel", "panel": LOGS }));
        }
        ("reload_logs", _) => {
            if let Some(l) = &state.logs {
                jobs.push(Job::Logs {
                    id: l.id.clone(),
                    tail: l.tail,
                });
            }
        }
        ("clear_logs", _) => {
            if let Some(l) = &mut state.logs {
                l.text.clear();
            }
        }
        (tail, _) if tail.starts_with("tail_") => {
            if let (Some(l), Ok(n)) = (&mut state.logs, tail[5..].parse()) {
                l.tail = n;
                jobs.push(Job::Logs {
                    id: l.id.clone(),
                    tail: n,
                });
            }
        }
        ("containers", _) => actions.push(json!({ "type": "open_panel", "panel": CONTAINERS })),
        ("shell", Some(c)) => actions.push(json!({
            "type": "terminal",
            "command": format!("docker exec -it {} sh", c.name),
        })),
        (verb @ ("start" | "stop" | "restart" | "rm"), Some(c))
            if !state.busy.iter().any(|(i, _)| *i == c.id) =>
        {
            let verb: &'static str = match verb {
                "start" => "start",
                "stop" => "stop",
                "restart" => "restart",
                _ => "rm",
            };
            state.busy.push((c.id.clone(), verb));
            jobs.push(Job::Act {
                verb,
                id: c.id,
                name: c.name,
            });
        }
        _ => {}
    }
    (jobs, actions)
}

/// Record the outcome of a finished job.
pub fn finish_job(state: &mut State, job: &Job, result: Result<(String, String), String>) {
    match job {
        Job::List { .. } => {
            state.loading = false;
            state.loaded = Some(Instant::now());
            match result {
                Ok((out, _)) => {
                    state.containers = parse_containers(&out);
                    state.error = None;
                }
                Err(e) => state.error = Some(e),
            }
        }
        Job::Act { verb, id, name } => {
            state.busy.retain(|(i, _)| i != id);
            let done = match *verb {
                "start" => "Started",
                "stop" => "Stopped",
                "restart" => "Restarted",
                _ => "Removed",
            };
            state.toasts.push(match result {
                Ok(_) => format!("{done} {name}"),
                Err(e) => format!("{name}: {e}"),
            });
        }
        Job::Logs { id, .. } => {
            let Some(l) = state.logs.as_mut().filter(|l| l.id == *id) else {
                return; // another container was chosen meanwhile
            };
            l.loading = false;
            l.loaded = Some(Instant::now());
            match result {
                Ok((out, err)) => {
                    l.text = merge_logs(&out, &err, l.timestamps);
                    l.error = None;
                }
                Err(e) => l.error = Some(e),
            }
        }
    }
}

/// Jobs due when `panel` is shown: the container list, and the logs while
/// following them.
pub fn due_jobs(state: &mut State, panel: &str) -> Vec<Job> {
    let mut jobs = Vec::new();
    if !state.loading && state.loaded.is_none_or(|t| t.elapsed() >= LIST_REFRESH) {
        state.loading = true;
        jobs.push(Job::List {
            all: state.show_all,
        });
    }
    if panel == LOGS {
        if let Some(l) = &mut state.logs {
            let due = l.loaded.is_none()
                || (l.follow && l.loaded.is_some_and(|t| t.elapsed() >= LOGS_REFRESH));
            if !l.loading && due {
                l.loading = true;
                jobs.push(Job::Logs {
                    id: l.id.clone(),
                    tail: l.tail,
                });
            }
        }
    }
    jobs
}

fn container_actions(c: &Container, busy: bool) -> Value {
    let enabled = !busy;
    let mut a = vec![json!({ "id": "logs", "label": "Logs", "icon": "list" })];
    if c.is_running() {
        a.push(json!({ "id": "stop", "label": "Stop", "icon": "stop", "enabled": enabled }));
        a.push(
            json!({ "id": "restart", "label": "Restart", "icon": "refresh", "enabled": enabled }),
        );
        a.push(json!({ "id": "shell", "label": "Shell", "icon": "terminal",
                       "tooltip": format!("docker exec -it {} sh", c.name) }));
    } else {
        a.push(json!({ "id": "start", "label": "Start", "icon": "play", "enabled": enabled }));
        a.push(
            json!({ "id": "rm", "label": "Remove", "icon": "trash", "style": "danger",
                       "enabled": enabled, "confirm": format!("Remove container {}?", c.name) }),
        );
    }
    Value::Array(a)
}

fn containers_view(state: &State) -> Vec<Value> {
    let running = state.containers.iter().filter(|c| c.is_running()).count();
    let mut children = vec![
        json!({ "type": "heading", "text": "Docker containers" }),
        json!({ "type": "row", "children": [
            { "type": "button", "id": "refresh", "label": "Refresh", "icon": "refresh",
              "enabled": !state.loading },
            { "type": "checkbox", "id": "all", "label": "Show stopped containers",
              "checked": state.show_all },
        ]}),
    ];
    if let Some(e) = &state.error {
        children.push(json!({ "type": "text", "text": e, "style": "error" }));
    } else if state.loaded.is_some() {
        children.push(json!({ "type": "text", "style": "muted", "text":
            format!("{} container(s), {running} running", state.containers.len()) }));
    } else {
        children.push(json!({ "type": "spinner", "text": "Loading containers…" }));
    }
    for (id, verb) in &state.busy {
        let name = state
            .containers
            .iter()
            .find(|c| c.id == *id)
            .map_or(id.as_str(), |c| &c.name);
        children.push(json!({ "type": "spinner", "text": format!("docker {verb} {name}…") }));
    }
    let busy = |c: &Container| state.busy.iter().any(|(i, _)| *i == c.id);
    let rows: Vec<Value> = state
        .containers
        .iter()
        .map(|c| {
            json!({ "id": c.id, "cells": [c.name, c.image, c.status, c.ports],
                         "actions": container_actions(c, busy(c)) })
        })
        .collect();
    children.push(
        json!({ "type": "table", "columns": ["Name", "Image", "Status", "Ports"],
        "rows": rows, "empty": "No containers." }),
    );
    children
}

fn logs_view(state: &State) -> Vec<Value> {
    let Some(l) = &state.logs else {
        return vec![
            json!({ "type": "heading", "text": "Docker logs" }),
            json!({ "type": "text", "style": "muted",
                    "text": "Choose a container and click Logs." }),
            json!({ "type": "button", "id": "containers", "label": "Containers", "icon": "container" }),
        ];
    };
    let running = state
        .containers
        .iter()
        .find(|c| c.id == l.id)
        .map(Container::is_running);
    let status = match running {
        Some(true) => "running",
        Some(false) => "stopped",
        None => "",
    };
    let mut tail_buttons: Vec<Value> = TAILS
        .iter()
        .map(|n| {
            json!({ "type": "button", "id": format!("tail_{n}"), "label": format!("Last {n}"),
                         "style": if *n == l.tail { "primary" } else { "default" } })
        })
        .collect();
    let mut toolbar = vec![
        json!({ "type": "checkbox", "id": "follow", "label": "Follow", "checked": l.follow }),
        json!({ "type": "checkbox", "id": "timestamps", "label": "Timestamps", "checked": l.timestamps }),
        json!({ "type": "button", "id": "reload_logs", "label": "Reload", "icon": "refresh" }),
        json!({ "type": "button", "id": "clear_logs", "label": "Clear", "icon": "broom" }),
    ];
    toolbar.append(&mut tail_buttons);
    toolbar.push(
        json!({ "type": "button", "id": "containers", "label": "Containers", "icon": "container" }),
    );
    let mut children = vec![
        json!({ "type": "heading", "text": format!("Logs — {}", l.name) }),
        json!({ "type": "text", "style": "muted", "text": format!("{} {status}", l.id) }),
        json!({ "type": "row", "children": toolbar }),
    ];
    if let Some(e) = &l.error {
        children.push(json!({ "type": "text", "text": e, "style": "error" }));
    }
    if l.text.is_empty() && l.loading {
        children.push(json!({ "type": "spinner", "text": "Loading logs…" }));
    }
    children.push(json!({ "type": "log", "text": l.text }));
    children
}

/// The JSON view of `panel`; drains the pending toasts.
pub fn build_view(state: &mut State, panel: &str) -> Value {
    let children = if panel == LOGS {
        logs_view(state)
    } else {
        containers_view(state)
    };
    let toasts: Vec<Value> = state
        .toasts
        .drain(..)
        .map(|t| json!({ "type": "toast", "text": t }))
        .collect();
    let following = panel == LOGS && state.logs.as_ref().is_some_and(|l| l.follow);
    let poll = if !state.busy.is_empty() || following {
        500
    } else {
        2000
    };
    json!({ "poll_ms": poll, "actions": toasts, "children": children })
}

// ── Docker CLI ────────────────────────────────────────────────────────────────

fn run_job(job: &Job) -> Result<(String, String), String> {
    match job {
        Job::List { all } => {
            let mut args = vec!["ps", "--format", "{{json .}}"];
            if *all {
                args.push("--all");
            }
            docker_output(&args)
        }
        Job::Act { verb, id, .. } => docker_output(&[*verb, id.as_str()]),
        Job::Logs { id, tail } => {
            docker_output(&["logs", "--timestamps", "--tail", &tail.to_string(), id])
        }
    }
}

fn start(job: Job) {
    std::thread::spawn(move || {
        let result = run_job(&job);
        let relist = matches!(job, Job::Act { .. });
        let show_all = {
            let mut s = state();
            finish_job(&mut s, &job, result);
            if relist {
                s.loading = true;
            }
            s.show_all
        };
        if relist {
            start(Job::List { all: show_all });
        }
    });
}

// ── Entry points used by the FFI layer ───────────────────────────────────────

pub fn handles(panel: &str) -> bool {
    panel == CONTAINERS || panel == LOGS
}

pub fn view(panel: &str) -> Option<String> {
    if !handles(panel) {
        return None;
    }
    let jobs = due_jobs(&mut state(), panel);
    for job in jobs {
        start(job);
    }
    Some(build_view(&mut state(), panel).to_string())
}

pub fn event(panel: &str, event: &str) -> Option<String> {
    if !handles(panel) {
        return None;
    }
    let event: Value = serde_json::from_str(event).ok()?;
    Some(Value::Array(run_event(&event)).to_string())
}

/// Running containers, for the Docker sidebar panel (its buttons send
/// `container_logs` / `container_stop` events, see [`sidebar_event`]).
pub fn running_section(state: &State) -> Vec<Value> {
    let mut section = vec![
        json!({ "type": "separator" }),
        json!({ "type": "heading", "text": "Running containers" }),
    ];
    if let Some(e) = &state.error {
        section.push(json!({ "type": "text", "text": e, "style": "error" }));
        return section;
    }
    if state.loaded.is_none() {
        section.push(json!({ "type": "spinner", "text": "Loading containers…" }));
        return section;
    }
    let items: Vec<Value> = state
        .containers
        .iter()
        .filter(|c| c.is_running())
        .map(|c| {
            let busy = state.busy.iter().any(|(i, _)| *i == c.id);
            let detail = if c.ports.is_empty() {
                c.status.clone()
            } else {
                format!("{} · {}", c.status, c.ports)
            };
            json!({ "id": c.id, "title": c.name, "subtitle": c.image, "detail": detail,
                    "actions": [
                        { "id": "container_logs", "label": "Logs", "icon": "list" },
                        { "id": "container_stop", "label": "Stop", "icon": "stop", "enabled": !busy },
                    ] })
        })
        .collect();
    section.push(json!({ "type": "list", "items": items, "empty": "No running containers." }));
    section
}

/// The running-containers section of the sidebar, refreshing the list when
/// due.
pub fn sidebar_section() -> Vec<Value> {
    for job in due_jobs(&mut state(), CONTAINERS) {
        start(job);
    }
    running_section(&state())
}

/// A sidebar `container_*` event: handled like the containers page's.
pub fn sidebar_event(event: &Value) -> Vec<Value> {
    let mut event = event.clone();
    let id = event["id"].as_str().unwrap_or("").to_string();
    event["id"] = id.trim_start_matches("container_").into();
    run_event(&event)
}

/// Apply `event`, start its jobs, and return the actions for the IDE.
fn run_event(event: &Value) -> Vec<Value> {
    let (jobs, actions) = handle_event(&mut state(), event);
    for job in jobs {
        if matches!(job, Job::List { .. }) {
            state().loading = true;
        }
        if let Job::Logs { .. } = job {
            if let Some(l) = &mut state().logs {
                l.loading = true;
            }
        }
        start(job);
    }
    actions
}

#[cfg(test)]
mod tests {
    use super::*;

    const PS: &str = r#"{"ID":"c1","Names":"web","Image":"nginx","Status":"Up 2 hours","State":"running","Ports":"0.0.0.0:80->80/tcp"}
{"ID":"c2","Names":"db","Image":"postgres:16","Status":"Exited (0) 3 days ago","State":"exited","Ports":""}
garbage
"#;

    fn loaded() -> State {
        State {
            containers: parse_containers(PS),
            loaded: Some(Instant::now()),
            show_all: true,
            ..Default::default()
        }
    }

    fn click(id: &str, row: Option<&str>) -> Value {
        let mut e = json!({ "type": "click", "id": id, "inputs": {} });
        if let Some(r) = row {
            e["row"] = r.into();
        }
        e
    }

    #[test]
    fn parses_containers() {
        let c = parse_containers(PS);
        assert_eq!(c.len(), 2);
        assert_eq!(c[0].name, "web");
        assert!(c[0].is_running());
        assert!(!c[1].is_running());
        assert_eq!(c[0].ports, "0.0.0.0:80->80/tcp");
    }

    #[test]
    fn logs_of_both_streams_are_merged_by_time() {
        let out = "2026-10-09T08:00:01.000000000Z started\n2026-10-09T08:00:03.000000000Z ready";
        let err = "2026-10-09T08:00:02.000000000Z warning: x";
        assert_eq!(merge_logs(out, err, false), "started\nwarning: x\nready");
        assert!(merge_logs(out, err, true).starts_with("2026-10-09T08:00:01.000000000Z started"));
        assert_eq!(merge_logs("", "", false), "");
    }

    #[test]
    fn row_actions_depend_on_the_container_state() {
        let s = loaded();
        let ids = |c: &Container| -> Vec<String> {
            container_actions(c, false)
                .as_array()
                .unwrap()
                .iter()
                .map(|a| a["id"].as_str().unwrap().to_string())
                .collect()
        };
        assert_eq!(ids(&s.containers[0]), ["logs", "stop", "restart", "shell"]);
        assert_eq!(ids(&s.containers[1]), ["logs", "start", "rm"]);
        assert_eq!(
            container_actions(&s.containers[1], false)[2]["style"],
            "danger"
        );
    }

    #[test]
    fn stop_and_start_run_once_per_container() {
        let mut s = loaded();
        let (jobs, _) = handle_event(&mut s, &click("stop", Some("c1")));
        assert_eq!(
            jobs,
            [Job::Act {
                verb: "stop",
                id: "c1".into(),
                name: "web".into()
            }]
        );
        assert!(
            handle_event(&mut s, &click("stop", Some("c1")))
                .0
                .is_empty(),
            "busy"
        );
        let (jobs, _) = handle_event(&mut s, &click("start", Some("c2")));
        assert_eq!(
            jobs,
            [Job::Act {
                verb: "start",
                id: "c2".into(),
                name: "db".into()
            }]
        );
        assert!(handle_event(&mut s, &click("stop", Some("gone")))
            .0
            .is_empty());

        finish_job(
            &mut s,
            &Job::Act {
                verb: "stop",
                id: "c1".into(),
                name: "web".into(),
            },
            Ok(Default::default()),
        );
        assert_eq!(s.toasts, ["Stopped web"]);
        assert!(!s.busy.iter().any(|(i, _)| i == "c1"));
    }

    #[test]
    fn logs_open_the_logs_page_and_follow() {
        let mut s = loaded();
        let (jobs, actions) = handle_event(&mut s, &click("logs", Some("c1")));
        assert!(jobs.is_empty());
        assert_eq!(actions[0], json!({ "type": "open_panel", "panel": LOGS }));
        let l = s.logs.as_ref().unwrap();
        assert_eq!((l.name.as_str(), l.follow, l.tail), ("web", true, 200));

        // Shown: fetched at once, then again only once LOGS_REFRESH passed.
        let jobs = due_jobs(&mut s, LOGS);
        assert!(jobs.contains(&Job::Logs {
            id: "c1".into(),
            tail: 200
        }));
        assert!(due_jobs(&mut s, LOGS).is_empty(), "already loading");
        let job = Job::Logs {
            id: "c1".into(),
            tail: 200,
        };
        finish_job(
            &mut s,
            &job,
            Ok(("2026-10-09T08:00:01Z hi".into(), String::new())),
        );
        assert_eq!(s.logs.as_ref().unwrap().text, "hi");
        assert!(due_jobs(&mut s, LOGS).is_empty(), "fresh");

        let (jobs, _) = handle_event(&mut s, &click("tail_1000", None));
        assert_eq!(
            jobs,
            [Job::Logs {
                id: "c1".into(),
                tail: 1000
            }]
        );
        handle_event(
            &mut s,
            &json!({ "type": "toggle", "id": "follow", "checked": false }),
        );
        assert!(!s.logs.as_ref().unwrap().follow);

        // A late result for another container is ignored.
        finish_job(
            &mut s,
            &Job::Logs {
                id: "c2".into(),
                tail: 200,
            },
            Ok(("x y".into(), String::new())),
        );
        assert_eq!(s.logs.as_ref().unwrap().text, "hi");
    }

    #[test]
    fn show_stopped_toggle_relists() {
        let mut s = loaded();
        let (jobs, _) = handle_event(
            &mut s,
            &json!({ "type": "toggle", "id": "all", "checked": false }),
        );
        assert_eq!(jobs, [Job::List { all: false }]);
        assert!(!s.show_all);
    }

    #[test]
    fn views() {
        let mut s = loaded();
        let v = build_view(&mut s, CONTAINERS);
        let text = v.to_string();
        assert!(text.contains("2 container(s), 1 running"), "{text}");
        assert!(text.contains("\"type\":\"table\""));

        let v = build_view(&mut s, LOGS);
        assert!(v.to_string().contains("Choose a container"));
        handle_event(&mut s, &click("logs", Some("c1")));
        s.logs.as_mut().unwrap().text = "a\nb".into();
        let v = build_view(&mut s, LOGS);
        let log = v["children"]
            .as_array()
            .unwrap()
            .iter()
            .find(|c| c["type"] == "log")
            .unwrap();
        assert_eq!(log["text"], "a\nb");
        assert_eq!(v["poll_ms"], 500, "following");
        assert!(v.to_string().contains("Logs — web"));
    }

    /// Against the local Docker: list the containers, then read the logs of
    /// the first one through the same entry points as the IDE.
    #[test]
    #[ignore = "needs docker"]
    fn real_docker() {
        view(CONTAINERS);
        for _ in 0..40 {
            if state().loaded.is_some() {
                break;
            }
            std::thread::sleep(Duration::from_millis(250));
        }
        let first = state().containers.first().cloned().expect("a container");
        println!(
            "{} containers, first {}",
            state().containers.len(),
            first.name
        );
        let ev = json!({ "type": "click", "id": "logs", "row": first.id }).to_string();
        println!("actions {}", event(CONTAINERS, &ev).unwrap());
        view(LOGS);
        for _ in 0..40 {
            if state().logs.as_ref().is_some_and(|l| l.loaded.is_some()) {
                break;
            }
            std::thread::sleep(Duration::from_millis(250));
        }
        let s = state();
        let l = s.logs.as_ref().unwrap();
        println!(
            "logs error {:?}, {} lines, last: {:?}",
            l.error,
            l.text.lines().count(),
            l.text.lines().last()
        );
        assert!(l.loaded.is_some());
    }

    #[test]
    fn running_section_lists_running_containers_only() {
        let s = loaded();
        let v = Value::Array(running_section(&s));
        let list = v
            .as_array()
            .unwrap()
            .iter()
            .find(|c| c["type"] == "list")
            .unwrap();
        let items = list["items"].as_array().unwrap();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0]["title"], "web");
        assert_eq!(items[0]["detail"], "Up 2 hours · 0.0.0.0:80->80/tcp");
        assert_eq!(items[0]["actions"][0]["id"], "container_logs");

        let s = State {
            error: Some("daemon down".into()),
            ..Default::default()
        };
        assert!(Value::Array(running_section(&s))
            .to_string()
            .contains("daemon down"));
        let s = State::default();
        assert!(Value::Array(running_section(&s))
            .to_string()
            .contains("Loading containers"));
    }

    #[test]
    fn unknown_panels_are_refused() {
        assert!(view("docker.images").is_none());
        assert!(event("other", "{}").is_none());
    }
}
