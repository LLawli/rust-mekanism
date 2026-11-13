#!/usr/bin/env python3
"""Os condutores termodinâmicos no jogo (frente `condutor-jogo`, Fase 3, decisão 115).

    H_NAME=horizon PASTA=<pasta do game.log> h2_condutor_jogo.py serie        # a linha de 5 condutores com o Heat Generator em cima do último, tick a tick (grava o rastro do jogo)
    H_NAME=horizon PASTA=<pasta do game.log> h2_condutor_jogo.py quebrar      # o condutor do meio quebrado e reposto: as redes e o calor
    H_NAME=horizon PASTA=<pasta do game.log> h2_condutor_jogo.py none         # o Configurator agachado no braço leste do condutor do meio de uma linha de 3, com o antes e o depois recortados
    H_NAME=horizon PASTA=<pasta do game.log> h2_condutor_jogo.py ar           # um condutor solto aquecido perde calor para o ar
    NOME=... h2_condutor_jogo.py video                                        # o vídeo com som

A sonda (`TRANSMITTER_PROBE = true` na cópia de teste, `enviar_fontes.sh`) escreve no stderr do jogo (`PASTA/game.log`): `MBH {p, conn, kind, raw, net}` a cada vez que um condutor publica as conexões dele e
`MBQ {t, runs, nets: [[média, perda, troca, condutores, aceitadores], ...], cond: [[x, y, z, calor, pacotes], ...]}` a cada `ServerTickEvent.Post`. O calor sai do dono com os mesmos bits que o
rastro do jogo repete no teste da lib (`jogo_calor_trace.rs`).
"""
import json, os, subprocess, sys, time

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
os.environ.setdefault("H_NAME", "horizon")
import h2_ajustes_jogo as a
import h2_identity as h

CONDUCTOR = "mekanism:basic_thermodynamic_conductor"
GENERATOR = "mekanismgenerators:heat_generator"
a.KINDS["condutor"] = {"block": CONDUCTOR, "prefix": "MBH"}
RASTROS = os.environ.get("RASTROS", os.path.expanduser("~/.cache/rust-mekanism/condutor-jogo/rastros"))
cmd, wait, at, txt = a.cmd, a.wait, a.at, a.txt
LAVA = 'energy:0,machinery_basis:{v:1,tanks:[{resource:"minecraft:lava",amount:1000L}]}'


def rows(prefix="MBQ"):
    return a.log_lines(prefix)


def heat_of(x, y=0):
    """O calor do condutor em `at(x, y)` na última linha `MBQ`."""
    last = rows()[-1]
    for cx, cy, cz, heat, _ in last["cond"]:
        if (cx, cy, cz) == at(x, y):
            return heat
    return None


def montar_linha(n=5, generator=True):
    a.clear()
    cmd("gamemode creative @s")
    cmd("clear @s")
    for x in range(n):
        cmd("setblock %s %s" % (txt(at(x)), CONDUCTOR))
    if generator:
        cmd("setblock %s %s" % (txt(at(n - 1, 1)), GENERATOR))
    wait(60)


def serie(passos=40, h0=8000.0):
    """O Heat Generator em cima do último de cinco condutores básicos, sem lava, com o calor posto por `data merge` (o calor do original que `gheat` põe no rastro): com o servidor parado
    e `tick step 1`, cada passo imprime uma linha `MBQ` com o calor dos cinco condutores e as médias da rede. O rastro do jogo vai para `RASTROS/jogo_calor.jsonl` (o teste da lib o repete)."""
    montar_linha(5)
    cmd("tick freeze")
    time.sleep(1.2)
    gen = at(4, 1)
    em_repouso = sorted(rows()[-1]["cond"])
    cmd("data merge block %s {heat:%sd}" % (txt(gen), repr(h0)))
    time.sleep(0.5)
    lido = cmd("data get block %s heat" % txt(gen))
    print("calor posto no gerador:", lido)
    antes = len(rows())
    for _ in range(passos):
        a.step(1)
    time.sleep(1.5)
    novas = [r for r in rows()[antes:] if r["runs"]]
    print("passos com a rede andando:", len(novas), "de", passos)
    os.makedirs(RASTROS, exist_ok=True)
    ar = {}
    for row in rows("MBA"):
        ar[tuple(row["at"])] = row["v"]
    setup = {"e": "setup", "conductors": 5, "tier": "basic", "gen_heat0": h0, "origin": list(at(0)), "ambient_reads": [[*k, v] for k, v in sorted(ar.items())], "ambient": em_repouso[0][3], "rest": [c[3] for c in em_repouso]}
    with open(f"{RASTROS}/jogo_calor.jsonl", "w") as out:
        out.write(json.dumps(setup) + "\n")
        for i, r in enumerate(novas):
            cond = sorted(r["cond"], key=lambda c: (c[0], c[1], c[2]))
            out.write(json.dumps({"e": "step", "i": i, "t": r["t"], "cond": [c[3] for c in cond], "nets": r["nets"]}) + "\n")
    cmd("tick unfreeze")
    for r in novas[:3] + novas[-2:]:
        print("   t=%s redes %s calor %s" % (r["t"], r["nets"], [c[3] for c in sorted(r["cond"])]))
    quente = [c[3] for c in sorted(novas[-1]["cond"])] if novas else []
    good = len(novas) >= passos - 2 and len(quente) == 5 and quente[4] > quente[0] > 0
    print(("OK   " if good else "DIFF ") + f"o calor desceu pela linha: {quente}")
    return good


