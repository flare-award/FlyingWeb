fn fw_screen_mouse(
    game: &crate::game::Game<'_>,
    screen: &horizon_client::mod_api::NativeScreenView<'_>,
    button: i32,
    canceled: &mut bool,
) {
    crate::game::flyingweb_glue::on_screen_mouse_pressed(game, screen, button, canceled);
}
