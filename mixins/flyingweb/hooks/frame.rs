fn fw_frame(game: &mut crate::game::Game<'_>, elapsed: std::time::Duration) {
    crate::game::flyingweb_glue::on_frame(game, elapsed);
}
