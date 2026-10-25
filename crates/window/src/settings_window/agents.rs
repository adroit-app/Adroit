//! The groups of options the Agents pages give each agent's new sessions.

use adroit_model::config::{
    CodexOptions, OpenCodeOptions, PiOptions, SessionOptions, format_list, format_rules,
    parse_list, parse_rules,
};
use dioxus::prelude::*;

use crate::components::{Select, SettingGroup, SettingRow, TextArea, TextField, Toggle};

/// The permission modes Claude Code takes, as it names them, with what the
/// Settings window calls each; empty leaves it to Claude Code.
const MODES: [(&str, &str); 5] = [
    ("", "Claude Code's own"),
    ("default", "Ask before acting"),
    ("acceptEdits", "Accept edits"),
    ("plan", "Plan only"),
    ("bypassPermissions", "Bypass permissions"),
];

/// The models offered to choose from; another can be typed.
const MODELS: [&str; 3] = ["opus", "sonnet", "haiku"];

#[component]
pub(super) fn CodexGroup(
    title: String,
    options: CodexOptions,
    set: Callback<CodexOptions>,
) -> Element {
    let (model, effort, approval, sandbox) = (
        options.clone(),
        options.clone(),
        options.clone(),
        options.clone(),
    );
    rsx! {
        SettingGroup { title: "{title}",
            SettingRow { label: "Model", hint: "Empty to use Codex's own model.",
                TextField {
                    value: options.model,
                    aria_label: "Model",
                    placeholder: "Codex's own",
                    onchange: move |value: String| set(CodexOptions { model: value.trim().into(), ..model.clone() }),
                }
            }
            SettingRow { label: "Reasoning effort", hint: "Use a level supported by the selected model, or leave empty for Codex's own.",
                TextField {
                    list: "codex-efforts",
                    aria_label: "Reasoning effort",
                    value: options.reasoning_effort,
                    placeholder: "Codex's own",
                    onchange: move |value: String| set(CodexOptions { reasoning_effort: value.trim().into(), ..effort.clone() }),
                }
                datalist { id: "codex-efforts",
                    for value in ["low", "medium", "high", "xhigh", "max", "ultra"] {
                        option { key: "{value}", value: value }
                    }
                }
            }
            SettingRow { label: "Approval policy", hint: "When Codex asks before running a command. Never ask keeps sandbox restrictions in place.",
                Select { label: "Approval policy", value: options.approval_policy,
                    choices: vec![("", "Codex's own"), ("on-request", "Ask when needed"), ("never", "Never ask")],
                    onchange: move |value| set(CodexOptions { approval_policy: value, ..approval.clone() }),
                }
            }
            SettingRow { label: "Sandbox", hint: "Where commands can write. Full access removes the sandbox.",
                Select { label: "Sandbox", value: options.sandbox_mode,
                    choices: vec![("", "Codex's own"), ("read-only", "Read only"), ("workspace-write", "Workspace write"), ("danger-full-access", "Full access")],
                    onchange: move |value| set(CodexOptions { sandbox_mode: value, ..sandbox.clone() }),
                }
            }
        }
    }
}

/// How Pi sessions run: their model, thinking level and the tools they are
/// given, which is all Pi has for permissions.
#[component]
pub(super) fn PiGroup(title: String, options: PiOptions, set: Callback<PiOptions>) -> Element {
    let (model, thinking, tools) = (options.clone(), options.clone(), options.clone());
    rsx! {
        SettingGroup { title: "{title}",
            SettingRow { label: "Model", hint: "As provider/id, the way Pi's --model takes it; empty for Pi's own.",
                TextField {
                    value: options.model,
                    aria_label: "Model",
                    placeholder: "Pi's own",
                    onchange: move |value: String| set(PiOptions { model: value.trim().into(), ..model.clone() }),
                }
            }
            SettingRow { label: "Thinking", hint: "A level the selected model supports, or empty for Pi's own.",
                TextField {
                    list: "pi-thinking",
                    aria_label: "Thinking",
                    value: options.thinking,
                    placeholder: "Pi's own",
                    onchange: move |value: String| set(PiOptions { thinking: value.trim().into(), ..thinking.clone() }),
                }
                datalist { id: "pi-thinking",
                    for value in ["off", "minimal", "low", "medium", "high", "xhigh", "max"] {
                        option { key: "{value}", value: value }
                    }
                }
            }
            SettingRow { label: "Tools", hint: "The tools they may use, separated by commas; empty for Pi's own set. Pi asks before no tool call and has no sandbox: a session given bash runs any command.",
                TextField {
                    value: "{format_list(&options.tools)}",
                    aria_label: "Tools",
                    placeholder: "read, bash, edit, write",
                    onchange: move |value: String| set(PiOptions { tools: parse_list(&value), ..tools.clone() }),
                }
            }
        }
    }
}

