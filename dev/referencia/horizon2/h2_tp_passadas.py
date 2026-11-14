#!/usr/bin/env python3
"""As passadas do `/tp` entre dimensões, contadas do instante em que o jogador deixa o nível (frente `tp-dimensao`, decisão 218).

    h2_tp_passadas.py GAME.LOG [ORIGINAL.jsonl]

Lê a sonda `MBT` do stderr do jogo (`TRANSMITTER_PROBE = true` na cópia de teste): a linha `hold` sai quando muda o motivo que segura um nível, e a primeira com o dono fora do Overworld é a passada N em que o
jogador deixou o nível (o comando roda no começo dela); `idle_unload` é a passada em que o nível solta os chunks; `Dealing with N orphan Transmitters` é a passada em que as redes voltam do NBT. Conta tudo a partir de N
(0 = a passada do comando), como o original conta a partir do tick em que o `tp` roda (o `marca` do rastro é a do tick anterior, então o tick do comando é `gt` + 1). Contar a partir da hora em que o
`horizon-ctl` mandou o comando (o `marcar` do `h2_reset_mundo.py`) dá 2 a 4 passadas a mais: é a latência do comando, não do jogo (a decisão 193 (2) tinha caído nisso).
"""
import json, re, sys

OVERWORLD = "minecraft:overworld"


def port(path):
    """Por ida e volta: a passada do comando, a da descarga e a da volta das redes."""
    holds, unloads, orphans = [], [], []
    for line in open(path, errors="replace"):
        m = re.match(r'MBT \{"t":(\d+),"e":"hold".*?"razoes":"(.*)"\}', line)
        if m:
            guests = re.search(rf"{OVERWORLD}:\{{held:\w+,guests:(\d+)", m.group(2))
            if guests:
                holds.append((int(m.group(1)), int(guests.group(1))))
            continue
        m = re.match(r'MBT \{"t":(\d+),"e":"idle_unload"', line)
        if m:
            unloads.append(int(m.group(1)))
            continue
        m = re.match(r'MBT \{"t":(\d+),"e":"pre"', line)
        if m:
            current = int(m.group(1))
        if "Dealing with" in line and "orphan Transmitters" in line:
            orphans.append(current)
    trips, left = [], None
    for t, guests in holds:
        if guests == 0:
            left = t
        elif left is not None:
            back = t
            unload = next((u for u in unloads if left < u <= back), None)
            net = next((o for o in orphans if o >= back and o <= back + 30 and o > (unload or back)), None)
            trips.append((left, unload, back, net))
            left = None
    return trips


def original(path):
    rows = [json.loads(line) for line in open(path)]
    out = {}
    for mark, tag in (("tp_nether", "no_nether"), ("tp_overworld", "de_volta")):
        gt = next(r["gt"] for r in rows if r["e"] == "marca" and r["nome"] == mark) + 1
        obs = [r for r in rows if r["e"] == "obs" and r["tag"] == tag]
        unload = next((r["gt"] for r in obs if r["gt"] >= gt and r["descarregados"].get(OVERWORLD)), None)
        nets = next((r["gt"] for r in obs if r["gt"] >= gt and r["redes"] >= 6), None)
        out[mark] = (gt, unload, nets)
    return out


def main():
    for left, unload, back, net in port(sys.argv[1]):
        print(f"port: o jogador deixa o Overworld na passada {left}; descarrega em +{(unload - left) if unload else '?'}; volta na {back}; as redes voltam em +{(net - back) if net else '?'}")
    if len(sys.argv) > 2:
        o = original(sys.argv[2])
        gt, unload, _ = o["tp_nether"]
        back, _, nets = o["tp_overworld"]
        print(f"original: descarrega em +{unload - gt}; as redes voltam em +{nets - back}")


if __name__ == "__main__":
    main()
