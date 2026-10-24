use super::*;
use adroit_model::config::{OpenCodeOptions, OpenCodeRule};
use adroit_model::profile::{AgentOptions, LoginStanding};

fn with(options: OpenCodeOptions) -> AgentContext {
    AgentContext {
        options: AgentOptions::OpenCode(options),
        ..AgentContext::builtin(Agent::OpenCode)
    }
}

/// What OpenCode is started with for `args`: its arguments, and the
/// configuration it is handed for the run, if any.
fn launched(context: &AgentContext, args: Vec<String>) -> (Vec<String>, Option<Value>) {
    let (_, args, env) = OpenCode.launch(context, args, Vec::new());
    let config = env
        .iter()
        .find(|(name, _)| name == CONFIG)
        .map(|(_, value)| serde_json::from_str(value).unwrap());
    (args, config)
}

#[test]
fn a_session_starts_opencode_s_interface_with_a_server_of_its_own_on_the_id_adroit_chose() {
    let context = AgentContext::builtin(Agent::OpenCode);
    let id = OpenCode.conversation();
    assert!(id.starts_with("ses_"), "{id}");
    let start = OpenCode.start_args(&context, "fix", &id, "read the code");
    let (args, config) = launched(&context, start);
    assert_eq!(
        args,
        [
            "--standalone",
            "--session",
            id.as_str(),
            "--prompt=read the code"
        ]
    );
    assert_eq!(config, None);
    let (_, _, env) = OpenCode.launch(
        &context,
        OpenCode.start_args(&context, "", &id, ""),
        Vec::new(),
    );
    assert!(env.contains(&(SESSION.to_string(), id.clone())), "{env:?}");
}

#[test]
fn a_first_prompt_that_starts_like_a_flag_is_still_the_prompt() {
    let context = AgentContext::builtin(Agent::OpenCode);
    let args = OpenCode.start_args(&context, "", "ses_x", "--help me");
    assert_eq!(args.last().map(String::as_str), Some("--prompt=--help me"));
    let args = OpenCode.start_args(&context, "", "ses_x", "");
    assert!(!args.iter().any(|a| a.starts_with("--prompt")), "{args:?}");
}

#[test]
fn a_profile_s_model_agent_and_rules_are_opencode_s_configuration_and_auto_its_flag() {
    let rule = OpenCodeRule {
        action: "shell".into(),
        resource: "git *".into(),
        effect: "allow".into(),
    };
    let context = with(OpenCodeOptions {
        model: " ollama/qwen3:8b ".into(),
        agent: "plan".into(),
        permissions: vec![rule],
        auto: true,
    });
    let (args, config) = launched(&context, OpenCode.start_args(&context, "", "ses_x", ""));
    assert_eq!(args, ["--standalone", "--auto", "--session", "ses_x"]);
    assert_eq!(
        config,
        Some(json!({
            "model": "ollama/qwen3:8b",
            "default_agent": "plan",
            "permissions": [{"action":"shell", "resource":"git *", "effect":"allow"}],
            "agents": {
                "plan": {"permissions": [{"action":"shell", "resource":"git *", "effect":"allow"}]},
                "build": {"permissions": [{"action":"shell", "resource":"git *", "effect":"allow"}]},
            },
        }))
    );
}

#[test]
fn a_conversation_resumes_by_its_id_run_as_a_new_session_would_be() {
    let context = with(OpenCodeOptions {
        model: "fake/slow".into(),
        ..OpenCodeOptions::default()
    });
    let (args, config) = launched(&context, OpenCode.resume_args(&context, "ses_old").unwrap());
    assert_eq!(args, ["--standalone", "--session", "ses_old"]);
    assert_eq!(config, Some(json!({"model": "fake/slow"})));
}

