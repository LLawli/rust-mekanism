// Entra em `crates/horizon-client/src/game/lan_crafting.rs` como `machinery_bar_machine`. O menu de um contêiner de mod que o servidor roda
// (`lan_machine_click`) nasce de `Menu::new(Kind::Mod(t))`, que devolve `Self` e por isso não recebe injeção. Antes do clique, quando o tipo
// declarou uma barra de armadura, este módulo deixa pronto o menu da conexão (`crafting.machine`) com os cinco slots a mais
// (`machinery_bar_menu::extend`), e o clique o reaproveita (`take().filter(janela e tipo iguais)`) no lugar de montar outro.
use super::{Kind, LanHost, Menu};
use horizon_protocol::packets::play;

pub(crate) fn prepare(host: &mut LanHost, conn: u32, p: &play::ContainerClick) {
    let Some(menu_type) = host
        .hub
        .guest(conn)
        .and_then(|g| g.menu.as_ref())
        .filter(|m| p.container != 0 && m.window == p.container)
        .and_then(|m| m.mod_type)
    else {
        return;
    };
    if !horizon_client::menu::machinery_bar_menu::declared(menu_type.id) {
        return;
    }
    let kind = Kind::Mod(menu_type);
    let machine = &mut host.play.entry(conn).or_default().crafting.machine;
    match machine {
        Some((window, menu)) if *window == p.container && menu.kind() == kind => {
            horizon_client::menu::machinery_bar_menu::extend(menu);
        }
        _ => {
            let mut menu = Menu::new(kind);
            horizon_client::menu::machinery_bar_menu::extend(&mut menu);
            *machine = Some((p.container, Box::new(menu)));
        }
    }
}
