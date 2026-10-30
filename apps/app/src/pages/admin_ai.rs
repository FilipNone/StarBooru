//! The admin's AI panel: for each tool, off, StarBooru's own models or an
//! API. The server looks after the models; this only says what's wanted.

use std::future::Future;

use dioxus::prelude::*;
use starbooru_client::ClientError;
use starbooru_core::ai::MAX_AI_FIELD_CHARS;
use starbooru_core::{
    AiAdminStatus, AiGeneralUpdate, AiMode, AiModelOption, AiPreset, AiRuntimeStatus, AiTool,
    AiToolSettings, AiToolState, AiToolStatus, AiToolUpdate, Hardware,
};

use super::{ErrorBanner, Spinner};
use crate::store;
use crate::util::human_size;

/// How often the panel looks while a download runs.
const BUSY_POLL_MS: u32 = 1500;
const IDLE_POLL_MS: u32 = 15_000;

/// Waits in the browser and the webviews alike.
async fn wait_ms(ms: u32) {
    let _ = document::eval(&format!(
        "await new Promise((r) => setTimeout(r, {ms})); return true;"
    ))
    .join::<bool>()
    .await;
}

fn bytes(n: u64) -> String {
    human_size(i64::try_from(n).unwrap_or(i64::MAX))
}

/// An answer time: `0.8 s`, `12 s`, `2 min`.
fn duration(ms: u32) -> String {
    if ms < 10_000 {
        format!("{:.1} s", f64::from(ms) / 1000.0)
    } else if ms < 90_000 {
        format!("{} s", ms / 1000)
    } else {
        format!("{} min", ms.saturating_add(30_000) / 60_000)
    }
}

/// `≈` marks a time estimated from how fast this machine is.
fn model_time(m: &AiModelOption) -> String {
    let time = duration(m.est_ms);
    if m.measured {
        time
    } else {
        format!("≈ {time}")
    }
}

fn percent(done: u64, total: u64) -> u64 {
    done.saturating_mul(100) / total.max(1)
}

fn tool_label(tool: AiTool) -> String {
    match tool {
        AiTool::Tagging => t!("admin-ai-tagging"),
        AiTool::Translation => t!("admin-ai-translation"),
        AiTool::Comics => t!("admin-ai-comics"),
    }
}

fn mode_label(mode: AiMode) -> String {
    match mode {
        AiMode::Off => t!("admin-ai-mode-off"),
        AiMode::Local => t!("admin-ai-mode-local"),
        AiMode::Api => t!("admin-ai-mode-api"),
    }
}

fn preset_label(preset: AiPreset) -> String {
    match preset {
        AiPreset::Fast => t!("admin-ai-preset-fast"),
        AiPreset::Balanced => t!("admin-ai-preset-balanced"),
        AiPreset::Accurate => t!("admin-ai-preset-accurate"),
    }
}

/// One line about the machine: board or processor, threads, memory, GPUs.
fn hardware_line(hw: &Hardware) -> String {
    let mut parts = Vec::new();
    if let Some(name) = hw.board.as_ref().or(hw.cpu.as_ref()) {
        parts.push(name.clone());
    }
    parts.push(t!("admin-ai-hw-threads", count = hw.threads));
    if hw.ram_total > 0 {
        parts.push(t!("admin-ai-hw-memory", total = bytes(hw.ram_total)));
    }
    parts.extend(hw.gpus.iter().map(|gpu| gpu.name.clone()));
    parts.join(" · ")
}

/// A saved tool's setup, as the starting point of a change.
fn update_of(s: &AiToolSettings) -> AiToolUpdate {
    AiToolUpdate {
        mode: s.mode,
        preset: s.preset,
        model: s.model.clone(),
        api_url: s.api_url.clone(),
        api_model: s.api_model.clone(),
        api_key: None,
    }
}

/// The model a tool in local mode is meant to run: the admin's or the preset's.
/// The preset in effect: Balanced stands in for one this machine can't meet.
fn effective_preset(tool: &AiToolStatus) -> AiPreset {
    let met = tool
        .presets
        .iter()
        .any(|p| p.preset == tool.settings.preset && p.available);
    if met {
        tool.settings.preset
    } else {
        AiPreset::Balanced
    }
}

fn wanted_model(tool: &AiToolStatus) -> Option<&AiModelOption> {
    let id = if tool.settings.model.is_empty() {
        let preset = effective_preset(tool);
        tool.presets
            .iter()
            .find(|p| p.preset == preset)
            .and_then(|p| p.model.as_deref())
            .or(tool.active.as_deref())?
    } else {
        tool.settings.model.as_str()
    };
    tool.models.iter().find(|m| m.id == id)
}

