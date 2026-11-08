#!/usr/bin/env python3
"""Roda no container `horizon` (H_NAME=horizon) os cenários de `cenas/som_medida.txt`, os mesmos que o original roda com `testes/CenaSomMedida.java`, e
guarda o que o som do port escreveu no stderr (`MEKSOUND ...`, só nas cópias de teste com `TRACE = true` em `tile_sound.rs`).

Cada cenário: escreve o `mekanism/client.toml` do jogo (o Horizon relê o arquivo em até 1 s), põe a máquina como um jogador (o item dá a capacidade), instala o
silêncio se o cenário pede, põe o jogador onde ele tem de estar (os PÉS, relativos ao centro do bloco), liga com `data merge` (energia e item na entrada)
e desliga zerando a energia, nos instantes do cenário (20 ticks = 1 s), e recorta do log o que saiu entre o começo e o fim.

Uso: H_NAME=horizon h2_som_medida.py CENARIOS LOG_NO_DESKTOP SAIDA [nome ...]
  CENARIOS       cenas/som_medida.txt
  LOG_NO_DESKTOP o arquivo para onde o jogo escreve o stderr (no desktop)
  SAIDA          o arquivo no notebook: um bloco `=== nome` por cenário
Cenários que mudam o silêncio com o jogo andando (`ev=T:muffling:N`) só valem para subir (o item de upgrade em cima da máquina); tirar o upgrade passa pela tela.
"""
import json, os, shlex, subprocess, sys, time

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import h2_identity as h

POS = (70, -60, 60)
CENTER = (POS[0] + 0.5, POS[1] + 0.5, POS[2] + 0.5)
CONFIG = "~/.local/share/jogo-isolado/horizon/data/horizon/config/mekanism/client.toml"
BLOCK = {
    "chamber": "mekanism:enrichment_chamber",
    "crusher": "mekanism:crusher",
    "smelter": "mekanism:energized_smelter",
    "infuser": "mekanism:metallurgic_infuser",
}
# Onde o item de entrada vai (o slot do menu de cada máquina) e qual item entra.
ENTRADA = {
    "chamber": (0, "minecraft:coal"),
    "crusher": (0, "minecraft:cobblestone"),
    "smelter": (0, "minecraft:raw_iron"),
    "infuser": (1, "minecraft:iron_ingot"),
}
ENERGIA = {"chamber": 1000, "crusher": 1600, "smelter": 1000, "infuser": 1000}
TICK = 0.05


def ssh(text):
    return subprocess.run(["ssh", "desktop", text], capture_output=True, text=True).stdout


def silenciar():
    """Regra: o jogo de teste nunca toca nas caixas do usuário. Sem `SOM_NULO=1` (o stream já ligado a um sink nulo por `audio_nulo.sh`), o volume mestre vai a 0."""
    if os.environ.get("SOM_NULO") != "1":
        h.ctl("options-set", "soundCategory_master", "0")


def parse(path):
    out = []
    for line in open(path, encoding="utf-8"):
        line = line.strip()
        if not line or line.startswith("#"):
            continue
        sc = {"nome": "x", "maquina": "chamber", "muffling": 0, "base": 1.0, "enable": True, "jogador": (3.0, -0.5, 0.0), "liga": 20, "desliga": 200, "fim": 260, "eventos": []}
        for word in line.split():
            key, _, value = word.partition("=")
            if key in ("muffling", "liga", "desliga", "fim"):
                sc[key] = int(value)
            elif key == "base":
                sc[key] = float(value)
            elif key == "enable":
                sc[key] = value == "true"
            elif key == "jogador":
                sc[key] = tuple(float(p) for p in value.split(","))
            elif key == "ev":
                sc["eventos"].append(value.split(":", 2))
            elif key in ("nome", "maquina"):
                sc[key] = value
        out.append(sc)
    return out


def config_text(base, enable):
    return (
        "#Settings for configuring Mekanism's Sounds\n[sounds]\n\t#Adjust Mekanism sounds' base volume. < 1 is softer, higher is louder.\n"
        f"\t# Default: 1.0\n\t# Range: 0.0 ~ 10.0\n\tbaseVolume = {base!r}\n\t#Play sounds for active machines.\n\tenableMachine = {str(enable).lower()}\n"
    )


def write_config(base, enable):
    ssh(f"mkdir -p $(dirname {CONFIG}) && printf %s {shlex.quote(config_text(base, enable))} > {CONFIG}")


def feet(sc, offset=None):
    o = offset or sc["jogador"]
    return (CENTER[0] + o[0], CENTER[1] + o[1], CENTER[2] + o[2])


def stand(offset):
    """Põe o jogador nos pés dados; no ar, um bloco de barreira embaixo o segura."""
    x, y, z = offset
    if y > POS[1] + 1:
        h.command(f"setblock {int(x // 1)} {int(y // 1) - 1} {int(z // 1)} minecraft:barrier")
    h.command(f"tp @s {x:.3f} {y:.3f} {z:.3f} 0 12")


