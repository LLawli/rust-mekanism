#!/usr/bin/env python3
"""A conexão cabo com cabo depois de sair do mundo e reabri-lo (frente `cabo-forma`, a segunda rodada): a mesma cena de `testes/CenaReabrir.java` no port.

    H_NAME=horizon2 PASTA=<pasta do game.log> h2_cabo_reabrir.py montar   # monta a cena de `h2_cabo_forma.py` (o cabo com os cubos e o cabo ao sul, a reta e a esquina) e põe `PUSH` e `PULL` pelo Configurator
    H_NAME=horizon2 PASTA=<pasta do game.log> h2_cabo_reabrir.py ler ROTULO # a vista publicada (a sonda `MBV`, só no build com `trace`) de cada cabo da cena: o tipo cru, o visível e a conexão de cada lado

Depois de `montar` o jogo é fechado (`ctl.sh quit`, que salva) e reaberto no mesmo mundo; o `ler` do port sai do `game.log` do jogo reaberto. Os lados na ordem do jogo: baixo, cima, norte, sul, oeste, leste.
"""
import json, os, sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import h2_cabo_forma as f

# A cena de `cabo-forma` (o cabo com os cubos, a reta de três e a esquina) e as formas da `rede-reabrir` (a reta de cinco com um cubo, a cruz e o cabo entre dois cubos), como em `CenaReabrir.java`.
CABOS = [(0, 0, 0), (0, 0, 1), (0, 0, 24), (1, 0, 24), (2, 0, 24), (0, 0, 30), (1, 0, 30), (0, 0, 31),
         (20, 0, 0), (21, 0, 0), (22, 0, 0), (23, 0, 0), (24, 0, 0),
         (20, 0, 10), (19, 0, 10), (21, 0, 10), (20, 0, 9), (20, 0, 11),
         (21, 0, 20),
         (41, 0, 0), (41, 0, 3), (41, 0, 6), (41, 0, 9), (41, 0, 12), (41, 0, 15)]
CASOS_CUBO = [(0, 0), (0, 1), (1, 0), (1, 1), (4, 0), (4, 1)]  # `DataType.ordinal()` dos seis lados do cubo básico e o `eject`: `NONE`, `INPUT`, `OUTPUT`
CUBOS_EXTRA = [(25, 0, 0), (20, 0, 20), (22, 0, 20)]
LETRA = {"normal": "N", "push": "P", "pull": "P", "none": "N"}  # a primeira letra do `ConnectionType` que `CenaReabrir` escreve: `NORMAL` e `NONE` dão `N`, `PUSH` e `PULL` dão `P`


def linha(rotulo, c, v):
    """`cena: reabrir ROTULO x y z cli MASCARA TIPOS`, no formato do original (a máscara `getAllCurrentConnections` em hexadecimal, o tipo visível de cada lado)."""
    mascara = sum(1 << i for i, ligado in enumerate(v["conn"]) if ligado)
    tipos = "".join(LETRA[k] for k in v["kind"])
    return f"cena: reabrir {rotulo} {c[0]} {c[1]} {c[2]} cli {mascara:02x} {tipos}"


def ler(rotulo):
    h, aj = f.port()
    vistas = aj.last_views("MBV")
    for c in CABOS:
        v = vistas.get(f.mundo(c))
        print(linha(rotulo, c, v) if v else f"cena: reabrir {rotulo} {c[0]} {c[1]} {c[2]} sem vista")


def montar_extra():
    """A reta de cinco com um cubo a leste, a cruz e o cabo entre dois cubos, na área x de 19 a 25 (o `montar` de `h2_cabo_forma.py` só limpa até x = 12)."""
    h, aj = f.port()
    cmd, wait = aj.cmd, aj.wait
    o = f.origem()
    cmd("fill %d %d %d %d %d %d minecraft:stone" % (o[0] + 14, o[1] - 1, o[2] - 8, o[0] + 48, o[1] - 1, o[2] + 40))
    cmd("fill %d %d %d %d %d %d minecraft:air" % (o[0] + 14, o[1], o[2] - 8, o[0] + 48, o[1] + 8, o[2] + 40))
    wait(10)
    for p in CABOS[8:]:
        cmd("setblock %d %d %d %s" % (*f.mundo(p), f.CABO))
    for i in range(len(CASOS_CUBO)):
        cmd("setblock %d %d %d mekanism:basic_energy_cube" % f.mundo((40, 0, 3 * i)))
    for p in CUBOS_EXTRA:
        cmd("setblock %d %d %d %s" % (*f.mundo(p), f.CUBO))
    wait(10)
    for p in CUBOS_EXTRA:
        cmd("data merge block %d %d %d %s" % (*f.mundo(p), f.CONFIG_CUBO))
    for i, (dado, eject) in enumerate(CASOS_CUBO):
        cmd("data merge block %d %d %d {component_config:{config0:[I;%s],eject0:%db}}" % (*f.mundo((40, 0, 3 * i)), ",".join([str(dado)] * 6), eject))
    wait(40)


