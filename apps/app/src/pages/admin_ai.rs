//! The admin's AI settings: where each tool runs (off, a hosted provider or
//! local AI), the hosted provider, and the local runtime with a check of how
//! well each model runs on the server.

use dioxus::prelude::*;
use starbooru_core::ai::MAX_AI_FIELD_CHARS;
use starbooru_core::local_ai::{MAX_MODEL_CHARS, TIERS};
use starbooru_core::{
    AiRoute, AiSettings, AiSettingsUpdate, Fit, FitCheck, Hardware, LocalAiSettings, LocalAiStatus,
    LocalTestResult, LocalTier, Processor,
};

use super::{ErrorBanner, Spinner};
use crate::icons::{Ico, Icon};
use crate::store;
use crate::util::human_size;

/// How often a running download is checked.
const POLL_MS: u32 = 1500;

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

fn route_label(route: AiRoute) -> String {
    match route {
        AiRoute::Off => t!("admin-ai-route-off"),
        AiRoute::Hosted => t!("admin-ai-route-hosted"),
        AiRoute::Local => t!("admin-ai-route-local"),
    }
}

fn tier_label(tier: LocalTier) -> String {
    match tier {
        LocalTier::Small => t!("admin-ai-tier-small"),
        LocalTier::Medium => t!("admin-ai-tier-medium"),
        LocalTier::Large => t!("admin-ai-tier-large"),
    }
}

fn tier_hint(tier: LocalTier) -> String {
    match tier {
        LocalTier::Small => t!("admin-ai-tier-small-hint"),
        LocalTier::Medium => t!("admin-ai-tier-medium-hint"),
        LocalTier::Large => t!("admin-ai-tier-large-hint"),
    }
}

/// The verdict pill's class and text.
fn fit_pill(fit: Fit) -> (&'static str, String) {
    match fit {
        Fit::Good => ("pill ok", t!("admin-ai-fit-good")),
        Fit::Slow => ("pill", t!("admin-ai-fit-slow")),
        Fit::Poor => ("pill bad", t!("admin-ai-fit-poor")),
        Fit::NoFit => ("pill bad", t!("admin-ai-fit-nofit")),
        Fit::Unknown => ("pill", t!("admin-ai-fit-unknown")),
    }
}

/// Where the model runs and how much memory it needs.
fn fit_detail(check: &FitCheck) -> String {
    let need = bytes(check.need_bytes);
    match check.runs_on {
        Processor::Gpu => t!("admin-ai-runs-gpu", need = need),
        Processor::PartlyGpu => t!("admin-ai-runs-partly-gpu", need = need),
        Processor::SharedGpu => t!("admin-ai-runs-shared-gpu", need = need),
        Processor::Cpu => t!("admin-ai-runs-cpu", need = need),
        Processor::None => t!("admin-ai-runs-none", need = need),
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
        parts.push(t!(
            "admin-ai-hw-memory",
            total = bytes(hw.ram_total),
            free = bytes(hw.ram_available)
        ));
    }
    for gpu in &hw.gpus {
        parts.push(match gpu.vram_bytes {
            Some(v) => format!("{} {}", gpu.name, bytes(v)),
            None => match gpu.kind {
                starbooru_core::GpuKind::Integrated => {
                    t!("admin-ai-hw-igpu", name = gpu.name.clone())
                }
                starbooru_core::GpuKind::Dedicated => gpu.name.clone(),
            },
        });
    }
    if hw.gpus.is_empty() {
        parts.push(t!("admin-ai-hw-no-gpu"));
    }
    parts.join(" · ")
}

/// Off / hosted / local for one tool.
#[component]
fn RouteField(label: String, hint: String, route: Signal<AiRoute>) -> Element {
    rsx! {
        div { class: "field",
            span { "{label}" }
            div { class: "segmented", role: "radiogroup", aria_label: "{label}",
                for r in [AiRoute::Off, AiRoute::Hosted, AiRoute::Local] {
                    button {
                        key: "{r.as_str()}",
                        r#type: "button",
                        role: "radio",
                        aria_checked: if route() == r { "true" } else { "false" },
                        class: if route() == r { "seg on" } else { "seg" },
                        onclick: move |_| route.set(r),
                        {route_label(r)}
                    }
                }
            }
            span { class: "muted field-hint", "{hint}" }
        }
    }
}

