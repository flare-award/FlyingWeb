fn fw_player_render_listened(game: &crate::game::Game<'_>, listened: &mut bool) {
    crate::game::flyingweb_glue::on_player_render_listened(game, listened);
}
