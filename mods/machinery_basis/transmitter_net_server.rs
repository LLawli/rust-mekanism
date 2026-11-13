fn machinery_basis_transmitter_net_server(channel: &str, data: &[u8]) {
    crate::game::machinery_transmitter_client::net_from_server(channel, data);
}
