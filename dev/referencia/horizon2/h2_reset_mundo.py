#!/usr/bin/env python3
"""O mundo que sai e volta no port (frente `reset-mundo`, decisão 183 (c)): o mesmo roteiro da `CenaReset.java` do original (`testes/CenaReset.java`).

    H_NAME=horizon2 PASTA=<pasta do game.log> h2_reset_mundo.py roteiro SAIDA.jsonl

O jogo tem de estar aberto no mundo `ResetA` (superplano, criado por `h2_mundo_plano.py ResetA 20261113`, a partir do título: `abrir_som_plano.sh titulo`; o `ResetB` o roteiro cria). O roteiro monta as cinco
famílias (cabo, cano, tubo, condutor e transportador, nas mesmas posições da cena do original), enche cada rede, sai para o título pelo menu de pausa, reabre o mesmo mundo, põe um cabo na
ponta da linha, vai ao Nether e volta, sai com o Nether carregado, cria o `ResetB`, volta ao `ResetA`, abre o `ResetB`, e sai. A cada passo escreve uma linha em SAIDA.jsonl com o que as sondas
do stderr do jogo (`TRANSMITTER_PROBE = true` na cópia de teste: `MBE` energia, `MBP` fluido, `MTN` químico, `MBQ` calor, `MBI` transportador, `MBT` os eventos e o `reset`) dão desde o passo
anterior (a última linha de cada rede) e o NBT dos blocos que importam.
"""
import json, os, re, subprocess, sys, time

TOOLS = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, TOOLS)
os.environ.setdefault("H_NAME", "horizon2")
import h2_identity as h

PASTA = os.environ.get("PASTA", "~/.cache/rust-mekanism/reset-b1")
Y = -61
CABLE, PIPE, TUBE = "mekanism:basic_universal_cable", "mekanism:basic_mechanical_pipe", "mekanism:basic_pressurized_tube"
CONDUCTOR, TRANSPORTER = "mekanism:basic_thermodynamic_conductor", "mekanism:basic_logistical_transporter"


def cmd(text):
    try:
        return h.command(text)[0]
    except (KeyError, IndexError):
        return ""


def ctl(*args, check=True):
    return h.ctl(*args)


def wait(n):
    ctl("wait", str(n))


def ssh(command):
    return subprocess.run(["ssh", "desktop", command], capture_output=True, text=True, timeout=180).stdout


def state():
    try:
        return json.loads(ctl("state"))
    except ValueError:
        return {}


def log_total():
    out = ssh(f"wc -l < {PASTA}/game.log").strip()
    return int(out) if out.isdigit() else 0


def log_since(start):
    """As linhas do stderr do jogo depois da linha `start`."""
    return ssh(f"tail -n +{start + 1} {PASTA}/game.log").splitlines()


def parse(lines, prefix):
    rows = []
    for line in lines:
        if line.startswith(prefix + " "):
            try:
                rows.append(json.loads(line[len(prefix) + 1:]))
            except ValueError:
                pass
    return rows


def resumo(lines):
    """A última linha de cada sonda de rede e o que a sonda de eventos viu (contagem por tipo, e `reset` com o número da linha)."""
    out = {}
    for name, prefix in (("energia", "MBE"), ("fluido", "MBP"), ("quimico", "MTN"), ("calor", "MBQ")):
        rows = parse(lines, prefix)
        out[name] = {"linhas": len(rows), "ultima": rows[-1] if rows else None}
    transit = parse(lines, "MBI")
    out["transportador"] = {"linhas": len(transit), "ultimas": list({json.dumps(r["p"]): r for r in transit}.values())[-9:]}
    events = {}
    for row in parse(lines, "MBT"):
        events[row.get("e", "?")] = events.get(row.get("e", "?"), 0) + 1
    out["eventos"] = events
    out["log_reset"] = [i for i, line in enumerate(lines) if line.startswith("MBT ") and '"e":"reset"' in line]
    return out


class Roteiro:
    def __init__(self, saida):
        self.saida = open(saida, "w")
        self.marca = log_total()

    def snap(self, rotulo, extra=None):
        total = log_total()
        lines = log_since(self.marca)
        self.marca = total
        row = {"e": "snap", "rotulo": rotulo, "estado": {k: v for k, v in state().items() if k in ("world", "dimension", "screen")}, "sondas": resumo(lines), "nbt": self.nbt()}
        if extra:
            row.update(extra)
        self.saida.write(json.dumps(row) + "\n")
        self.saida.flush()
        print("snap", rotulo, json.dumps(row["sondas"]["energia"]["ultima"]), json.dumps(row["sondas"]["fluido"]["ultima"]), flush=True)

    def nbt(self):
        out = {}
        for nome, pos in (("cabo0", (0, Y, 0)), ("cano0", (0, Y, 2)), ("tubo0", (0, Y, 4)), ("condutor0", (0, Y, 6)), ("transportador1", (1, Y, 8))):
            out[nome] = cmd("data get block %d %d %d" % pos)[:600]
        return out

    def evento(self, rotulo):
        self.saida.write(json.dumps({"e": rotulo}) + "\n")
        self.saida.flush()


def montar():
    cmd("gamerule advance_time false")
    cmd("gamerule advance_weather false")
    cmd("time set 6000")
    cmd("weather clear")
    cmd("gamemode creative @s")
    cmd("tp @s 2.5 -60 -6")
    ctl("wait", "20")
    for x in range(5):
        cmd(f"setblock {x} {Y} 0 {CABLE}")
        cmd(f"setblock {x} {Y} 2 {PIPE}")
        cmd(f"setblock {x} {Y} 4 {TUBE}")
        cmd(f"setblock {x} {Y} 6 {CONDUCTOR}")
    for x in range(1, 9):
        cmd(f"setblock {x} {Y} 8 {TRANSPORTER}")
    cmd(f"setblock 9 {Y} 8 minecraft:chest")
    wait(60)


