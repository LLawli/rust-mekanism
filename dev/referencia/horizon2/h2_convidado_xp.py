#!/usr/bin/env python3
"""A aba de xp da Enrichment Chamber clicada pelo convidado de LAN (`PacketGuiInteract.COLLECT_XP`, `SupportsXpContainment.popExperience`): as receitas
guardadas viram bolas de experiência no jogador que clicou, e o `recipes_used` zera. Roda no container `horizon` com os dois clientes (`abrir_dois.sh`).

Passos: o convidado põe a câmara; o anfitrião instala o upgrade `xp_containment` e a energia (data merge, como `cena_telas.py`); 2 carvões andam alguns ticks;
o convidado abre a tela e clica no botão da aba de xp (esquerdo, e depois o direito, que não conta); lê-se o XP do convidado e do anfitrião (`data get entity`) e o
`recipes_used` do bloco. Esperado (o rastro `chamber_xp_containment`, e o clique do anfitrião em 2026-11-07): 2 carvões dão `recipes_used {enriching/enriched/carbon: 2}`,
o clique esquerdo dá 1 ponto de experiência a quem clicou (o convidado, não o anfitrião) e zera `recipes_used`; o direito não faz nada.

Uso: H_NAME=horizon h2_convidado_xp.py"""
import json, os, re, sys, time

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import h2_dois as d

SPOT = (-34, 70, 12)
STAND = (-34, 70, 9)
BLOCK = "mekanism:enrichment_chamber"
# O canto da tela de 176 por 166 a guiScale 3 numa janela de 1920 por 1080 (640 por 360 em pixels de GUI) e o botão da aba de xp (179, 94, 18 por 18).
LEFT, TOP = 232, 97
XP_BUTTON = (LEFT + 188, TOP + 103)


def host_cmd(text):
    return d.host_command(text)


def block_data(path=""):
    chat = host_cmd(f"data get block {SPOT[0]} {SPOT[1]} {SPOT[2]} {path}".strip())[0]
    return chat.split("data: ", 1)[-1] if "data:" in chat else chat


def xp_of(name):
    out = {}
    for key in ("XpTotal", "XpLevel", "XpP"):
        chat = host_cmd(f"data get entity {name} {key}")[0]
        m = re.search(r"([-0-9.]+)[A-Za-z]?$", chat.strip())
        out[key] = m.group(1) if m else chat
    return out


def main():
    ok = True
    host_cmd("tick freeze")
    host_cmd(f"gamemode creative {d.GUEST_NAME}")
    host_cmd(f"tp {d.GUEST_NAME} {STAND[0]}.5 {STAND[1]} {STAND[2]}.5 0 0")
    host_cmd(f"setblock {SPOT[0]} {SPOT[1]} {SPOT[2]} minecraft:air")
    time.sleep(0.5)
    host_cmd(f"clear {d.GUEST_NAME}")
    d.guest("give", BLOCK, "1")
    out = d.guest("place", *map(str, SPOT), "--item", BLOCK)
    if '"error"' in out:
        raise SystemExit(f"não pôs a câmara: {out[:200]}")
    host_cmd(f"clear {d.GUEST_NAME}")
    pos = " ".join(map(str, SPOT))
    host_cmd(f'data merge block {pos} {{component_upgrade:{{upgrades:{{"mekanism:xp_containment":1,"mekanism:speed":8}}}},redstone:0b}}')
    host_cmd("tick step 1")
    time.sleep(0.6)
    host_cmd(f'data merge block {pos} {{Items:[{{Slot:0b,id:"minecraft:coal",count:8}}]}}')
    # A energia de cada rodada paga uma ou duas operações (consumo da velocidade 8): repõe e anda 30 ticks, quatro vezes.
    for _ in range(4):
        host_cmd(f"data merge block {pos} {{energy:10000}}")
        host_cmd("tick step 30")
        time.sleep(0.35 + 0.05 * 30)
    print("bloco antes do clique:", block_data())
    before = {"convidado": xp_of(d.GUEST_NAME), "anfitriao": xp_of("Player90366")}
    print("xp antes", before)
    # O convidado abre a tela e clica.
    d.guest("close")
    d.guest("look", "0", "20")
    for _ in range(4):
        d.guest("interact", *map(str, SPOT))
        time.sleep(0.7)
        if '"screen":"container"' in d.guest("screen").replace(" ", ""):
            break
    else:
        raise SystemExit("o menu não abriu para o convidado")
    d.guest("screenshot", "/home/luka/.local/share/jogo-isolado/horizon/data/shots/dc_xp_aberta.png")
    d.guest("click-at", str(XP_BUTTON[0]), str(XP_BUTTON[1]), "right")
    host_cmd("tick step 2")
    time.sleep(0.6)
    mid = {"convidado": xp_of(d.GUEST_NAME), "recipes_used": block_data("recipes_used")}
    print("depois do clique direito:", mid)
    d.guest("click-at", str(XP_BUTTON[0]), str(XP_BUTTON[1]))
    host_cmd("tick step 10")
    time.sleep(1.0)
    after = {"convidado": xp_of(d.GUEST_NAME), "anfitriao": xp_of("Player90366"), "recipes_used": block_data("recipes_used")}
    print("depois do clique esquerdo:", after)
    d.guest("screenshot", "/home/luka/.local/share/jogo-isolado/horizon/data/shots/dc_xp_depois.png")
    d.guest("close")
    host_cmd("tick unfreeze")
    host_cmd(f"setblock {SPOT[0]} {SPOT[1]} {SPOT[2]} minecraft:air")


if __name__ == "__main__":
    main()
