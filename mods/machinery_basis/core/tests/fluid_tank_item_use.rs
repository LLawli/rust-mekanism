//! O item do Fluid Tank usado como balde no mundo, contra o rastro do original (`tank_fluid_item_use.jsonl`, de `TankTraceTest.fluidItemUse`): 19 usos de
//! `ItemBlockFluidTank.use`, cada um com o resultado, a mão, o que foi para o inventário e o bloco do alvo depois.
use machinery_basis_core::bucket_item::{
    BUCKET_VOLUME, FluidWorld, UseHit, UseOutcome, use_as_bucket,
};
use machinery_basis_core::component_container::{
    AttachedCodec, ComponentBackedHandler, ComponentTankSpec, ComponentTankSpecOf,
};
use machinery_basis_core::components::Components;
use machinery_basis_core::containers::ItemResource;
use machinery_basis_core::fluid::FluidResource;
use machinery_basis_core::item_access::{HandAccess, ItemAccess, OneByOne};
use machinery_basis_core::resource_tank::StackContents;
use machinery_basis_core::transaction::Transaction;
use serde_json::{Value, json};
use std::cell::Cell;
use std::path::Path;
use std::rc::Rc;

const FLUIDS: &str = "mekanism:fluids";
/// O alvo da cena (`new BlockPos(2, 1, 2)`), o chão debaixo dele e o bloco de cima.
const TARGET: [i32; 3] = [2, 1, 2];
const FLOOR: [i32; 3] = [2, 0, 2];
const ABOVE: [i32; 3] = [2, 2, 2];

fn read(name: &str) -> Vec<Value> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../../dev/referencia/rastros")
        .join(name);
    std::fs::read_to_string(path)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect()
}

/// `FluidTankTier`: a capacidade e a taxa.
fn tier_of(id: &str) -> (u64, i32) {
    match id
        .strip_prefix("mekanism:")
        .and_then(|id| id.strip_suffix("_fluid_tank"))
        .unwrap()
    {
        "basic" => (32_000, 1_000),
        "advanced" => (64_000, 4_000),
        "elite" => (128_000, 16_000),
        "ultimate" => (256_000, 64_000),
        "creative" => (i64::MAX as u64, i32::MAX),
        other => panic!("tier desconhecido: {other}"),
    }
}

struct TankItems;

impl AttachedCodec<FluidResource> for TankItems {
    fn contents(&self, item: &ItemResource) -> StackContents<FluidResource> {
        match item.components.get(FLUIDS) {
            Some(text) => {
                let (id, amount) = text.split_once('|').unwrap();
                StackContents::new(Some(FluidResource::new(id)), amount.parse().unwrap())
            }
            None => StackContents::default(),
        }
    }

    fn with_contents(
        &self,
        item: &ItemResource,
        contents: &StackContents<FluidResource>,
    ) -> Option<ItemResource> {
        let components = match &contents.resource {
            Some(resource) => Components::new([(
                FLUIDS,
                format!("{}|{}", resource.id, contents.amount).as_str(),
            )]),
            None => Components::none(),
        };
        Some(ItemResource::with_components(
            &item.id,
            item.max_stack_size,
            components,
        ))
    }
}

/// `ftank:<tier>:<fluido>:<mB>:<quantidade>` e `ftankfull:creative:<fluido>:<quantidade>` (o criativo cheio da aba criativa).
fn stack_of(spec: &str) -> (ItemResource, i32) {
    let part: Vec<&str> = spec.split(':').collect();
    let empty = ItemResource::new(&format!("mekanism:{}_fluid_tank", part[1]), 64);
    let (amount, count): (u64, i32) = match part[0] {
        "ftank" => (part[3].parse().unwrap(), part[4].parse().unwrap()),
        "ftankfull" => (i64::MAX as u64, part[3].parse().unwrap()),
        other => panic!("pilha desconhecida: {other}"),
    };
    if amount == 0 {
        return (empty, count);
    }
    let contents = StackContents::new(
        Some(FluidResource::new(&format!("minecraft:{}", part[2]))),
        amount,
    );
    (TankItems.with_contents(&empty, &contents).unwrap(), count)
}

