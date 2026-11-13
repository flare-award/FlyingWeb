fn fw_settings_click(game: &mut crate::game::Game<'_>, mod_id: &str, button: usize) {
    crate::game::flyingweb_glue::on_settings_click(game, mod_id, button);
}
