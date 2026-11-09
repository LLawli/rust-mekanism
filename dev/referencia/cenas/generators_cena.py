#!/usr/bin/env python3
"""Escreve `generators.json`, o roteiro da cena de comparação dos blocos do Mekanism Generators: o mesmo para o original (`testes/CenaBlocos.java`, por `oficial_blocos.sh`) e para o port
(`horizon2/h2_generators.py cena`).

Mesmo formato de `blocos.json` (`blocos_cena.py`): cada tiro monta uma fileira de blocos com `setblock` (os estados têm os mesmos nomes nos dois jogos), põe a câmera à frente e pede o
print. O roteiro sai dos blocos que o original em jogo registra (`rastros/blocos_generators.jsonl`): cada bloco só recebe as propriedades que tem.
"""
import json
import math
import os

HERE = os.path.dirname(os.path.abspath(__file__))
RASTRO = os.path.join(HERE, "..", "rastros", "blocos_generators.jsonl")
X0, Y, Z = 60, -60, 82
EYE = Y + 1.62
NS = "mekanismgenerators"

FAMILIES = {
    # O `heat_generator` fica fora: o port o tem com o gabarito do forno (`lit` em vez de `active`, Fase 5) e o `comparar_blocos.py` o conferiu na frente dele.
    "geradores": "solar_generator gas_burning_generator bio_generator".split(),
    "geradores_grandes": "advanced_solar_generator wind_generator".split(),
    "turbina": "turbine_rotor rotational_complex electromagnetic_coil turbine_casing turbine_valve turbine_vent saturating_condenser".split(),
    "fissao": "fission_reactor_casing fission_reactor_port fission_reactor_port fission_reactor_port fission_reactor_logic_adapter fission_fuel_assembly "
              "control_rod_assembly".split(),
    "fusao": "fusion_reactor_controller fusion_reactor_controller fusion_reactor_frame fusion_reactor_port fusion_reactor_port fusion_reactor_logic_adapter "
             "laser_focus_matrix reactor_glass".split(),
}
SPACING = {"geradores_grandes": 4}
# O `mode` de cada uma das três portas do reator de fissão, em ordem.
PORT_MODES = ["input", "output_waste", "output_coolant"]


def rows():
    out = {}
    with open(RASTRO) as handle:
        for line in handle:
            row = json.loads(line)
            out[row["id"].split(":")[1]] = row
    return out


def state(row, active, lava, nth):
    props = {p["name"]: p["values"] for p in row["properties"]}
    chosen = []
    if "facing" in props:
        chosen.append("facing=north")
    if "active" in props:
        # O controlador e a porta de fusão aparecem nos dois estados na mesma fileira (o primeiro apagado, o segundo aceso), os geradores seguem o tiro.
        chosen.append(f"active={'true' if active else 'false'}")
    if "mode" in props:
        chosen.append(f"mode={PORT_MODES[nth % 3]}")
    if "fluid_logged" in props and lava:
        chosen.append("fluid_logged=lava")
    return "[" + ",".join(chosen) + "]" if chosen else ""


def shot(name, blocks, by_id, active=False, lava=False, alternate=False):
    spacing = SPACING.get(name.split("_ativos")[0].split("_lava")[0], 1)
    cmds = []
    seen = {}
    for i, block in enumerate(blocks):
        nth = seen.get(block, 0)
        seen[block] = nth + 1
        # `alternate`: o segundo bloco repetido da fileira vem aceso.
        cmds.append(f"setblock {X0 + i * spacing} {Y} {Z} {NS}:{block}{state(by_id[block], active or (alternate and nth == 1), lava, nth)}")
    width = (len(blocks) - 1) * spacing
    cx = X0 + width / 2 + 0.5
    distance = max(3.5, width * 0.45 + 2.5)
    aim = Y + (2.0 if name.startswith("geradores_grandes") else 0.6)
    pitch = round(math.degrees(math.atan2(EYE - aim, distance)), 1)
    return {"name": name, "cmds": cmds, "tp": f"{cx} {EYE} {Z - distance} 0 {pitch}"}


def glass_shot():
    """O vidro do reator e a matriz de foco do laser: uma parede 3 por 3 de vidro com a matriz no meio, diante de uma parede de tábuas (a translucidez, o corte de faces entre eles)."""
    cmds = [
        f"fill {X0 - 1} {Y} {Z + 1} {X0 + 3} {Y + 3} {Z + 1} minecraft:oak_planks",
        f"fill {X0} {Y} {Z} {X0 + 2} {Y + 2} {Z} {NS}:reactor_glass",
        f"setblock {X0 + 1} {Y + 1} {Z} {NS}:laser_focus_matrix",
    ]
    return {"name": "vidro", "cmds": cmds, "tp": f"{X0 + 1.5} {EYE + 0.5} {Z - 5} 0 8"}


def main():
    by_id = rows()
    shots = []
    for family, blocks in FAMILIES.items():
        shots.append(shot(family, blocks, by_id, alternate=family == "fusao"))
        if any("active" in {p["name"] for p in by_id[b]["properties"]} for b in blocks) and family != "fusao":
            shots.append(shot(family + "_ativos", blocks, by_id, active=True))
    shots.append(shot("geradores_lava", FAMILIES["geradores"], by_id, active=True, lava=True))
    shots.append(glass_shot())
    spec = {
        "setup": ["difficulty peaceful", "time set 6000", "weather clear", "gamemode creative"]
        + [f"fill {X0 - 8 + 24 * i} {Y} {Z - 30} {X0 + 15 + 24 * i} {Y + 8} {Z + 8} minecraft:air" for i in range(3)],
        "clear": f"fill {X0 - 8} {Y} {Z - 4} {X0 + 60} {Y + 8} {Z + 8} minecraft:air",
        "shots": shots,
    }
    with open(os.path.join(HERE, "generators.json"), "w") as handle:
        json.dump(spec, handle, indent=1)
    print(len(shots), "tiros")


if __name__ == "__main__":
    main()
