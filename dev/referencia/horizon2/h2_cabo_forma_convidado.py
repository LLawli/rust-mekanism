#!/usr/bin/env python3
"""A forma de colisão dos transmissores no convidado de LAN (frente `cabo-forma`, decisão 211): o convidado cai sobre os braços da cena de `h2_cabo_forma.py` (a mesma lista de pessoas) e anda de encontro a eles,
com o anfitrião e o convidado em `horizon2` (`abrir_dois.sh`), e as posições vão a `SAIDA.json` para o `comparar` de `h2_cabo_forma.py` (port contra original). A cena já está no mundo (`h2_cabo_forma.py montar` no
anfitrião, salvo ao sair). O roteador de `h2_dois.py` manda o `tp` e o `state` ao convidado e os comandos ao anfitrião (com `@s` trocado pelo nome do convidado).

    H_NAME=horizon2 h2_cabo_forma_convidado.py SAIDA.json"""
import os, sys
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
os.environ.setdefault("H_NAME", "horizon2")
import h2_dois
h2_dois.install_router()
import h2_cabo_forma as f

if __name__ == "__main__":
    f.pessoa(sys.argv[1])
