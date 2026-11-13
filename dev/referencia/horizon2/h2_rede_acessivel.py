#!/usr/bin/env python3
"""A rede partida da decisão 198 (frente `rede-acessivel`): abre o mundo `SetupsA` muitas vezes, uma depois da outra, e resume a rede de energia de cada abertura.

    H_NAME=horizon2 PASTA=<pasta do game.log> h2_rede_acessivel.py construir          (cria o mundo e monta os cinco setups, como o `h2_setups_reais.py roteiro`)
    H_NAME=horizon2 PASTA=<pasta do game.log> h2_rede_acessivel.py abrir N DESTINO    (N aberturas; cada uma grava DESTINO/abertura_<i>.log com as sondas e imprime os tamanhos das redes)
    SOCK=<nome do gravar-jogo> LOCAL=<pasta> ... h2_rede_acessivel.py midia          (o vídeo com som de se afastar e voltar, o Network Reader em x = 330 e em x = 100, e o print do leitor)

O jogo tem de estar no título, com a cópia de teste (`enviar_fontes.sh WS PASTA setups`: `SETUPS_PROBE = true`). Cada abertura espera `ABRE_TICKS` passadas do servidor (padrão 150) e lê da sonda
leve a última `MBE` (a rede de energia: tamanhos em transmissores), os tickets de acessibilidade (`MBK`), a carga de cada chunk (`MBL`) e a primeira passada de cada chunk (`MBU`).
"""
import json, os, sys, time

TOOLS = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, TOOLS)
import h2_setups_reais as s
from h2_reset_mundo import abrir, cmd, sair, ssh
import h2_reset_mundo as base

Y = s.Y

SAIDA = "/^(MBE|MBK|MBL|MBU|MBC|MBD) / {print; next} /^MBT .*\"e\":\"(pre|open_ready|estado|reset)\"/ {print}"


def construir():
    s.criar_mundo()
    cmd("gamemode creative @s")
    s.roda("construir")
    sair()


def resumo(linhas):
    """Os tamanhos (em transmissores) das redes de energia da última `MBE`, e a primeira passada em que a maior chegou ao tamanho final."""
    ultimas, historico = None, []
    for linha in linhas:
        if linha.startswith("MBE "):
            try:
                row = json.loads(linha[4:])
            except ValueError:
                continue
            ultimas = sorted((n[3] for n in row["nets"]), reverse=True)
            historico.append(ultimas)
    return ultimas, historico


def abertura(i, destino, ticks):
    inicio = base.log_total()
    abrir(s.MUNDO)
    s.espera_ticks(ticks)
    linhas = ssh(f"tail -n +{inicio + 1} {s.PASTA}/game.log | awk '{SAIDA}'").splitlines()
    os.makedirs(destino, exist_ok=True)
    open(os.path.join(destino, f"abertura_{i}.log"), "w").write("\n".join(linhas) + "\n")
    final, hist = resumo(linhas)
    print(f"abertura {i}: redes de energia {final} (linhas {len(linhas)})", flush=True)
    sair()
    return final


def midia():
    """O vídeo com som de se afastar e voltar (decisão 198): o gerador e as Enrichment Chambers andando, o Network Reader no cabo de x = 330 com o jogador lá (o original lê 352 transmissores), de volta ao
    começo e depois em x = 100 (383), e o print `rede_acessivel_port_leitor` desse último clique, o mesmo que o original grava em `setups_oficial_leitor.png`. O jogo no título, aberto com `JOGO_PW_SOCK`, o
    mundo `SetupsA` já construído e o sink conferido por `audio_nulo.sh`."""
    import subprocess
    import h2_ajustes_jogo as a
    import h2_network_reader as nr
    sock = os.environ["SOCK"]
    local = os.path.expanduser(os.environ.get("LOCAL", "~/.cache/rust-mekanism/wa/rede-acessivel"))
    a.SHOTS = "/home/luka/.cache/rust-mekanism/rede-acessivel/prints"
    a.LOCAL = local
    abrir(s.MUNDO)
    s.espera_ticks(60)
    for regra in ("advance_time false", "advance_weather false", "time set 6000", "weather clear", "gamemode creative @s"):
        cmd(("gamerule " if regra.startswith("advance") else "") + regra)
    base.ctl("options-set", "chatVisibility", "0")
    base.ctl("options-set", "guiScale", "3")
    base.ctl("give", nr.READER, "1")
    s.passo_nomeado("@combustivel")
    s.passo_nomeado("@minerio")
    perto = (100.5, -60.0, -3.0)
    longe = (330.5, -60.0, -3.0)
    cmd("tp @s %.2f %.2f %.2f 0 20" % (perto[0], perto[1] + 1, perto[2] - 2))
    s.espera_ticks(120)
    base.ctl("select", nr.READER)
    for categoria in ("master", "block", "ui", "player"):
        base.ctl("options-set", f"soundCategory_{categoria}", "1.0")
    proc = subprocess.Popen(["ssh", "desktop", f"~/.cache/rust-mekanism/gravar-jogo rec {sock} rede_acessivel.mp4 56 {os.environ['H_NAME']}"], stdout=subprocess.DEVNULL)
    time.sleep(3.0)
    try:
        s.espera_ticks(60)
        base.ctl("options-set", "chatVisibility", "0")
        cmd("tp @s %.2f %.2f %.2f 0 20" % longe)
        s.espera_ticks(80)
        nr.clicar((330, Y, 0), (0.5, 0.5, 0.5), sneak=False, stand=longe)
        s.espera_ticks(30)
        cmd("tp @s 2.5 -60 -6")
        s.espera_ticks(80)
        base.ctl("options-set", "chatVisibility", "0")
        nr.clicar((100, Y, 0), (0.5, 0.5, 0.5), sneak=False, stand=perto)
        prints = a.shot("rede_acessivel_port_leitor")
        proc.wait()
    finally:
        base.ctl("options-set", "soundCategory_master", "0.0")
    print("print:", prints)


def main():
    what = sys.argv[1]
    if what == "construir":
        construir()
    elif what == "abrir":
        n, destino = int(sys.argv[2]), sys.argv[3]
        ticks = int(os.environ.get("ABRE_TICKS", "150"))
        for i in range(n):
            abertura(i, destino, ticks)
    elif what == "midia":
        midia()
    else:
        raise SystemExit(__doc__)


if __name__ == "__main__":
    main()
