fn fw_world_primitives(
    game: &crate::game::Game<'_>,
    partial: f32,
    primitives: &mut horizon_client::world_primitives::WorldPrimitives,
) {
    crate::game::flyingweb_glue::on_world_primitives(game, partial, primitives);
}