def preparar_midia():
    """Dia limpo e parado, o jogador criativo com o Network Reader na mão e o chat à vista."""
    h, aj = f.port()
    import h2_nota_instrumento as n
    n.sky()
    aj.cmd("gamemode creative @s")
    aj.cmd("clear @s")
    h.ctl("options-set", "chatVisibility", "0")
    h.ctl("options-set", "guiScale", "3")
    h.ctl("give", "mekanism:network_reader", "1")
    aj.wait(6)
    h.ctl("select", "mekanism:network_reader")
    return h, aj


def tp_vista(aj, h, olho, alvo):
    aj.cmd("tp @s %.2f %.2f %.2f" % f.mundo(olho))
    aj.wait(14)
    h.ctl("look-at", *("%.3f" % v for v in f.mundo(alvo)))
    aj.wait(8)


def prints(rotulo):
    """O cabo do meio com os braços ao sul, a oeste e a leste (cubo `PUSH` e `PULL`), e o Network Reader no cabo do sul: `meio_<rotulo>.png` e `leitor_sul_<rotulo>.png` em `LOCAL`."""
    h, aj = preparar_midia()
    aj.SHOTS = os.environ.get("SHOTS", "~/.cache/rust-mekanism/rede-reabrir/prints")
    aj.LOCAL = os.environ.get("LOCAL", os.path.expanduser("~/.cache/rust-mekanism/wa/rede-reabrir"))
    os.makedirs(aj.LOCAL, exist_ok=True)
    tp_vista(aj, h, (1.5, 2.6, -1.6), (0.5, 0.5, 0.9))
    print(aj.shot("meio_" + rotulo))
    import h2_network_reader as nr
    nr.clicar(f.mundo((0, 0, 1)), (0.5, 0.5, 0.5), stand=f.mundo((1.5, 2.6, 3.2)))
    print(aj.shot("leitor_sul_" + rotulo))


def roteiro_reabrir(pausa, marca):
    h, aj = f.port()
    import h2_network_reader as nr
    h.ctl("options-set", "chatVisibility", "0")
    h.ctl("give", "mekanism:network_reader", "1")
    aj.wait(6)
    h.ctl("select", "mekanism:network_reader")
    tp_vista(aj, h, (1.5, 2.6, -1.6), (0.5, 0.5, 0.9))
    marca("meio")
    pausa(5.0)
    nr.clicar(f.mundo((0, 0, 1)), (0.5, 0.5, 0.5), stand=f.mundo((1.5, 2.6, 3.2)))
    marca("leitor_sul")
    pausa(6.0)
    nr.clicar(f.mundo((0, 0, 0)), (0.5, 0.5, 0.5), stand=f.mundo((-1.5, 2.6, -1.6)))
    marca("leitor_meio")
    pausa(6.0)


def video_reabrir():
    """O vídeo com som no mundo reaberto: o cabo do meio com o do sul ligado, e o Network Reader nos dois (`NOME`, `SEGUNDOS`, `ARQUIVO` como em `h2_cabo_forma.py video`)."""
    f.roteiro_video = roteiro_reabrir
    f.video()


if __name__ == "__main__":
    modo = sys.argv[1]
    if modo == "montar":
        f.montar()
        montar_extra()
    elif modo == "ler":
        ler(sys.argv[2])
    elif modo == "prints":
        prints(sys.argv[2] if len(sys.argv) > 2 else "depois")
    elif modo == "video":
        video_reabrir()
    else:
        print(__doc__)
        sys.exit(2)
