//! Tauri backend of Prism: commands, background poller, config and audit log.

mod commands;
mod config;
mod demo;
mod events;
mod exec;
mod menu;
mod overrides;
mod refresh;
mod scope;
mod settings;
mod source;
mod state;

use state::AppState;
use tauri::Manager;

/// Builds and runs the Tauri application.
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            let state = if demo::enabled() {
                demo::setup_window(app)?;
                let demo = source::Source::Demo(demo::DemoSource::new());
                AppState::with_source(demo::config_dir()?, demo)?
            } else {
                AppState::load(app.path().app_config_dir()?)?
            };
            app.manage(state);
            menu::install(app)?;
            refresh::start_poller(app.handle().clone());
            Ok(())
        })
        .on_menu_event(menu::on_event)
        .invoke_handler(tauri::generate_handler![
            commands::get_inbox,
            commands::refresh,
            commands::act,
            commands::failed_jobs,
            commands::get_rules,
            commands::save_rules,
            commands::get_settings,
            commands::save_settings,
            commands::scope_preview,
            commands::planned_actions,
            commands::apply_planned,
            commands::audit_log,
            commands::open_url,
            commands::app_info,
            commands::demo_view,
        ])
        .run(tauri::generate_context!())
        .unwrap_or_else(|error| panic!("error while running Prism: {error}"));
}