def quebrar():
    """O condutor do meio de uma linha de 5 quebrado como o jogador quebra (`setblock ... air destroy`) e reposto. Com o servidor andando: a rede vira duas (a ponta quente e a fria), o calor
    do lado frio só perde para o ar, e com o condutor reposto a rede é uma de novo e o calor volta a andar. O gerador fica aceso com lava (refeita a cada meio segundo)."""
    montar_linha(5)
    gen = at(4, 1)

    def lava():
        cmd("data merge block %s {%s}" % (txt(gen), LAVA))

    for _ in range(8):
        lava()
        time.sleep(0.5)
    uma = rows()[-1]["nets"]
    print("antes de quebrar: redes", uma, "calor", [c[3] for c in sorted(rows()[-1]["cond"])])
    cmd("setblock %s minecraft:air destroy" % txt(at(2)))
    for _ in range(8):
        lava()
        time.sleep(0.5)
    duas = rows()[-1]
    frio = [c[3] for c in sorted(duas["cond"]) if c[0] < at(2)[0]]
    print("quebrado: redes", duas["nets"], "calor", [c[3] for c in sorted(duas["cond"])])
    cmd("setblock %s %s" % (txt(at(2)), CONDUCTOR))
    for _ in range(8):
        lava()
        time.sleep(0.5)
    de_novo = rows()[-1]
    print("reposto: redes", de_novo["nets"], "calor", [c[3] for c in sorted(de_novo["cond"])])
    good = len(uma) == 1 and len(duas["nets"]) == 2 and len(de_novo["nets"]) == 1 and uma[0][3] == 5 and duas["nets"][0][3] + duas["nets"][1][3] == 4 and de_novo["nets"][0][3] == 5 and frio[0] < 400
    print(("OK   " if good else "DIFF ") + "a rede se divide ao quebrar o do meio e se junta ao repor")
    return good


def ar():
    """Um condutor solto aquecido perde calor para o ar: o Heat Generator quente em cima de um condutor só o aquece, e depois do pico (o gerador esfria) o condutor só perde para o ar."""
    montar_linha(1, generator=True)
    gen = at(0, 1)
    cmd("tick freeze")
    time.sleep(1.2)
    cmd("data merge block %s {heat:4000.0d}" % txt(gen))
    antes = len(rows())
    for _ in range(60):
        a.step(1)
    time.sleep(1.5)
    novas = [r for r in rows()[antes:] if r["runs"]]
    heats = [r["cond"][0][3] for r in novas]
    print("calor do condutor solto, 1 passo em cada 6:", [round(x, 3) for x in heats[::6]])
    # Depois de esquentar com o gerador (que esfria), o condutor perde para o ar: o calor de depois do pico só cai.
    pico = max(range(len(heats)), key=lambda i: heats[i])
    desce = all(heats[i + 1] <= heats[i] + 1e-9 for i in range(pico, len(heats) - 1))
    cmd("tick unfreeze")
    good = len(heats) >= 55 and heats[pico] > heats[0] and desce
    print(("OK   " if good else "DIFF ") + f"o condutor sobe até o passo {pico} e desce depois, perdendo calor para o ar")
    return good


def none():
    """O `NONE`: o Configurator agachado no braço leste do condutor do meio de uma linha de 3 (PUSH, PULL e NONE) com o antes e o depois recortados, e as redes e o calor depois."""
    return a.none("condutor")


