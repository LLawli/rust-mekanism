//! Porte de `TileComponentEjector`: `tickServer`, `eject`, `outputItems`, `getSidesForData` e o estado que o componente guarda (a
//! cor de saída, a cor de entrada de cada lado, `strictInput`). Quem manda é a configuração de lados (`TileComponentConfig`): o
//! ejetor só sai pelos lados em um `DataType` que sai, e só o que o `ConfigInfo` do tipo manda ejetar. A transação de cada envio
//! cobre a máquina e o inventário do vizinho, então os dois lados voltam juntos se algo falhar.
use crate::containers::InventorySlot;
use crate::item_handler::{ItemCapabilities, ItemHandler, SlotHandler};
use crate::side_config::{
    ConfigInfo, DataType, RelativeSide, TileComponentConfig, TransmissionType,
};
use crate::sides::Face;
use crate::transaction::Transaction;
use crate::transit::{HandlerTransitRequest, eject_item_map};
use std::rc::Rc;

/// `MekanismUtils.TICKS_PER_HALF_SECOND`.
pub const TICKS_PER_HALF_SECOND: u32 = 10;

/// `java.util.Random.nextInt(bound)` não se repete entre rodadas; aqui o sorteio vem de um xorshift fixo, que só decide a
/// ordem em que os slots são vistos.
struct Xorshift(u64);

impl Xorshift {
    fn below(&mut self, bound: usize) -> usize {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        (self.0 % bound as u64) as usize
    }
}

/// O que o ejetor não faz sozinho com os tipos que não são item: perguntar se um contêiner está vazio e mandar o conteúdo para as
/// capabilities dos vizinhos (`ResourceUtils.emit`, `EnergyUtils.emit`). As máquinas portadas não têm um `DataType` de saída em
/// nenhum desses tipos, então nada é chamado; o gancho fica para quando houver (Fase 3, a rede).
pub trait EjectEnv {
    /// Se o contêiner do tipo está vazio.
    fn is_empty(&self, kind: TransmissionType, container: usize) -> bool;
    /// `canTankEject`: se o tanque de químico pode ser ejetado.
    fn can_tank_eject(&self, _container: usize) -> bool {
        true
    }
    /// Ejeta o contêiner pelas faces.
    fn emit(&mut self, kind: TransmissionType, container: usize, sides: &[Face]);
}

/// Sem contêiner de nenhum outro tipo.
pub struct NoOtherEject;

impl EjectEnv for NoOtherEject {
    fn is_empty(&self, _: TransmissionType, _: usize) -> bool {
        true
    }

    fn emit(&mut self, _: TransmissionType, _: usize, _: &[Face]) {}
}

/// `AttachedEjector`: o componente `mekanism:ejector` do item. A cor é o `ordinal` de `EnumColor`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct AttachedEjector {
    pub input_colors: [Option<u8>; 6],
    pub strict_input: bool,
    pub output_color: Option<u8>,
}

impl AttachedEjector {
    /// `AttachedEjector.DEFAULT`.
    pub const DEFAULT: AttachedEjector = AttachedEjector {
        input_colors: [None; 6],
        strict_input: false,
        output_color: None,
    };
}

/// O que `serialize` grava e `deserialize` lê de `component_ejector`: `strict_input` (só quando verdadeiro), `color` (o nome do
/// `EnumColor`, só quando há) e `input_color` (`int[]` com o `ordinal` ou -1, só quando algum lado tem cor). Sem nenhum, o filho
/// inteiro some do NBT.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct EjectorData {
    pub strict_input: Option<bool>,
    pub color: Option<u8>,
    pub input_color: Option<Vec<i32>>,
}

impl EjectorData {
    /// `ValueOutput.isEmpty` do filho: nada a gravar.
    pub fn is_empty(&self) -> bool {
        self.strict_input.is_none() && self.color.is_none() && self.input_color.is_none()
    }
}

/// `TransporterUtils.getColorIndex`.
fn color_index(color: Option<u8>) -> i32 {
    color.map_or(-1, i32::from)
}