/// Whether something in the panel is about to change by itself.
fn busy(s: &AiAdminStatus) -> bool {
    let starting = !s.runtime.ready && s.runtime.reason.is_none();
    let downloading = s
        .tools
        .iter()
        .any(|t| t.models.iter().any(|m| m.progress.is_some()));
    starting || downloading
}

type Status = Signal<Option<AiAdminStatus>>;
type Failure = Signal<Option<String>>;

/// Runs a change and shows what the server says afterwards.
fn apply(
    mut status: Status,
    mut error: Failure,
    change: impl Future<Output = Result<AiAdminStatus, ClientError>> + 'static,
) {
    spawn(async move {
        match change.await {
            Ok(s) => {
                error.set(None);
                status.set(Some(s));
            }
            Err(e) if e.is_unauthorized() => store::forget_session(),
            Err(e) => error.set(Some(e.to_string())),
        }
    });
}

#[component]
pub fn AiCard() -> Element {
    let mut status: Status = use_signal(|| None);
    let mut error: Failure = use_signal(|| None);
    use_future(move || async move {
        loop {
            if let Some(client) = store::client() {
                match client.admin_ai().await {
                    Ok(s) => status.set(Some(s)),
                    Err(e) if e.is_unauthorized() => return store::forget_session(),
                    // A later look may work; what's on screen stays.
                    Err(e) if status.peek().is_none() => error.set(Some(e.to_string())),
                    Err(_) => {}
                }
            }
            let wait = if status.peek().as_ref().is_none_or(busy) {
                BUSY_POLL_MS
            } else {
                IDLE_POLL_MS
            };
            wait_ms(wait).await;
        }
    });
    let current = status();
    rsx! {
        section { class: "card",
            h2 { class: "card-title", {t!("admin-ai-title")} }
            if let Some(message) = error() {
                ErrorBanner { message }
            }
            match current {
                None => rsx! {
                    if error().is_none() {
                        Spinner {}
                    }
                },
                Some(s) => rsx! {
                    p { class: "muted hint ai-machine", {hardware_line(&s.hardware)} }
                    if let Some(reason) = s.runtime.reason.clone() {
                        div { class: "banner warn", role: "status",
                            span { {t!("admin-ai-unavailable", reason = reason)} }
                        }
                    }
                    for tool in s.tools.iter().cloned() {
                        ToolBlock {
                            key: "{tool.tool.as_str()}",
                            tool,
                            runtime: s.runtime.clone(),
                            autotag: s.autotag,
                            auto_translate: s.auto_translate.clone(),
                            status,
                            error,
                        }
                    }
                },
            }
        }
    }
}

