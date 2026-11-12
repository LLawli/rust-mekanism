#!/usr/bin/env python3
"""O instrumento do bloco de nota em cima do sal e do bloco de bio fuel (`MekanismBlocks.java:980-982`, decisão 17 do plano), em jogo (`H_NAME=horizon2`, mundo de teste).

    h2_nota_instrumento.py medir [PASTA]   cinco bases (sal, bio fuel, lápis, areia, feno), o bloco de nota posto por jogador em cima de cada uma e o instrumento lido de volta;
                                           depois um bloco de redstone encostado toca a nota: o som medido é o evento de som do jogo (`heard` do `status`, o que o jogo
                                           tocou para o jogador: o jogo de teste é mudo). Uma segunda rodada põe o bloco de nota sobre a pedra (tambor) e troca a base por sal e bio fuel
                                           (a atualização de vizinho recalcula o instrumento). Sai 0 só se todo instrumento e todo som são os do original.
    h2_nota_instrumento.py cena PASTA      a fileira montada, um print
    h2_nota_instrumento.py video           o roteiro do vídeo (a nota tocando em cada base, da esquerda para a direita, com legenda de som ligada), gravado com o som do jogo
                                           (`NOME=<nome do gravar-jogo start>`; veja `video()`)

Clima limpo e meio-dia com os ciclos parados; as gamerules voltam ao fim.
"""
import json
import os
import subprocess
import sys
import threading
import time

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import h2_identity as h

# Cada base e o instrumento que o original dá ao bloco de nota em cima dela (`NoteBlockInstrument` do `instrument(...)` do bloco; o sal e o bio fuel são os do Mekanism).
BASES = (
    ("mekanism:block_salt", "snare"),
    ("mekanism:block_bio_fuel", "banjo"),
    ("minecraft:lapis_block", "harp"),
    ("minecraft:sand", "snare"),
    ("minecraft:hay_block", "banjo"),
)
# O mundo de teste (`limpo`) é uma caverna a esta altura: a fileira fica numa plataforma acima do terreno, com céu, e o `tp` do `horizon-ctl` (que não escreve no chat) leva o jogador.
X0, STEP, Y, Z = 62, 3, 150, 80
PLATFORM = ("fill 55 150 74 90 160 96 minecraft:air", "fill 55 149 74 90 149 96 minecraft:stone")
# De frente para o norte (yaw 180), o x cresce para a direita: o sal fica à esquerda e o feno à direita, na ordem de BASES.
CAMERA = (68.5, 152.4, 89.0, 180, 14)


MARK = os.environ.get("MARCA")


def x_of(i):
    return X0 + STEP * i


def sky():
    h.ctl("close")
    for c in ("gamemode creative", "weather clear", "time set noon", "gamerule advance_time false", "gamerule advance_weather false", "gamerule spawn_mobs false", "difficulty peaceful", "kill @e[type=!minecraft:player]"):
        h.command(c)


def restore():
    h.command("tick unfreeze")
    h.command("gamerule advance_time true")
    h.command("gamerule advance_weather true")
    h.command("gamerule spawn_mobs true")


def clear():
    for c in PLATFORM:
        h.command(c)
    time.sleep(0.5)


def state_of(x):
    text = h.ctl("block", str(x), str(Y + 1), str(Z))
    return json.loads(text)["state"]


def instrument_of(x):
    state = state_of(x)
    if "note_block[" not in state:
        return state
    return state.split("instrument=")[1].split(",")[0].rstrip("]")


def events():
    """O `heard` do `status`: quantas vezes cada som foi tocado para o jogador (o evento de som do jogo, mudo ou não), somado desde a abertura."""
    return json.loads(h.ctl("status"))["heard"]


def play(x):
    """Um bloco de redstone encostado no bloco de nota: a vizinhança liga o sinal e `NoteBlock.playNote` manda o som. Devolve o que o jogo mandou tocar de nota."""
    before = events()
    h.ctl("setblock", str(x), str(Y + 1), str(Z + 1), "minecraft:redstone_block")
    time.sleep(1.0)
    after = events()
    h.ctl("setblock", str(x), str(Y + 1), str(Z + 1), "minecraft:air")
    time.sleep(0.3)
    return {n: after[n] - before.get(n, 0) for n in after if n.startswith("minecraft:block.note_block") and after[n] != before.get(n, 0)}