/// `TransporterUtils.readColor`: negativo é sem cor, e o que passa do fim vira a primeira (`EnumColor.BY_ID`, `OutOfBoundsStrategy.ZERO`).
fn read_color(index: i32) -> Option<u8> {
    if index < 0 {
        return None;
    }
    Some(
        u8::try_from(index)
            .ok()
            .filter(|&index| index < 18)
            .unwrap_or(0),
    )
}

/// `TileComponentEjector`.
pub struct TileComponentEjector {
    /// `setOutputData`: os tipos de que o ejetor lê o `ConfigInfo` na configuração.
    output_types: Vec<TransmissionType>,
    can_eject: Option<Rc<dyn Fn(TransmissionType) -> bool>>,
    strict_input: bool,
    output_color: Option<u8>,
    input_colors: [Option<u8>; 6],
    tick_delay: u32,
    rng: Xorshift,
}

impl Default for TileComponentEjector {
    fn default() -> Self {
        Self::new()
    }
}

impl TileComponentEjector {
    pub fn new() -> Self {
        Self {
            output_types: Vec::new(),
            can_eject: None,
            strict_input: false,
            output_color: None,
            input_colors: [None; 6],
            tick_delay: 0,
            rng: Xorshift(0x9E37_79B9_7F4A_7C15),
        }
    }

    /// `setOutputData(config, types...)`: só os tipos que a configuração tem entram.
    pub fn set_output_data(&mut self, config: &TileComponentConfig, types: &[TransmissionType]) {
        for &kind in types {
            if config.supports(kind) && !self.output_types.contains(&kind) {
                self.output_types.push(kind);
            }
        }
    }

    /// `setCanEject(predicate)`.
    pub fn set_can_eject(&mut self, can_eject: impl Fn(TransmissionType) -> bool + 'static) {
        self.can_eject = Some(Rc::new(can_eject));
    }

    /// `isEjecting(info, type)`.
    pub fn is_ejecting(&self, info: &ConfigInfo, kind: TransmissionType) -> bool {
        info.is_ejecting()
            && self
                .can_eject
                .as_ref()
                .is_none_or(|can_eject| can_eject(kind))
    }

    /// `tickServer`: os tipos na ordem de `TransmissionType.VALUES`; o item espera o `tickDelay`, os outros (menos o calor) ejetam
    /// todo tick. `inventory` é `getInventorySlots`, na ordem que a configuração usa.
    pub fn tick_server(
        &mut self,
        config: &TileComponentConfig,
        facing: Face,
        inventory: &[InventorySlot],
        neighbours: &dyn ItemCapabilities,
        env: &mut dyn EjectEnv,
    ) {
        for kind in TransmissionType::VALUES {
            if !self.output_types.contains(&kind) {
                continue;
            }
            let Some(info) = config.config(kind) else {
                continue;
            };
            if !self.is_ejecting(info, kind) {
                continue;
            }
            if kind == TransmissionType::Item {
                if self.tick_delay == 0 {
                    self.output_items(facing, info, inventory, neighbours);
                } else {
                    self.tick_delay -= 1;
                }
            } else if kind != TransmissionType::Heat {
                Self::eject(kind, facing, info, env);
            }
        }
    }

    /// `getSidesForData`: as faces do mundo em que o `DataType` está, na ordem do `EnumSet<Direction>`.
    pub fn sides_for_data(info: &ConfigInfo, facing: Face, data_type: DataType) -> Vec<Face> {
        let mut sides: Vec<Face> = info
            .side_config()
            .filter(|&(_, found)| found == data_type)
            .map(|(side, _)| side.direction(facing))
            .collect();
        sides.sort_by_key(|face| *face as usize);
        sides.dedup();
        sides
    }