/// `[item, quantidade, fluido do item, mB]`, como o rastro grava uma pilha.
fn stack_json(item: &ItemResource, count: i32) -> Value {
    let contents = TankItems.contents(item);
    json!([
        &*item.id,
        count,
        contents
            .resource
            .map_or(String::new(), |fluid| fluid.id.to_string()),
        contents.amount
    ])
}

/// O mundo da cena: o alvo com ar, água ou lava (uma fonte), sobre um chão sólido.
struct Scene {
    target: Option<FluidResource>,
    sounds: Vec<(String, bool)>,
}

impl FluidWorld for Scene {
    fn fluid_to_pick(&self, pos: [i32; 3]) -> Option<FluidResource> {
        if pos == TARGET {
            self.target.clone()
        } else {
            None
        }
    }

    fn pick_up(&mut self, pos: [i32; 3]) -> Option<FluidResource> {
        if pos == TARGET {
            self.target.take()
        } else {
            None
        }
    }

    fn place(&mut self, resource: &FluidResource, pos: [i32; 3], _validate_placed: bool) -> bool {
        // O chão não é ar nem substituível; o ar e um líquido são (`destBlockState.canBeReplaced(fluid)`), e o fluido novo toma o lugar.
        if pos != TARGET {
            return false;
        }
        self.target = Some(resource.clone());
        self.sounds.push((resource.id.to_string(), false));
        true
    }

    fn picked_up(&mut self, resource: &FluidResource, _pos: [i32; 3]) {
        self.sounds.push((resource.id.to_string(), true));
    }
}

/// A mira do jogador da cena, que olha para baixo de cima do alvo: `SOURCE_ONLY` para na fonte do alvo, `NONE` (agachado) passa por ela e para no chão.
fn hit_of(scene: &Scene, sneaking: bool) -> UseHit {
    if !sneaking && scene.target.is_some() {
        UseHit {
            pos: TARGET,
            offset: ABOVE,
        }
    } else {
        UseHit {
            pos: FLOOR,
            offset: TARGET,
        }
    }
}

fn spec_of(rate: Option<i32>) -> ComponentTankSpecOf<FluidResource> {
    Rc::new(move |item: &ItemResource| {
        let (capacity, tier_rate) = tier_of(&item.id);
        ComponentTankSpec::open(
            capacity,
            rate.unwrap_or(tier_rate),
            item.id.contains("creative"),
        )
    })
}

/// O segundo uso (o primeiro, no modo balde, só toma posse do item): `ItemBlockFluidTank.java:149-179`.
fn use_item(
    hand: &HandAccess,
    scene: &mut Scene,
    sneaking: bool,
    rate: Option<i32>,
    manual: bool,
) -> UseOutcome {
    let codec: Rc<dyn AttachedCodec<FluidResource>> = Rc::new(TankItems);
    let one = OneByOne(hand);
    let handler = ComponentBackedHandler::new(&one, codec, spec_of(rate), Rc::new(Cell::new(1)));
    let handler = if manual { handler.manual() } else { handler };
    let transaction = Transaction::open_safe();
    let hit = hit_of(scene, sneaking);
    let outcome = use_as_bucket(
        Some(&handler),
        sneaking,
        Some(hit),
        true,
        scene,
        &transaction,
    );
    if outcome == UseOutcome::Success {
        transaction.commit();
    }
    outcome
}

