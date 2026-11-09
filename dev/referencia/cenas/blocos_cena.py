#!/usr/bin/env python3
"""Escreve `blocos.json`, o roteiro da cena de comparação dos blocos de máquina: o mesmo para o original (`testes/CenaBlocos.java`) e para o port (`horizon2/h2_blocos.py cena`).

Cada tiro monta uma fileira de blocos com `setblock` (os estados têm os mesmos nomes nos dois jogos), põe a câmera à frente e pede o print. O roteiro sai dos blocos que o
original em jogo registra (`rastros/blocos.jsonl`): cada bloco só recebe as propriedades que tem.
"""
import json
import math
import os

HERE = os.path.dirname(os.path.abspath(__file__))
RASTRO = os.path.join(HERE, "..", "rastros", "blocos.jsonl")
X0, Y, Z = 60, -60, 82
EYE = Y + 1.62

FAMILIES = {
    "fabricas_basic": [f"basic_{k}_factory" for k in "smelting enriching crushing compressing combining purifying injecting infusing sawing".split()],
    "fabricas_ultimate": [f"ultimate_{k}_factory" for k in "smelting enriching crushing compressing combining purifying injecting infusing sawing".split()],
    "fabricas_tiers": [f"{t}_enriching_factory" for t in "basic advanced elite ultimate".split()] + [f"{t}_infusing_factory" for t in "basic advanced elite ultimate".split()],
    "cubos": "combiner osmium_compressor purification_chamber chemical_injection_chamber precision_sawmill painting_machine pigment_extractor formulaic_assemblicator "
             "fuelwood_heater oredictionificator dimensional_stabilizer teleporter".split(),
    "quimica": "chemical_oxidizer chemical_infuser chemical_dissolution_chamber chemical_washer chemical_crystallizer electrolytic_separator rotary_condensentrator "
               "pressurized_reaction_chamber isotopic_centrifuge nutritional_liquifier antiprotonic_nucleosynthesizer".split(),
    "grandes": "digital_miner modification_station security_desk seismic_vibrator solar_neutron_activator pigment_mixer resistive_heater electric_pump fluidic_plenisher".split(),
    "lasers_qio": "laser laser_amplifier laser_tractor_beam logistical_sorter quantum_entangloporter qio_dashboard qio_drive_array qio_exporter qio_importer "
                  "qio_redstone_adapter".split(),
    "avulsos": "chargepad industrial_alarm radioactive_waste_barrel personal_barrel personal_chest cardboard_box".split(),
}
# A fileira com `active=true` (e a lava dentro, nos que a têm): o estado aceso e o fluido de dentro.
GRANDES_SPACING = {"grandes": 4, "quimica": 2, "lasers_qio": 2, "lasers": 2}


def rows():
    out = {}
    with open(RASTRO) as handle:
        for line in handle:
            row = json.loads(line)
            out[row["id"].split(":")[1]] = row
    return out


def state(row, active, lava=False):
    props = {p["name"]: p["values"] for p in row["properties"]}
    chosen = []
    if "facing" in props:
        chosen.append("facing=up" if "up" in props["facing"] and row["facing_placement"] == "SELECTED_FACE" else "facing=north")
    if "active" in props and active:
        chosen.append("active=true")
    if "fluid_logged" in props and lava:
        chosen.append("fluid_logged=lava")
    return "[" + ",".join(chosen) + "]" if chosen else ""


def shot(name, blocks, by_id, active=False, lava=False):
    spacing = GRANDES_SPACING.get(name.split("_ativos")[0].split("_lava")[0], 1)
    cmds = [f"setblock {X0 + i * spacing} {Y} {Z} mekanism:{block}{state(by_id[block], active, lava)}" for i, block in enumerate(blocks)]
    width = (len(blocks) - 1) * spacing
    cx = X0 + width / 2 + 0.5
    # A câmera olha o meio da fileira, de frente: com o campo de visão padrão (70 graus na vertical, 16:9) a fileira de `width` blocos cabe a uma distância de
    # cerca de 0,45 dela; a inclinação mira a metade da altura de um bloco (um pouco mais alto nos grandes).
    distance = max(3.5, width * 0.45 + 2.5)
    aim = Y + (1.6 if name.startswith(("grandes", "lasers_qio")) else 0.6)
    pitch = round(math.degrees(math.atan2(EYE - aim, distance)), 1)
    return {"name": name, "cmds": cmds, "tp": f"{cx} {EYE} {Z - distance} 0 {pitch}"}


def main():
    by_id = rows()
    shots = []
    for family, blocks in FAMILIES.items():
        shots.append(shot(family, blocks, by_id))
        shots.append(shot(family + "_ativos", blocks, by_id, active=True))
    shots.append(shot("quimica_lava", FAMILIES["quimica"], by_id, active=True, lava=True))
    spec = {
        "setup": ["difficulty peaceful", "time set 6000", "weather clear", "gamemode creative"]
        # `fill` aceita até 32.768 blocos: a área da cena em três fatias.
        + [f"fill {X0 - 8 + 24 * i} {Y} {Z - 30} {X0 + 15 + 24 * i} {Y + 8} {Z + 8} minecraft:air" for i in range(3)],
        "clear": f"fill {X0 - 8} {Y} {Z - 4} {X0 + 60} {Y + 8} {Z + 8} minecraft:air",
        "shots": shots,
    }
    with open(os.path.join(HERE, "blocos.json"), "w") as handle:
        json.dump(spec, handle, indent=1)
    print(len(shots), "tiros")


if __name__ == "__main__":
    main()
