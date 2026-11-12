#!/usr/bin/env python3
"""A frente `restos-fase2` em jogo (`H_NAME=horizon2`, mundo de teste, plataforma a y 150 com céu): três medidas, uma por item.

    h2_restos_fase2.py categoria   o evento real do bloco de nota (um bloco de redstone encostado), gravado com o som do jogo duas vezes, uma só com a categoria `record` ligada e uma só com
                                   `block`, e o volume medido no arquivo (`volumedetect`): no original só a `record` toca (`NoteBlock.java:162`, decisão 92).
                                   `NOME=<nome do gravar-jogo start>`; o container precisa ter nascido com `JOGO_PW_SOCK`.
    h2_restos_fase2.py balde       o balde cheio numa laje com um cabo em cima dela (a laje se enche, o cabo fica), no cabo como alvo (recusa) e a lava na laje (o cabo é o alvo: recusa), decisão 86.
    h2_restos_fase2.py forno PASTA a luz do céu sob o Metallurgic Infuser, o Heat Generator, o Crusher e uma pedra (decisão 93), e o print da cena em PASTA/forno.png.
    h2_restos_fase2.py video       o vídeo do bloco de nota com som, só com `record` ligada (`h2_nota_instrumento.py video`).

Clima limpo e meio-dia com os ciclos parados; as gamerules voltam ao fim. Sai 0 só se tudo é o do original.
"""
import json
import os
import subprocess
import sys
import threading
import time

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import h2_identity as h
import h2_nota_instrumento as n

SX, SY, SZ = 60, 150, 80  # a laje e o cabo; a plataforma de pedra fica a y 149


def state(x, y, z):
    return json.loads(h.ctl("block", str(x), str(y), str(z)))["state"]


def counts():
    return json.loads(h.ctl("inventory")).get("counts", {})


def delta(before, after):
    return {k: after.get(k, 0) - before.get(k, 0) for k in set(before) | set(after) if after.get(k, 0) != before.get(k, 0)}


def volume(arquivo):
    """`max_volume` e `mean_volume` do arquivo no desktop (-91 dB é silêncio digital)."""
    out = subprocess.run(
        ["ssh", "desktop", f"ffmpeg -hide_banner -i ~/.cache/rust-mekanism/gravador/saida/{arquivo} -af volumedetect -vn -f null - 2>&1 | grep -E 'max_volume|mean_volume'"],
        capture_output=True,
        text=True,
    ).stdout
    got = {}
    for line in out.splitlines():
        key = "max" if "max_volume" in line else "mean"
        got[key] = float(line.split(":")[1].replace("dB", "").strip())
    return got


def gravar_nota(nome, categorias, arquivo, segundos=8):
    """Um bloco de nota sobre o sal, a nota tocada duas vezes pelo bloco de redstone, com só as categorias de `categorias` ligadas. Devolve (volume do arquivo, eventos de nota ouvidos)."""
    n.sky()
    h.command("tick unfreeze")
    n.build_row()
    n.mute()
    for categoria in n.SOUND_OFF + ("record", "block"):
        h.ctl("options-set", f"soundCategory_{categoria}", "0")
    for categoria in categorias:
        h.ctl("options-set", f"soundCategory_{categoria}", "1.0")
    ligado = False
    for _ in range(60):
        if "movidos 0" not in n.desktop(f"ligar {n.CONTAINER}").stdout and n.desktop(f"verificar {n.CONTAINER}").stdout.startswith("ok"):
            ligado = True
            break
        time.sleep(1)
    if not ligado:
        print("o stream do jogo não foi para o sink nulo: nada gravado, volume mestre em 0")
        sys.exit(1)
    parar = threading.Event()

    def vigia():
        while not parar.is_set():
            if not n.desktop(f"seguro {n.CONTAINER}").stdout.startswith("ok"):
                print("stream fora dos sinks de teste: volume mestre em 0", flush=True)
                n.mute()
            parar.wait(0.5)

    thread = threading.Thread(target=vigia, daemon=True)
    thread.start()
    rec = None
    antes = n.events()
    try:
        h.ctl("options-set", "soundCategory_master", "1.0")
        time.sleep(3.0)
        rec = subprocess.Popen(["ssh", "desktop", f"{n.GRAVAR} rec {nome} {arquivo} {segundos} {n.CONTAINER}"], stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)
        time.sleep(3.0)
        # O gamescope só manda quadro quando a imagem muda: com a cena parada o arquivo sai vazio. A câmera balança 2 graus a cada 0,3 s, e as duas notas tocam em 1 s e 3 s.
        inicio = time.time()
        tocando = {1.0: True, 2.0: False, 3.0: True, 4.0: False}
        passo = 0
        while time.time() - inicio < 5.5:
            h.ctl("look", str(178 + 4 * (passo % 2)), "14")
            passo += 1
            for t, liga in list(tocando.items()):
                if time.time() - inicio >= t:
                    x = n.x_of(0)
                    h.ctl("setblock", str(x), str(n.Y + 1), str(n.Z + 1), "minecraft:redstone_block" if liga else "minecraft:air")
                    del tocando[t]
        rec.communicate(timeout=int(segundos) + 90)
    finally:
        n.mute()
        parar.set()
        thread.join(timeout=5)
        if rec is not None and rec.poll() is None:
            rec.wait(timeout=60)
        depois = n.events()
        n.clear()
    heard = {k: v - antes.get(k, 0) for k, v in depois.items() if k.startswith("minecraft:block.note_block") and v != antes.get(k, 0)}
    return volume(arquivo), heard


