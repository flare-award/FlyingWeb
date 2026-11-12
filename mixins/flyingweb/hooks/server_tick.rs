fn fw_server_tick(
    game: &mut crate::game::Game<'_>,
    phase: horizon_client::mod_api::ServerTickPhase,
) {
    crate::game::flyingweb_glue::on_server_tick(game, phase);
}
