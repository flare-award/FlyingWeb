fn fw_key(game: &mut crate::game::Game<'_>, key: &str, pressed: bool) {
    crate::game::flyingweb_glue::on_key(game, key, pressed);
}