#[component]
pub fn AiCard() -> Element {
    let mut url = use_signal(String::new);
    let mut model = use_signal(String::new);
    let mut key = use_signal(String::new);
    let mut key_set = use_signal(|| false);
    let mut clear_key = use_signal(|| false);
    let mut autotag = use_signal(|| false);
    let mut local_on = use_signal(|| false);
    let mut local_url = use_signal(String::new);
    let mut local_model = use_signal(String::new);
    let mut tagging = use_signal(|| AiRoute::Off);
    let mut translation = use_signal(|| AiRoute::Off);
    let mut auto_translate = use_signal(String::new);
    let mut loaded = use_signal(|| false);
    let mut busy = use_signal(|| false);
    let mut error = use_signal(|| None::<String>);
    let mut saved = use_signal(|| false);
    // Bumped after each save so the local panel reads the runtime again.
    let mut generation = use_signal(|| 0u32);
    let mut fill = move |s: AiSettings| {
        url.set(s.url);
        model.set(s.model);
        key.set(String::new());
        key_set.set(s.key_set);
        clear_key.set(false);
        autotag.set(s.autotag);
        local_on.set(s.local.enabled);
        local_url.set(s.local.url);
        local_model.set(s.local.model);
        tagging.set(s.tagging);
        translation.set(s.translation);
        auto_translate.set(s.auto_translate);
        loaded.set(true);
    };
    use_hook(move || {
        let Some(client) = store::client() else {
            return;
        };
        spawn(async move {
            match client.admin_ai().await {
                Ok(s) => fill(s),
                Err(e) if e.is_unauthorized() => store::forget_session(),
                Err(e) => error.set(Some(e.to_string())),
            }
        });
    });
    let save = move |e: FormEvent| {
        e.prevent_default();
        let Some(client) = store::client() else {
            return;
        };
        let typed = key.peek().trim().to_owned();
        let update = AiSettingsUpdate {
            url: url.peek().clone(),
            model: model.peek().clone(),
            key: if *clear_key.peek() {
                Some(String::new())
            } else if typed.is_empty() {
                None
            } else {
                Some(typed)
            },
            autotag: *autotag.peek(),
            local: LocalAiSettings {
                enabled: *local_on.peek(),
                url: local_url.peek().clone(),
                model: local_model.peek().clone(),
            },
            tagging: *tagging.peek(),
            translation: *translation.peek(),
            // Translation off turns automatic translation off with it.
            auto_translate: if *translation.peek() == AiRoute::Off {
                String::new()
            } else {
                auto_translate.peek().trim().to_owned()
            },
        };
        busy.set(true);
        error.set(None);
        saved.set(false);
        spawn(async move {
            match client.set_admin_ai(&update).await {
                Ok(s) => {
                    fill(s);
                    saved.set(true);
                    generation += 1;
                }
                Err(e) => error.set(Some(e.to_string())),
            }
            busy.set(false);
        });
    };
    rsx! {
        section { class: "card",
            h2 { class: "card-title", {t!("admin-ai-title")} }
            p { class: "muted hint", {t!("admin-ai-lead")} }
            if let Some(message) = error() {
                ErrorBanner { message }
            }
            if !loaded() {
                if error.read().is_none() {
                    Spinner {}
                }
            } else {
                form { onsubmit: save, oninput: move |_| saved.set(false),
                    RouteField {
                        label: t!("admin-ai-tagging"),
                        hint: t!("admin-ai-tagging-hint"),
                        route: tagging,
                    }
                    if tagging() != AiRoute::Off {
                        div { class: "field",
                            label { class: "check",
                                input {
                                    r#type: "checkbox",
                                    checked: autotag(),
                                    onchange: move |e| autotag.set(e.checked()),
                                }
                                span { {t!("admin-ai-autotag")} }
                            }
                            span { class: "muted field-hint", {t!("admin-ai-autotag-hint")} }
                        }
                    }
                    RouteField {
                        label: t!("admin-ai-translation"),
                        hint: t!("admin-ai-translation-hint"),
                        route: translation,
                    }
                    if translation() != AiRoute::Off {
                        label { class: "field",
                            span { {t!("admin-ai-auto-translate")} }
                            input {
                                placeholder: "en, it, pt-BR",
                                maxlength: "35",
                                autocapitalize: "off",
                                spellcheck: "false",
                                value: "{auto_translate}",
                                oninput: move |e| auto_translate.set(e.value()),
                            }
                            span { class: "muted field-hint", {t!("admin-ai-auto-translate-hint")} }
                        }
                    }

                    h3 { class: "ai-section", {t!("admin-ai-hosted")} }
                    p { class: "muted hint", {t!("admin-ai-hosted-lead")} }
                    label { class: "field",
                        span { {t!("admin-ai-url")} }
                        input {
                            r#type: "url",
                            placeholder: "https://openrouter.ai/api/v1",
                            maxlength: "{MAX_AI_FIELD_CHARS}",
                            value: "{url}",
                            oninput: move |e| url.set(e.value()),
                        }
                        span { class: "muted field-hint", {t!("admin-ai-url-hint")} }
                    }
                    label { class: "field",
                        span { {t!("admin-ai-model")} }
                        input {
                            placeholder: "google/gemini-2.5-flash-lite",
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
                            disabled: clear_key(),
                            placeholder: if key_set() { t!("admin-ai-key-set") } else { t!("admin-ai-key-none") },
                            value: "{key}",
                            oninput: move |e| key.set(e.value()),
                        }
                        span { class: "muted field-hint", {t!("admin-ai-key-hint")} }
                    }
                    if key_set() {
                        div { class: "field",
                            label { class: "check",
                                input {
                                    r#type: "checkbox",
                                    checked: clear_key(),
                                    onchange: move |e| clear_key.set(e.checked()),
                                }
                                span { {t!("admin-ai-key-clear")} }
                            }
                        }
                    }

                    h3 { class: "ai-section", {t!("admin-ai-local")} }
                    p { class: "muted hint", {t!("admin-ai-local-lead")} }
                    div { class: "field",
                        label { class: "check",
                            input {
                                r#type: "checkbox",
                                checked: local_on(),
                                onchange: move |e| local_on.set(e.checked()),
                            }
                            span { {t!("admin-ai-local-on")} }
                        }
                    }
                    if local_on() {
                        label { class: "field",
                            span { {t!("admin-ai-local-url")} }
                            input {
                                r#type: "url",
                                placeholder: "http://ollama:11434",
                                maxlength: "{MAX_AI_FIELD_CHARS}",
                                value: "{local_url}",
                                oninput: move |e| local_url.set(e.value()),
                            }
                            span { class: "muted field-hint", {t!("admin-ai-local-url-hint")} }
                        }
                        LocalPanel { model: local_model, generation: generation() }
                    }
                    div { class: "row-end",
                        if saved() {
                            span { class: "muted saved-note", role: "status",
                                Ico { icon: Icon::Check, size: 18 }
                                {t!("admin-saved")}
                            }
                        }
                        button { class: "btn", r#type: "submit", disabled: busy(),
                            {t!("admin-save")}
                        }
                    }
                }
            }
        }
    }
}