#[test]
fn every_fragment_of_configuration_is_merged_into_one_and_the_launch_s_own_kept_under_it() {
    let mut context = AgentContext::builtin(Agent::OpenCode);
    context.launch.env.insert(
        CONFIG.into(),
        r#"{"providers":{"ollama":{"settings":{"baseURL":"http://h/v1"}}},"permissions":[{"action":"*","resource":"*","effect":"ask"}],"model":"mine/m"}"#.into(),
    );
    let args = [
        config_args(json!({"plugins": ["/p"], "model": "ours/m"})),
        vec!["--standalone".into()],
        config_args(json!({"plugins": ["/q"], "permissions": [{"action":"adroit_*","resource":"*","effect":"allow"}]})),
        vec![FRAGMENT.into(), "not json".into()],
    ]
    .concat();
    let (args, config) = launched(&context, args);
    assert_eq!(args, ["--standalone"]);
    let rules = json!([
        {"action":"*","resource":"*","effect":"ask"},
        {"action":"adroit_*","resource":"*","effect":"allow"}
    ]);
    assert_eq!(
        config,
        Some(json!({
            "providers": {"ollama": {"settings": {"baseURL": "http://h/v1"}}},
            "permissions": rules,
            "agents": {"build": {"permissions": rules}},
            "model": "ours/m",
            "plugins": ["/p", "/q"],
        }))
    );
}

