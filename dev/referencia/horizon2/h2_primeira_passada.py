#!/usr/bin/env python3
"""A rede na primeira passada do servidor (frente `rede-primeira-passada`, decisão 195): o roteiro de abrir o mundo no port, com as sondas dos primeiros ticks, e o vídeo do leitor de rede logo
depois de abrir.

    H_NAME=horizon2 PASTA=<pasta do game.log> h2_primeira_passada.py roteiro SAIDA.jsonl
    SOCK=<nome do gravar-jogo> H_NAME=horizon2 h2_primeira_passada.py video

O `roteiro` monta as cinco famílias do `h2_reset_mundo.py` no `ResetA`, e reabre o mundo várias vezes, a primeira e a terceira com a injeção `owner_tickets` ligada e a segunda com ela
desligada (o arquivo `/tmp/mb_sem_primeira_passada` no container, que só a cópia de teste lê: `TRANSMITTER_PROBE`), depois repete com o Nether forçado (três cabos) e abre um segundo mundo.
Cada abertura vira uma linha de SAIDA.jsonl com as linhas `MBT`, `MBE`, `MBP`, `MTN` e `MBQ` das primeiras passadas depois do `open_ready`, as mesmas que a `CenaReset` do original grava
em cada um dos primeiros ticks (`tick_pre` e `tick`). O jogo já tem de estar no título (`abrir_som_plano.sh titulo`), com a cópia de teste (`enviar_fontes.sh`).
"""
import json, os, subprocess, sys, time

TOOLS = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, TOOLS)
os.environ.setdefault("H_NAME", "horizon2")
import h2_reset_mundo as base
from h2_reset_mundo import CABLE, Y, abrir, cmd, ctl, encher, log_since, log_total, montar, parse, sair, ssh, state, wait

FLAG = "/tmp/mb_sem_primeira_passada"
PASSADAS = 6
FAMILIAS = ("MBE", "MBP", "MTN", "MBQ")


def sem_injecao(ligado):
    """A cópia de teste lê o arquivo: com ele a injeção `owner_tickets` deixa o corpo original (o port de antes)."""
    ssh(f"docker exec {os.environ['H_NAME']} {'touch' if ligado else 'rm -f'} {FLAG}")


def agora_ms():
    out = ssh("date +%s%3N").strip()
    return int(out) if out.isdigit() else 0


def abertura(saida, rotulo, nome, baseline):
    """Reabre o mundo `nome` e grava as primeiras passadas depois do `open_ready`."""
    sem_injecao(baseline)
    marca = log_total()
    inicio = agora_ms()
    abrir(nome)
    wait(30)
    lines = log_since(marca)
    # A ordem das linhas é a ordem do servidor: o que vem depois do `open_ready` é a primeira passada em diante; cada `pre` abre uma passada.
    passadas = [{"k": 0, "mbt": [], **{f: [] for f in FAMILIAS}}]
    achou = False
    open_ready_ms = None
    for line in lines:
        prefix, _, rest = line.partition(" ")
        if prefix not in ("MBT",) + FAMILIAS:
            continue
        try:
            row = json.loads(rest)
        except ValueError:
            continue
        if prefix == "MBT" and row.get("e") == "open_ready" and not achou:
            achou = True
            open_ready_ms = row.get("ms")
            continue
        if not achou:
            continue
        if prefix == "MBT" and row.get("e") == "pre":
            if len(passadas) > PASSADAS:
                break
            passadas.append({"k": len(passadas), "mbt": [], **{f: [] for f in FAMILIAS}})
        atual = passadas[-1]
        if prefix == "MBT":
            if row.get("e") not in ("neighbor", "cap", "idle_step"):
                atual["mbt"].append(row)
        else:
            atual[prefix].append(row)
    row = {"e": "abertura", "rotulo": rotulo, "mundo": nome, "baseline": baseline, "inicio_ms": inicio, "open_ready_ms": open_ready_ms, "passadas": passadas}
    saida.write(json.dumps(row) + "\n")
    saida.flush()
    estados = [e for p in passadas for e in p["mbt"] if e.get("e") == "estado"]
    print(rotulo, "baseline" if baseline else "injecao", "open_ready", open_ready_ms, "estados", [(e["t"], e["jogadores"], e["carregados"], e["anda00"]) for e in estados][:4], flush=True)


def nether():
    """Três cabos num chunk forçado do Nether, com energia, como o `a4_nether_cabo` do roteiro do original."""
    cmd("execute in minecraft:the_nether run forceload add 0 0 0 0")
    wait(40)
    for x in range(3):
        cmd(f"execute in minecraft:the_nether run setblock {x} 70 0 {CABLE}")
    wait(40)
    cmd("execute in minecraft:the_nether run data merge block 0 70 0 {energy:555}")
    wait(20)


