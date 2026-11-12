fn fw_commands(commands: &mut Vec<&'static str>) {
    crate::game::flyingweb_glue::on_commands_register(commands);
}
