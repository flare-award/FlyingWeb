fn fw_camera_pose(
    game: &crate::game::Game<'_>,
    partial: f32,
    position: &mut [f64; 3],
    yaw: &mut f32,
    pitch: &mut f32,
) {
    crate::game::flyingweb_glue::on_camera_pose(game, partial, position, yaw, pitch);
}
