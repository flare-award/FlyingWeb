fn fw_command(game: &mut crate::game::Game<'_>, command: &str, ran: &mut bool) {
    crate::game::flyingweb_glue::on_command_run(game, command, ran);
}
