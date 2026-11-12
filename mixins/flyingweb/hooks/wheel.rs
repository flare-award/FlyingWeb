fn fw_wheel(game: &mut crate::game::Game<'_>, steps: f32, claimed: &mut bool) {
    crate::game::flyingweb_glue::on_wheel(game, steps, claimed);
}
