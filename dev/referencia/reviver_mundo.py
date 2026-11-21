#!/usr/bin/env python3
"""Devolve o `GameTestWorld` do cliente do original a um estado que funciona depois de o jogador morrer de radiação (frente `tanques`, 8.ª rodada).
Troca no lugar, sem mudar o tamanho de nada: a vida do jogador (20), o `DeathTime` (0), a dose de radiação dele (0) e a de cada fonte guardada no mundo (0: a
fonte some no primeiro decaimento). Uso: reviver_mundo.py PASTA_DO_MUNDO. Rode com o jogo fechado e com um backup feito."""
import gzip, struct, sys, glob, os

def patch(data, kind, name, value, every=False):
    key = bytes([kind]) + struct.pack('>H', len(name)) + name.encode()
    fmt = {2: '>h', 5: '>f', 6: '>d'}[kind]
    n, pos = 0, 0
    while True:
        pos = data.find(key, pos)
        if pos < 0:
            break
        at = pos + len(key)
        old = struct.unpack_from(fmt, data, at)[0]
        struct.pack_into(fmt, data, at, value)
        print(f"  {name}: {old} -> {value}")
        n += 1
        pos = at
        if not every:
            break
    return n

def edit(path, changes):
    raw = open(path, 'rb').read()
    packed = raw[:2] == b'\x1f\x8b'
    data = bytearray(gzip.decompress(raw) if packed else raw)
    print(path)
    total = sum(patch(data, *c) for c in changes)
    if total:
        open(path, 'wb').write(gzip.compress(bytes(data)) if packed else bytes(data))
    return total

world = sys.argv[1]
for player in glob.glob(os.path.join(world, 'players/data/*.dat')):
    # O composto `mekanism:radiation` do jogador guarda um double `radiation`; a vida é um float e o `DeathTime` um short.
    edit(player, [(5, 'Health', 20.0), (2, 'DeathTime', 0), (6, 'radiation', 0.0)])
for dim in glob.glob(os.path.join(world, 'dimensions/*/*/data/neoforge/data_attachments.dat')):
    edit(dim, [(6, 'radiation', 0.0, True)])