def montar_video():
    """A cena do vídeo: cinco condutores em `z 0` (x 0 a 4) com o Heat Generator em cima do último, e uma linha de três em `z 5` para o Configurator."""
    cmd("tick unfreeze")
    a.clear()
    cmd("fill %d %d %d %d %d %d minecraft:stone" % (a.BASE[0] - 6, a.BASE[1] - 1, a.BASE[2] - 8, a.BASE[0] + 18, a.BASE[1] - 1, a.BASE[2] + 10))
    cmd("fill %d %d %d %d %d %d minecraft:air" % (a.BASE[0] - 6, a.BASE[1], a.BASE[2] - 8, a.BASE[0] + 18, a.BASE[1] + 8, a.BASE[2] + 10))
    wait(10)
    cmd("gamemode creative @s")
    cmd("clear @s")
    for x in range(5):
        cmd("setblock %s %s" % (txt(at(x, 0, 0)), CONDUCTOR))
    cmd("setblock %s %s" % (txt(at(4, 1, 0)), GENERATOR))
    for x in range(3):
        cmd("setblock %s %s" % (txt(at(x, 0, 6)), CONDUCTOR))
    wait(40)
    h.ctl("give", "mekanism:configurator", "1")
    h.ctl("options-set", "chatVisibility", "0")


def ler(x, z=0):
    """O leitor: o `data get` do calor do condutor, que o chat mostra."""
    cmd("data get block %s heat_capacitor.state.heat" % txt(at(x, 0, z)))


def roteiro_video(pausa):
    """O que o vídeo mostra, por `CENA`. `tudo` é `calor` e `configurador` em seguida. `calor`: o Heat Generator aceso (lava e energia refeitas) esquentando a ponta da linha e o calor descendo pelos condutores (o chat mostra o `data get`
    do calor de uma ponta e da outra), o condutor do meio quebrado (o calor da ponta fria só cai) e reposto (volta a andar). `configurador`: o Configurator agachado no braço leste do condutor
    do meio de outra linha, até o `NONE` (o braço some, e o do vizinho com ele), e de volta a `NORMAL` na face."""
    h.ctl("options-set", "chatVisibility", "1")
    cena = os.environ.get("CENA", "calor")
    gen = at(4, 1, 0)
    mid = at(2, 0, 0)
    if cena in ("calor", "tudo"):
        quantos = (3, 2, 2) if cena == "tudo" else (5, 4, 4)
        eye = (mid[0] + 0.5, mid[1] + 1.2, mid[2] - 4.2)
        a.olhar(eye, (mid[0] + 0.5, mid[1] + 0.5, mid[2] + 0.5))

        def aceso(n):
            for _ in range(n):
                cmd("data merge block %s {%s}" % (txt(gen), LAVA))
                ler(0)
                ler(4)
                pausa(0.4)

        aceso(quantos[0])
        # Como o jogador quebra (`break`: o som do bloco vai ao vídeo).
        h.ctl("break", *map(str, mid))
        aceso(quantos[1])
        cmd("setblock %s %s" % (txt(mid), CONDUCTOR))
        aceso(quantos[2])
        if cena == "calor":
            return
    middle = at(1, 0, 6)
    eye2 = (middle[0] + 0.5, middle[1] + 1.0, middle[2] - 2.6)
    centro = (middle[0] + 0.5, middle[1] + 0.5, middle[2] + 0.5)
    a.olhar(eye2, centro)
    pausa(1.0)
    h.ctl("key", "shift", "down")
    h.ctl("look-at", *("%.3f" % (middle[i] + a.ARM["east"][i]) for i in range(3)))
    wait(6)
    for _ in ("push", "pull", "none"):
        h.ctl("mouse", "right", "click")
        wait(20)
        pausa(1.2)
    # De volta a `NORMAL` na face: sem braço o raio cai na face leste do centro; o vizinho sai para a face ficar à vista e volta.
    cmd("setblock %s minecraft:air" % txt(at(2, 0, 6)))
    cmd("tp @s %.2f %.2f %.2f" % (middle[0] + 3.0, middle[1] + 0.5, middle[2] + 0.5))
    wait(10)
    h.ctl("look-at", *("%.3f" % (middle[i] + (0.69, 0.5, 0.5)[i]) for i in range(3)))
    wait(6)
    h.ctl("mouse", "right", "click")
    wait(8)
    h.ctl("key", "shift", "up")
    cmd("setblock %s %s" % (txt(at(2, 0, 6)), CONDUCTOR))
    a.olhar(eye2, centro)
    pausa(2.0)


def video():
    """O vídeo com som (`NOME`, `SEGUNDOS`, `ARQUIVO`): a cena do `h2_cabo_jogo.video`, com o roteiro dos condutores."""
    import h2_cabo_jogo as c
    c.montar_cena = montar_video
    c.video_roteiro = roteiro_video
    c.video()


def main():
    what = sys.argv[1] if len(sys.argv) > 1 else "serie"
    table = {"serie": serie, "quebrar": quebrar, "none": none, "ar": ar}
    if what in table:
        sys.exit(0 if table[what]() else 1)
    if what == "video":
        video()
        return
    if what == "cena":
        montar_video()
        roteiro_video(lambda t: time.sleep(t))
        return
    raise SystemExit("uso: h2_condutor_jogo.py serie | quebrar | none | ar | video")


if __name__ == "__main__":
    main()
