//! The About tab: logo, version, links, GitHub connection status and the license notice.

use crate::api;
use crate::state::Ui;
use crate::widgets::Loader;
use leptos::prelude::*;
use protocol::{AppInfo, Viewer};

const REPO: &str = "https://github.com/dfuchss/prism";
const LINKS: [(&str, &str); 4] = [
    ("Repository", REPO),
    ("Report an issue", "https://github.com/dfuchss/prism/issues"),
    ("Releases", "https://github.com/dfuchss/prism/releases"),
    (
        "License",
        "https://github.com/dfuchss/prism/blob/main/LICENSE",
    ),
];

const LICENSE: &str = "Copyright \u{a9} 2026 Dominik Fuchß. PRism is free software: you can \
redistribute it and/or modify it under the terms of the GNU General Public License as \
published by the Free Software Foundation, either version 3 of the License, or (at your \
option) any later version. PRism is distributed in the hope that it will be useful, but \
WITHOUT ANY WARRANTY; without even the implied warranty of MERCHANTABILITY or FITNESS FOR A \
PARTICULAR PURPOSE. See the GNU General Public License for more details.";

#[component]
pub(crate) fn About() -> impl IntoView {
    let ui = expect_context::<Ui>();
    let info = RwSignal::new(None::<AppInfo>);
    Loader::new().load(ui, api::app_info(), move |loaded| info.set(Some(loaded)));
    view! {
        <section class="page about">
            <div class="about-head">
                <img class="about-logo" src="prism.svg" alt=""/>
                <div>
                    <h2 class="about-name">"PRism"</h2>
                    <div class="muted">
                        {move || {
                            info.get().map_or_else(String::new, |i| format!("Version {}", i.version))
                        }}
                    </div>
                    <div class="about-tagline">"Pull Requests \u{2014} Inspect, Sort, Merge"</div>
                </div>
            </div>
            <p>
                "A desktop inbox for GitHub pull requests: it sorts them into buckets, shows CI and review state, and runs approve, merge and rebase actions in bulk or by rules."
            </p>
            <div class="links">
                {LINKS
                    .map(|(label, url)| {
                        view! { <button on:click=move |_| api::open_url(url)>{label}</button> }
                    })}
            </div>
            <h4>"Application"</h4>
            {move || info.get().map(|i| view! { <Facts info=i/> })}
            <h4>"License"</h4>
            <p class="license">{LICENSE}</p>
            <p class="muted">"Built with Tauri and Leptos."</p>
        </section>
    }
}

#[component]
fn Facts(info: AppInfo) -> impl IntoView {
    let github = match &info.viewer {
        Viewer::SignedIn(login) => format!("signed in as {login}"),
        Viewer::Unavailable(error) => format!("not connected: {error}"),
    };
    view! {
        <dl>
            <dt>"GitHub"</dt>
            <dd>{github}</dd>
            <dt>"Token source"</dt>
            <dd>{info.auth_source}</dd>
            <dt>"Config directory"</dt>
            <dd class="mono">{info.config_dir}</dd>
            <dt>"Platform"</dt>
            <dd>{info.platform}</dd>
            <dt>"Tauri"</dt>
            <dd>{info.tauri_version}</dd>
        </dl>
    }
}
