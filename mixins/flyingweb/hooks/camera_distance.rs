fn fw_camera_distance(
    game: &crate::game::Game<'_>,
    mirrored: bool,
    distance: &mut f32,
) {
    crate::game::flyingweb_glue::on_camera_distance(game, mirrored, distance);
}
