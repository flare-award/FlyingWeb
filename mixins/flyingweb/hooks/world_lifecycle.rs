fn fw_world_lifecycle(phase: horizon_client::mod_api::WorldPhase) {
    crate::game::flyingweb_glue::on_world_lifecycle(phase);
}