def categoria():
    nome = os.environ["NOME"]
    bad = []
    for ligadas, esperado in ((("record",), "toca"), (("block",), "mudo")):
        arquivo = f"cat_{ligadas[0]}.mp4"
        vol, heard = gravar_nota(nome, ligadas, arquivo)
        toca = vol.get("max", -91.0) > -60.0
        ok = (toca == (esperado == "toca")) and sum(heard.values()) == 2
        print(f"só {ligadas[0]:6} ligada: max {vol.get('max')} dB, média {vol.get('mean')} dB, eventos {heard}; original: {esperado}  {'ok' if ok else 'DIFERE'}", flush=True)
        if not ok:
            bad.append((ligadas, vol, heard))
    print("TUDO IGUAL AO ORIGINAL" if not bad else f"HÁ DIFERENÇAS: {bad}")
    return 0 if not bad else 1


def mira(player, alvo):
    h.ctl("tp", *[str(v) for v in player])
    time.sleep(0.6)
    h.ctl("look-at", *[str(v) for v in alvo])
    time.sleep(0.6)


def usa(item, player, alvo):
    h.command("clear @s")
    h.command(f"give @s {item}")
    h.ctl("select", "1")
    mira(player, alvo)
    before = counts()
    h.ctl("use-item")
    time.sleep(1.0)
    return delta(before, counts())