#[component]
fn ToolBlock(
    tool: AiToolStatus,
    runtime: AiRuntimeStatus,
    autotag: bool,
    auto_translate: String,
    status: Status,
    error: Failure,
) -> Element {
    let id = tool.tool;
    // The API's form opens before it can be saved: it needs an address first.
    let mut api_open = use_signal(|| false);
    let settings = tool.settings.clone();
    let shown_mode = if api_open() {
        AiMode::Api
    } else {
        settings.mode
    };
    let wanted = wanted_model(&tool).cloned();
    let progress = wanted.as_ref().and_then(|m| m.progress);
    rsx! {
        div { class: "ai-tool",
            div { class: "ai-tool-head",
                h3 { {tool_label(id)} }
                match tool.state {
                    AiToolState::Off => rsx! {},
                    AiToolState::Ready => rsx! {
                        span { class: "pill ok", {t!("admin-ai-state-ready")} }
                    },
                    AiToolState::Installing => rsx! {
                        span { class: "pill", role: "status",
                            match progress {
                                Some(p) => t!("admin-ai-state-downloading", percent = percent(p.done, p.total)),
                                None => t!("admin-ai-state-preparing"),
                            }
                        }
                    },
                    AiToolState::Failed => rsx! {
                        span { class: "pill bad", {t!("admin-ai-state-failed")} }
                    },
                }
            }
            div { class: "segmented", role: "radiogroup", aria_label: tool_label(id),
                for mode in [AiMode::Off, AiMode::Local, AiMode::Api] {
                    button {
                        key: "{mode.as_str()}",
                        r#type: "button",
                        role: "radio",
                        aria_checked: if shown_mode == mode { "true" } else { "false" },
                        class: if shown_mode == mode { "seg on" } else { "seg" },
                        disabled: mode == AiMode::Local && !runtime.ready,
                        onclick: {
                            let settings = settings.clone();
                            move |_| {
                                let needs_form = mode == AiMode::Api
                                    && (settings.api_url.is_empty() || settings.api_model.is_empty());
                                api_open.set(needs_form);
                                if needs_form {
                                    return;
                                }
                                let Some(client) = store::client() else {
                                    return;
                                };
                                let update = AiToolUpdate {
                                    mode,
                                    ..update_of(&settings)
                                };
                                apply(status, error, async move { client.set_ai_tool(id, &update).await });
                            }
                        },
                        {mode_label(mode)}
                    }
                }
            }
            match shown_mode {
                AiMode::Off => rsx! {},
                AiMode::Local => rsx! {
                    LocalSetup { tool: tool.clone(), status, error }
                },
                AiMode::Api => rsx! {
                    ApiSetup { tool: id, settings: settings.clone(), status, error }
                },
            }
            if shown_mode != AiMode::Off && id == AiTool::Tagging {
                label { class: "check",
                    input {
                        r#type: "checkbox",
                        checked: autotag,
                        onchange: {
                            let auto_translate = auto_translate.clone();
                            move |e: FormEvent| {
                                let Some(client) = store::client() else {
                                    return;
                                };
                                let update = AiGeneralUpdate {
                                    autotag: e.checked(),
                                    auto_translate: auto_translate.clone(),
                                };
                                apply(status, error, async move { client.set_ai_general(&update).await });
                            }
                        },
                    }
                    span { {t!("admin-ai-autotag")} }
                }
            }
            if settings.mode != AiMode::Off && id == AiTool::Comics {
                label { class: "field",
                    span { {t!("admin-ai-auto-translate")} }
                    input {
                        maxlength: "35",
                        placeholder: "en",
                        value: "{auto_translate}",
                        onchange: move |e: FormEvent| {
                            let Some(client) = store::client() else {
                                return;
                            };
                            let update = AiGeneralUpdate {
                                autotag,
                                auto_translate: e.value(),
                            };
                            apply(status, error, async move { client.set_ai_general(&update).await });
                        },
                    }
                    span { class: "muted field-hint", {t!("admin-ai-auto-translate-hint")} }
                }
            }
        }
    }
}

#[component]
fn LocalSetup(tool: AiToolStatus, status: Status, error: Failure) -> Element {
    let id = tool.tool;
    let settings = tool.settings.clone();
    let wanted = wanted_model(&tool).cloned();
    let preset = effective_preset(&tool);
    let unavailable: Vec<String> = tool
        .presets
        .iter()
        .filter(|p| !p.available)
        .map(|p| preset_label(p.preset))
        .collect();
    rsx! {
        div { class: "segmented", role: "radiogroup", aria_label: t!("admin-ai-speed"),
            for option in tool.presets.iter().cloned() {
                {
                    let on = settings.model.is_empty() && preset == option.preset;
                    let budget = option.preset.budget_ms() / 1000;
                    let settings = settings.clone();
                    rsx! {
                        button {
                            key: "{option.preset.as_str()}",
                            r#type: "button",
                            role: "radio",
                            aria_checked: if on { "true" } else { "false" },
                            class: if on { "seg on" } else { "seg" },
                            disabled: !option.available,
                            onclick: move |_| {
                                let Some(client) = store::client() else {
                                    return;
                                };
                                // A preset replaces a model chosen by hand.
                                let update = AiToolUpdate {
                                    preset: option.preset,
                                    model: String::new(),
                                    ..update_of(&settings)
                                };
                                apply(status, error, async move { client.set_ai_tool(id, &update).await });
                            },
                            {preset_label(option.preset)}
                            small { "{budget} s" }
                        }
                    }
                }
            }
        }
        if !unavailable.is_empty() {
            p { class: "muted ai-note", {t!("admin-ai-presets-off", presets = unavailable.join(", "))} }
        }
        if let Some(m) = wanted.clone() {
            p { class: "ai-note", "{m.name} · {model_time(&m)}" }
        }
        if let Some(reason) = tool.error.clone() {
            div { class: "banner error", role: "alert",
                span { "{reason}" }
                if let Some(m) = wanted.filter(|m| m.error.is_some()) {
                    button {
                        class: "btn text",
                        r#type: "button",
                        onclick: move |_| {
                            let Some(client) = store::client() else {
                                return;
                            };
                            let id = m.id.clone();
                            apply(status, error, async move { client.install_ai_model(&id).await });
                        },
                        {t!("admin-ai-retry")}
                    }
                }
            }
        }
        details { class: "ai-models",
            summary { {t!("admin-ai-models")} }
            for model in tool.models.iter().cloned() {
                ModelRow {
                    key: "{model.id}",
                    tool: id,
                    in_use: tool.active.as_deref() == Some(model.id.as_str()),
                    settings: settings.clone(),
                    model,
                    status,
                    error,
                }
            }
        }
    }
}

