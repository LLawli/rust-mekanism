#!/usr/bin/env python3
"""pos_jogador.py ARQ.dat X Y Z: troca o `Pos` (lista de 3 doubles) do NBT comprimido de um jogador, sem o jogo aberto. Imprime o antes e o depois."""
import gzip, struct, sys
p = sys.argv[1]; novo = [float(v) for v in sys.argv[2:5]]
b = bytearray(gzip.decompress(open(p, "rb").read()))
k = b.find(b"\x09\x00\x03Pos\x06\x00\x00\x00\x03")
assert k >= 0, "sem Pos"
i = k + 11
print("antes", struct.unpack(">3d", b[i:i + 24]))
b[i:i + 24] = struct.pack(">3d", *novo)
print("depois", struct.unpack(">3d", b[i:i + 24]))
open(p, "wb").write(gzip.compress(bytes(b)))