#[test]
fn the_item_used_in_the_world_follows_the_original() {
    let lines = read("tank_fluid_item_use.jsonl");
    let mut steps = 0;
    for line in lines[1..].iter().filter(|line| line.get("step").is_some()) {
        let at = line["step"].as_str().unwrap();
        let (item, count) = stack_of(line["stack"].as_str().unwrap());
        let mode = line["mode"].as_bool().unwrap();
        let sneaking = line["sneak"].as_bool().unwrap();
        let creative = line["creative"].as_bool().unwrap();
        let mut scene = Scene {
            target: match line["block"].as_str().unwrap() {
                "air" => None,
                fluid => Some(FluidResource::new(&format!("minecraft:{fluid}"))),
            },
            sounds: Vec::new(),
        };
        let held = item.clone();
        let hand = HandAccess::new(
            Some(item),
            count,
            creative,
            Rc::new(move |resource| *resource == held),
        );
        // `getMode(stack)` falso: `PASS`, e nada muda. No modo balde o primeiro uso toma posse (`SUCCESS`) e o segundo age.
        let (first, result) = if mode {
            ("success", use_item(&hand, &mut scene, sneaking, None, true))
        } else {
            ("pass", UseOutcome::Pass)
        };
        assert_eq!(line["first"], first, "{at}: o primeiro uso");
        let name = match result {
            UseOutcome::Pass => "pass",
            UseOutcome::Fail => "fail",
            UseOutcome::Success => "success",
        };
        assert_eq!(line["result"], name, "{at}: o resultado");
        let state = hand.state();
        let recorded = line["hand"].as_array().unwrap();
        assert_eq!(
            Value::Array(recorded[..4].to_vec()),
            stack_json(state.resource.as_ref().unwrap(), state.count),
            "{at}: a mão"
        );
        let inventory: Vec<Value> = line["inventory"]
            .as_array()
            .unwrap()
            .iter()
            .map(|entry| Value::Array(entry.as_array().unwrap()[1..5].to_vec()))
            .collect();
        let extra: Vec<Value> = state
            .extra
            .iter()
            .map(|(item, count)| stack_json(item, *count))
            .collect();
        assert_eq!(inventory, extra, "{at}: o inventário");
        let after = match &scene.target {
            Some(fluid) => json!([&*fluid.id, &*fluid.id, true]),
            None => json!(["minecraft:air", "", false]),
        };
        assert_eq!(line["after"], after, "{at}: o alvo depois");
        // Um uso que deu certo toca o som uma vez; um que falhou, nenhuma.
        assert_eq!(
            scene.sounds.len(),
            usize::from(result == UseOutcome::Success),
            "{at}: o som"
        );
        steps += 1;
    }
    assert_eq!(steps, 19);
}

/// `getOneByOneFluidHandler` embrulha o handler em `AutomatedResourceHandler.manual` (`ItemBlockFluidTank.java:184-188`): o item serve de balde mesmo com a taxa do tier
/// abaixo de um balde. Sem rastro (nenhum tier tem taxa abaixo de 1.000 no padrão): confere a leitura do Java com uma taxa de 400.
#[test]
fn the_item_works_as_a_bucket_below_a_bucket_of_rate_only_as_manual() {
    let (item, _) = stack_of("ftank:basic:water:0:1");
    let water = || Scene {
        target: Some(FluidResource::new("minecraft:water")),
        sounds: Vec::new(),
    };
    let owned: Rc<dyn Fn(&ItemResource) -> bool> = Rc::new(|_| false);
    let hand = HandAccess::new(Some(item.clone()), 1, false, owned.clone());
    let mut scene = water();
    assert_eq!(
        use_item(&hand, &mut scene, false, Some(400), true),
        UseOutcome::Success
    );
    assert_eq!(
        TankItems.contents(&hand.resource().unwrap()).amount,
        BUCKET_VOLUME as u64
    );
    assert!(scene.target.is_none());
    let hand = HandAccess::new(Some(item), 1, false, owned);
    let mut scene = water();
    assert_eq!(
        use_item(&hand, &mut scene, false, Some(400), false),
        UseOutcome::Fail
    );
    assert!(hand.resource().unwrap().components.is_empty());
    assert!(scene.target.is_some());
}

