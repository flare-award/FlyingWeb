fn fw_net_from_server(channel: &str, data: &[u8]) {
    crate::game::flyingweb_glue::on_net_from_server(channel, data);
}