def encher():
    cmd(f"data merge block 0 {Y} 0 {{energy:12345}}")
    cmd(f'data merge block 0 {Y} 2 {{probe_push:{{fluid:"minecraft:water",amount:1500}}}}')
    cmd(f'data merge block 0 {Y} 4 {{probe_push:{{chem:"mekanism:hydrogen",amount:2500}}}}')
    # O calor: o Heat Generator em cima do último condutor, com 4000 K (como a cena do original).
    cmd(f"setblock 4 {Y + 1} 6 mekanismgenerators:heat_generator")
    wait(10)
    cmd(f"data merge block 4 {Y + 1} 6 {{heat:4000.0d}}")
    # Os itens: um baú sobre um funil sobre o primeiro transportador (um item a cada 8 ticks).
    cmd(f"setblock 1 {Y + 1} 8 minecraft:hopper[facing=down]")
    cmd(f"setblock 1 {Y + 2} 8 minecraft:chest")
    wait(10)
    cmd(f"container 1 {Y + 2} 8 minecraft:cobblestone 64")
    wait(60)


def sair():
    ctl("pause")
    time.sleep(1.0)
    ctl("click-button", "Save and Quit to Title")
    for _ in range(120):
        if state().get("menu"):
            break
        time.sleep(0.5)
    time.sleep(2.0)


def abrir(nome):
    ctl("open", nome)
    for _ in range(240):
        if state().get("world") == nome:
            break
        time.sleep(0.5)
    ctl("wait-ready")


def roteiro(saida):
    if state().get("world") != "ResetA":
        abrir("ResetA")
    r = Roteiro(saida)
    # Sempre com o servidor andando; o mundo A já está aberto.
    montar()
    r.snap("a0_montado")
    encher()
    r.snap("a1_antes_de_sair")
    r.evento("saindo_a1")
    sair()
    r.snap("a2_menu_t1")
    time.sleep(10)
    r.snap("a2_menu_t10s")
    abrir("ResetA")
    wait(2)
    r.snap("a3_reaberto_t1")
    wait(10)
    r.snap("a3_reaberto_t10")
    wait(30)
    r.snap("a3_reaberto_t40")
    wait(60)
    r.snap("a3_reaberto_t100")
    # Um cabo novo na ponta da linha de energia: a rede refeita se une.
    cmd(f"setblock 5 {Y} 0 {CABLE}")
    wait(20)
    r.snap("a3_cabo_novo")
    # Outra dimensão: três cabos no Nether (chunk forçado), o jogador vai e volta.
    cmd("execute in minecraft:the_nether run forceload add 0 0 0 0")
    wait(40)
    for x in range(3):
        cmd(f"execute in minecraft:the_nether run setblock {x} 70 0 {CABLE}")
    wait(40)
    cmd(f"execute in minecraft:the_nether run data merge block 0 70 0 {{energy:555}}")
    wait(20)
    r.snap("a4_nether_cabo")
    cmd("execute in minecraft:the_nether run tp @s 0.5 100 0.5")
    wait(60)
    r.snap("a4_no_nether")
    cmd("execute in minecraft:overworld run tp @s 2.5 -60 -6")
    wait(60)
    r.snap("a4_de_volta")
    r.evento("saindo_a4")
    sair()
    r.snap("a5_menu")
    # O segundo mundo, criado do título pela tela de criar mundo, como o `ResetA`.
    import h2_mundo_plano as mp
    sys.argv = ["h2_mundo_plano.py", "ResetB", "20261114"]
    mp.main()
    segunda_parte(r)
    return r


def segunda_parte(r):
    cmd("gamerule advance_time false")
    cmd("gamemode creative @s")
    cmd("tp @s 2.5 -60 -6")
    wait(20)
    for x in range(3):
        cmd(f"setblock {x} {Y} 0 {CABLE}")
    wait(60)
    cmd(f"data merge block 0 {Y} 0 {{energy:777}}")
    wait(10)
    r.snap("b1_novo")
    r.evento("saindo_b1")
    sair()
    r.snap("b2_menu")
    abrir("ResetA")
    for t, n in ((2, "a6_reaberto_t1"), (38, "a6_reaberto_t40"), (80, "a6_reaberto_t120")):
        wait(t)
        r.snap(n)
    # O Nether com o chunk forçado, depois de reabrir: o dono dele anda sem o jogador? e o que volta quando o jogador chega?
    r.snap("a6_nether_forcado", {"forceload": cmd("execute in minecraft:the_nether run forceload query 0 0")})
    cmd("execute in minecraft:the_nether run tp @s 0.5 100 0.5")
    wait(60)
    r.snap("a6_no_nether")
    cmd("execute in minecraft:overworld run tp @s 2.5 -60 -6")
    wait(40)
    r.snap("a6_de_volta")
    r.evento("saindo_a6")
    sair()
    abrir("ResetB")
    wait(60)
    r.snap("b3_reaberto_t60")
    sair()
    r.snap("fim_menu")
    r.evento("fim")


def main():
    what = sys.argv[1] if len(sys.argv) > 1 else ""
    if what == "roteiro":
        roteiro(sys.argv[2])
        return
    raise SystemExit("uso: h2_reset_mundo.py roteiro SAIDA.jsonl")


if __name__ == "__main__":
    main()