def place_note(x):
    h.ctl("tp", f"{x}.5", str(Y + 0.5), f"{Z - 2}.5")
    h.ctl("look", "0", "30")
    time.sleep(0.4)
    out = json.loads(h.ctl("place", str(x), str(Y + 1), str(Z)))
    assert "error" not in out, out


def measure():
    sky()
    h.command("tick unfreeze")
    clear()
    h.command("clear @s")
    h.command("give @s minecraft:note_block 64")
    time.sleep(0.5)
    h.ctl("select", "note_block")
    bad = []
    print("rodada 1: o bloco de nota posto por jogador em cima da base")
    for i, (base, instrument) in enumerate(BASES):
        x = x_of(i)
        h.ctl("setblock", str(x), str(Y), str(Z), base)
    time.sleep(1.0)
    for i, (base, instrument) in enumerate(BASES):
        x = x_of(i)
        place_note(x)
        time.sleep(0.5)
        got = instrument_of(x)
        sound = play(x)
        want = {f"minecraft:block.note_block.{instrument}": 1}
        ok = got == instrument and sound == want
        print(f"  {base:26} instrumento {got:6} (original {instrument:6}) som {sound}  {'ok' if ok else 'DIFERE'}")
        if not ok:
            bad.append((base, got, sound))
    print("rodada 2: o bloco de nota sobre a pedra e a base trocada por baixo (a vizinhança recalcula o instrumento)")
    for i, (base, instrument) in enumerate(BASES):
        x = x_of(i)
        h.ctl("setblock", str(x), str(Y + 1), str(Z), "minecraft:air")
        h.ctl("setblock", str(x), str(Y), str(Z), "minecraft:stone")
    time.sleep(1.0)
    for i, (base, instrument) in enumerate(BASES):
        x = x_of(i)
        place_note(x)
        time.sleep(0.5)
        on_stone = instrument_of(x)
        h.ctl("setblock", str(x), str(Y), str(Z), base)
        time.sleep(0.5)
        got = instrument_of(x)
        sound = play(x)
        want = {f"minecraft:block.note_block.{instrument}": 1}
        ok = on_stone == "basedrum" and got == instrument and sound == want
        print(f"  {base:26} sobre a pedra {on_stone:8} depois da troca {got:6} (original {instrument:6}) som {sound}  {'ok' if ok else 'DIFERE'}")
        if not ok:
            bad.append((base, on_stone, got, sound))
    print("TUDO IGUAL AO ORIGINAL" if not bad else f"HÁ DIFERENÇAS: {bad}")
    return 0 if not bad else 1


def build_row():
    clear()
    h.command("clear @s")
    h.command("give @s minecraft:note_block 64")
    time.sleep(0.5)
    h.ctl("select", "note_block")
    for i, (base, _) in enumerate(BASES):
        h.ctl("setblock", str(x_of(i)), str(Y), str(Z), base)
    time.sleep(1.0)
    for i, _ in enumerate(BASES):
        place_note(x_of(i))
        time.sleep(0.4)
    h.ctl("tp", *map(str, CAMERA[:3]))
    h.ctl("look", *map(str, CAMERA[3:]))
    h.ctl("hotbar", "2")


def scene(folder):
    sky()
    h.command("tick unfreeze")
    build_row()
    time.sleep(1.5)
    pass  # a pasta vem do chamador (no host: .../horizon2/data/shots/NOME; no container: o mesmo caminho com `horizon`)
    h.ctl("screenshot", f"{folder}/fileira.png")
    print("print fileira")


AUDIO_NULO = os.environ.get("AUDIO_NULO", "~/.cache/rust-mekanism/audio_nulo.sh")
CONTAINER = os.environ.get("H_NAME", "horizon2")
GRAVAR = "~/.cache/rust-mekanism/gravar-jogo"
SOUND_OFF = ("music", "weather", "hostile", "neutral", "player", "ambient", "voice", "ui", "block")


