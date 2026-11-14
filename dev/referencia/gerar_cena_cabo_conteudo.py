#!/usr/bin/env python3
"""Escreve `cenas/cabo_conteudo.json` (o original, `testes/CenaBlocos.java`) e `cenas/cabo_conteudo_port.json` (o port, `horizon2/h2_blocos.py cena`), o roteiro do conteúdo animado dos transmissores
(frente `cabo-conteudo`, Fase 4, decisões 212 a 214): quatro transmissores em fila (x 60 a 63, z 82) vistos pela mesma câmera, vazios, pela metade e cheios.

- O cabo: um Energy Cube básico a oeste (`#cubo`, todos os lados em saída e `#ejetar`, que liga o `eject` da energia, desligado por padrão) com a fração da capacidade da rede de quatro cabos (16.000 J, o cubo guarda 4.000.000): 0, metade e cheia.
- O cano: um Fluid Tank com água ao sul do primeiro (`#conexao ... south=pull`; a rede de quatro canos básicos guarda 8.000 mB e o cano básico puxa 250 por tick: o caldeirão só rende baldes inteiros, que o cano básico não puxa): 0, metade e cheia. O original recebe a água pelo NBT `fluid_tanks`, e o port pelo `#fluido` (o `machinery_basis.tanks` do tanque).
- O tubo: o original puxa de um Chemical Tank (o NBT `chemical_tanks`, hidrogênio), e o port, que ainda não tem o tanque, empurra pela sonda `probe_push` (`#quimico`, só na cópia de teste com `sonda`).
- O condutor: sem fonte de calor, só o ar do bioma (a cor do condutor no ambiente).
Os dois roteiros diferem só no que põe o químico no tubo."""
import json
import os

HERE = os.path.dirname(os.path.abspath(__file__))
Y = -60
Z = 82
X0 = 60
TUBO = 16000
CANO = 8000
CABO = 16000
CUBO = 4_000_000
LADOS = " ".join(f"{lado}=output" for lado in ("front", "left", "right", "back", "top", "bottom"))
CAMERA = "62.0 -59.3 79.6 0 25"
ESPERA = 80
NIVEIS = (("vazio", 0.0), ("meio", 0.5), ("cheio", 1.0))


def fila(bloco):
    return [f"setblock {X0 + i} {Y} {Z} mekanism:{bloco}" for i in range(4)]


def cabo(fracao):
    cmds = ["setblock 59 -60 82 mekanism:basic_energy_cube[facing=east]", *fila("basic_universal_cable"), "#esperar 20"]
    cmds.append(f"#cubo 59 {Y} {Z} {fracao * CABO / CUBO:.7f} {LADOS}")
    cmds.append(f"#ejetar 59 {Y} {Z}")
    return [*cmds, f"#esperar {ESPERA}"]


def cano(fracao, port):
    quantidade = int(fracao * CANO)
    cmds = fila("basic_mechanical_pipe")
    if quantidade == 0:
        return [*cmds, f"#esperar {ESPERA}"]
    cmds.append(f"setblock {X0} {Y} {Z + 1} mekanism:basic_fluid_tank")
    cmds.append("#esperar 20")
    if port:
        cmds.append(f"#fluido {X0} {Y} {Z + 1} minecraft:water {quantidade}")
    else:
        cmds.append(f'data merge block {X0} {Y} {Z + 1} {{fluid_tanks:[{{container:0b,stored:{{type:"minecraft:water",amount:{quantidade}}}}}]}}')
    cmds.append(f"#conexao {X0} {Y} {Z} south=pull")
    return [*cmds, f"#esperar {ESPERA}"]


def tubo(fracao, port):
    quantidade = int(fracao * TUBO)
    cmds = fila("basic_pressurized_tube")
    if quantidade == 0:
        return [*cmds, f"#esperar {ESPERA}"]
    if port:
        return [*cmds, "#esperar 20", f"#quimico {X0} {Y} {Z} mekanism:hydrogen {quantidade}", f"#esperar {ESPERA}"]
    cmds.append(f"setblock {X0} {Y} {Z + 1} mekanism:basic_chemical_tank[facing=north]")
    cmds += ["#esperar 20", f'data merge block {X0} {Y} {Z + 1} {{chemical_tanks:[{{container:0b,stored:{{type:"mekanism:hydrogen",amount:{quantidade}L}}}}]}}']
    cmds.append(f"#conexao {X0} {Y} {Z} south=pull")
    return [*cmds, f"#esperar {ESPERA}"]


def condutor():
    return [*fila("basic_thermodynamic_conductor"), f"#esperar {ESPERA}"]


def roteiro(port):
    shots = []
    for nome, fracao in NIVEIS:
        shots.append(("cabo_" + nome, cabo(fracao)))
        shots.append(("cano_" + nome, cano(fracao, port)))
        shots.append(("tubo_" + nome, tubo(fracao, port)))
    shots.append(("condutor", condutor()))
    # Os mesmos cabo e cano cheios, 7 ticks depois: a animação do sprite (`mek_liquid/energy`, `water_still`) tem de mudar o quadro nos dois jogos.
    shots.append(("cabo_cheio_b", cabo(1.0) + ["#esperar 7"]))
    shots.append(("cano_cheio_b", cano(1.0, port) + ["#esperar 7"]))
    return {
        "setup": [
            "difficulty peaceful",
            "time set 6000",
            "weather clear",
            "gamemode creative",
            "fill 52 -60 52 75 -52 90 minecraft:air",
        ],
        "clear": "fill 52 -60 78 70 -52 90 minecraft:air",
        "shots": [{"name": name, "cmds": cmds, "tp": CAMERA} for name, cmds in shots],
    }


def main():
    for nome, port in (("cabo_conteudo.json", False), ("cabo_conteudo_port.json", True)):
        with open(os.path.join(HERE, "cenas", nome), "w", encoding="utf-8") as arquivo:
            json.dump(roteiro(port), arquivo, indent=1, ensure_ascii=False)
            arquivo.write("\n")


if __name__ == "__main__":
    main()