/// The machine, the presets with how each would run, the runtime's own
/// models, downloads and the speed test. `model` is the form's local model.
#[component]
fn LocalPanel(model: Signal<String>, generation: u32) -> Element {
    let mut status = use_signal(|| None::<LocalAiStatus>);
    let mut error = use_signal(|| None::<String>);
    let mut test = use_signal(|| None::<LocalTestResult>);
    let mut testing = use_signal(|| false);
    let mut custom = use_signal(|| false);
    let load = move || async move {
        let Some(client) = store::client() else {
            return;
        };
        match client.local_ai().await {
            Ok(s) => {
                status.set(Some(s));
                error.set(None);
            }
            Err(e) => error.set(Some(e.to_string())),
        }
    };
    // Load on mount and after each save; keep polling while a download runs.
    use_effect(use_reactive!(|generation| {
        let _ = generation;
        spawn(async move {
            load().await;
            while status
                .peek()
                .as_ref()
                .and_then(|s| s.pull.as_ref())
                .is_some_and(|p| !p.done)
            {
                wait_ms(POLL_MS).await;
                load().await;
            }
        });
    }));
    let pull = move |name: String| {
        let Some(client) = store::client() else {
            return;
        };
        spawn(async move {
            match client.pull_local_model(&name).await {
                Ok(s) => status.set(Some(s)),
                Err(e) => {
                    error.set(Some(e.to_string()));
                    return;
                }
            }
            while status
                .peek()
                .as_ref()
                .and_then(|s| s.pull.as_ref())
                .is_some_and(|p| !p.done)
            {
                wait_ms(POLL_MS).await;
                load().await;
            }
        });
    };
    let run_test = move |_| {
        let Some(client) = store::client() else {
            return;
        };
        testing.set(true);
        test.set(None);
        error.set(None);
        spawn(async move {
            match client.test_local_ai().await {
                Ok(r) => test.set(Some(r)),
                Err(e) => error.set(Some(e.to_string())),
            }
            testing.set(false);
        });
    };

    let Some(s) = status() else {
        return rsx! {
            if let Some(message) = error() {
                ErrorBanner { message }
            } else {
                Spinner {}
            }
        };
    };
    // An empty model means the recommended preset.
    let chosen = {
        let m = model();
        if m.is_empty() {
            s.recommended
                .map(|t| t.preset().model.to_owned())
                .unwrap_or_default()
        } else {
            m
        }
    };
    let is_preset = TIERS.iter().any(|p| p.model == chosen);
    let show_custom = custom() || !is_preset;
    let installed = |name: &str| {
        s.runtime.as_ref().is_some_and(|r| {
            r.models
                .iter()
                .any(|m| m.name == name || m.name == format!("{name}:latest"))
        })
    };
    let chosen_installed = installed(&chosen);
    let pulling = s.pull.as_ref().filter(|p| !p.done).cloned();
    let pull_failed = s
        .pull
        .as_ref()
        .filter(|p| p.done)
        .and_then(|p| p.error.clone().map(|e| (p.model.clone(), e)));
    let others: Vec<_> = s
        .runtime
        .as_ref()
        .map(|r| {
            r.models
                .iter()
                .filter(|m| !TIERS.iter().any(|p| p.model == m.name))
                .cloned()
                .collect()
        })
        .unwrap_or_default();

    rsx! {
        div { class: "local-ai",
            p { class: "local-hw",
                strong { {t!("admin-ai-hw")} }
                " {hardware_line(&s.hardware)}"
            }
            match (&s.runtime, &s.runtime_error) {
                (Some(r), _) => rsx! {
                    p { class: "muted hint", {t!("admin-ai-runtime-ok", version = r.version.clone())} }
                },
                (None, Some(e)) => rsx! {
                    div { class: "banner error", role: "alert",
                        span { {t!("admin-ai-runtime-down", error = e.clone())} }
                    }
                },
                (None, None) => rsx! {},
            }
            if let Some(message) = error() {
                ErrorBanner { message }
            }

            div { class: "tiers", role: "radiogroup", aria_label: t!("admin-ai-local-model"),
                for tc in s.tiers.iter().cloned() {
                    {
                        let on = !show_custom && chosen == tc.model;
                        let (pill_class, pill_text) = fit_pill(tc.check.fit);
                        let name = tc.model.clone();
                        let pick = tc.model.clone();
                        let recommended = s.recommended == Some(tc.tier);
                        rsx! {
                            div {
                                key: "{tc.model}",
                                class: if on { "tier on" } else { "tier" },
                                role: "radio",
                                tabindex: "0",
                                aria_checked: if on { "true" } else { "false" },
                                onclick: move |_| {
                                    custom.set(false);
                                    model.set(pick.clone());
                                },
                                div { class: "tier-head",
                                    strong { {tier_label(tc.tier)} }
                                    if recommended {
                                        span { class: "pill", {t!("admin-ai-recommended")} }
                                    }
                                    span { class: "{pill_class}", "{pill_text}" }
                                }
                                span { class: "muted", {tier_hint(tc.tier)} }
                                span { class: "muted tier-model",
                                    code { "{tc.model}" }
                                    " · {bytes(tc.file_bytes)} · {fit_detail(&tc.check)}"
                                }
                                if tc.installed {
                                    span { class: "tier-installed",
                                        Ico { icon: Icon::Check, size: 16 }
                                        {t!("admin-ai-installed")}
                                    }
                                } else if s.runtime.is_some() {
                                    button {
                                        class: "btn text",
                                        r#type: "button",
                                        disabled: pulling.is_some(),
                                        onclick: move |e| {
                                            e.stop_propagation();
                                            pull(name.clone());
                                        },
                                        Ico { icon: Icon::Download, size: 18 }
                                        {t!("admin-ai-download")}
                                    }
                                }
                            }
                        }
                    }
                }
                div {
                    class: if show_custom { "tier on" } else { "tier" },
                    role: "radio",
                    tabindex: "0",
                    aria_checked: if show_custom { "true" } else { "false" },
                    onclick: move |_| custom.set(true),
                    div { class: "tier-head",
                        strong { {t!("admin-ai-tier-custom")} }
                    }
                    span { class: "muted", {t!("admin-ai-tier-custom-hint")} }
                    if show_custom {
                        input {
                            class: "tier-input",
                            placeholder: "gemma3:4b",
                            maxlength: "{MAX_MODEL_CHARS}",
                            aria_label: t!("admin-ai-local-model"),
                            value: if is_preset { "" } else { "{chosen}" },
                            oninput: move |e| model.set(e.value()),
                        }
                        for m in others.iter() {
                            {
                                let (pill_class, pill_text) = fit_pill(m.check.fit);
                                let pick = m.name.clone();
                                rsx! {
                                    button {
                                        key: "{m.name}",
                                        class: "chip",
                                        r#type: "button",
                                        onclick: move |_| model.set(pick.clone()),
                                        "{m.name} · {bytes(m.file_bytes)} "
                                        span { class: "{pill_class}", "{pill_text}" }
                                    }
                                }
                            }
                        }
                        if !chosen.is_empty() && !chosen_installed && s.runtime.is_some() {
                            {
                                let name = chosen.clone();
                                rsx! {
                                    button {
                                        class: "btn text",
                                        r#type: "button",
                                        disabled: pulling.is_some(),
                                        onclick: move |_| pull(name.clone()),
                                        Ico { icon: Icon::Download, size: 18 }
                                        {t!("admin-ai-download")}
                                    }
                                }
                            }
                        }
                    }
                }
            }

            if let Some(p) = pulling {
                div { class: "pull", role: "status",
                    span { {t!("admin-ai-downloading", model = p.model.clone(), status = p.status.clone())} }
                    if p.total > 0 {
                        span { class: "meter",
                            span { style: "width: {p.completed.saturating_mul(100) / p.total.max(1)}%" }
                        }
                        span { class: "muted", "{bytes(p.completed)} / {bytes(p.total)}" }
                    }
                }
            }
            if let Some((name, e)) = pull_failed {
                div { class: "banner error", role: "alert",
                    span { {t!("admin-ai-download-failed", model = name, error = e)} }
                }
            }
            if s.runtime.is_some() && !chosen_installed && pulling_is_none(&s) {
                p { class: "muted hint", {t!("admin-ai-not-downloaded")} }
            }

            div { class: "local-test",
                button {
                    class: "btn tonal",
                    r#type: "button",
                    disabled: testing() || s.runtime.is_none() || !chosen_installed,
                    onclick: run_test,
                    if testing() { {t!("admin-ai-testing")} } else { {t!("admin-ai-test")} }
                }
                if let Some(r) = test() {
                    {
                        let (pill_class, pill_text) = fit_pill(r.fit);
                        let seconds = format!("{:.1}", r.seconds);
                        rsx! {
                            span { class: "{pill_class}", "{pill_text}" }
                            span { class: "muted",
                                match r.gpu_percent {
                                    Some(gpu) => t!("admin-ai-test-result-gpu", seconds = seconds, gpu = gpu),
                                    None => t!("admin-ai-test-result", seconds = seconds),
                                }
                            }
                        }
                    }
                }
            }
            p { class: "muted field-hint", {t!("admin-ai-test-hint")} }
        }
    }
}

fn pulling_is_none(s: &LocalAiStatus) -> bool {
    s.pull.as_ref().is_none_or(|p| p.done)
}
