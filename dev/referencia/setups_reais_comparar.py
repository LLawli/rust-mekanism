#!/usr/bin/env python3
"""Os setups reais, o original ao lado do port, tick a tick (frente `setups-reais`, decisão 196).

    setups_reais_comparar.py ORIGINAL.jsonl PORT.jsonl [SAIDA.txt] [TAG...]

`ORIGINAL.jsonl` é o `setups-reais.jsonl` da `testes/CenaSetups.java` (`oficial_setups.sh`), `PORT.jsonl` o de `horizon2/h2_setups_reais.py roteiro`. As linhas `tick` de cada janela (`abre1`, `fluxo`,
`afasta`, `volta`, `abre2`) são comparadas tick a tick: a fila de chunks da linha (cx de -2 a 30: `T` andando, `L` carregado e parado, `.` descarregado), e por família de rede (`En` energia,
`Fl` fluido, `Ch` químico, `He` calor) a lista dos transmissores e dos aceitadores de cada rede, mais o que ela carrega (o buffer e o que saiu no tick, a temperatura média no calor) e as
pilhas em trânsito. Uma linha com `*` difere. As linhas `marca` (o conteúdo dos aceitadores num instante) saem no fim, lado a lado.
"""
import json
import sys

FAMILIAS = (("Energy", "En"), ("Fluid", "Fl"), ("Chemical", "Ch"), ("Heat", "He"))
JANELAS = ("abre1", "fluxo", "afasta", "volta", "abre2")


def ler(caminho):
    ticks, marcas = {}, []
    for linha in open(caminho):
        linha = linha.strip()
        if not linha:
            continue
        row = json.loads(linha)
        if row["e"] == "tick":
            ticks.setdefault(row["tag"], {})[row["k"]] = row
        elif row["e"] == "marca":
            marcas.append(row)
    return ticks, marcas


def estrutura(row):
    """O que tem de ser igual nos dois jogos: a fila, e de cada família os transmissores e os aceitadores de cada rede."""
    out = {"fila": row.get("fila")}
    for nome, _ in FAMILIAS:
        redes = [r for r in row.get("redes", []) if r["k"] == nome]
        out[nome] = [(r["tx"], r["acc"]) for r in redes]
    out["transito"] = row.get("transito")
    return out


def fluxo(row):
    """O que as redes carregam: o buffer e o que saiu no tick (a temperatura média no calor)."""
    out = {}
    for nome, _ in FAMILIAS:
        redes = [r for r in row.get("redes", []) if r["k"] == nome]
        out[nome] = [(round(r["temp"], 4) if "temp" in r else r.get("buf"), r.get("saiu")) for r in redes]
    return out


def fila_curta(fila):
    if not fila:
        return "?"
    # `.` não aparece: só até o último carregado, e o corte do `T` para o `L`.
    t = fila.count("T")
    l = fila.count("L")
    return f"T{t}+L{l}"


def descreve(row):
    if row is None:
        return "(sem linha)"
    partes = [fila_curta(row.get("fila"))]
    for nome, curto in FAMILIAS:
        redes = [r for r in row.get("redes", []) if r["k"] == nome]
        if redes:
            partes.append(curto + " " + ",".join(f"{r['tx']}/{r['acc']}" for r in redes))
    if row.get("transito"):
        partes.append(f"tr {row['transito']}")
    return " ".join(partes)


def main():
    orig, marcas_o = ler(sys.argv[1])
    port, marcas_p = ler(sys.argv[2])
    tags = [t for t in sys.argv[4:]] or list(JANELAS)
    saida = []
    for tag in tags:
        o, p = orig.get(tag, {}), port.get(tag, {})
        if not o and not p:
            continue
        saida.append(f"== janela {tag}: {len(o)} ticks no original, {len(p)} no port ==")
        iguais_estrutura = iguais_fluxo = 0
        total = 0
        primeira_dif = None
        for k in sorted(set(o) | set(p)):
            ro, rp = o.get(k), p.get(k)
            if ro is None or rp is None:
                continue
            total += 1
            eo, ep = estrutura(ro), estrutura(rp)
            dif = eo != ep
            fo, fp = fluxo(ro), fluxo(rp)
            dif_fluxo = fo != fp
            iguais_estrutura += 0 if dif else 1
            iguais_fluxo += 0 if dif_fluxo else 1
            if dif and primeira_dif is None:
                primeira_dif = k
            # As 12 primeiras de cada janela, e toda linha que difere (as 60 primeiras que diferem).
            if k <= 12 or (dif and saida.count("*") < 0):
                pass
            mostrar = k <= 12 or dif or dif_fluxo and k % 25 == 0
            if mostrar:
                saida.append(f"{'*' if dif else ' '}{k:4d} original {descreve(ro)}")
                saida.append(f"      port     {descreve(rp)}")
        saida.append(f"-- {tag}: estrutura igual em {iguais_estrutura} de {total} ticks; buffer e fluxo iguais em {iguais_fluxo} de {total}; primeira diferença de estrutura: tick {primeira_dif}")
        saida.append("")
    saida.append("== marcas (o conteúdo dos aceitadores) ==")
    for mo in marcas_o:
        mp = next((m for m in marcas_p if m["tag"] == mo["tag"]), None)
        saida.append(f"{mo['tag']}:")
        for campo in ("gerador", "cubos", "tanques", "baus", "transito"):
            if campo in mo:
                saida.append(f"   {campo:9s} original {mo.get(campo)}")
                saida.append(f"   {'':9s} port     {mp.get(campo) if mp else None}")
    texto = "\n".join(saida)
    print(texto)
    if len(sys.argv) > 3 and sys.argv[3] != "-":
        open(sys.argv[3], "w").write(texto + "\n")


if __name__ == "__main__":
    main()
