fn fw_living_incoming_damage(entity: i32, damage_type: &str, canceled: &mut bool) {
    crate::game::flyingweb_glue::on_living_incoming_damage(entity, damage_type, canceled);
}