    /// `eject`, para energia, fluido e químico: junta, por contêiner, as faces de todos os `DataType` que saem e o têm.
    fn eject(kind: TransmissionType, facing: Face, info: &ConfigInfo, env: &mut dyn EjectEnv) {
        let mut output_data: Vec<(usize, Vec<Face>)> = Vec::new();
        let mut add_data = |container: usize, sides: &[Face]| match output_data
            .iter_mut()
            .find(|(known, _)| *known == container)
        {
            Some((_, known)) => {
                known.extend_from_slice(sides);
                known.sort_by_key(|face| *face as usize);
                known.dedup();
            }
            None => output_data.push((container, sides.to_vec())),
        };
        for data_type in info.supported_data_types() {
            if !data_type.can_output() {
                continue;
            }
            let Some(slot_info) = info.slot_info_of(data_type) else {
                continue;
            };
            if slot_info.is_empty(&|kind, container| env.is_empty(kind, container)) {
                continue;
            }
            let sides = Self::sides_for_data(info, facing, data_type);
            if sides.is_empty() || slot_info.kind != kind {
                continue;
            }
            match kind {
                TransmissionType::Chemical | TransmissionType::Fluid => {
                    for &container in &slot_info.containers {
                        let allowed =
                            kind != TransmissionType::Chemical || env.can_tank_eject(container);
                        if !env.is_empty(kind, container) && allowed {
                            add_data(container, &sides);
                        }
                    }
                }
                TransmissionType::Energy => {
                    if let Some(&container) = slot_info.containers.first() {
                        add_data(container, &sides);
                    }
                }
                _ => {}
            }
        }
        for (container, sides) in output_data {
            env.emit(kind, container, &sides);
        }
    }

    /// `outputItems`, com `transaction == null`: cada envio abre a sua raiz.
    fn output_items(
        &mut self,
        facing: Face,
        info: &ConfigInfo,
        inventory: &[InventorySlot],
        neighbours: &dyn ItemCapabilities,
    ) {
        for data_type in info.supported_data_types() {
            if !data_type.can_output() {
                continue;
            }
            let Some(slot_info) = info.slot_info_of(data_type) else {
                continue;
            };
            let slots: Vec<InventorySlot> = slot_info
                .containers
                .iter()
                .filter_map(|&index| inventory.get(index).cloned())
                .collect();
            if slots.iter().all(InventorySlot::is_empty) {
                // "don't even bother getting caps etc".
                continue;
            }
            if slot_info.kind != TransmissionType::Item {
                continue;
            }
            let outputs = Self::sides_for_data(info, facing, data_type);
            if outputs.is_empty() {
                continue;
            }
            // O que a máquina mostra por qualquer lado de um tipo são os mesmos slots (`getHandler(side)`), e é nisso que
            // `EjectTransitRequest` se apoia.
            let handler: Rc<dyn ItemHandler> = Rc::new(SlotHandler::new(slots.clone()));
            let mut eject_map: Option<HandlerTransitRequest> = None;
            for &side in &outputs {
                // Um transportador é o `TransporterItemHandler` da capability: o pedido vai a ele inteiro, com a cor de saída.
                let transporter = neighbours.transporter(side);
                let capability = if transporter.is_some() {
                    None
                } else {
                    let Some(capability) = neighbours.item_handler(side) else {
                        // Sem alvo neste lado.
                        continue;
                    };
                    Some(capability)
                };
                if let Some(request) = &eject_map {
                    // O handler do lado de onde o item sai, para a resposta usar as regras dele.
                    request.set_handler(Some(handler.clone()));
                } else {
                    let request = eject_item_map(
                        HandlerTransitRequest::new(Some(handler.clone())),
                        &slots,
                        &mut |bound| self.rng.below(bound),
                        None,
                    );
                    if request.is_empty() {
                        break;
                    }
                    eject_map = Some(request);
                }
                let Some(request) = &eject_map else {
                    break;
                };
                let sub = Transaction::open(None);
                let response = match &transporter {
                    Some(transporter) => request.eject_to_transporter(
                        transporter.as_ref(),
                        1,
                        self.output_color,
                        &sub,
                    ),
                    None => request.eject(capability.as_ref(), 1, &sub),
                };
                if response.use_all(&sub) {
                    sub.commit();
                    if request.is_empty() {
                        break;
                    }
                }
            }
        }
        self.tick_delay = TICKS_PER_HALF_SECOND;
    }

    /// O atraso até o próximo disparo de itens (`tickDelay`).
    pub fn tick_delay(&self) -> u32 {
        self.tick_delay
    }

    pub fn has_strict_input(&self) -> bool {
        self.strict_input
    }

    pub fn set_strict_input(&mut self, strict: bool) {
        self.strict_input = strict;
    }

    pub fn output_color(&self) -> Option<u8> {
        self.output_color
    }

