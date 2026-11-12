//! `BlockMekanism.canBeReplaced(state, fluid)` é falso em todo bloco do Mekanism (`rastros/blocos_oclusao.jsonl`, `testes/OclusaoTraceTest.java`: `canBeReplaced(WATER)` e `(LAVA)` falsos nos 206 blocos), e o
//! balde do Horizon troca por fluido o bloco que não é sólido (o gabarito `chorus_plant` dos cabos e transmissores). O gancho `fill_bucket` recusa o balde cheio num transmissor (`rules::bucket_refused_by`).
use mekanism_rules as rules;

use serde_json::Value;

fn rastro() -> Vec<Value> {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../dev/referencia/rastros/blocos_oclusao.jsonl");
    std::fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("{}: {error}", path.display()))
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect()
}

#[test]
fn a_filled_bucket_does_not_pour_into_a_transmitter() {
    let mut transmitters = 0;
    for row in rastro() {
        let id = row["id"].as_str().unwrap();
        for facts in row["facts"].as_array().unwrap() {
            // O original: nenhum fluido substitui nenhum bloco do Mekanism.
            assert_eq!(facts["replace_water"], false, "{id}");
            assert_eq!(facts["replace_lava"], false, "{id}");
        }
        // O que o Horizon deixaria trocar (não sólido) é o que o original sabe que é um transmissor: o gabarito deles é o `chorus_plant`.
        let refused = rules::bucket_refused_by("minecraft:water_bucket", id);
        if rules::transmitter_center(id).is_some() {
            assert!(refused, "{id}");
            assert!(
                rules::bucket_refused_by("minecraft:lava_bucket", id),
                "{id}"
            );
            assert!(
                rules::bucket_refused_by("minecraft:tropical_fish_bucket", id),
                "{id}"
            );
            assert!(
                rules::bucket_refused_by("mekanism:brine_bucket", id),
                "{id}"
            );
            // O balde vazio, o de leite e o de neve em pó não derramam fluido.
            for bucket in [
                "minecraft:bucket",
                "minecraft:milk_bucket",
                "minecraft:powder_snow_bucket",
                "minecraft:stone",
            ] {
                assert!(!rules::bucket_refused_by(bucket, id), "{bucket} {id}");
            }
            transmitters += 1;
        } else {
            assert!(!refused, "{id}: o gabarito sólido já recusa");
        }
    }
    assert_eq!(transmitters, 22);
}

/// `BucketItem.use` pergunta ao gancho `fill_bucket` pelo bloco que recebe o fluido e, quando o clicado é um contêiner de água (laje, escada), também pelo ao lado (a conferência `mayUseItemAt`,
/// que não passa fluido). O original só olha o `canBeReplaced` do bloco que recebe: um transmissor ao lado de uma laje não impede de encher a laje; o próprio transmissor como alvo, sim.
#[test]
fn only_the_block_that_receives_the_fluid_refuses_the_bucket() {
    let cable = "mekanism:basic_universal_cable";
    let target = [10, 64, 10];
    let beside = [10, 64, 11];
    // O transmissor é o alvo (o bloco clicado não é um contêiner de água, então o fluido vai para o ao lado): recusa.
    assert!(rules::bucket_refused_at(
        "minecraft:water_bucket",
        cable,
        target,
        Some(target)
    ));
    assert!(rules::bucket_refused_at(
        "minecraft:lava_bucket",
        cable,
        target,
        Some(target)
    ));
    // O alvo é uma laje e o transmissor está ao lado dela (a segunda pergunta do gancho): a laje se enche, o gancho não recusa.
    assert!(!rules::bucket_refused_at(
        "minecraft:water_bucket",
        cable,
        beside,
        Some(target)
    ));
    // Nenhum clique de balde visto: nada a recusar.
    assert!(!rules::bucket_refused_at(
        "minecraft:water_bucket",
        cable,
        target,
        None
    ));
    // O alvo que não é um transmissor, ou um balde que não derrama, nunca recusa.
    assert!(!rules::bucket_refused_at(
        "minecraft:water_bucket",
        "minecraft:stone_slab",
        target,
        Some(target)
    ));
    assert!(!rules::bucket_refused_at(
        "minecraft:bucket",
        cable,
        target,
        Some(target)
    ));
}