#[tokio::test]
async fn every_command_adroit_runs_opencode_with_runs_a_server_of_its_own() {
    let context = AgentContext::builtin(Agent::OpenCode);
    for args in [
        OpenCode.login_args(),
        OpenCode.status_args(&context),
        OpenCode.start_args(&context, "", "ses_x", "go"),
        OpenCode.resume_args(&context, "ses_x").unwrap(),
    ] {
        assert!(args.contains(&STANDALONE.to_string()), "{args:?}");
    }
    // What adroit asks of it on its own: its models, its conversations, and
    // one of them whole.
    let (dir, opencode) = crate::testing::program(
        "opencode",
        "printf '%s\\n' \"$*\" >> \"$0.asked\"\necho '{}'",
    );
    let mut context = context;
    context.launch.command = vec![opencode.display().to_string()];
    OpenCode
        .models_within(
            context.clone(),
            dir.path().into(),
            std::time::Duration::from_secs(10),
        )
        .await;
    OpenCode.past(&context, dir.path(), None);
    let session = adroit_model::Session {
        agent: Agent::OpenCode,
        session_id: "ses_x".into(),
        cwd: dir.path().into(),
        ..Default::default()
    };
    let mut reader = OpenCode.transcript(&context, dir.path(), &session).unwrap();
    let asked = opencode.with_extension("asked");
    let lines = || std::fs::read_to_string(&asked).unwrap_or_default();
    for _ in 0..3000 {
        reader.read();
        if lines().lines().count() == 3 {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    let lines = lines();
    let asked: Vec<&str> = lines.lines().collect();
    assert_eq!(asked.len(), 3, "{asked:?}");
    for line in asked {
        assert!(line.split(' ').any(|word| word == STANDALONE), "{line}");
    }
    assert_eq!(OpenCode.login_args(), ["auth", "login", "--standalone"]);
    assert_eq!(
        OpenCode.status_args(&context),
        ["auth", "list", "--standalone", "--format", "json"]
    );
}

#[test]
fn opencode_is_logged_in_once_it_lists_any_connection() {
    let standing = |code, out: &str| OpenCode.standing(Some(code), out);
    assert_eq!(standing(0, "[]\n"), LoginStanding::LoggedOut);
    assert_eq!(
        standing(
            0,
            r#"[{"id":"anthropic","name":"Anthropic","connections":[{"type":"credential","id":"cred_1","label":"me","method":"oauth"}]}]"#
        ),
        LoginStanding::LoggedIn
    );
    assert_eq!(
        standing(
            0,
            r#"[{"id":"openai","name":"OpenAI","connections":[{"type":"env","name":"OPENAI_API_KEY"}]}]"#
        ),
        LoginStanding::LoggedIn
    );
    assert_eq!(standing(1, ""), LoginStanding::Unknown);
    assert_eq!(
        standing(0, "No authenticated integrations"),
        LoginStanding::Unknown
    );
    assert_eq!(standing(127, ""), LoginStanding::Missing);
}

#[test]
fn an_account_of_its_own_keeps_every_root_of_opencode_s_in_its_home() {
    let set = OpenCode.connection_variables(Path::new("/w/oc"));
    assert_eq!(
        set,
        [
            ("XDG_CONFIG_HOME", "/w/oc/config"),
            ("XDG_DATA_HOME", "/w/oc/data"),
            ("XDG_STATE_HOME", "/w/oc/state"),
            ("XDG_CACHE_HOME", "/w/oc/cache"),
            ("TMPDIR", "/w/oc/tmp"),
            ("OPENCODE_DISABLE_CLAUDE_CODE", "1"),
        ]
        .map(|(n, v)| (n.to_string(), v.to_string()))
    );
    assert_eq!(OpenCode.kept_outside(), ROOT_NAMES);
}

#[test]
fn an_account_of_its_own_lets_through_only_what_locates_the_user_and_adroit_s_own() {
    for name in [
        "PATH",
        "HOME",
        "TERM",
        "LANG",
        "LC_ALL",
        "TERMINFO_DIRS",
        "VISUAL",
        "SSH_AUTH_SOCK",
        "HTTPS_PROXY",
        "ADROIT_DELEGATION_TOKEN",
        "ADROIT_USER_EDITOR",
        "XDG_RUNTIME_DIR",
    ] {
        assert!(!OpenCode.removes(name), "{name}");
    }
    for name in [
        "ANTHROPIC_API_KEY",
        "OPENAI_API_KEY",
        "GITHUB_TOKEN",
        "GITLAB_TOKEN",
        "AWS_PROFILE",
        "GOOGLE_APPLICATION_CREDENTIALS",
        "OLLAMA_API_KEY",
        "SOMEONE_NEW_API_KEY",
        "OPENCODE_CONFIG_CONTENT",
        "OPENCODE_AUTH_CONTENT",
        "OPENCODE_DB",
        "OPENCODE_PASSWORD",
    ] {
        assert!(OpenCode.removes(name), "{name}");
    }
    for name in [
        "OPENCODE_AUTH_CONTENT",
        "OPENCODE_CONFIG",
        "OPENCODE_CONFIG_DIR",
        "OPENCODE_DB",
        "OPENCODE_PERMISSION",
        "OPENCODE_PASSWORD",
        "OPENCODE_SERVER_PASSWORD",
        "OPENCODE_SERVER_USERNAME",
    ] {
        assert!(OpenCode.denies(name), "{name}");
    }
    // What a profile's launch may set for its sessions, the account's own
    // configuration added to adroit's among them.
    for name in ["OPENCODE_CONFIG_CONTENT", "OLLAMA_API_KEY", "JAVA_HOME"] {
        assert!(!OpenCode.denies(name), "{name}");
    }
}

#[test]
fn a_release_older_than_the_second_line_is_refused_saying_how_to_change_it() {
    let context = AgentContext::builtin(Agent::OpenCode);
    for out in [
        "1.19.4\n",
        "opencode v2.0.32\n",
        "\x1b[1mopencode\x1b[0m \x1b[2mv\x1b[0m\x1b[1m1.3.0\x1b[0m",
    ] {
        let told = refused(Some(0), out, &context).unwrap_or_else(|| panic!("{out:?}"));
        assert!(told.contains("profile `opencode`"), "{told}");
        assert!(told.contains("2.1 or later"), "{told}");
        assert!(told.contains("npm i -g @opencode/cli"), "{told}");
    }
    let told = refused(Some(0), "1.19.4", &context).unwrap();
    assert!(told.contains("OpenCode 1.19.4"), "{told}");
    for out in ["opencode v2.1.4\n", "opencode v2.10.0", "3.0.0"] {
        assert_eq!(refused(Some(0), out, &context), None, "{out}");
    }
    // A program that tells nothing adroit can read is left to start.
    assert_eq!(refused(Some(0), "opencode", &context), None);
    assert_eq!(refused(Some(1), "1.0.0", &context), None);
}

#[tokio::test]
async fn the_version_is_asked_as_the_session_would_start_opencode() {
    let (dir, opencode) =
        crate::testing::program("opencode", "printf '%s' \"$*\" > \"$0.args\"\necho 1.19.4");
    let mut context = AgentContext::builtin(Agent::OpenCode);
    context.launch.command = vec![opencode.display().to_string()];
    let told = OpenCode.refusal(context.clone(), dir.path().into()).await;
    assert!(told.is_some_and(|t| t.contains("1.19.4")));
    assert_eq!(
        std::fs::read_to_string(opencode.with_extension("args")).unwrap(),
        "--version"
    );
    context.launch.command = vec![dir.path().join("missing").display().to_string()];
    assert_eq!(OpenCode.refusal(context, dir.path().into()).await, None);
}

#[tokio::test]
async fn the_models_are_those_opencode_lists_as_provider_and_model() {
    let (dir, opencode) = crate::testing::program(
        "opencode",
        "[ \"$*\" = 'models --standalone' ] || exit 1\nprintf 'opencode/big-pickle\\nollama/qwen3:8b\\n\\nnot-a-model\\n'",
    );
    let mut context = AgentContext::builtin(Agent::OpenCode);
    context.launch.command = vec![opencode.display().to_string()];
    let models = OpenCode
        .models_within(
            context,
            dir.path().into(),
            std::time::Duration::from_secs(10),
        )
        .await
        .unwrap();
    assert_eq!(
        models.iter().map(|m| m.id.as_str()).collect::<Vec<_>>(),
        ["opencode/big-pickle", "ollama/qwen3:8b"]
    );
}

#[test]
fn adroit_s_instructions_and_plugin_are_configuration_for_the_run() {
    let dir = tempfile::tempdir().unwrap();
    let args = OpenCode.integration_args(Path::new("/opt/adroit"), dir.path());
    let (_, config) = fragments(args);
    let plugin = dir.path().join("opencode/plugin");
    assert_eq!(config["plugins"], json!([plugin]));
    assert_eq!(
        config["skills"],
        json!([dir.path().join("opencode/skills")])
    );
    assert!(plugin.join("index.js").is_file());
    let path = plugin::instructions(dir.path(), "be brief").unwrap();
    assert_eq!(std::fs::read_to_string(path).unwrap(), "be brief");
}

#[test]
fn images_are_named_by_their_quoted_paths_in_the_first_prompt_and_pasted_after() {
    let images = [
        PathBuf::from("/tmp/my shot.png"),
        PathBuf::from("/tmp/b.png"),
    ];
    assert_eq!(
        OpenCode.image_prompt("look", &images),
        "look \"/tmp/my shot.png\" \"/tmp/b.png\""
    );
    assert_eq!(OpenCode.image_prompt("", &images[1..]), "\"/tmp/b.png\"");
    assert_eq!(OpenCode.image_prompt("look", &[]), "look");
    assert_eq!(
        OpenCode.image_pastes(&images),
        ["\"/tmp/my shot.png\"", "\"/tmp/b.png\""]
    );
    assert!(OpenCode.image_args(&images).is_empty());
}

#[test]
fn opencode_offers_no_account_usage_and_runs_shell_lines_typed_after_a_bang() {
    let capabilities = OpenCode.capabilities();
    assert!(!capabilities.usage_limits);
    assert!(capabilities.shell_bang);
    assert!(capabilities.transcript);
    assert!(capabilities.screen_state);
    assert_eq!(OpenCode.usage_url(), None);
    assert_eq!(OpenCode.rename_command("fix"), None);
}

/// Whether OpenCode lets `agent` do `action` to `resource`, its rules taken
/// from `documents` in the order it loads them as it takes them: every
/// document's rules for all agents, then every document's for `agent`, the
/// last that matches deciding.
fn decided<'a>(documents: &'a [Value], agent: &str, action: &str, resource: &str) -> &'a str {
    let all = documents.iter().map(|d| &d["permissions"]);
    let own = documents.iter().map(|d| &d["agents"][agent]["permissions"]);
    all.chain(own)
        .flat_map(|rules| rules.as_array().into_iter().flatten())
        .rfind(|rule| {
            matched(rule["action"].as_str().unwrap(), action)
                && matched(rule["resource"].as_str().unwrap(), resource)
        })
        .map_or("ask", |rule| rule["effect"].as_str().unwrap())
}