    pub fn set_output_color(&mut self, color: Option<u8>) {
        self.output_color = color;
    }

    /// `isInputSideEnabled`: sem `ConfigInfo` de itens, vale.
    pub fn is_input_side_enabled(config: &TileComponentConfig, side: RelativeSide) -> bool {
        config
            .config(TransmissionType::Item)
            .is_none_or(|info| info.is_side_enabled(side))
    }

    /// `setInputColor`: um lado desligado não muda.
    pub fn set_input_color(
        &mut self,
        config: &TileComponentConfig,
        side: RelativeSide,
        color: Option<u8>,
    ) {
        if Self::is_input_side_enabled(config, side) {
            self.input_colors[side.ordinal()] = color;
        }
    }

    pub fn input_color(&self, side: RelativeSide) -> Option<u8> {
        self.input_colors[side.ordinal()]
    }

    /// `applyImplicitComponents`: o item colocado traz o ejetor.
    pub fn apply_implicit_components(&mut self, attached: Option<&AttachedEjector>) {
        if let Some(attached) = attached {
            self.input_colors = attached.input_colors;
            self.strict_input = attached.strict_input;
            self.output_color = attached.output_color;
        }
    }

    /// `collectImplicitComponents`.
    pub fn collect_implicit_components(&self) -> AttachedEjector {
        AttachedEjector {
            input_colors: self.input_colors,
            strict_input: self.strict_input,
            output_color: self.output_color,
        }
    }

    /// `deserialize`: sem `input_color` no NBT, as cores de entrada voltam a nenhuma.
    pub fn deserialize(&mut self, data: &EjectorData) {
        self.strict_input = data.strict_input.unwrap_or(false);
        self.output_color = data.color;
        match &data.input_color {
            Some(colors) => {
                for (slot, &color) in self.input_colors.iter_mut().zip(colors) {
                    *slot = read_color(color);
                }
            }
            None => self.input_colors = [None; 6],
        }
    }

    /// `serialize`.
    pub fn serialize(&self) -> EjectorData {
        let has_color = self.input_colors.iter().any(Option::is_some);
        EjectorData {
            strict_input: self.strict_input.then_some(true),
            color: self.output_color,
            input_color: has_color.then(|| {
                self.input_colors
                    .iter()
                    .map(|&color| color_index(color))
                    .collect()
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config() -> TileComponentConfig {
        let mut config = TileComponentConfig::new(&[TransmissionType::Item]);
        config.setup_item_io_config(&[0], &[1], 2, false);
        config
    }

    struct NoNeighbours;

    impl ItemCapabilities for NoNeighbours {
        fn item_handler(&self, _: Face) -> Option<Rc<dyn ItemHandler>> {
            None
        }
    }

    struct Recorder(Vec<(TransmissionType, usize, Vec<Face>)>);

    impl EjectEnv for Recorder {
        fn is_empty(&self, _: TransmissionType, container: usize) -> bool {
            container == 7
        }

        fn emit(&mut self, kind: TransmissionType, container: usize, sides: &[Face]) {
            self.0.push((kind, container, sides.to_vec()));
        }
    }

    /// Um transportador como o ejetor o vê: guarda com que cor, mínimo e itens o pedido chegou, e devolve a resposta vazia.
    /// A cor de saída, o mínimo e os itens (id e quantidade) de um pedido.
    type Seen = (Option<u8>, u32, Vec<(String, u32)>);

    struct Transporter {
        seen: std::cell::RefCell<Vec<Seen>>,
    }

    impl crate::item_handler::TransporterTarget for Transporter {
        fn insert_request(
            &self,
            request: &crate::transit::TransitRequest,
            output_color: Option<u8>,
            min: u32,
            _: &Transaction,
        ) -> crate::transit::TransitResponse {
            let items = request
                .item_data()
                .iter()
                .map(|data| {
                    let (resource, count) = crate::transit::item_data_info(data);
                    (resource.id.to_string(), count)
                })
                .collect();
            self.seen.borrow_mut().push((output_color, min, items));
            crate::transit::TransitResponse::EMPTY
        }
    }

    struct Beside<'a>(&'a Transporter);

    impl ItemCapabilities for Beside<'_> {
        fn item_handler(&self, _: Face) -> Option<Rc<dyn ItemHandler>> {
            panic!("um transportador não passa pelo handler");
        }

        fn transporter(
            &self,
            face: Face,
        ) -> Option<Box<dyn crate::item_handler::TransporterTarget + '_>> {
            (face == Face::West).then(|| {
                Box::new(Share(self.0)) as Box<dyn crate::item_handler::TransporterTarget + '_>
            })
        }
    }

    struct Share<'a>(&'a Transporter);

