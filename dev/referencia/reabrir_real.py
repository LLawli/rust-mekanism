#!/usr/bin/env python3
"""A tela aberta num mundo de verdade salvo e reaberto, antes do primeiro tick do tile (o que `reabrir=1` de `cena_telas.py` emula no mesmo mundo).

    reabrir_real.py montar            monta as máquinas de `CASOS` longe do jogador (cada uma na sua caixa de concreto) e salva o mundo (`quit`)
    reabrir_real.py abrir DESTINO ESCALA   com o jogo já reaberto: para o servidor, vai até cada máquina e tira o print da tela, sem nenhum tick do tile

O mundo reaberto não nasce parado (`tick freeze` não persiste): as máquinas ficam a 600 blocos do jogador, num chunk que só carrega quando o jogador vai até lá, já com o
servidor parado, então o tile nasce do NBT e a tela abre antes de qualquer tick dele. O `CenaTelas` do original faz o mesmo com `reabrir=1` (grava e relê o tile). Roda no
Horizon do desktop (`docker exec horizon`), como `cena_telas.py`.
"""
import sys

import cena_telas as ct

# (nome do caso da tabela, x da máquina): o caso sem o `reabrir`, que monta a máquina em andamento; o print sai com o nome `port_real_<caso>_e<escala>`.
CASOS = [("chamber_reaberta", 600), ("chamber_reaberta_upgrades", 640), ("infuser_reaberta", 680), ("generator_reaberto", 720)]


def caso(nome):
    c = next(c for c in ct.todos_os_casos() if c["nome"] == nome)
    args = {k: v for k, v in c["args"].items() if k != "reabrir"}
    return dict(c, args=args)


def montar():
    ct.ctl("options-set", "chatVisibility", "2")
    for nome, x in CASOS:
        # O chunk da máquina só existe depois de o jogador chegar: o `fill` da caixa e o `place` dependem dele.
        ct.ctl("tp", f"{x}.0", "-60.0", "0.0")
        ct.ctl("wait", "60")
        ct.monta_port(caso(nome), x, 2)
        print("montado", nome, x, flush=True)
    # De volta ao ponto de partida da cena, longe das máquinas, e o mundo sai salvo com o servidor ainda parado.
    ct.ctl("tp", "3.0", "-60.0", "0.0")
    ct.ctl("wait", "20")
    ct.ctl("quit")


def abrir(destino, escala):
    ct.ctl("wait-ready", check=False)
    ct.comando("tick freeze")
    ct.ctl("options-set", "chatVisibility", "2")
    ct.ctl("options-set", "guiScale", str(escala))
    ct.iguala_as_opcoes()
    for nome, x in CASOS:
        c = dict(caso(nome), nome="real_" + nome)
        ct.ctl("tp", f"{x}.0", "-60.0", "0.0")
        ct.ctl("wait", "40")
        ct.tira_port(c, x, escala, destino)
        print("aberto", nome, flush=True)


if __name__ == "__main__":
    if sys.argv[1] == "montar":
        montar()
    else:
        abrir(sys.argv[2], int(sys.argv[3]))
