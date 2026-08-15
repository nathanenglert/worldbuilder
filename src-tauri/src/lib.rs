pub mod commands;
pub mod edit;
pub mod terrain;

pub fn run() {
    #[allow(unused_mut)]
    let mut builder = tauri::Builder::default();

    // UI automation for development only — never present in a release build.
    #[cfg(debug_assertions)]
    {
        builder = builder.plugin(tauri_plugin_pilot::init());
    }

    builder
        .manage(commands::AppState::default())
        .invoke_handler(tauri::generate_handler![
            commands::open_world,
            commands::example_world_path,
            commands::snapshot,
            terrain::terrain,
            terrain::map_image,
            commands::timeline,
            commands::resolve_expr,
            commands::format_day,
            commands::check_world,
            commands::list_proposals,
            commands::proposal_detail,
            commands::decide_proposal,
            // Authoring. Reading a record is separate from reading a *snapshot* on
            // purpose: the snapshot is a rendered view at a date and cannot be saved back.
            edit::entity_record,
            edit::event_record,
            edit::preview_entity,
            edit::preview_event,
            edit::preview_delete,
            edit::save_entity,
            edit::save_event,
            edit::save_geometry,
            edit::delete_record,
            terrain::terrain_places,
        ])
        .run(tauri::generate_context!())
        .expect("failed to start Worldbuilder");
}
