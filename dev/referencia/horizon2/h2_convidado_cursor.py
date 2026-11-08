#!/usr/bin/env python3
"""`h2_cursor.py` com o convidado de LAN no lugar do jogador: o mesmo rastro do Java (`slot_click_<máquina>.jsonl`), mas o clique é do segundo cliente (entra pelo
`lan_machine_click` do Horizon, o caminho que o original não tem: lá o convidado e o anfitrião passam pelo mesmo `ServerGamePacketListenerImpl`). Os comandos
(`data merge`, `item replace`, `setblock`) vão pelo anfitrião.

Uso: H_NAME=horizon h2_convidado_cursor.py MAQUINA [--todas]"""
import os, sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import h2_dois
h2_dois.install_router()
import h2_cursor as c

# Uma plataforma de pedra a 69 no mundo `dois` (terreno de teste do Horizon), a 3 blocos uma da outra.
c.SPOT = {"enrichment_chamber": (-34, 70, 12), "crusher": (-30, 70, 12), "energized_smelter": (-26, 70, 12),
          "metallurgic_infuser": (-22, 70, 12), "heat_generator": (-18, 70, 12)}

# O rastro do Java tem também `throw`, `throw_all`, `clone` e `double`, que `h2_cursor.py` não sabe fazer (nem o roteiro do anfitrião): ficam de fora.
_select = c.select
c.select = lambda rows, every, machine=None: _select([r for r in rows if r["action"] in ("left", "right", "number", "drag")], every, machine)

if __name__ == "__main__":
    c.main()