def roteiro(caminho):
    if state().get("world") != "ResetA":
        for nome in ("ResetA", "ResetB"):
            ctl("world-delete", nome, "--confirm")
        import h2_mundo_plano as mp
        sys.argv = ["h2_mundo_plano.py", "ResetA", "20261113"]
        mp.main()
    saida = open(caminho, "w")
    sem_injecao(False)
    montar()
    encher()
    sair()
    # As aberturas 2 e 4 da `CenaReset` do original: o mesmo mundo, sem e com o Nether forçado.
    abertura(saida, "a_injecao_1", "ResetA", False)
    sair()
    abertura(saida, "a_base_1", "ResetA", True)
    sair()
    abertura(saida, "a_injecao_2", "ResetA", False)
    nether()
    sair()
    abertura(saida, "n_injecao_1", "ResetA", False)
    sair()
    abertura(saida, "n_base_1", "ResetA", True)
    sair()
    abertura(saida, "n_injecao_2", "ResetA", False)
    sair()
    # O segundo mundo (a abertura 5 do original): uma rede de três cabos com 777.
    import h2_mundo_plano as mp
    sys.argv = ["h2_mundo_plano.py", "ResetB", "20261114"]
    mp.main()
    cmd("gamemode creative @s")
    cmd("tp @s 2.5 -60 -6")
    wait(20)
    for x in range(3):
        cmd(f"setblock {x} {Y} 0 {CABLE}")
    wait(60)
    cmd(f"data merge block 0 {Y} 0 {{energy:777}}")
    wait(10)
    sair()
    abertura(saida, "b_injecao_1", "ResetB", False)
    sair()
    abertura(saida, "b_base_1", "ResetB", True)
    sem_injecao(False)
    sair()
    saida.close()


def video():
    """O vídeo com som de abrir o mundo e o leitor de rede no cabo logo no começo. O jogo já tem de estar no título, com `SOCK` do `gravar-jogo start` e o sink conferido por `audio_nulo.sh`."""
    import h2_ajustes_jogo as a
    import h2_network_reader as nr
    YV = -60
    sock = os.environ["SOCK"]
    nome = os.environ.get("NOME", "primeira_passada")
    local = os.path.expanduser(os.environ.get("LOCAL", "~/.cache/rust-mekanism/wa/rede-primeira-passada"))
    a.SHOTS = "/home/luka/.cache/rust-mekanism/rede-primeira-passada/prints"
    a.LOCAL = local
    stand = (2.5, -60.0, -2.4)
    if state().get("world") != "ResetV":
        ctl("world-delete", "ResetV", "--confirm")
        import h2_mundo_plano as mp
        sys.argv = ["h2_mundo_plano.py", "ResetV", "20261115"]
        mp.main()
    cmd("gamemode creative @s")
    cmd("clear @s")
    cmd("weather clear")
    ctl("time", "6000")
    cmd("gamerule advance_time false")
    cmd("gamerule advance_weather false")
    ctl("options-set", "chatVisibility", "0")
    ctl("options-set", "guiScale", "3")
    cmd("fill 0 -61 0 4 -61 0 minecraft:grass_block")
    cmd("fill 0 -60 0 4 -60 0 minecraft:air")
    wait(40)
    for x in range(5):
        cmd(f"setblock {x} {YV} 0 {CABLE}")
    wait(40)
    cmd(f"data merge block 0 {YV} 0 {{energy:12345}}")
    wait(40)
    ctl("give", nr.READER, "1")
    wait(10)
    ctl("select", nr.READER)
    cmd("tp @s %.2f %.2f %.2f" % stand)
    wait(120)
    sair()
    a.CROP = "1000x300+0+680"
    for categoria in ("master", "block", "ui", "player"):
        ctl("options-set", f"soundCategory_{categoria}", "1.0")
    proc = subprocess.Popen(["ssh", "desktop", f"~/.cache/rust-mekanism/gravar-jogo rec {sock} {nome}.mp4 30 {os.environ['H_NAME']}"], stdout=subprocess.DEVNULL)
    time.sleep(3.0)
    try:
        abrir("ResetV")
        ctl("select", nr.READER)
        nr.clicar((2, YV, 0), (0.5, 0.5, 0.5), sneak=False, stand=stand)
        depois = a.shot("primeira_passada_leitor")
        wait(40)
        proc.wait()
    finally:
        ctl("options-set", "soundCategory_master", "0.0")
    return depois


def main():
    what = sys.argv[1] if len(sys.argv) > 1 else ""
    if what == "video":
        video()
    elif what == "roteiro":
        roteiro(sys.argv[2])
    elif what == "limpar":
        for nome in ("ResetA", "ResetB", "ResetV"):
            ctl("world-delete", nome, "--confirm")
    else:
        raise SystemExit("uso: h2_primeira_passada.py roteiro SAIDA.jsonl | video | limpar")


if __name__ == "__main__":
    main()
