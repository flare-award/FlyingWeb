fn fw_player_submit(
    game: &crate::game::Game<'_>,
    player: &crate::game::native_mob_draw::NativeMobRender<'_>,
    who: &crate::game::native_mob_draw::NativePlayerRender<'_>,
    draw: &mut crate::game::native_block_entity_draw::BlockEntityDraw,
    drawn: &mut bool,
) {
    crate::game::flyingweb_glue::on_player_submit(game, player, who, draw, drawn);
}
