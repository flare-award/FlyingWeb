fn fw_compute_fov(game: &crate::game::Game<'_>, degrees: &mut f32) {
    crate::game::flyingweb_glue::on_compute_fov(game, degrees);
}
