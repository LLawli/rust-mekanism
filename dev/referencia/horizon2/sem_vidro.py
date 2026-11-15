#!/usr/bin/env python3
"""Tira (ou devolve) do conteúdo do `mekanism` em `~/Games/Horizon*/mods/conteudo` as 648 partes de vidro por cor dos quatro transportadores logísticos (decisão 216), para medir o tempo de carga
com e sem elas (frente `transportador-sobras`). Roda no desktop.

    sem_vidro.py tirar PASTA_DE_MODS     # guarda os quatro blockstates em PASTA/../sem-vidro-antes e tira do `multipart` as partes `logistical_glass/`; o conteúdo do vidro (modelos) sai de `models/`
    sem_vidro.py devolver PASTA_DE_MODS  # põe tudo de volta
"""
import json, os, shutil, sys

TIERS = ("basic", "advanced", "elite", "ultimate")


def main():
    modo, mods = sys.argv[1], os.path.expanduser(sys.argv[2])
    raiz = os.path.join(mods, "conteudo/mekanism/assets/mekanism")
    guarda = os.path.join(mods, "sem-vidro-antes")
    glass = os.path.join(raiz, "models/block/transmitter_part/logistical_glass")
    if modo == "tirar":
        os.makedirs(guarda, exist_ok=True)
        total = 0
        for tier in TIERS:
            arq = os.path.join(raiz, f"blockstates/{tier}_logistical_transporter.json")
            shutil.copy2(arq, os.path.join(guarda, os.path.basename(arq)))
            dados = json.load(open(arq))
            antes = len(dados["multipart"])
            dados["multipart"] = [m for m in dados["multipart"] if "logistical_glass/" not in json.dumps(m)]
            total += antes - len(dados["multipart"])
            json.dump(dados, open(arq, "w"), indent=2)
        shutil.move(glass, os.path.join(guarda, "logistical_glass"))
        print("partes tiradas dos quatro blockstates:", total)
    else:
        for tier in TIERS:
            nome = f"{tier}_logistical_transporter.json"
            shutil.copy2(os.path.join(guarda, nome), os.path.join(raiz, "blockstates", nome))
        shutil.move(os.path.join(guarda, "logistical_glass"), glass)
        print("devolvido")


if __name__ == "__main__":
    main()
