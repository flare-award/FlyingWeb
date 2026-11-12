fn fw_tick(game: &mut crate::game::Game<'_>) {
    crate::game::flyingweb_glue::on_client_tick(game);
}
