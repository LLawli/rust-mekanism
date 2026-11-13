#!/usr/bin/env python3
"""Os primeiros ticks de um mundo aberto, o original ao lado do port (frente `rede-primeira-passada`, decisão 195).

    primeira_passada_comparar.py ORIGINAL.jsonl PORT.jsonl [SAIDA.txt]

`ORIGINAL.jsonl` é o `reset-mundo.jsonl` da `testes/CenaReset.java` (as linhas `started`, `tick_pre` e `tick` de cada abertura), `PORT.jsonl` o de `horizon2/h2_primeira_passada.py roteiro`
(as passadas depois do `open_ready`). Cada abertura do original aparece com as do port que lhe correspondem: a 2 (o mesmo mundo reaberto) com `a_*`, a 4 (com o Nether forçado) com `n_*`
e a 5 (o segundo mundo) com `b_*`. Por tick: os jogadores, os chunks carregados, o chunk (0, 0) andando, e quantas redes cada registro tem no fim do tick (`Post`).
"""
import json
import sys

# A abertura do original (a ordem das linhas `started`, de 1) para os rótulos do port que lhe correspondem.
PARES = {2: "a_", 4: "n_", 5: "b_"}
TICKS = 4


def original(caminho):
    aberturas, atual = [], None
    for linha in open(caminho):
        row = json.loads(linha)
        if row["e"] == "started":
            atual = {"pre": {}, "post": {}}
            aberturas.append(atual)
        elif atual is not None and row["e"] in ("tick_pre", "tick"):
            atual["pre" if row["e"] == "tick_pre" else "post"][row["k"]] = row
    return aberturas


def redes_original(row):
    """As redes do registro no fim do tick, sem a do transportador (`InventoryNetwork`): o port não tem sonda de rede dele, as quatro famílias com sonda são a energia, o fluido, o químico e o calor."""
    registro = row.get("registro") or {}
    tipos = sorted(item["k"].replace("Network", "") for item in registro.get("lista") or [])
    tipos = [tipo for tipo in tipos if tipo != "Inventory"]
    return len(tipos), tipos


def port(caminho):
    return [json.loads(linha) for linha in open(caminho)]


def redes_port(passada):
    familias = {"MBE": "Energy", "MBP": "Fluid", "MTN": "Chemical", "MBQ": "Heat"}
    # Cada nível que o servidor anda tem uma linha por família no `Post`: soma as redes de todas.
    return sum(len(linha.get("nets", [])) for f in familias for linha in passada.get(f, []))


def main():
    saida = []
    orig = original(sys.argv[1])
    por = port(sys.argv[2])
    for i, abertura in enumerate(orig, 1):
        if i not in PARES:
            continue
        saida.append(f"== abertura {i} do original ({'mesmo mundo reaberto' if i == 2 else 'com o Nether forçado' if i == 4 else 'o segundo mundo'}) ==")
        saida.append("original (CenaReset):")
        for k in range(1, TICKS + 1):
            pre, post = abertura["pre"].get(k), abertura["post"].get(k)
            if not post:
                continue
            n, tipos = redes_original(post)
            saida.append(f"  tick {k}: jogadores {pre['jogadores']}, chunks {pre['carregados']['minecraft:overworld']}, (0,0) anda {pre['anda00']} -> {post['anda00']}, redes no Post (sem o transportador) {n} {','.join(tipos)}")
        for row in por:
            if not row["rotulo"].startswith(PARES[i]):
                continue
            saida.append(f"port, {row['rotulo']} ({'sem a injeção' if row['baseline'] else 'com a injeção'}):")
            for passada in row["passadas"][1 : TICKS + 1]:
                estado = next((e for e in passada["mbt"] if e.get("e") == "estado"), None)
                ticks = sum(1 for e in passada["mbt"] if e.get("e") == "tick")
                extra = f", jogadores {estado['jogadores']}, chunks {estado['carregados']}, (0,0) anda {estado['anda00']}" if estado else ""
                saida.append(f"  passada {passada['k']}: block entities andaram {ticks}{extra}, redes no Post (sem o transportador) {redes_port(passada)}")
    texto = "\n".join(saida)
    print(texto)
    if len(sys.argv) > 3:
        open(sys.argv[3], "w").write(texto + "\n")


if __name__ == "__main__":
    main()