def place(machine):
    h.command("tick unfreeze")
    h.command(f"setblock {POS[0]} {POS[1]} {POS[2]} minecraft:air")
    h.ctl("close")
    h.ctl("tp", str(POS[0]), str(POS[1]), str(POS[2] - 3))
    h.ctl("wait", "5")
    h.command("clear @s")
    h.ctl("give", BLOCK[machine], "1")
    out = json.loads(h.ctl("place", *map(str, POS), "--item", BLOCK[machine]))
    assert "error" not in out, out
    h.command("clear @s")


def add_muffling():
    """O item de upgrade em cima da máquina: o que um jogador faz (`ItemUpgrade`)."""
    h.command('give @s mekanism:upgrade[mekanism:upgrade_type="mekanism:muffling"] 1')
    time.sleep(0.4)
    h.ctl("key", "shift", "down")
    h.ctl("interact", *map(str, POS))
    h.ctl("key", "shift", "up")


# A janela de upgrades (veja `cena_telas.py`): a aba, a linha do silêncio na lista e o botão Uninstall, em pixels de GUI relativos ao canto da tela de 176 por 166.
ABA, LINHA, UNINSTALL = (190, 19), (29, 40), (132, 75)
GUI_SCALE = 3  # o `guiScale` do options.txt do container


def remove_muffling():
    """O botão Uninstall da janela de upgrades (`PacketRemoveUpgrade`): é o único caminho do jogador que tira o upgrade."""
    h.ctl("interact", *map(str, POS))
    h.ctl("wait", "60")
    left = ((-(-1920 // GUI_SCALE)) - 176) // 2
    top = ((-(-1080 // GUI_SCALE)) - 166) // 2
    for x, y in (ABA, LINHA, UNINSTALL):
        h.ctl("click-at", str(left + x), str(top + y))
        h.ctl("wait", "6")
    h.ctl("close")


def power(machine, on):
    slot, item = ENTRADA[machine]
    pos = " ".join(map(str, POS))
    if on:
        extra = ""
        if machine == "infuser":
            # A moeda de carvão no slot de infusão não converte neste port ao ser posta por `data merge` (o original converte); o tanque cheio de carbono faz a máquina andar.
            extra = ',chemical_tanks:[{container:0b,stored:{type:"mekanism:carbon",amount:1000L}}]'
        h.command(f'data merge block {pos} {{energy:{ENERGIA[machine]},Items:[{{Slot:{slot}b,id:"{item}",count:64}}]{extra}}}')
    else:
        h.command(f"data merge block {pos} {{energy:0}}")


def run(sc, log):
    machine = sc["maquina"]
    write_config(sc["base"], sc["enable"])
    time.sleep(2.5)
    place(machine)
    if sc["muffling"]:
        for _ in range(sc["muffling"]):
            add_muffling()
        time.sleep(1.5)
    h.ctl("close")
    stand(feet(sc))
    time.sleep(1.0)
    start = int(ssh(f"stat -c %s {log}"))
    t0 = time.monotonic()
    pending = sorted([(int(e[0]) * TICK, e) for e in sc["eventos"]] + [(sc["liga"] * TICK, ["", "liga", ""]), (sc["desliga"] * TICK, ["", "desliga", ""])], key=lambda p: p[0])
    for when, ev in pending:
        wait = t0 + when - time.monotonic()
        if wait > 0:
            time.sleep(wait)
        what, value = ev[1], ev[2]
        print(f"  {time.monotonic() - t0:6.2f}s {what} {value}", flush=True)
        if what == "liga":
            power(machine, True)
        elif what == "desliga":
            power(machine, False)
        elif what == "muffling":
            if int(value) > 0:
                add_muffling()
            else:
                remove_muffling()
            print("   upgrades:", h.command(f"data get block {' '.join(map(str, POS))} component_upgrade")[0][-90:], flush=True)
        elif what == "base":
            write_config(float(value), sc["enable"])
            sc["base"] = float(value)
        elif what == "enable":
            write_config(sc["base"], value == "true")
            sc["enable"] = value == "true"
        elif what == "jogador":
            stand(feet(sc, tuple(float(p) for p in value.split(";"))))
        elif what == "tick":
            h.command(f"tick {value}")
        elif what == "energia":
            power(machine, value == "1")
    wait = t0 + sc["fim"] * TICK - time.monotonic()
    if wait > 0:
        time.sleep(wait)
    h.command("tick unfreeze")
    time.sleep(1.0)
    end = int(ssh(f"stat -c %s {log}"))
    h.command(f"setblock {POS[0]} {POS[1]} {POS[2]} minecraft:air")
    return ssh(f"tail -c +{start + 1} {log} | head -c {end - start} | grep -a MEKSOUND")


def main():
    scenarios = parse(sys.argv[1])
    only = sys.argv[4:]
    out = open(sys.argv[3], "w", encoding="utf-8")
    # O arquivo guarda também a posição da janela de upgrades: o conteúdo de antes volta no fim.
    antes = ssh(f"cat {CONFIG} 2>/dev/null")
    silenciar()
    h.ctl("options-set", "soundCategory_music", "0")
    h.command("gamemode creative")
    for sc in scenarios:
        if only and sc["nome"] not in only:
            continue
        print(sc["nome"], flush=True)
        text = run(sc, sys.argv[2])
        out.write(f"=== {sc['nome']}\n{text}\n")
        out.flush()
    ssh(f"printf %s {shlex.quote(antes)} > {CONFIG}")


if __name__ == "__main__":
    main()
