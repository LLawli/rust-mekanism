#!/usr/bin/env python3
"""O custo por tick dos transmissores no port (frente `rede-desempenho`, decisão 202): a construção de `cenas/setups_reais.txt` (cinco pistas, cerca de 1.100 transmissores), o ritmo do servidor com tudo e
tirando as pistas uma a uma, e o perfil por etapa que a cópia de teste escreve (`MBF`, `enviar_fontes.sh WS PASTA perfil`: `PERF_PROBE = true`).

    H_NAME=horizon PASTA=<pasta do game.log> h2_rede_desempenho.py medir SAIDA.jsonl [TAG]
    H_NAME=horizon PASTA=<pasta do game.log> h2_rede_desempenho.py tabela SAIDA.jsonl [SAIDA_DEPOIS.jsonl]
    SOCK=<nome do gravar-jogo start> H_NAME=horizon PASTA=<pasta do game.log> h2_rede_desempenho.py video [LEGENDA]
    H_NAME=horizon PASTA=<pasta do game.log> h2_rede_desempenho.py limpar

O jogo tem de estar no título (`abrir_som_plano.sh titulo`). O `medir` cria o mundo `DesempenhoA` (superplano), monta as cinco pistas, sai e reabre o mundo (a abertura de sempre), põe o combustível, a lava,
o carbono, o calor e os itens (o `fluxo` da cena), espera o carregamento assentar, e mede: uma linha de SAIDA por configuração (`tudo`, `sem_transportadores`, `sem_condutores`, `sem_tubos`, `sem_canos`,
`sem_cabos`), com os milissegundos de parede por tick e por etapa (a média das janelas de 40 passadas, sem a primeira de cada configuração). A medida é por `MBF`, não por `time query gametime`: ela
dá também o tempo de cada etapa. O `limpar` apaga o mundo.
"""
import json, os, re, sys, time

TOOLS = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, TOOLS)
os.environ.setdefault("H_NAME", "horizon")
import h2_reset_mundo as base
from h2_reset_mundo import abrir, cmd, ctl, sair, ssh, state

PASTA = base.PASTA
MUNDO = os.environ.get("MUNDO", "DesempenhoA")
SEMENTE = "20261116"
Y = -60
CENA = os.path.join(TOOLS, "..", "cenas", "setups_reais.txt")
CUBOS = (20, 100, 200, 232, 300)
TANQUES = (100, 190)
# Quanto cada pista sai do mundo, na ordem em que a medida as tira: `(etiqueta, z, x final)`. O `fill` só alcança os chunks carregados (a visão de 16 chunks vai a x = 255).
PISTAS = (("sem_transportadores", 24, 159), ("sem_condutores", 18, 239), ("sem_tubos", 12, 239), ("sem_canos", 6, 239), ("sem_cabos", 0, 255))
JANELAS = 5


def espera(segundos):
    time.sleep(segundos)


def mbf_total():
    out = ssh(f"grep -ac '^MBF ' {PASTA}/game.log").strip()
    return int(out) if out.isdigit() else 0


def mbf_linhas(n):
    """As últimas `n` linhas `MBF` do stderr do jogo."""
    rows = []
    for line in ssh(f"grep -a '^MBF ' {PASTA}/game.log | tail -n {n}").splitlines():
        try:
            rows.append(json.loads(line[4:]))
        except ValueError:
            pass
    return rows


def secao(nome):
    linhas, dentro = [], False
    for linha in open(CENA):
        if linha.startswith("== "):
            dentro = linha[3:].strip() == nome
        elif dentro and linha.strip() and not linha.startswith("#"):
            linhas.append(linha.strip())
    return linhas


