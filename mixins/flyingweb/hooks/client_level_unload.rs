fn fw_client_level_unload(game: &mut crate::game::Game<'_>) {
    crate::game::flyingweb_glue::on_client_world_reset(game);
}
