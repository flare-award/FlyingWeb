fn fw_settings(
    game: &mut crate::game::Game<'_>,
    mod_id: &str,
    buttons: &mut Vec<String>,
) {
    crate::game::flyingweb_glue::on_settings_buttons(game, mod_id, buttons);
}