def desktop(args):
    return subprocess.run(["ssh", "desktop", f"{AUDIO_NULO} {args}"], capture_output=True, text=True)


def mute():
    h.ctl("options-set", "soundCategory_master", "0")


def video():
    """A fileira montada, a legenda de som ligada, e a nota tocando em cada base da esquerda para a direita (sal, bio fuel, lápis, areia, feno), com o som do jogo no vídeo.

    O bloco de nota toca na categoria `record` (Jukebox/Note Blocks): só ela fica ligada, e o volume mestre só sobe depois de o stream do container estar no sink nulo
    (`audio_nulo.sh ligar` e `verificar`). Uma thread confere a cada meio segundo (`seguro`: nenhum link fora de `som_nulo` e `som_grav_*`) e, se o stream sair, o volume mestre volta a 0
    na hora. A gravação é a do `gravar-jogo rec <NOME> nota.mp4 <s> <container>` (o container com `JOGO_PW_SOCK`), que grava o som do jogo num sink próprio e o junta ao vídeo.
    Variáveis: `NOME` (o nome do `gravar-jogo start`), `SEGUNDOS` (padrão 20), `ARQUIVO` (padrão nota.mp4)."""
    nome, segundos, arquivo = os.environ["NOME"], os.environ.get("SEGUNDOS", "20"), os.environ.get("ARQUIVO", "nota.mp4")
    sky()
    h.command("tick unfreeze")
    build_row()
    mute()
    h.ctl("options-set", "showSubtitles", "true")
    h.ctl("options-set", "chatVisibility", "2")
    for categoria in SOUND_OFF:
        h.ctl("options-set", f"soundCategory_{categoria}", "0")
    h.ctl("options-set", "soundCategory_record", "1.0")
    ligado = False
    for _ in range(60):
        if "movidos 0" not in desktop(f"ligar {CONTAINER}").stdout and desktop(f"verificar {CONTAINER}").stdout.startswith("ok"):
            ligado = True
            break
        time.sleep(1)
    if not ligado:
        print("o stream do jogo não foi para o sink nulo: nada gravado, volume mestre em 0")
        sys.exit(1)
    print("stream no sink nulo:", desktop(f"verificar {CONTAINER}").stdout.strip(), flush=True)
    parar = threading.Event()

    def vigia():
        while not parar.is_set():
            if not desktop(f"seguro {CONTAINER}").stdout.startswith("ok"):
                print("stream fora dos sinks de teste: volume mestre em 0", flush=True)
                mute()
            parar.wait(0.5)

    thread = threading.Thread(target=vigia, daemon=True)
    thread.start()
    rec = None
    try:
        h.ctl("options-set", "soundCategory_master", "1.0")
        time.sleep(3.0)
        if MARK:
            open(MARK, "w").write("começa\n")
        rec = subprocess.Popen(["ssh", "desktop", f"{GRAVAR} rec {nome} {arquivo} {segundos} {CONTAINER}"], stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)
        time.sleep(3.0)
        for i, _ in enumerate(BASES):
            x = x_of(i)
            h.ctl("setblock", str(x), str(Y + 1), str(Z + 1), "minecraft:redstone_block")
            time.sleep(1.2)
            h.ctl("setblock", str(x), str(Y + 1), str(Z + 1), "minecraft:air")
            time.sleep(1.4)
        time.sleep(1.0)
        out, err = rec.communicate(timeout=int(segundos) + 90)
        print(out, err, flush=True)
    finally:
        mute()
        parar.set()
        thread.join(timeout=5)
        if rec is not None and rec.poll() is None:
            rec.wait(timeout=60)
        clear()


if __name__ == "__main__":
    mode = sys.argv[1]
    code = 0
    try:
        if mode == "medir":
            code = measure()
        elif mode == "cena":
            scene(sys.argv[2])
        elif mode == "video":
            video()
    finally:
        restore()
    sys.exit(code)