/// Whether `pattern`, where `*` stands for anything, matches `text`.
fn matched(pattern: &str, text: &str) -> bool {
    match pattern.split_once('*') {
        None => pattern == text,
        Some((head, rest)) => {
            text.starts_with(head)
                && (0..=text.len() - head.len()).any(|at| matched(rest, &text[head.len() + at..]))
        }
    }
}

/// A document of a repository's or an account's own that gives every agent
/// and `agent` leave for everything.
fn allowing_everything(agent: &str) -> Value {
    let all = json!([{"action":"*", "resource":"*", "effect":"allow"}]);
    json!({"permissions": all, "agents": {agent: {"permissions": all}}})
}

#[test]
fn a_read_only_session_may_read_but_change_no_file_and_run_no_command_whatever_else_allows_it() {
    let mut context = with(OpenCodeOptions {
        agent: "build".into(),
        auto: true,
        permissions: vec![OpenCodeRule {
            action: "shell".into(),
            resource: "*".into(),
            effect: "allow".into(),
        }],
        ..OpenCodeOptions::default()
    });
    read_only(&mut context);
    let start = [
        OpenCode.start_args(&context, "", "ses_x", ""),
        config_args(json!({"permissions": [{"action":TOOLS, "resource":"*", "effect":"allow"}]})),
    ]
    .concat();
    let (_, args, env) = OpenCode.launch(&context, start, Vec::new());
    assert!(!args.contains(&"--auto".to_string()), "{args:?}");
    // No configuration of the repository's is read.
    assert!(env.contains(&(NO_PROJECT.into(), "1".into())), "{env:?}");
    let ours: Value = env
        .iter()
        .find(|(name, _)| name == CONFIG)
        .map(|(_, value)| serde_json::from_str(value).unwrap())
        .unwrap();
    assert_eq!(ours["default_agent"], "plan");
    // Were a repository's read, or as the account's own says, ours load last;
    // started with a prompt, OpenCode runs it as its own default agent.
    for agent in ["plan", DEFAULT_AGENT] {
        let documents = [allowing_everything(agent), ours.clone()];
        for (action, resource, effect) in [
            ("shell", "rm -rf /", "deny"),
            ("edit", "src/main.rs", "deny"),
            ("subagent", "general", "deny"),
            ("read", "src/main.rs", "allow"),
            ("grep", "*", "allow"),
            ("external_directory", "/elsewhere/*", "allow"),
            ("adroit_send", "*", "allow"),
        ] {
            assert_eq!(
                decided(&documents, agent, action, resource),
                effect,
                "{agent}: {action} {resource}"
            );
        }
    }
}

