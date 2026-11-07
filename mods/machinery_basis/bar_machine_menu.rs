fn machinery_basis_bar_machine_menu(host: &mut crate::game::lan::LanHost, conn: u32, p: &horizon_protocol::packets::play::ContainerClick) {
    machinery_bar_machine::prepare(host, conn, p);
}