def passo(nome):
    if nome == "@maquinas":
        for x in CUBOS:
            cmd("data merge block %d %d 1 {component_config:{config0:[I;1,1,1,1,1,1]}}" % (x, Y))
        for x in TANQUES:
            cmd("data merge block %d %d 13 {component_config:{config2:[I;1,1,1,1,1,1],config3:[I;1,1,1,1,1,1]}}" % (x, Y))
    elif nome == "@minerio":
        for x in CUBOS:
            cmd('data merge block %d %d 1 {Items:[{Slot:0b,id:"minecraft:iron_ore",count:64}]}' % (x, Y))
    elif nome == "@combustivel":
        cmd('data merge block 0 %d 0 {Items:[{Slot:0b,id:"minecraft:coal",count:64}]}' % Y)
    elif nome == "@lava":
        cmd('data merge block 1 %d 6 {probe_push:{fluid:"minecraft:lava",amount:20000}}' % Y)
    elif nome == "@carbono":
        cmd('data merge block 1 %d 12 {probe_push:{chem:"mekanism:carbon",amount:5000}}' % Y)
    elif nome == "@calor":
        cmd("data merge block 0 %d 18 {heat:30000.0d}" % Y)
    elif nome == "@itens":
        cmd('data merge block 1 %d 24 {Items:[{Slot:0b,id:"minecraft:cobblestone",count:64}]}' % (Y + 2))
    else:
        raise SystemExit("passo desconhecido: " + nome)


def roda(nome):
    for linha in secao(nome):
        if linha.startswith("cmd "):
            cmd(linha[4:])
        elif linha.startswith("espera "):
            # A cópia de teste anda a 7 a 20 ticks por segundo: espera pelo relógio, com folga.
            espera(int(linha[7:]) / 5.0 + 2)
        elif linha.startswith("@"):
            passo(linha)
        else:
            raise SystemExit("linha desconhecida: " + linha)


def criar_mundo():
    ctl("world-delete", MUNDO, "--confirm")
    import h2_mundo_plano as mp
    sys.argv = ["h2_mundo_plano.py", MUNDO, SEMENTE]
    mp.main()


def mede(saida, tag):
    """Espera `JANELAS` janelas de 40 passadas e grava a média das últimas `JANELAS - 1` (a primeira pega a transição)."""
    inicio = mbf_total()
    fim = time.time() + 600
    while mbf_total() < inicio + JANELAS and time.time() < fim:
        espera(3.0)
    rows = mbf_linhas(JANELAS)[1:]
    if not rows:
        raise SystemExit("sem linhas MBF: o build tem de ser o do modo perfil")
    stages = {}
    for row in rows:
        for name, (ms, n) in row["s"].items():
            acc = stages.setdefault(name, [0.0, 0.0])
            acc[0] += ms / len(rows)
            acc[1] += n / len(rows)
    wall = sum(row["wall"] for row in rows) / len(rows)
    out = {"tag": tag, "wall_ms": round(wall, 3), "ticks_por_s": round(1000.0 / wall, 2) if wall else 0, "janelas": len(rows), "sinks": rows[-1]["sinks"],
           "etapas": {name: [round(ms, 3), round(n, 1)] for name, (ms, n) in stages.items()}}
    saida.write(json.dumps(out) + "\n")
    saida.flush()
    print(tag, "wall", out["wall_ms"], "ms", out["ticks_por_s"], "ticks/s", flush=True)


def medir(caminho, prefixo=""):
    saida = open(caminho, "w")
    if state().get("world") != MUNDO:
        criar_mundo()
    cmd("gamemode creative @s")
    roda("construir")
    sair()
    abrir(MUNDO)
    espera(30)
    roda("fluxo")
    # O carregamento dos chunks e as redes assentam.
    espera(60)
    mede(saida, prefixo + "tudo")
    for etiqueta, z, fim in PISTAS:
        cmd("fill 0 %d %d %d %d %d air" % (Y, z, fim, Y, z))
        espera(2)
        mede(saida, prefixo + etiqueta)
    saida.close()