/// Sem bloco na mira o uso passa, e sem permissão no lugar ele falha (`:151-160`), sem tocar no item.
#[test]
fn nothing_in_reach_passes_and_a_protected_spot_fails() {
    let (item, _) = stack_of("ftank:basic:water:5000:1");
    let hand = HandAccess::new(Some(item.clone()), 1, false, Rc::new(|_| false));
    let codec: Rc<dyn AttachedCodec<FluidResource>> = Rc::new(TankItems);
    let one = OneByOne(&hand);
    let handler =
        ComponentBackedHandler::new(&one, codec, spec_of(None), Rc::new(Cell::new(1))).manual();
    let mut scene = Scene {
        target: None,
        sounds: Vec::new(),
    };
    let transaction = Transaction::open_safe();
    assert_eq!(
        use_as_bucket(Some(&handler), true, None, true, &mut scene, &transaction),
        UseOutcome::Pass
    );
    let hit = UseHit {
        pos: FLOOR,
        offset: TARGET,
    };
    assert_eq!(
        use_as_bucket(
            Some(&handler),
            true,
            Some(hit),
            false,
            &mut scene,
            &transaction
        ),
        UseOutcome::Fail
    );
    assert_eq!(
        use_as_bucket(None, true, Some(hit), true, &mut scene, &transaction),
        UseOutcome::Fail
    );
    assert_eq!(hand.resource(), Some(item));
    assert!(scene.target.is_none());
}

/// Um mundo em que todo bloco aceita o fluido, para ver onde ele foi posto.
struct Open(Vec<[i32; 3]>);

impl FluidWorld for Open {
    fn fluid_to_pick(&self, _pos: [i32; 3]) -> Option<FluidResource> {
        None
    }

    fn pick_up(&mut self, _pos: [i32; 3]) -> Option<FluidResource> {
        None
    }

    fn place(&mut self, _resource: &FluidResource, pos: [i32; 3], validate_placed: bool) -> bool {
        // `tryPlaceFluid(fluidHandler, player, level, pos, true, transaction)`: o uso do item sempre confere o que o bloco aceitou.
        assert!(validate_placed);
        self.0.push(pos);
        true
    }

    fn picked_up(&mut self, _resource: &FluidResource, _pos: [i32; 3]) {}
}

/// Agachado, o bloco acertado vem antes do vizinho (`ItemBlockFluidTank.java:171-172`), e só um balde sai. Sem rastro: na cena do rastro o bloco acertado é sempre o chão.
#[test]
fn sneaking_places_into_the_hit_block_before_the_neighbour() {
    let (item, _) = stack_of("ftank:basic:water:5000:1");
    let hand = HandAccess::new(Some(item), 1, false, Rc::new(|_| false));
    let codec: Rc<dyn AttachedCodec<FluidResource>> = Rc::new(TankItems);
    let one = OneByOne(&hand);
    let handler =
        ComponentBackedHandler::new(&one, codec, spec_of(None), Rc::new(Cell::new(1))).manual();
    let mut world = Open(Vec::new());
    let transaction = Transaction::open_safe();
    let hit = UseHit {
        pos: TARGET,
        offset: ABOVE,
    };
    assert_eq!(
        use_as_bucket(
            Some(&handler),
            true,
            Some(hit),
            true,
            &mut world,
            &transaction
        ),
        UseOutcome::Success
    );
    transaction.commit();
    assert_eq!(world.0, vec![TARGET]);
    assert_eq!(TankItems.contents(&hand.resource().unwrap()).amount, 4_000);
}

/// `InfiniteMaterialsItemAccess.insert`: no criativo o item novo só entra se os slots principais ainda não têm um igual, e só um.
#[test]
fn a_creative_player_who_already_has_the_result_gets_nothing_more() {
    let (item, _) = stack_of("ftank:basic:water:0:1");
    let scene = || Scene {
        target: Some(FluidResource::new("minecraft:water")),
        sounds: Vec::new(),
    };
    let hand = HandAccess::new(Some(item.clone()), 1, true, Rc::new(|_| true));
    let mut world = scene();
    assert_eq!(
        use_item(&hand, &mut world, false, None, true),
        UseOutcome::Success
    );
    assert!(world.target.is_none());
    assert_eq!(hand.state().extra, Vec::new());
    assert_eq!(hand.resource(), Some(item.clone()));
    // Sem um igual, um só entra, mesmo com duas trocas seguidas.
    let hand = HandAccess::new(Some(item), 1, true, Rc::new(|_| false));
    for _ in 0..2 {
        let mut world = scene();
        assert_eq!(
            use_item(&hand, &mut world, false, None, true),
            UseOutcome::Success
        );
    }
    assert_eq!(hand.state().extra.len(), 1);
    assert_eq!(hand.state().extra[0].1, 1);
}
