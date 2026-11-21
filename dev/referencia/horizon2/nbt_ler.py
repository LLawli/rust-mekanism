#!/usr/bin/env python3
"""Lê um arquivo NBT (gzip ou cru) e o imprime numa linha, no jeito do SNBT. Uso: nbt_ler.py ARQUIVO. Roda no desktop (`freq` do `h2_dois_roteiro.py`)."""
import gzip, struct, sys


def read(data, pos, kind):
    if kind == 1:
        return f"{struct.unpack_from('>b', data, pos)[0]}b", pos + 1
    if kind == 2:
        return f"{struct.unpack_from('>h', data, pos)[0]}s", pos + 2
    if kind == 3:
        return str(struct.unpack_from('>i', data, pos)[0]), pos + 4
    if kind == 4:
        return f"{struct.unpack_from('>q', data, pos)[0]}L", pos + 8
    if kind == 5:
        return f"{struct.unpack_from('>f', data, pos)[0]}f", pos + 4
    if kind == 6:
        return f"{struct.unpack_from('>d', data, pos)[0]}d", pos + 8
    if kind in (7, 11, 12):
        n = struct.unpack_from('>i', data, pos)[0]
        code, size, tag = {7: ('b', 1, 'B'), 11: ('i', 4, 'I'), 12: ('q', 8, 'L')}[kind]
        values = struct.unpack_from(f'>{n}{code}', data, pos + 4)
        return f"[{tag}; " + ", ".join(map(str, values)) + "]", pos + 4 + n * size
    if kind == 8:
        n = struct.unpack_from('>H', data, pos)[0]
        return '"' + data[pos + 2:pos + 2 + n].decode('utf-8', 'replace') + '"', pos + 2 + n
    if kind == 9:
        inner, n = data[pos], struct.unpack_from('>i', data, pos + 1)[0]
        pos += 5
        out = []
        for _ in range(n):
            value, pos = read(data, pos, inner)
            out.append(value)
        return "[" + ", ".join(out) + "]", pos
    if kind == 10:
        out = []
        while data[pos] != 0:
            inner = data[pos]
            n = struct.unpack_from('>H', data, pos + 1)[0]
            name = data[pos + 3:pos + 3 + n].decode('utf-8', 'replace')
            value, pos = read(data, pos + 3 + n, inner)
            out.append(f"{name}: {value}")
        return "{" + ", ".join(sorted(out)) + "}", pos + 1
    raise ValueError(f"tag {kind}")


def main():
    raw = open(sys.argv[1], 'rb').read()
    data = gzip.decompress(raw) if raw[:2] == b'\x1f\x8b' else raw
    n = struct.unpack_from('>H', data, 1)[0]
    print(read(data, 3 + n, data[0])[0])


if __name__ == "__main__":
    main()