#[test]
fn adroit_s_tools_and_the_profile_s_rules_beat_any_document_s_rules_for_the_agent_run() {
    for (agent, run) in [("", DEFAULT_AGENT), ("review", "review")] {
        let context = with(OpenCodeOptions {
            agent: agent.into(),
            permissions: vec![
                OpenCodeRule {
                    action: "*".into(),
                    resource: "*".into(),
                    effect: "ask".into(),
                },
                OpenCodeRule {
                    action: "shell".into(),
                    resource: "git *".into(),
                    effect: "allow".into(),
                },
            ],
            ..OpenCodeOptions::default()
        });
        let args = [
            OpenCode.start_args(&context, "", "ses_x", ""),
            config_args(
                json!({"permissions": [{"action":TOOLS, "resource":"*", "effect":"allow"}]}),
            ),
        ]
        .concat();
        let (_, ours) = launched(&context, args);
        for run in [run, DEFAULT_AGENT] {
            let denying = json!({"agents": {run: {"permissions": [
                {"action":"*", "resource":"*", "effect":"deny"},
            ]}}});
            let documents = [denying, ours.clone().unwrap()];
            assert_eq!(decided(&documents, run, "adroit_send", "*"), "allow");
            assert_eq!(decided(&documents, run, "shell", "git status"), "allow");
            assert_eq!(decided(&documents, run, "edit", "a.rs"), "ask");
        }
    }
}
