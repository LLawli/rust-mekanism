//! O que `RenderLogisticalTransporter` desenha de um transportador: as pilhas em trânsito que o cliente guarda (`ClientExtra.transit`), onde cada uma fica entre dois ticks
//! (`TransporterUtils.getStackPosition`, a conta em `float` do JOML) e quais delas o desenho junta numa só. O jogo desenha o que esta conta devolve (`transporter_glue.rs`).
use super::pos::{NO_POS, side_difference};
use crate::sides::Face;
use crate::transmission::client::ClientStack;

/// `Direction.step()`: o passo unitário do lado, em `float`.
pub fn step(side: Face) -> [f32; 3] {
    let [x, y, z] = side.offset();
    [x as f32, y as f32, z as f32]
}

/// `TransporterStack.getSide(transporter)` no cliente: `getPrev` e `getNext` valem `clientPrev` e `clientNext` (o pacote de atualização os leva, `Long.MAX_VALUE` sem posição).
/// Na primeira metade do caminho o lado é o de onde a pilha veio, na segunda o para onde vai; sem lado (posições que não são vizinhas) o original devolve `DOWN`.
pub fn client_side(
    progress: i32,
    previous: Option<i64>,
    next: Option<i64>,
    transporter: i64,
) -> Face {
    let mut side = None;
    if progress < 50 {
        let prev = previous.unwrap_or(NO_POS);
        if prev != NO_POS {
            side = side_difference(transporter, prev);
        }
    } else {
        let next = next.unwrap_or(NO_POS);
        if next != NO_POS {
            side = side_difference(next, transporter);
        }
    }
    side.unwrap_or(Face::Down)
}

/// `TransporterUtils.getStackPosition(transmitter, stack, partial)`: `side.step().mul(((progress + partial) / 100F) - 0.5F).add(0.5F, 0.25F, 0.5F)`, em `float` como o JOML, e
/// `partial` é `partialTick * tier.getSpeed()`.
pub fn stack_position(side: Face, progress: i32, partial: f32) -> [f32; 3] {
    let scale = (progress as f32 + partial) / 100.0 - 0.5;
    let [x, y, z] = step(side);
    [x * scale + 0.5, y * scale + 0.25, z * scale + 0.5]
}

/// Uma pilha que o desenho põe na tela: onde, com que cor da caixa (`EnumColor` pelo `ordinal`), qual item e a semente do modelo (`(int) stack.originalLocation`, a mesma enquanto a
/// pilha anda de um transportador ao próximo, para o modelo aleatório não mudar).
#[derive(Clone, Debug, PartialEq)]
pub struct DrawnStack<'a> {
    pub position: [f32; 3],
    pub color: Option<u8>,
    pub stack: &'a ClientStack,
    pub seed: i32,
}

/// `RenderLogisticalTransporter.extractRenderState`, as pilhas: `transit` na ordem do `getTransit()`, `speed` o `getSpeed()` do tier e `partial_tick` o do quadro. Não desenha a pilha vazia,
/// e as pilhas do mesmo item, com a mesma cor e no mesmo `progress` viram uma só (o tamanho da pilha não conta).
pub fn drawn_stacks<'a>(
    transit: &[&'a ClientStack],
    transporter: i64,
    speed: i32,
    partial_tick: f32,
) -> Vec<DrawnStack<'a>> {
    let partial = partial_tick * speed as f32;
    let mut seen: Vec<(
        i32,
        &str,
        Option<&crate::transmission::transmitter::Tag>,
        Option<u8>,
    )> = Vec::new();
    let mut drawn = Vec::new();
    for stack in transit {
        let Some((id, _)) = &stack.item else {
            continue;
        };
        let key = (
            stack.progress,
            id.as_str(),
            stack.components.as_ref(),
            stack.color,
        );
        if seen.contains(&key) {
            continue;
        }
        seen.push(key);
        let side = client_side(stack.progress, stack.previous, stack.next, transporter);
        drawn.push(DrawnStack {
            position: stack_position(side, stack.progress, partial),
            color: stack.color,
            stack,
            seed: stack.original_location as i32,
        });
    }
    drawn
}
