fn fw_render_hand_pose(
    game: &crate::game::Game<'_>,
    hand: horizon_client::first_person::Hand,
    stack: Option<&horizon_client::items::ItemStack>,
    pitch: &mut f32,
    y: &mut f32,
    z: &mut f32,
) {
    crate::game::flyingweb_glue::on_render_hand_pose(game, hand, stack, pitch, y, z);
}
