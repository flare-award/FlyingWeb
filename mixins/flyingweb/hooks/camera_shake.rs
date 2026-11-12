fn fw_camera_shake(
    game: &crate::game::Game<'_>,
    shake: &mut crate::game::native_hooks::NativeShake,
) {
    crate::game::flyingweb_glue::on_camera_shake(game, shake);
}