    impl crate::item_handler::TransporterTarget for Share<'_> {
        fn insert_request(
            &self,
            request: &crate::transit::TransitRequest,
            output_color: Option<u8>,
            min: u32,
            transaction: &Transaction,
        ) -> crate::transit::TransitResponse {
            self.0
                .insert_request(request, output_color, min, transaction)
        }
    }

    /// `TileComponentEjector.outputItems` com um `TransporterItemHandler` do outro lado: `ejectMap.eject(tile, level, capability, 1, this.outputColor, subTransaction)` entrega o pedido
    /// inteiro ao transportador com a cor de saída, e sem cor entrega `null` (o transportador usa a dele).
    #[test]
    fn a_transporter_neighbour_gets_the_whole_request_and_the_output_color() {
        let mut config = config();
        let info = config.config_mut(TransmissionType::Item).unwrap();
        info.set_data_type(DataType::Output, RelativeSide::Right);
        info.set_ejecting(true);
        let output = InventorySlot::output(None, None);
        output.set_contents(
            Some(crate::containers::ItemResource::new(
                "minecraft:clay_ball",
                64,
            )),
            7,
            None,
        );
        let inventory = [InventorySlot::new(None), output];
        let transporter = Transporter {
            seen: Default::default(),
        };
        for (color, expected) in [(Some(4), Some(4)), (None, None)] {
            let mut ejector = TileComponentEjector::new();
            ejector.set_output_data(&config, &[TransmissionType::Item]);
            ejector.set_output_color(color);
            // Facing norte: o lado direito é o oeste.
            ejector.tick_server(
                &config,
                Face::North,
                &inventory,
                &Beside(&transporter),
                &mut Recorder(Vec::new()),
            );
            let seen = transporter
                .seen
                .borrow()
                .last()
                .cloned()
                .expect("o transportador foi chamado");
            assert_eq!(
                seen,
                (expected, 1, vec![("minecraft:clay_ball".to_owned(), 7)])
            );
        }
        assert_eq!(transporter.seen.borrow().len(), 2);
    }

    #[test]
    fn the_sides_of_a_data_type_follow_the_facing_in_direction_order() {
        let mut config = config();
        let info = config.config_mut(TransmissionType::Item).unwrap();
        info.set_data_type(DataType::Output, RelativeSide::Right);
        info.set_data_type(DataType::Output, RelativeSide::Bottom);
        info.set_data_type(DataType::Output, RelativeSide::Front);
        let info = config.config(TransmissionType::Item).unwrap();
        assert_eq!(
            TileComponentEjector::sides_for_data(info, Face::North, DataType::Output),
            [Face::Down, Face::North, Face::West]
        );
        assert_eq!(
            TileComponentEjector::sides_for_data(info, Face::East, DataType::Output),
            [Face::Down, Face::North, Face::East]
        );
        assert!(TileComponentEjector::sides_for_data(info, Face::East, DataType::Input).is_empty());
    }

    #[test]
    fn ejecting_needs_the_flag_and_the_predicate() {
        let mut config = config();
        let mut ejector = TileComponentEjector::new();
        ejector.set_output_data(&config, &[TransmissionType::Item, TransmissionType::Fluid]);
        config
            .config_mut(TransmissionType::Item)
            .unwrap()
            .set_ejecting(true);
        let info = config.config(TransmissionType::Item).unwrap();
        assert!(ejector.is_ejecting(info, TransmissionType::Item));
        ejector.set_can_eject(|kind| kind != TransmissionType::Item);
        assert!(!ejector.is_ejecting(info, TransmissionType::Item));
    }

    #[test]
    fn the_saved_form_leaves_defaults_out() {
        let mut ejector = TileComponentEjector::new();
        assert!(ejector.serialize().is_empty());
        ejector.set_strict_input(true);
        assert_eq!(ejector.serialize().strict_input, Some(true));
        ejector.set_strict_input(false);
        assert!(ejector.serialize().is_empty());
        ejector.set_output_color(Some(4));
        let config = config();
        ejector.set_input_color(&config, RelativeSide::Left, Some(12));
        let data = ejector.serialize();
        assert_eq!(data.color, Some(4));
        assert_eq!(data.input_color, Some(vec![-1, 12, -1, -1, -1, -1]));
        let mut other = TileComponentEjector::new();
        other.deserialize(&data);
        assert_eq!(other.serialize(), data);
        assert_eq!(other.input_color(RelativeSide::Left), Some(12));
    }

    #[test]
    fn reading_without_input_colors_clears_them_and_bad_ids_become_the_first() {
        let mut ejector = TileComponentEjector::new();
        ejector.deserialize(&EjectorData {
            strict_input: None,
            color: None,
            input_color: Some(vec![99, -5, 3]),
        });
        assert_eq!(ejector.input_color(RelativeSide::Front), Some(0));
        assert_eq!(ejector.input_color(RelativeSide::Left), None);
        assert_eq!(ejector.input_color(RelativeSide::Right), Some(3));
        ejector.deserialize(&EjectorData::default());
        assert!(ejector.serialize().is_empty());
    }

    #[test]
    fn a_disabled_side_keeps_its_input_color() {
        let mut config = config();
        config.add_disabled_sides(&[RelativeSide::Top]);
        let mut ejector = TileComponentEjector::new();
        ejector.set_input_color(&config, RelativeSide::Top, Some(2));
        assert_eq!(ejector.input_color(RelativeSide::Top), None);
        ejector.set_input_color(&config, RelativeSide::Front, Some(2));
        assert_eq!(ejector.input_color(RelativeSide::Front), Some(2));
    }

    #[test]
    fn the_item_default_carries_the_ejector_state() {
        let mut ejector = TileComponentEjector::new();
        ejector.apply_implicit_components(Some(&AttachedEjector {
            input_colors: [Some(1), None, None, None, None, None],
            strict_input: true,
            output_color: Some(9),
        }));
        assert_eq!(ejector.collect_implicit_components().output_color, Some(9));
        ejector.apply_implicit_components(Some(&AttachedEjector::DEFAULT));
        assert_eq!(
            ejector.collect_implicit_components(),
            AttachedEjector::DEFAULT
        );
    }

    #[test]
    fn the_other_types_merge_the_sides_of_every_output_data_type_per_container() {
        let mut config = TileComponentConfig::new(&[TransmissionType::Fluid]);
        config.setup_io_config(TransmissionType::Fluid, 0, false);
        let info = config.config_mut(TransmissionType::Fluid).unwrap();
        info.set_data_type(DataType::Output, RelativeSide::Right);
        info.set_data_type(DataType::InputOutput, RelativeSide::Back);
        info.set_ejecting(true);
        let mut ejector = TileComponentEjector::new();
        ejector.set_output_data(&config, &[TransmissionType::Fluid]);
        let mut recorder = Recorder(Vec::new());
        ejector.tick_server(&config, Face::North, &[], &NoNeighbours, &mut recorder);
        assert_eq!(
            recorder.0,
            [(TransmissionType::Fluid, 0, vec![Face::South, Face::West])]
        );
    }

    #[test]
    fn an_empty_container_is_not_ejected() {
        let mut config = TileComponentConfig::new(&[TransmissionType::Fluid]);
        config.setup_io_config(TransmissionType::Fluid, 7, false);
        let info = config.config_mut(TransmissionType::Fluid).unwrap();
        info.set_data_type(DataType::Output, RelativeSide::Right);
        info.set_ejecting(true);
        let mut ejector = TileComponentEjector::new();
        ejector.set_output_data(&config, &[TransmissionType::Fluid]);
        let mut recorder = Recorder(Vec::new());
        ejector.tick_server(&config, Face::North, &[], &NoNeighbours, &mut recorder);
        assert!(recorder.0.is_empty());
    }
}