def video():
    """O setup rodando, com a velocidade do servidor na tela (`/tick query` no chat) e o som do jogo: a energia do gerador de x = 0 chega por 100 cabos à Enrichment Chamber de x = 100, que
    mói o minério, enquanto as outras quatro pistas andam. Monta o mundo (`DesempenhoA`) como o `medir`, põe clima limpo e meio-dia com os ciclos parados, e grava 40 s pela captura PipeWire do
    gamescope (`gravar-jogo rec`, com o som em sink nulo próprio). `SOCK` é o nome do `gravar-jogo start`, `LOCAL` a pasta de saída (padrão ~/.cache/rust-mekanism/wa/rede-desempenho). O jogo no título,
    aberto com `JOGO_PW_SOCK`, e o `audio_nulo.sh` conferido."""
    import subprocess
    import h2_ajustes_jogo as a
    sock = os.environ["SOCK"]
    local = os.path.expanduser(os.environ.get("LOCAL", "~/.cache/rust-mekanism/wa/rede-desempenho"))
    a.SHOTS = "/home/luka/.cache/rust-mekanism/rede-desempenho/prints"
    a.LOCAL = local
    if state().get("world") != MUNDO:
        criar_mundo()
    cmd("gamemode creative @s")
    roda("construir")
    sair()
    abrir(MUNDO)
    espera(30)
    roda("fluxo")
    espera(45)
    for regra in ("advance_time false", "advance_weather false"):
        cmd("gamerule " + regra)
    cmd("time set noon")
    cmd("weather clear")
    ctl("options-set", "chatVisibility", "0")
    ctl("options-set", "guiScale", "3")
    stand = (100.5, -59.0, -4.0)
    cmd("tp @s %.2f %.2f %.2f 0 25" % stand)
    espera(15)
    print("tick query:", cmd("tick query"), flush=True)
    a.shot("rede_desempenho_tela")
    for categoria in ("master", "block", "ui", "player"):
        ctl("options-set", f"soundCategory_{categoria}", "1.0")
    proc = subprocess.Popen(["ssh", "desktop", f"~/.cache/rust-mekanism/gravar-jogo rec {sock} rede_desempenho.mp4 40 {os.environ['H_NAME']}"], stdout=subprocess.DEVNULL)
    try:
        espera(4)
        cmd("tick query")
        espera(14)
        cmd("tick query")
        espera(14)
        cmd("tick query")
        proc.wait()
    finally:
        ctl("options-set", "soundCategory_master", "0.0")
    print("tick query (fim):", cmd("tick query"), flush=True)


def carrega(caminho):
    return {json.loads(l)["tag"]: json.loads(l) for l in open(caminho) if l.strip()}


def tabela(antes, depois=None):
    a = carrega(antes)
    d = carrega(depois) if depois else {}
    print("%-22s %10s %10s %10s %10s" % ("configuração", "ms/tick", "ticks/s", "depois ms", "depois t/s"))
    for tag, row in a.items():
        o = d.get(tag)
        print("%-22s %10.1f %10.1f %10s %10s" % (tag, row["wall_ms"], row["ticks_por_s"], "%.1f" % o["wall_ms"] if o else "", "%.1f" % o["ticks_por_s"] if o else ""))
    print()
    for tag, row in a.items():
        etapas = sorted(row["etapas"].items(), key=lambda kv: -kv[1][0])[:14]
        print(tag, "; ".join("%s=%.2fms(x%.0f)" % (k, v[0], v[1]) for k, v in etapas))


def main():
    what = sys.argv[1] if len(sys.argv) > 1 else ""
    if what == "medir":
        medir(sys.argv[2], sys.argv[3] if len(sys.argv) > 3 else "")
    elif what == "tabela":
        tabela(sys.argv[2], sys.argv[3] if len(sys.argv) > 3 else None)
    elif what == "video":
        video()
    elif what == "limpar":
        ctl("world-delete", MUNDO, "--confirm")
    else:
        raise SystemExit("uso: h2_rede_desempenho.py medir SAIDA.jsonl | tabela ANTES.jsonl [DEPOIS.jsonl] | limpar")


if __name__ == "__main__":
    main()
