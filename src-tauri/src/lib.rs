pub mod commands;
pub mod edit;
pub mod export;
pub mod kin;
pub mod story;
pub mod terrain;
pub mod version;

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
            commands::initial_world,
            commands::recent_worlds,
            commands::world_stamp,
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
            // What points at a record. The same question the delete confirmation asks,
            // asked without proposing to delete anything.
            edit::references,
            edit::preview_entity,
            edit::preview_event,
            edit::preview_delete,
            edit::save_entity,
            edit::save_event,
            edit::save_geometry,
            edit::delete_record,
            terrain::terrain_places,
            // The story. Scene editing reuses `edit.rs`'s plan-preview-commit machinery
            // wholesale rather than growing a second idea of what a safe save looks like.
            story::scenes,
            story::story,
            story::passage,
            story::resolve_prose,
            story::chapters,
            story::scene_record,
            story::preview_scene,
            story::save_scene,
            // Descent and the batons that pass along it. One payload, because a row and
            // a band on it are the same two primitives.
            kin::lineage,
            // Version control. Everything that moves the writer's files is gated on the
            // world folder being the repository root; reading is not.
            version::version_status,
            version::version_history,
            version::version_branches,
            version::version_compare,
            version::version_commit,
            version::version_branch,
            version::version_switch,
            version::version_merge,
            version::version_delete,
            version::version_discard,
            // Publishing, which has no agent-facing counterpart on purpose.
            export::preview_export,
            export::write_export,
        ])
        .run(tauri::generate_context!())
        .expect("failed to start Worldbuilder");
}
