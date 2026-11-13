fn fw_client_reset(game: &mut crate::game::Game<'_>) {
    crate::game::flyingweb_glue::on_client_world_reset(game);
}
