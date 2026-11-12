fn fw_screen_key(
    game: &crate::game::Game<'_>,
    key: &str,
    canceled: &mut bool,
) {
    crate::game::flyingweb_glue::on_screen_key_pressed(game, key, canceled);
}
