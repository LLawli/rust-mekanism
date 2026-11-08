#!/usr/bin/env python3
"""A barra de armadura do Heat Generator quando OUTRO agente muda a armadura de quem tem a tela aberta (um comando que o outro jogador manda): quando a tela atualiza.
Roda no container `horizon` com os dois clientes. O alvo (quem tem a tela aberta) é o anfitrião ou o convidado, e o agente é o outro cliente (`/item replace entity ALVO
armor.head with iron_helmet`, pelo chat dele). Medidas, para cada alvo:
  corrido    o servidor andando: o tempo (relógio) entre o comando e o slot 500 da tela mostrar o capacete, lido a cada ~0,1 s
  parado     `tick freeze`: o capacete aparece sem nenhum `tick step`? (o jogador do original tica parado, e o `broadcastChanges` roda nele)
  passo 1    parado, e depois de quantos `tick step 1` ele aparece
Uso: H_NAME=horizon h2_dois_armadura.py [anfitriao|convidado]..."""
import json, os, sys, time

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import h2_dois as d

SPOT = (-18, 70, 18)
STAND = "-17.5 70 15.5"
HOST_NAME = "Player90366"
HEAD_SLOT = 500


def slot_head(who):
    try:
        scr = json.loads(who("screen"))
    except json.JSONDecodeError:
        return "erro"
    for e in scr.get("slots", []):
        if e["slot"] == HEAD_SLOT:
            return e["item"] or ""
    return "sem slot"


def prepare(target, other, name):
    d.host("close"); d.guest("close")
    d.host("command", "tick unfreeze")
    d.host("command", f"gamemode creative {HOST_NAME}")
    d.host("command", f"gamemode creative {d.GUEST_NAME}")
    d.host("command", f"tp {HOST_NAME} {STAND} 0 0")
    d.host("command", f"tp {d.GUEST_NAME} -19.5 70 15.5 0 0")
    d.host("command", f"setblock {SPOT[0]} {SPOT[1]} {SPOT[2]} mekanismgenerators:heat_generator")
    time.sleep(1.0)
    d.host("command", f"data merge block {SPOT[0]} {SPOT[1]} {SPOT[2]} {{machinery_basis:{{v:1,tanks:[{{resource:\"minecraft:lava\",amount:1000L}}]}}}}")
    d.host("command", f"item replace entity {name} armor.head with minecraft:air")
    time.sleep(0.5)
    for _ in range(4):
        target("interact", *map(str, SPOT))
        time.sleep(0.8)
        if '"screen":"container"' in target("screen").replace(" ", ""):
            break
    else:
        raise SystemExit("o menu não abriu")
    time.sleep(0.6)
    assert slot_head(target) == "", slot_head(target)


def agent_command(other, name):
    """O comando sai do chat do outro cliente (o agente)."""
    t0 = time.time()
    other("command", f"item replace entity {name} armor.head with minecraft:iron_helmet")
    return t0


def poll(target, t0, limit):
    t = time.time()
    first = None
    while time.time() - t0 < limit:
        if slot_head(target) == "minecraft:iron_helmet":
            first = time.time() - t0
            break
    return first


def run(role):
    target, other, name = (d.host, d.guest, HOST_NAME) if role == "anfitriao" else (d.guest, d.host, d.GUEST_NAME)
    print(f"--- alvo: {role} ({name})")
    prepare(target, other, name)
    t0 = agent_command(other, name)
    seen = poll(target, t0, 5)
    print(f"corrido: capacete na tela depois de {seen:.2f} s" if seen is not None else "corrido: NÃO apareceu em 5 s")
    d.host("command", "tick unfreeze")
    prepare(target, other, name)
    d.host("command", "tick freeze")
    time.sleep(0.5)
    t0 = agent_command(other, name)
    seen = poll(target, t0, 2.0)
    print(f"parado, sem passo: {'apareceu em %.2f s' % seen if seen is not None else 'NÃO apareceu em 2 s'}")
    for k in range(1, 6):
        d.host("command", "tick step 1")
        time.sleep(0.5)
        if slot_head(target) == "minecraft:iron_helmet":
            print(f"parado: apareceu depois de {k} passo(s)")
            break
    else:
        print("parado: NÃO apareceu em 5 passos")
    d.host("command", "tick unfreeze")
    target("close")
    d.host("command", f"item replace entity {name} armor.head with minecraft:air")


if __name__ == "__main__":
    for role in (sys.argv[1:] or ["anfitriao", "convidado"]):
        run(role)