/// How OpenCode sessions run: their model, the agent they start as, their
/// permission rules and whether they approve on their own, which is all
/// OpenCode has for permissions.
#[component]
pub(super) fn OpenCodeGroup(
    title: String,
    options: OpenCodeOptions,
    set: Callback<OpenCodeOptions>,
) -> Element {
    let (model, agent, rules, auto) = (
        options.clone(),
        options.clone(),
        options.clone(),
        options.clone(),
    );
    rsx! {
        SettingGroup { title: "{title}",
            SettingRow { label: "Model", hint: "As provider/model, the way OpenCode lists its models; empty for OpenCode's own.",
                TextField {
                    value: options.model,
                    aria_label: "Model",
                    placeholder: "OpenCode's own",
                    onchange: move |value: String| set(OpenCodeOptions { model: value.trim().into(), ..model.clone() }),
                }
            }
            SettingRow { label: "Agent", hint: "The primary agent they start as, one of OpenCode's or its login's own; empty for OpenCode's default.",
                TextField {
                    list: "opencode-agents",
                    aria_label: "Agent",
                    value: options.agent,
                    placeholder: "build",
                    onchange: move |value: String| set(OpenCodeOptions { agent: value.trim().into(), ..agent.clone() }),
                }
                datalist { id: "opencode-agents",
                    for value in ["build", "plan"] {
                        option { key: "{value}", value: value }
                    }
                }
            }
            SettingRow { label: "Permission rules", hint: "One a line: allow, ask or deny, then what a tool does, such as edit, shell or webfetch, then what it is done to, * for anything. They come after its login's own, and the last that matches a tool call decides. OpenCode has no sandbox: a session allowed shell runs any command.",
                TextArea {
                    value: "{format_rules(&options.permissions)}",
                    placeholder: "ask shell *",
                    onchange: move |value: String| set(OpenCodeOptions { permissions: parse_rules(&value), ..rules.clone() }),
                }
            }
            SettingRow { label: "Approve on their own", hint: "Every tool call no rule denies runs without asking, as OpenCode's --auto does.",
                Toggle {
                    checked: options.auto,
                    onchange: move |on: bool| set(OpenCodeOptions { auto: on, ..auto.clone() }),
                }
            }
        }
    }
}

/// How sessions of one kind run: their model, permission mode, the tools
/// denied them, and whether Adroit's channel warning is confirmed for them.
#[component]
pub(super) fn SessionGroup(
    title: String,
    options: SessionOptions,
    set: Callback<SessionOptions>,
) -> Element {
    let (a, b, c, d) = (
        options.clone(),
        options.clone(),
        options.clone(),
        options.clone(),
    );
    let list = format!("models-{}", title.replace(' ', "-"));
    rsx! {
        SettingGroup { title: "{title}",
            SettingRow { label: "Model", hint: "The model they start with, as Claude Code's --model takes it; empty for Claude Code's own.",
                TextField {
                    list: "{list}",
                    value: "{options.model}",
                    placeholder: "Claude Code's own",
                    onchange: move |value: String| set(SessionOptions { model: value.trim().to_string(), ..a.clone() }),
                }
                datalist { id: "{list}",
                    for m in MODELS {
                        option { key: "{m}", value: "{m}" }
                    }
                }
            }
            SettingRow { label: "Permission mode", hint: "How they ask before they act.",
                select {
                    onchange: move |e| set(SessionOptions { permission_mode: e.value(), ..b.clone() }),
                    for (mode , name) in MODES {
                        option { key: "{mode}", value: "{mode}", selected: options.permission_mode == mode, "{name}" }
                    }
                }
            }
            SettingRow { label: "Tools denied", hint: "Tools they may not use, separated by commas.",
                TextField {
                    value: "{format_list(&options.disallowed_tools)}",
                    placeholder: "Edit, Write",
                    onchange: move |value: String| set(SessionOptions { disallowed_tools: parse_list(&value), ..c.clone() }),
                }
            }
            SettingRow { label: "Confirm Adroit's channel warning", hint: "Confirm, once a launch, Claude Code's warning about loading development channels when it names Adroit's own channel alone; every other dialog stays yours.",
                Toggle {
                    checked: options.confirm_channel_warning,
                    onchange: move |on| set(SessionOptions { confirm_channel_warning: on, ..d.clone() }),
                }
            }
        }
    }
}