#[component]
fn ModelRow(
    tool: AiTool,
    model: AiModelOption,
    in_use: bool,
    settings: AiToolSettings,
    status: Status,
    error: Failure,
) -> Element {
    let details = format!(
        "{} · {}",
        bytes(model.size),
        if model.fits {
            model_time(&model)
        } else {
            t!("admin-ai-no-memory")
        }
    );
    let use_it = {
        let id = model.id.clone();
        move |_| {
            let Some(client) = store::client() else {
                return;
            };
            let update = AiToolUpdate {
                model: id.clone(),
                ..update_of(&settings)
            };
            apply(status, error, async move {
                client.set_ai_tool(tool, &update).await
            });
        }
    };
    let install = {
        let id = model.id.clone();
        move |_| {
            let Some(client) = store::client() else {
                return;
            };
            let id = id.clone();
            apply(
                status,
                error,
                async move { client.install_ai_model(&id).await },
            );
        }
    };
    let uninstall = {
        let id = model.id.clone();
        move |_| {
            let Some(client) = store::client() else {
                return;
            };
            let id = id.clone();
            apply(status, error, async move {
                client.uninstall_ai_model(&id).await
            });
        }
    };
    rsx! {
        div { class: "ai-model",
            div { class: "ai-model-text",
                strong { "{model.name}" }
                span { class: "muted", "{details}" }
                if model.high_accuracy {
                    span { class: "pill", {t!("admin-ai-high-accuracy")} }
                }
                if model.gpu {
                    span { class: "pill", {t!("admin-ai-gpu")} }
                }
                if let Some(reason) = model.error.clone() {
                    span { class: "ai-model-error", "{reason}" }
                }
            }
            div { class: "ai-model-actions",
                if let Some(p) = model.progress {
                    span { class: "meter ai-meter", role: "progressbar",
                        aria_valuenow: "{percent(p.done, p.total)}", aria_valuemin: "0", aria_valuemax: "100",
                        span { style: "width: {percent(p.done, p.total)}%" }
                    }
                    span { class: "muted", "{percent(p.done, p.total)}%" }
                } else if model.installed {
                    if in_use {
                        span { class: "pill ok", {t!("admin-ai-in-use")} }
                    } else {
                        button { class: "btn tonal", r#type: "button", onclick: use_it, {t!("admin-ai-use")} }
                        button { class: "btn text", r#type: "button", onclick: uninstall, {t!("admin-ai-uninstall")} }
                    }
                } else if model.fits {
                    button { class: "btn tonal", r#type: "button", onclick: install,
                        if model.error.is_some() { {t!("admin-ai-retry")} } else { {t!("admin-ai-install")} }
                    }
                }
            }
        }
    }
}

#[component]
fn ApiSetup(tool: AiTool, settings: AiToolSettings, status: Status, error: Failure) -> Element {
    let mut url = use_signal(|| settings.api_url.clone());
    let mut model = use_signal(|| settings.api_model.clone());
    let mut key = use_signal(String::new);
    let mut tested = use_signal(|| None::<String>);
    let mut testing = use_signal(|| false);
    let key_set = settings.api_key_set;
    let saved = settings.mode == AiMode::Api;
    let save = {
        let settings = settings.clone();
        move |e: FormEvent| {
            e.prevent_default();
            let Some(client) = store::client() else {
                return;
            };
            let typed = key().trim().to_owned();
            let update = AiToolUpdate {
                mode: AiMode::Api,
                api_url: url(),
                api_model: model(),
                api_key: (!typed.is_empty()).then_some(typed),
                ..update_of(&settings)
            };
            key.set(String::new());
            tested.set(None);
            apply(status, error, async move {
                client.set_ai_tool(tool, &update).await
            });
        }
    };
    let clear_key = {
        let settings = settings.clone();
        move |_| {
            let Some(client) = store::client() else {
                return;
            };
            let update = AiToolUpdate {
                api_key: Some(String::new()),
                ..update_of(&settings)
            };
            apply(status, error, async move {
                client.set_ai_tool(tool, &update).await
            });
        }
    };
    let test = move |_| {
        let Some(client) = store::client() else {
            return;
        };
        testing.set(true);
        tested.set(None);
        spawn(async move {
            match client.test_ai_tool(tool).await {
                Ok(r) => tested.set(Some(t!("admin-ai-test-ok", time = duration(r.ms)))),
                Err(e) => {
                    let mut error = error;
                    error.set(Some(e.to_string()));
                }
            }
            testing.set(false);
        });
    };
    rsx! {
        form { class: "ai-api", onsubmit: save,
            label { class: "field",
                span { {t!("admin-ai-url")} }
                input {
                    inputmode: "url",
                    autocomplete: "off",
                    placeholder: "https://api.openai.com/v1",
                    maxlength: "{MAX_AI_FIELD_CHARS}",
                    value: "{url}",
                    oninput: move |e| url.set(e.value()),
                }
            }
            label { class: "field",
                span { {t!("admin-ai-model")} }
                input {
                    autocomplete: "off",
                    maxlength: "{MAX_AI_FIELD_CHARS}",
                    value: "{model}",
                    oninput: move |e| model.set(e.value()),
                }
            }
            label { class: "field",
                span { {t!("admin-ai-key")} }
                input {
                    r#type: "password",
                    autocomplete: "off",
                    maxlength: "{MAX_AI_FIELD_CHARS}",
                    placeholder: if key_set { t!("admin-ai-key-set") } else { t!("admin-ai-key-none") },
                    value: "{key}",
                    oninput: move |e| key.set(e.value()),
                }
            }
            div { class: "ai-api-actions",
                button { class: "btn", r#type: "submit", {t!("save")} }
                if saved {
                    button { class: "btn tonal", r#type: "button", disabled: testing(), onclick: test,
                        if testing() { {t!("admin-ai-testing")} } else { {t!("admin-ai-test")} }
                    }
                }
                if key_set {
                    button { class: "btn text", r#type: "button", onclick: clear_key, {t!("admin-ai-key-clear")} }
                }
                if let Some(result) = tested() {
                    span { class: "muted", role: "status", "{result}" }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn answer_times_read_well() {
        assert_eq!(duration(816), "0.8 s");
        assert_eq!(duration(9_999), "10.0 s");
        assert_eq!(duration(16_928), "16 s");
        assert_eq!(duration(116_354), "2 min");
        // A corrupt timing doesn't overflow.
        assert_eq!(duration(u32::MAX), "71582 min");
    }

    #[test]
    fn progress_never_divides_by_zero() {
        assert_eq!(percent(0, 0), 0);
        assert_eq!(percent(5, 10), 50);
        assert_eq!(percent(u64::MAX, 10), u64::MAX / 10);
    }

    #[test]
    fn balanced_stands_in_for_a_preset_out_of_reach() {
        let option = |preset, available| starbooru_core::AiPresetOption {
            preset,
            available,
            model: available.then(|| preset.as_str().to_owned()),
            est_ms: None,
        };
        let mut tool = AiToolStatus {
            models: vec![
                AiModelOption {
                    id: "balanced".into(),
                    ..AiModelOption::default()
                },
                AiModelOption {
                    id: "accurate".into(),
                    ..AiModelOption::default()
                },
            ],
            presets: vec![
                option(AiPreset::Fast, false),
                option(AiPreset::Balanced, true),
                option(AiPreset::Accurate, true),
            ],
            ..AiToolStatus::default()
        };
        tool.settings.preset = AiPreset::Accurate;
        assert_eq!(effective_preset(&tool), AiPreset::Accurate);
        assert_eq!(wanted_model(&tool).unwrap().id, "accurate");
        tool.settings.preset = AiPreset::Fast;
        assert_eq!(effective_preset(&tool), AiPreset::Balanced);
        assert_eq!(wanted_model(&tool).unwrap().id, "balanced");
    }

    #[test]
    fn the_wanted_model_follows_the_hand_pick_then_the_preset() {
        let option = |id: &str| AiModelOption {
            id: id.into(),
            ..AiModelOption::default()
        };
        let mut tool = AiToolStatus {
            models: vec![option("a"), option("b")],
            presets: vec![starbooru_core::AiPresetOption {
                preset: AiPreset::Balanced,
                available: true,
                model: Some("a".into()),
                est_ms: Some(1),
            }],
            ..AiToolStatus::default()
        };
        assert_eq!(wanted_model(&tool).map(|m| m.id.as_str()), Some("a"));
        tool.settings.model = "b".into();
        assert_eq!(wanted_model(&tool).map(|m| m.id.as_str()), Some("b"));
        tool.settings.model = "gone".into();
        assert_eq!(wanted_model(&tool), None);
    }
}
