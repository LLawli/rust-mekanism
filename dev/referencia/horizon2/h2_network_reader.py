#!/usr/bin/env python3
"""O Network Reader no jogo (frente `fase3-faltas`, Fase 3): o clique nos transmissores das cinco famílias, num Heat Generator e num bloco vizinho.

    H_NAME=horizon h2_network_reader.py textos        # as cinco famílias sem máquina por perto: o chat do port contra as linhas `*_plain` do rastro do original, uma a uma
    H_NAME=horizon h2_network_reader.py vizinhos      # o clique agachado no Heat Generator (a temperatura) e no baú que um transportador toca (as redes que o têm como aceitador)
    H_NAME=horizon h2_network_reader.py prints        # o clique em cada família com o print do chat recortado, empilhado
    NOME=... h2_network_reader.py video               # o vídeo com som

O texto do chat vem de `horizon-ctl messages`, a tradução já aplicada (o idioma é o en_us do mod). O rastro do original (`rastros/network_reader.jsonl`) guarda o `Component` de cada linha; aqui o texto
dele é montado com o mesmo `en_us.json` (`render`), para comparar o que o jogador lê.
"""
import json, os, re, subprocess, sys, time

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
os.environ.setdefault("H_NAME", "horizon")
import h2_ajustes_jogo as a
import h2_identity as h

TOOLS = os.path.dirname(os.path.abspath(__file__))
RASTRO = os.path.join(TOOLS, "..", "rastros", "network_reader.jsonl")
LANG = os.path.join(TOOLS, "..", "..", "..", "mods", "mekanism", "content", "assets", "mekanism", "lang", "en_us.json")
READER = "mekanism:network_reader"
GENERATOR = "mekanismgenerators:heat_generator"
BLOCKS = {
    "cable": "mekanism:basic_universal_cable",
    "pipe": "mekanism:basic_mechanical_pipe",
    "tube": "mekanism:basic_pressurized_tube",
    "conductor": "mekanism:basic_thermodynamic_conductor",
    "transporter": "mekanism:basic_logistical_transporter",
}
cmd, wait, at, txt = a.cmd, a.wait, a.at, a.txt


def lang():
    with open(LANG) as file:
        return json.load(file)


def render(component, table, inherit=None):
    """O texto que o jogador lê de um `Component` JSON do original, com a tradução em inglês do mod (e a do vanilla para água e lava)."""
    if isinstance(component, str):
        return component
    if isinstance(component, (int, float)):
        return str(component)
    if "text" in component:
        text = component["text"] + "".join(render(extra, table) for extra in component.get("extra", []))
        return text
    key = component["translate"]
    args = [render(arg, table) for arg in component.get("with", [])]
    vanilla = {"block.minecraft.water": "Water", "block.minecraft.lava": "Lava"}
    template = table.get(key) or vanilla.get(key) or key
    out = template
    for i, arg in enumerate(args, 1):
        out = out.replace("%%%d$s" % i, arg)
    return out.replace("%%", "%")


def expected(case, face="up"):
    """O chat do original (texto) para a primeira linha do caso e da face."""
    table = lang()
    with open(RASTRO) as file:
        for line in file:
            row = json.loads(line)
            if row.get("case") == case and row.get("face") == face and not row.get("sneak"):
                return [render(c, table) for c in row["chat"]]
    raise SystemExit("sem a linha %s %s no rastro" % (case, face))


def messages(n=12):
    """O chat do port, do mais antigo ao mais novo, só o que o leitor escreveu (tira o que for do `/give` e dos comandos)."""
    raw = json.loads(h.ctl("messages", str(n)))
    chat = raw.get("chat") or raw.get("messages") or []
    lines = []
    for item in chat:
        lines.append(item if isinstance(item, str) else item.get("text") or item.get("message") or json.dumps(item))
    return list(reversed(lines))


def montar(kind, extra=None):
    a.clear()
    cmd("gamemode creative @s")
    cmd("clear @s")
    h.ctl("options-set", "chatVisibility", "0")
    for x in range(3):
        cmd("setblock %s %s" % (txt(at(x)), BLOCKS[kind]))
    if extra:
        extra()
    wait(60)
    h.ctl("give", READER, "1")
    wait(6)
    h.ctl("select", READER)


def clicar(pos, point=(0.5, 0.5, 0.5), sneak=False, stand=None):
    """O clique direito com o leitor de quem olha `point` (no bloco `pos`)."""
    if stand is not None:
        cmd("tp @s %.2f %.2f %.2f" % stand)
        wait(12)
    if sneak:
        h.ctl("key", "shift", "down")
        wait(6)
    h.ctl("look-at", *("%.3f" % (pos[i] + point[i]) for i in range(3)))
    wait(6)
    h.ctl("mouse", "right", "click")
    wait(10)
    if sneak:
        h.ctl("key", "shift", "up")
    time.sleep(1.0)
    return messages(40)