def balde():
    n.sky()
    h.command("tick unfreeze")
    n.clear()
    h.command("gamemode survival")
    bad = []

    def caso(nome, montagem, item, player, alvo, quer_estados, quer_inventario):
        h.command(f"fill {SX - 2} {SY} {SZ - 2} {SX + 10} {SY + 4} {SZ + 2} minecraft:air")
        for c in montagem:
            h.command(c)
        time.sleep(0.8)
        mudou = usa(item, player, alvo)
        got = {pos: state(*pos) for pos in quer_estados}
        ok = all(want in got[pos] for pos, want in quer_estados.items()) and mudou == quer_inventario
        print(f"  {nome}: {got} inventário {mudou}  {'ok' if ok else 'DIFERE'}", flush=True)
        if not ok:
            bad.append((nome, got, mudou))

    laje = (SX, SY, SZ)
    cabo_sobre = (SX, SY + 1, SZ)
    print("controle: o balde de água na laje sem cabo nenhum (a mesma mira)")
    caso(
        "agua_na_laje_sem_cabo",
        [f"setblock {SX} {SY} {SZ} minecraft:stone_slab[type=bottom]"],
        "minecraft:water_bucket",
        (SX + 0.1, SY + 3.0, SZ + 2.5),
        (SX + 0.15, SY + 0.5, SZ + 0.15),
        {laje: "waterlogged=true"},
        {"minecraft:water_bucket": -1, "minecraft:bucket": 1},
    )
    print("o balde de água na laje, com o cabo em cima dela (a laje é o alvo; o cabo é o bloco ao lado do clique)")
    caso(
        "agua_na_laje",
        [f"setblock {SX} {SY} {SZ} minecraft:stone_slab[type=bottom]", f"setblock {SX} {SY + 1} {SZ} mekanism:basic_universal_cable"],
        "minecraft:water_bucket",
        (SX + 0.1, SY + 3.0, SZ + 2.5),
        (SX + 0.15, SY + 0.5, SZ + 0.15),
        {laje: "waterlogged=true", cabo_sobre: "basic_universal_cable"},
        {"minecraft:water_bucket": -1, "minecraft:bucket": 1},
    )
    print("o balde de água num bloco liso, com o cabo como alvo (o cabo recusa)")
    cx = SX + 4
    caso(
        "agua_no_cabo",
        [f"setblock {cx} {SY} {SZ} mekanism:basic_universal_cable"],
        "minecraft:water_bucket",
        (cx + 0.1, SY + 3.0, SZ + 2.5),
        (cx + 0.12, SY, SZ + 0.12),
        {(cx, SY, SZ): "basic_universal_cable"},
        {},
    )
    print("o balde de lava na laje, com o cabo em cima dela (a lava não enche a laje, o cabo é o alvo: recusa)")
    caso(
        "lava_na_laje",
        [f"setblock {SX} {SY} {SZ} minecraft:stone_slab[type=bottom]", f"setblock {SX} {SY + 1} {SZ} mekanism:basic_universal_cable"],
        "minecraft:lava_bucket",
        (SX + 0.1, SY + 3.0, SZ + 2.5),
        (SX + 0.15, SY + 0.5, SZ + 0.15),
        {laje: "waterlogged=false", cabo_sobre: "basic_universal_cable"},
        {},
    )
    print("TUDO IGUAL AO ORIGINAL (no que o port recusa)" if not bad else f"HÁ DIFERENÇAS: {bad}")
    return 0 if not bad else 1


def luz(x, y, z):
    """A luz do céu armazenada na célula (`horizon-ctl light`)."""
    out = json.loads(h.ctl("light", str(x), str(y), str(z), str(x), str(y), str(z)))
    return out


FORNO = (
    ("mekanism:metallurgic_infuser", 0, 15, "solid_render falso: a luz do céu passa"),
    ("mekanismgenerators:heat_generator", 3, 15, "solid_render falso: a luz do céu passa"),
    ("mekanism:crusher", 6, None, "cubo cheio como o gabarito"),
    ("minecraft:stone", 9, None, "controle: cubo cheio"),
)


def forno(folder):
    n.sky()
    h.command("tick unfreeze")
    n.clear()
    h.ctl("tp", "70.5", "151.0", "70.5")
    time.sleep(0.5)
    y_alto = SY + 3
    for bloco, dx, _, _ in FORNO:
        h.command(f"setblock {SX + dx} {y_alto} {SZ} {bloco}")
    time.sleep(2.0)
    print("luz do céu na célula logo abaixo de cada bloco, flutuando a 3 blocos do chão (o original: o Infuser e o Heat Generator deixam a luz do céu passar, amortecimento 0)")
    for bloco, dx, _, nota in FORNO:
        print(f"  {bloco:36} abaixo: {luz(SX + dx, y_alto - 1, SZ)}  no bloco: {luz(SX + dx, y_alto, SZ)}  ({nota})", flush=True)
    # O print: os quatro no chão, de frente.
    n.clear()
    for bloco, dx, _, _ in FORNO:
        h.command(f"setblock {SX + dx} {SY} {SZ} {bloco}")
    time.sleep(1.0)
    h.ctl("tp", f"{SX + 4.5}", f"{SY + 2.0}", f"{SZ + 6.5}")
    h.ctl("look-at", f"{SX + 4.5}", f"{SY + 0.5}", f"{SZ + 0.5}")
    time.sleep(1.5)
    h.ctl("screenshot", f"{folder}/forno.png")
    print("print forno")
    return 0


if __name__ == "__main__":
    mode = sys.argv[1]
    code = 0
    try:
        if mode == "categoria":
            code = categoria()
        elif mode == "balde":
            code = balde()
        elif mode == "forno":
            code = forno(sys.argv[2])
        elif mode == "video":
            n.video()
    finally:
        n.restore()
        h.command("gamemode creative")
    sys.exit(code)