def mascara(linhas, kind):
    """O condutor lê o calor do ar do bioma, que no jogo (290 a 297 K) não é o do GameTest do original (decisão 146): os números das linhas de calor saem da comparação, o resto fica."""
    if kind != "conductor":
        return [l.strip() for l in linhas]
    return [re.sub(r"\d+(\.\d+)?%", "#%", re.sub(r"[-+]?\d+(\.\d+)?[\u00b5mkM]?K", "#K", l.strip())) for l in linhas]


def textos():
    ok = True
    for kind in BLOCKS:
        # `transporter_plain` do rastro: um baú no fim da linha (um aceitador).
        montar(kind, (lambda: cmd("setblock %s minecraft:chest" % txt(at(3)))) if kind == "transporter" else None)
        a.enquadrar(3)
        want = expected("%s_plain/middle" % kind)
        got = []
        for _ in range(3):
            got = clicar(at(1), (0.5, 0.5, 0.5), stand=(at(1)[0] + 0.5, at(1)[1] + 2.5, at(1)[2] - 1.4))
            if got and got[-1].strip() == want[-1].strip():
                break
        same = mascara(got[-len(want):], kind) == mascara(want, kind)
        ok &= same
        print("[%s] %s" % ("igual" if same else "DIFERE", kind))
        if not same:
            print("  jogo:", got[-len(want):])
            print("  original:", want)
    return ok


CORTE = "1000x232+0+743"
LOCAL = os.path.expanduser(os.environ.get("LOCAL", "~/.cache/rust-mekanism/wa/fase3-faltas"))
ORIGINAIS = os.path.join(LOCAL, "original")
# O nome do print do original (`CenaLeitor`, `cenas/leitor.txt`) e o do port, a cena e o clique: as mesmas do original.
CENAS = [
    ("cabo", "cable", None, False, 1),
    ("cano", "pipe", None, False, 1),
    ("tubo", "tube", None, False, 1),
    ("condutor", "conductor", None, False, 1),
    ("transportador", "transporter", "chest", False, 1),
    ("bau", "transporter", "chest", True, 3),
    ("gerador", None, None, True, 1),
]


def empilhar(original, port, saida):
    """O chat do original (em cima) e o do port (embaixo), recortados no mesmo trecho."""
    partes = []
    for arquivo, rotulo in ((original, "original"), (port, "port")):
        corte = arquivo + ".corte.png"
        subprocess.run(["magick", arquivo, "-crop", CORTE, "+repage", "-gravity", "NorthEast", "-fill", "white", "-undercolor", "#000000aa", "-pointsize", "28", "-annotate", "+8+8", rotulo, corte], check=True)
        partes.append(corte)
    subprocess.run(["magick", *partes, "-append", saida], check=True)
    for parte in partes:
        os.remove(parte)


def prints():
    """O clique do original (`CenaLeitor`) e o do port em cada cena, o chat recortado no mesmo trecho e empilhado (`comparacao_<nome>.png`)."""
    a.SHOTS = "/home/luka/.cache/rust-mekanism/fase3-faltas/prints"
    a.LOCAL = LOCAL
    cmd("weather clear")
    cmd("time set noon")
    cmd("gamerule advance_time false")
    cmd("gamerule advance_weather false")
    for nome, kind, extra, sneak, alvo in CENAS:
        def preparar(kind=kind, extra=extra, nome=nome):
            if kind is None:
                a.clear()
                cmd("gamemode creative @s")
                cmd("clear @s")
                h.ctl("options-set", "chatVisibility", "0")
                cmd("setblock %s %s" % (txt(at(1)), GENERATOR))
                wait(60)
                h.ctl("give", READER, "1")
                wait(6)
                h.ctl("select", READER)
            else:
                montar(kind, (lambda: cmd("setblock %s minecraft:chest" % txt(at(3)))) if extra == "chest" else None)
        preparar()
        mira = at(alvo)
        stand = (at(1)[0] + 0.5 + (1.0 if alvo == 3 else 0.0), at(1)[1] + 2.5, at(1)[2] - 1.4)
        a.enquadrar(4)
        clicar(mira, (0.5, 0.5, 0.5), sneak=sneak, stand=stand)
        arquivo = a.shot("leitor_" + nome)
        origem = os.path.join(ORIGINAIS, "leitor_oficial_%s.png" % nome)
        empilhar(origem, arquivo, os.path.join(LOCAL, "comparacao_%s.png" % nome))
        print("comparacao_%s.png" % nome)


def main():
    mode = sys.argv[1] if len(sys.argv) > 1 else "textos"
    if mode == "textos":
        sys.exit(0 if textos() else 1)
    if mode == "prints":
        prints()
        return
    raise SystemExit("modo desconhecido: " + mode)


if __name__ == "__main__":
    main()
